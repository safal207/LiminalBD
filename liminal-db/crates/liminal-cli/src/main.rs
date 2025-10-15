use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc as StdArc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use futures_util::{SinkExt, StreamExt};
use hex::encode as encode_hex;
use hex::FromHex;
use liminal_bridge_abi::ffi::{liminal_init, liminal_pull, liminal_push};
use liminal_bridge_abi::protocol::BridgeConfig;
use liminal_core::life_loop::run_loop;
use liminal_core::types::{Hint, Impulse, ImpulseKind};
use liminal_core::{
    parse_lql, ClusterField, LqlResponse, ReflexAction, ReflexRule, ReflexWhen, TrsConfig,
    ViewStats,
};
use liminal_sensor::start_host_sensors;
use liminal_store::{decode_delta, DiskJournal, Offset, SnapshotInfo, StoreStats};
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio::task;
use tokio_tungstenite::{accept_async, tungstenite::Message};

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut pipe_cbor = false;
    let mut store_path: Option<PathBuf> = None;
    let mut snap_interval_secs: u64 = 60;
    let mut snap_maxwal: u64 = 5_000;
    let mut ws_port: Option<u16> = None;
    let mut ws_format: Option<WsFormat> = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--pipe-cbor" => pipe_cbor = true,
            "--store" => {
                let Some(path) = args.next() else {
                    return Err(anyhow!("--store requires a path"));
                };
                store_path = Some(PathBuf::from(path));
            }
            "--snap-interval" => {
                let Some(value) = args.next() else {
                    return Err(anyhow!("--snap-interval requires seconds"));
                };
                snap_interval_secs = value.parse::<u64>().map_err(|_| {
                    anyhow!("--snap-interval expects positive seconds, got {value}")
                })?;
                if snap_interval_secs == 0 {
                    return Err(anyhow!("--snap-interval must be greater than zero"));
                }
            }
            "--snap-maxwal" => {
                let Some(value) = args.next() else {
                    return Err(anyhow!("--snap-maxwal requires a number"));
                };
                snap_maxwal = value
                    .parse::<u64>()
                    .map_err(|_| anyhow!("--snap-maxwal expects a number, got {value}"))?;
                if snap_maxwal == 0 {
                    return Err(anyhow!("--snap-maxwal must be greater than zero"));
                }
            }
            "--ws-port" => {
                let Some(value) = args.next() else {
                    return Err(anyhow!("--ws-port requires a port number"));
                };
                let port = value
                    .parse::<u16>()
                    .map_err(|_| anyhow!("--ws-port expects a valid port, got {value}"))?;
                ws_port = Some(port);
            }
            "--ws-format" => {
                let Some(value) = args.next() else {
                    return Err(anyhow!("--ws-format requires a value"));
                };
                ws_format = Some(WsFormat::from_str(&value)?);
            }
            other => return Err(anyhow!("unknown argument: {other}")),
        }
    }

    if ws_format.is_some() && ws_port.is_none() {
        return Err(anyhow!("--ws-format requires --ws-port"));
    }

    if pipe_cbor && ws_port.is_some() {
        return Err(anyhow!(
            "WebSocket options are not available in --pipe-cbor mode"
        ));
    }

    let store_config = store_path.map(|path| StoreRuntimeConfig {
        path,
        snap_interval: Duration::from_secs(snap_interval_secs),
        max_wal_events: snap_maxwal,
    });

    let ws_config = ws_port.map(|port| WsRuntimeConfig {
        port,
        format: ws_format.unwrap_or_default(),
    });

    if pipe_cbor {
        run_pipe_cbor(store_config).await
    } else {
        run_interactive(store_config, ws_config).await
    }
}

#[derive(Clone)]
struct StoreRuntimeConfig {
    path: PathBuf,
    snap_interval: Duration,
    max_wal_events: u64,
}

#[derive(Clone)]
struct WsRuntimeConfig {
    port: u16,
    format: WsFormat,
}

#[derive(Clone, Copy)]
enum WsFormat {
    Json,
}

impl Default for WsFormat {
    fn default() -> Self {
        WsFormat::Json
    }
}

impl FromStr for WsFormat {
    type Err = anyhow::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.trim().to_lowercase().as_str() {
            "json" => Ok(WsFormat::Json),
            other => Err(anyhow!("unsupported WebSocket format: {other}")),
        }
    }
}

async fn run_interactive(
    store: Option<StoreRuntimeConfig>,
    ws: Option<WsRuntimeConfig>,
) -> Result<()> {
    let (field, journal, store_cfg) = if let Some(cfg) = store.clone() {
        let journal = StdArc::new(DiskJournal::open(&cfg.path)?);
        let (mut field, replay_offset) =
            if let Some((seed, offset)) = journal.load_latest_snapshot()? {
                (seed.into_field(), offset)
            } else {
                (ClusterField::new(), Offset::start())
            };
        let mut stream = journal.stream_from(replay_offset)?;
        while let Some(record) = stream.next() {
            let bytes = record?;
            let delta = decode_delta(&bytes)?;
            field.apply_delta(&delta);
        }
        if field.cells.is_empty() {
            field.add_root("liminal/root");
        }
        field.set_journal(journal.clone());
        (field, Some(journal), Some(cfg))
    } else {
        let mut field = ClusterField::new();
        field.add_root("liminal/root");
        (field, None, None)
    };
    let field = StdArc::new(Mutex::new(field));

    let (tx, mut rx) = mpsc::channel::<Impulse>(128);
    let (event_tx, _) = broadcast::channel::<String>(1024);

    tokio::spawn(start_host_sensors(tx.clone()));

    if let (Some(journal_arc), Some(cfg)) = (journal.clone(), store_cfg.clone()) {
        let field_for_snap = field.clone();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(5));
            let mut last_snapshot = Instant::now();
            loop {
                ticker.tick().await;
                if journal_arc.delta_since_snapshot() >= cfg.max_wal_events
                    || last_snapshot.elapsed() >= cfg.snap_interval
                {
                    match snapshot_once(&field_for_snap, &journal_arc).await {
                        Ok(info) => {
                            last_snapshot = Instant::now();
                            if let Err(err) = journal_arc.run_gc(info.offset) {
                                eprintln!("snapshot GC failed: {err}");
                            }
                            log_snapshot("AUTO-", &info);
                        }
                        Err(err) => eprintln!("auto snapshot failed: {err}"),
                    }
                }
            }
        });
    }

    let field_for_impulses = field.clone();
    let event_tx_for_impulses = event_tx.clone();
    tokio::spawn(async move {
        while let Some(impulse) = rx.recv().await {
            let logs = {
                let mut guard = field_for_impulses.lock().await;
                guard.route_impulse(impulse)
            };
            for log in logs {
                if let Some(event) = convert_reflex_log(&log) {
                    let _ = event_tx_for_impulses.send(event);
                }
                println!("IMPULSE {}", log);
            }
        }
    });

    let hint_buffer: StdArc<StdMutex<Vec<Hint>>> = StdArc::new(StdMutex::new(Vec::new()));

    let loop_field = field.clone();
    let hints_for_metrics = hint_buffer.clone();
    let event_tx_for_metrics = event_tx.clone();
    let event_tx_for_events = event_tx.clone();
    tokio::spawn(async move {
        run_loop(
            loop_field,
            200,
            move |metrics| {
                let hints = {
                    let mut store = hints_for_metrics.lock().unwrap();
                    let snapshot = store.clone();
                    store.clear();
                    snapshot
                };
                let metric_event = json!({
                    "ev": "metrics",
                    "meta": {
                        "cells": metrics.cells,
                        "sleeping_pct": metrics.sleeping_pct,
                        "active_pct": metrics.active_pct,
                        "avg_metabolism": metrics.avg_metabolism,
                        "avg_latency": metrics.avg_latency_ms,
                        "live_load": metrics.live_load,
                        "hints": hints.iter().map(hint_label).collect::<Vec<_>>(),
                    }
                })
                .to_string();
                let _ = event_tx_for_metrics.send(metric_event);
                println!(
                    "METRICS cells={} sleeping={:.2} active={:.2} avgMet={:.2} avgLat={:.1} live={:.2} | HINTS: {:?}",
                    metrics.cells,
                    metrics.sleeping_pct,
                    metrics.active_pct,
                    metrics.avg_metabolism,
                    metrics.avg_latency_ms,
                    metrics.live_load,
                    hints
                );
            },
            move |event| {
                let _ = event_tx_for_events.send(event.to_string());
                print_event_line(event);
            },
            move |hint| {
                let mut store = hint_buffer.lock().unwrap();
                store.push(hint.clone());
            },
        )
        .await;
    });

    if let Some(ws_cfg) = ws.clone() {
        let ws_field = field.clone();
        let ws_impulse_tx = tx.clone();
        let ws_events = event_tx.clone();
        tokio::spawn(async move {
            if let Err(err) = run_ws_server(ws_cfg, ws_field, ws_impulse_tx, ws_events).await {
                eprintln!("WebSocket server error: {err}");
            }
        });
    }

    let tx_cli = tx.clone();
    let event_tx_for_cli = event_tx.clone();
    let journal_for_cli = journal.clone();
    let field_for_cli = field.clone();
    let input_task = tokio::spawn(async move {
        let stdin = BufReader::new(tokio::io::stdin());
        let mut lines = stdin.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(rest) = trimmed.strip_prefix(':') {
                match rest {
                    "snapshot" => {
                        if let Some(journal) = &journal_for_cli {
                            match snapshot_once(&field_for_cli, journal).await {
                                Ok(info) => {
                                    if let Err(err) = journal.run_gc(info.offset) {
                                        eprintln!("snapshot GC failed: {err}");
                                    }
                                    log_snapshot("", &info);
                                }
                                Err(err) => eprintln!("snapshot failed: {err}"),
                            }
                        } else {
                            eprintln!("storage not configured; use --store to enable snapshots");
                        }
                    }
                    "stats" => {
                        if let Some(journal) = &journal_for_cli {
                            match journal.stats() {
                                Ok(stats) => print_stats(&stats),
                                Err(err) => eprintln!("stats unavailable: {err}"),
                            }
                        } else {
                            eprintln!("storage not configured; use --store to enable stats");
                        }
                    }
                    command if command.starts_with("export") => {
                        let path_arg = command.trim_start_matches("export").trim();
                        if path_arg.is_empty() {
                            eprintln!("usage: :export <file>");
                        } else {
                            match export_snapshot(&field_for_cli, Path::new(path_arg)).await {
                                Ok(path) => {
                                    println!("EXPORTED snapshot -> {}", path.display());
                                }
                                Err(err) => eprintln!("export failed: {err}"),
                            }
                        }
                    }
                    command if command.starts_with("reflex") => {
                        handle_reflex_command(
                            command.trim_start_matches("reflex").trim(),
                            &field_for_cli,
                        )
                        .await;
                    }
                    command if command.starts_with("trs") => {
                        if let Err(err) = handle_trs_command(
                            command.trim_start_matches("trs").trim(),
                            &field_for_cli,
                        )
                        .await
                        {
                            eprintln!("TRS command failed: {err}");
                        }
                    }
                    other => {
                        eprintln!("Unknown command: :{}", other);
                    }
                }
                continue;
            }
            if trimmed.eq_ignore_ascii_case("lql") {
                eprintln!("usage: lql <SELECT|SUBSCRIBE|UNSUBSCRIBE ...>");
                continue;
            }
            if let Some((prefix, rest)) = trimmed.split_once(' ') {
                if prefix.eq_ignore_ascii_case("lql") {
                    let query = rest.trim();
                    if query.is_empty() {
                        eprintln!("usage: lql <SELECT|SUBSCRIBE|UNSUBSCRIBE ...>");
                    } else {
                        match parse_lql(query) {
                            Ok(ast) => {
                                let outcome = {
                                    let mut guard = field_for_cli.lock().await;
                                    guard.exec_lql(ast)
                                };
                                match outcome {
                                    Ok(result) => {
                                        if let Some(response) = result.response {
                                            print_lql_response(&response);
                                        }
                                        for event in result.events {
                                            let _ = event_tx_for_cli.send(event.clone());
                                            print_event_line(&event);
                                        }
                                    }
                                    Err(err) => eprintln!("LQL execution failed: {}", err),
                                }
                            }
                            Err(err) => eprintln!("LQL parse error: {}", err),
                        }
                    }
                    continue;
                }
            }
            match parse_command(trimmed) {
                Some(impulse) => {
                    if tx_cli.send(impulse).await.is_err() {
                        break;
                    }
                }
                None => {
                    eprintln!("Unknown command: {}", trimmed);
                }
            }
        }
    });

    input_task.await?;
    Ok(())
}

async fn run_ws_server(
    cfg: WsRuntimeConfig,
    field: StdArc<Mutex<ClusterField>>,
    impulse_tx: mpsc::Sender<Impulse>,
    event_tx: broadcast::Sender<String>,
) -> Result<()> {
    let addr = format!("127.0.0.1:{}", cfg.port);
    let listener = TcpListener::bind(&addr)
        .await
        .with_context(|| format!("failed to bind WebSocket listener on {addr}"))?;
    println!("WS server listening on ws://{}", addr);

    loop {
        let (stream, _) = listener.accept().await?;
        let ws_field = field.clone();
        let ws_impulse = impulse_tx.clone();
        let ws_events = event_tx.clone();
        let format = cfg.format;
        tokio::spawn(async move {
            if let Err(err) =
                handle_ws_connection(stream, ws_field, ws_impulse, ws_events, format).await
            {
                eprintln!("ws connection error: {err}");
            }
        });
    }
}

async fn handle_ws_connection(
    stream: tokio::net::TcpStream,
    field: StdArc<Mutex<ClusterField>>,
    impulse_tx: mpsc::Sender<Impulse>,
    event_tx: broadcast::Sender<String>,
    format: WsFormat,
) -> Result<()> {
    let ws_stream = accept_async(stream).await?;
    let (mut sink, mut source) = ws_stream.split();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Message>();

    let send_task = tokio::spawn(async move {
        while let Some(message) = out_rx.recv().await {
            if sink.send(message).await.is_err() {
                break;
            }
        }
    });

    let mut event_rx = event_tx.subscribe();
    let out_for_events = out_tx.clone();
    let events_task = tokio::spawn(async move {
        while let Ok(event) = event_rx.recv().await {
            if out_for_events.send(Message::Text(event)).is_err() {
                break;
            }
        }
    });

    while let Some(msg) = source.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                process_ws_message(text, &field, &impulse_tx, &event_tx, &out_tx, format).await?;
            }
            Ok(Message::Binary(bin)) => {
                if matches!(format, WsFormat::Json) {
                    if let Ok(text) = String::from_utf8(bin) {
                        process_ws_message(text, &field, &impulse_tx, &event_tx, &out_tx, format)
                            .await?;
                    } else {
                        send_ws_error(&out_tx, "binary payload is not valid UTF-8");
                    }
                }
            }
            Ok(Message::Ping(payload)) => {
                out_tx.send(Message::Pong(payload)).ok();
            }
            Ok(Message::Pong(_)) => {}
            Ok(Message::Frame(_)) => {}
            Ok(Message::Close(_)) => break,
            Err(err) => {
                return Err(err.into());
            }
        }
    }

    events_task.abort();
    drop(out_tx);
    send_task.abort();
    Ok(())
}

#[derive(Debug, Deserialize)]
struct WsRequest {
    cmd: String,
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    data: Option<WsImpulsePayload>,
    #[serde(default)]
    ts: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct WsImpulsePayload {
    #[serde(default)]
    k: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    p: Option<String>,
    #[serde(default)]
    pattern: Option<String>,
    #[serde(default)]
    s: Option<f32>,
    #[serde(default)]
    strength: Option<f32>,
    #[serde(default)]
    t: Option<u64>,
    #[serde(default)]
    ttl: Option<u64>,
    #[serde(default)]
    tags: Option<Vec<String>>,
}

impl WsImpulsePayload {
    fn into_impulse(self) -> Result<Impulse> {
        let pattern = self
            .p
            .or(self.pattern)
            .ok_or_else(|| anyhow!("impulse requires pattern"))?;
        let kind_str = self.k.or(self.kind).unwrap_or_else(|| "affect".to_string());
        let kind = parse_impulse_kind(&kind_str)
            .ok_or_else(|| anyhow!("unsupported impulse kind: {kind_str}"))?;
        let strength = self.s.or(self.strength).unwrap_or(0.6).clamp(0.0, 1.0);
        let ttl = self.t.or(self.ttl).unwrap_or(1_500);
        let tags = self.tags.unwrap_or_default();
        Ok(Impulse {
            kind,
            pattern,
            strength,
            ttl_ms: ttl,
            tags,
        })
    }
}

fn parse_impulse_kind(value: &str) -> Option<ImpulseKind> {
    match value.to_lowercase().as_str() {
        "affect" | "a" => Some(ImpulseKind::Affect),
        "query" | "q" => Some(ImpulseKind::Query),
        "write" | "w" => Some(ImpulseKind::Write),
        _ => None,
    }
}

async fn process_ws_message(
    raw: String,
    field: &StdArc<Mutex<ClusterField>>,
    impulse_tx: &mpsc::Sender<Impulse>,
    event_tx: &broadcast::Sender<String>,
    out_tx: &mpsc::UnboundedSender<Message>,
    format: WsFormat,
) -> Result<()> {
    if !matches!(format, WsFormat::Json) {
        return Ok(());
    }

    let request: WsRequest = match serde_json::from_str(&raw) {
        Ok(req) => req,
        Err(err) => {
            send_ws_error(out_tx, &format!("invalid json: {}", err));
            return Ok(());
        }
    };

    match request.cmd.as_str() {
        "lql" => {
            let Some(query) = request.q.clone() else {
                send_ws_error(out_tx, "lql command requires 'q'");
                return Ok(());
            };
            match parse_lql(&query) {
                Ok(ast) => {
                    let result = {
                        let mut guard = field.lock().await;
                        guard.exec_lql(ast)
                    };
                    match result {
                        Ok(exec) => {
                            for event in exec.events {
                                let _ = event_tx.send(event);
                            }
                        }
                        Err(err) => {
                            let response = json!({
                                "ev": "lql",
                                "meta": {
                                    "query": query,
                                    "error": err.to_string(),
                                }
                            })
                            .to_string();
                            out_tx.send(Message::Text(response)).ok();
                        }
                    }
                }
                Err(err) => {
                    let response = json!({
                        "ev": "lql",
                        "meta": {
                            "query": query,
                            "error": err.to_string(),
                        }
                    })
                    .to_string();
                    out_tx.send(Message::Text(response)).ok();
                }
            }
        }
        "impulse" => {
            let Some(payload) = request.data else {
                send_ws_error(out_tx, "impulse command requires 'data'");
                return Ok(());
            };
            match payload.into_impulse() {
                Ok(impulse) => {
                    if impulse_tx.send(impulse).await.is_err() {
                        send_ws_error(out_tx, "impulse queue unavailable");
                    }
                }
                Err(err) => {
                    send_ws_error(out_tx, &err.to_string());
                }
            }
        }
        "ping" => {
            let response = json!({
                "ev": "pong",
                "ts": request.ts,
            })
            .to_string();
            out_tx.send(Message::Text(response)).ok();
        }
        other => {
            send_ws_error(out_tx, &format!("unknown command: {}", other));
        }
    }

    Ok(())
}

fn send_ws_error(out_tx: &mpsc::UnboundedSender<Message>, message: &str) {
    let payload = json!({
        "ev": "error",
        "meta": {
            "message": message,
        }
    })
    .to_string();
    out_tx.send(Message::Text(payload)).ok();
}

async fn run_pipe_cbor(store: Option<StoreRuntimeConfig>) -> Result<()> {
    let config = BridgeConfig {
        tick_ms: 200,
        store_path: store
            .as_ref()
            .map(|cfg| cfg.path.to_string_lossy().to_string()),
        snap_interval: store
            .as_ref()
            .map(|cfg| cfg.snap_interval.as_secs().min(u64::from(u32::MAX)) as u32),
        snap_maxwal: store
            .as_ref()
            .map(|cfg| cfg.max_wal_events.min(u32::MAX as u64) as u32),
    };
    let cfg_bytes = serde_cbor::to_vec(&config)?;
    if !liminal_init(cfg_bytes.as_ptr(), cfg_bytes.len()) {
        return Err(anyhow!("failed to initialize liminal bridge"));
    }

    let mut buffer = vec![0u8; 4096];
    let stdin = BufReader::new(tokio::io::stdin());
    let mut lines = stdin.lines();

    loop {
        tokio::select! {
            line = lines.next_line() => {
                match line {
                    Ok(Some(raw)) => {
                        let trimmed = raw.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        match Vec::<u8>::from_hex(trimmed) {
                            Ok(bytes) => {
                                let result = task::spawn_blocking(move || {
                                    liminal_push(bytes.as_ptr(), bytes.len())
                                })
                                .await
                                .unwrap_or(0);
                                if result == 0 {
                                    eprintln!("bridge rejected impulse");
                                }
                            }
                            Err(err) => {
                                eprintln!("invalid hex input: {err}");
                            }
                        }
                        drain_outputs(&mut buffer)?;
                    }
                    Ok(None) => {
                        drain_outputs(&mut buffer)?;
                        break;
                    }
                    Err(err) => return Err(err.into()),
                }
            }
            _ = tokio::time::sleep(Duration::from_millis(250)) => {
                drain_outputs(&mut buffer)?;
            }
        }
    }

    Ok(())
}

fn drain_outputs(buffer: &mut [u8]) -> Result<()> {
    loop {
        let written = liminal_pull(buffer.as_mut_ptr(), buffer.len());
        if written == 0 {
            break;
        }
        println!("{}", encode_hex(&buffer[..written]));
    }
    Ok(())
}

async fn snapshot_once(
    field: &StdArc<Mutex<ClusterField>>,
    journal: &StdArc<DiskJournal>,
) -> Result<SnapshotInfo> {
    let guard = field.lock().await;
    journal.write_snapshot(&*guard)
}

fn log_snapshot(prefix: &str, info: &SnapshotInfo) {
    println!(
        "{}SNAPSHOT id={} size={}B path={} segment={} position={}",
        prefix,
        info.id,
        info.size_bytes,
        info.path.display(),
        info.offset.segment,
        info.offset.position
    );
}

fn print_stats(stats: &StoreStats) {
    println!(
        "STATS wal_segment={} position={} delta_since_snapshot={} last_snapshot={:?} segments={:?}",
        stats.current_segment,
        stats.current_position,
        stats.delta_since_snapshot,
        stats.last_snapshot,
        stats.wal_segments
    );
}

async fn export_snapshot(field: &StdArc<Mutex<ClusterField>>, path: &Path) -> Result<PathBuf> {
    let snapshot_json = {
        let guard = field.lock().await;
        json!({
            "now_ms": guard.now_ms,
            "next_id": guard.next_id,
            "cells": guard
                .cells
                .values()
                .map(liminal_core::journal::CellSnapshot::from)
                .collect::<Vec<_>>(),
        })
    };
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let pretty = serde_json::to_string_pretty(&snapshot_json)?;
    fs::write(path, pretty)?;
    Ok(path.to_path_buf())
}

async fn handle_reflex_command(command: &str, field: &StdArc<Mutex<ClusterField>>) {
    let mut parts = command.splitn(2, ' ');
    let sub = parts.next().unwrap_or("").trim();
    let rest = parts.next().unwrap_or("").trim();
    match sub {
        "" | "help" => {
            eprintln!("usage: :reflex <add|list|rm> ...");
        }
        "add" => {
            if rest.is_empty() {
                eprintln!("usage: :reflex add <json>");
                return;
            }
            match serde_json::from_str::<ReflexAddRequest>(rest) {
                Ok(spec) => {
                    let ReflexAddRequest {
                        token,
                        kind,
                        min_strength,
                        window_ms,
                        min_count,
                        then,
                        enabled,
                    } = spec;
                    let rule = ReflexRule {
                        id: 0,
                        when: ReflexWhen {
                            token,
                            kind,
                            min_strength,
                            window_ms,
                            min_count,
                        },
                        then,
                        enabled,
                    };
                    let id = {
                        let mut guard = field.lock().await;
                        guard.add_reflex(rule)
                    };
                    println!("REFLEX_ADDED id={}", id);
                }
                Err(err) => eprintln!("invalid reflex spec: {err}"),
            }
        }
        "list" => {
            let rules = {
                let guard = field.lock().await;
                guard.list_reflex()
            };
            match serde_json::to_string_pretty(&rules) {
                Ok(json) => println!("{}", json),
                Err(err) => eprintln!("failed to render rules: {err}"),
            }
        }
        "rm" => {
            if rest.is_empty() {
                eprintln!("usage: :reflex rm <id>");
                return;
            }
            match rest.parse::<u64>() {
                Ok(id) => {
                    let removed = {
                        let mut guard = field.lock().await;
                        guard.remove_reflex(id)
                    };
                    if removed {
                        println!("REFLEX_REMOVED id={}", id);
                    } else {
                        eprintln!("no reflex with id={} found", id);
                    }
                }
                Err(err) => eprintln!("invalid reflex id: {err}"),
            }
        }
        other => {
            eprintln!("Unknown reflex command: {}", other);
        }
    }
}

async fn handle_trs_command(command: &str, field: &StdArc<Mutex<ClusterField>>) -> Result<()> {
    let mut parts = command.splitn(2, ' ');
    let sub = parts.next().unwrap_or("").trim();
    match sub {
        "" | "show" => {
            let (config, err_i, last_err, last_ts, harmony) = {
                let guard = field.lock().await;
                let state = guard.trs.clone();
                (
                    state.to_config(),
                    state.err_i,
                    state.last_err,
                    state.last_ts,
                    guard.harmony_state(),
                )
            };
            let payload = json!({
                "config": {
                    "alpha": config.alpha,
                    "beta": config.beta,
                    "k_p": config.k_p,
                    "k_i": config.k_i,
                    "k_d": config.k_d,
                    "target": config.target_load,
                },
                "err_i": err_i,
                "last_err": last_err,
                "last_ts": last_ts,
                "harmony": {
                    "alpha": harmony.alpha,
                    "affinity_scale": harmony.affinity_scale,
                    "metabolism_scale": harmony.metabolism_scale,
                    "sleep_delta": harmony.sleep_delta,
                }
            });
            println!("{}", serde_json::to_string_pretty(&payload)?);
            Ok(())
        }
        "set" => {
            let raw = parts.next().unwrap_or("").trim();
            if raw.is_empty() {
                return Err(anyhow!(
                    "usage: :trs set {{\"alpha\":...,\"beta\":...,\"k_p\":...,\"k_i\":...,\"k_d\":...,\"target_load\":...}}"
                ));
            }
            let cfg: TrsConfig = serde_json::from_str(raw)?;
            let event = {
                let mut guard = field.lock().await;
                guard.set_trs_config(cfg)
            };
            println!("{}", event);
            Ok(())
        }
        "target" => {
            let value_raw = parts.next().unwrap_or("").trim();
            if value_raw.is_empty() {
                return Err(anyhow!("usage: :trs target <0.3..0.8>"));
            }
            let value: f32 = value_raw
                .parse()
                .map_err(|_| anyhow!("invalid target value: {}", value_raw))?;
            if !(0.3..=0.8).contains(&value) {
                return Err(anyhow!("target must be within 0.3..0.8"));
            }
            let event = {
                let mut guard = field.lock().await;
                guard.set_trs_target(value)
            };
            println!("{}", event);
            Ok(())
        }
        other => Err(anyhow!("unknown trs subcommand: {}", other)),
    }
}

#[derive(Debug, Deserialize)]
struct ReflexAddRequest {
    token: String,
    kind: ImpulseKind,
    min_strength: f32,
    window_ms: u32,
    min_count: u16,
    then: ReflexAction,
    #[serde(default = "default_enabled")]
    enabled: bool,
}

fn default_enabled() -> bool {
    true
}

fn hint_label(hint: &Hint) -> &'static str {
    match hint {
        Hint::SlowTick => "slow_tick",
        Hint::FastTick => "fast_tick",
        Hint::TrimField => "trim_field",
        Hint::WakeSeeds => "wake_seeds",
    }
}

fn convert_reflex_log(log: &str) -> Option<String> {
    if let Some(rest) = log.strip_prefix("REFLEX_FIRE ") {
        let mut id = None;
        let mut matched = None;
        for part in rest.split_whitespace() {
            if let Some(value) = part.strip_prefix("id=") {
                id = value.parse::<u64>().ok();
            } else if let Some(value) = part.strip_prefix("matched=") {
                matched = value.parse::<u16>().ok();
            }
        }
        if let (Some(id), Some(matched)) = (id, matched) {
            return Some(
                json!({
                    "ev": "reflex_fire",
                    "meta": {"id": id, "matched": matched},
                })
                .to_string(),
            );
        }
    }
    if let Some(rest) = log.strip_prefix("REFLEX_HINT ") {
        let mut id = None;
        let mut hint = None;
        for part in rest.split_whitespace() {
            if let Some(value) = part.strip_prefix("id=") {
                id = value.parse::<u64>().ok();
            } else if let Some(value) = part.strip_prefix("hint=") {
                hint = Some(value.trim_matches(|c| c == '{' || c == '}'));
            }
        }
        if let (Some(id), Some(hint_value)) = (id, hint) {
            return Some(
                json!({
                    "ev": "reflex_hint",
                    "meta": {"id": id, "hint": hint_value},
                })
                .to_string(),
            );
        }
    }
    if let Some(rest) = log.strip_prefix("REFLEX_SPAWN ") {
        let mut id = None;
        let mut node = None;
        let mut seed = None;
        let mut shift = None;
        for part in rest.split_whitespace() {
            if let Some(value) = part.strip_prefix("id=") {
                id = value.parse::<u64>().ok();
            } else if let Some(value) = part.strip_prefix("node=n") {
                node = value.parse::<u64>().ok();
            } else if let Some(value) = part.strip_prefix("seed=") {
                seed = Some(value.to_string());
            } else if let Some(value) = part.strip_prefix("shift=") {
                shift = value.parse::<f32>().ok();
            }
        }
        if let (Some(id), Some(node), Some(seed), Some(shift)) = (id, node, seed, shift) {
            return Some(
                json!({
                    "ev": "reflex_spawn",
                    "meta": {"id": id, "node": node, "seed": seed, "shift": shift},
                })
                .to_string(),
            );
        }
    }
    if let Some(rest) = log.strip_prefix("REFLEX_WAKE ") {
        let mut id = None;
        let mut requested = None;
        let mut woke = None;
        for part in rest.split_whitespace() {
            if let Some(value) = part.strip_prefix("id=") {
                id = value.parse::<u64>().ok();
            } else if let Some(value) = part.strip_prefix("requested=") {
                requested = value.parse::<u32>().ok();
            } else if let Some(value) = part.strip_prefix("woke=") {
                woke = value.parse::<u32>().ok();
            }
        }
        if let (Some(id), Some(requested), Some(woke)) = (id, requested, woke) {
            return Some(
                json!({
                    "ev": "reflex_wake",
                    "meta": {"id": id, "requested": requested, "woke": woke},
                })
                .to_string(),
            );
        }
    }
    if let Some(rest) = log.strip_prefix("REFLEX_BOOST ") {
        let mut id = None;
        let mut top = None;
        let mut factor = None;
        let mut boosted = None;
        for part in rest.split_whitespace() {
            if let Some(value) = part.strip_prefix("id=") {
                id = value.parse::<u64>().ok();
            } else if let Some(value) = part.strip_prefix("top=") {
                top = value.parse::<u32>().ok();
            } else if let Some(value) = part.strip_prefix("factor=") {
                factor = value.parse::<f32>().ok();
            } else if let Some(value) = part.strip_prefix("boosted=") {
                boosted = value.parse::<u32>().ok();
            }
        }
        if let (Some(id), Some(top), Some(factor), Some(boosted)) = (id, top, factor, boosted) {
            return Some(
                json!({
                    "ev": "reflex_boost",
                    "meta": {"id": id, "top": top, "factor": factor, "boosted": boosted},
                })
                .to_string(),
            );
        }
    }
    None
}

fn print_event_line(raw: &str) {
    if let Ok(value) = serde_json::from_str::<JsonValue>(raw) {
        if let Some(event) = value.get("ev").and_then(|ev| ev.as_str()) {
            if let Some(line) = match event {
                "view" => format_view_event(&value),
                "lql" => format_lql_event(&value),
                _ => None,
            } {
                println!("{}", line);
                return;
            }
        }
    }
    println!("{}", raw);
}

fn format_view_event(value: &JsonValue) -> Option<String> {
    let meta = value.get("meta")?;
    let id = meta.get("id")?.as_u64()?;
    let pattern = meta.get("pattern")?.as_str().unwrap_or("");
    let window = meta.get("window")?.as_u64().unwrap_or_default();
    let stats = meta.get("stats")?;
    let (count, avg_strength, avg_latency, top_nodes) = extract_stats_from_json(stats)?;
    Some(format!(
        "VIEW id={} pattern={} window={}ms count={} avg_str={:.2} avg_lat={:.1} top=[{}]",
        id, pattern, window, count, avg_strength, avg_latency, top_nodes
    ))
}

fn format_lql_event(value: &JsonValue) -> Option<String> {
    let meta = value.get("meta")?;
    if let Some(select) = meta.get("select") {
        let pattern = select.get("pattern")?.as_str().unwrap_or("");
        let window = select.get("window_ms")?.as_u64().unwrap_or_default();
        let min_strength = select
            .get("min_strength")
            .and_then(|v| v.as_f64())
            .map(|v| format!("{v:.2}"))
            .unwrap_or_else(|| "-".into());
        let stats = select.get("stats")?;
        let (count, avg_strength, avg_latency, top_nodes) = extract_stats_from_json(stats)?;
        return Some(format!(
            "LQL SELECT pattern={} window={}ms min>={} count={} avg_str={:.2} avg_lat={:.1} top=[{}]",
            pattern, window, min_strength, count, avg_strength, avg_latency, top_nodes
        ));
    }
    if let Some(subscribe) = meta.get("subscribe") {
        let id = subscribe.get("id")?.as_u64()?;
        let pattern = subscribe.get("pattern")?.as_str().unwrap_or("");
        let window = subscribe.get("window_ms")?.as_u64().unwrap_or_default();
        let every = subscribe.get("every_ms")?.as_u64().unwrap_or_default();
        return Some(format!(
            "LQL SUBSCRIBE id={} pattern={} window={}ms every={}ms",
            id, pattern, window, every
        ));
    }
    if let Some(unsubscribe) = meta.get("unsubscribe") {
        let id = unsubscribe.get("id")?.as_u64()?;
        let removed = unsubscribe.get("removed")?.as_bool().unwrap_or(false);
        return Some(format!("LQL UNSUBSCRIBE id={} removed={}", id, removed));
    }
    if let Some(err) = meta.get("error") {
        let query = meta.get("query").and_then(|q| q.as_str()).unwrap_or("");
        return Some(format!(
            "LQL ERROR query='{}' message={}",
            query,
            err.as_str().unwrap_or("unknown")
        ));
    }
    None
}

fn extract_stats_from_json(stats: &JsonValue) -> Option<(u32, f64, f64, String)> {
    let count = stats.get("count")?.as_u64()? as u32;
    let avg_strength = stats.get("avg_strength")?.as_f64()?;
    let avg_latency = stats.get("avg_latency")?.as_f64()?;
    let top_nodes = stats
        .get("top_nodes")
        .and_then(|array| array.as_array())
        .map(|items| format_top_nodes_json(items))
        .unwrap_or_else(|| "-".into());
    Some((count, avg_strength, avg_latency, top_nodes))
}

fn format_top_nodes_json(items: &[JsonValue]) -> String {
    if items.is_empty() {
        return "-".into();
    }
    let parts: Vec<String> = items
        .iter()
        .filter_map(|item| {
            let id = item.get("id")?.as_u64()?;
            let hits = item.get("hits")?.as_u64()?;
            Some(format!("n{}:{}", id, hits))
        })
        .collect();
    if parts.is_empty() {
        "-".into()
    } else {
        parts.join(",")
    }
}

fn print_lql_response(response: &LqlResponse) {
    match response {
        LqlResponse::Select(result) => {
            let (count, avg_strength, avg_latency, top) = format_stats_output(&result.stats);
            let threshold = result
                .min_strength
                .map(|v| format!("{v:.2}"))
                .unwrap_or_else(|| "-".into());
            println!(
                "LQL SELECT pattern={} window={}ms min>={} count={} avg_str={:.2} avg_lat={:.1} top=[{}]",
                result.pattern, result.window_ms, threshold, count, avg_strength, avg_latency, top
            );
        }
        LqlResponse::Subscribe(result) => {
            println!(
                "LQL SUBSCRIBE id={} pattern={} window={}ms every={}ms",
                result.id, result.pattern, result.window_ms, result.every_ms
            );
        }
        LqlResponse::Unsubscribe(result) => {
            println!(
                "LQL UNSUBSCRIBE id={} removed={}",
                result.id, result.removed
            );
        }
    }
}

fn format_stats_output(stats: &ViewStats) -> (u32, f64, f64, String) {
    let count = stats.count;
    let avg_strength = stats.avg_strength as f64;
    let avg_latency = stats.avg_latency as f64;
    let top = if stats.top_nodes.is_empty() {
        "-".into()
    } else {
        stats
            .top_nodes
            .iter()
            .map(|node| format!("n{}:{}", node.id, node.hits))
            .collect::<Vec<_>>()
            .join(",")
    };
    (count, avg_strength, avg_latency, top)
}

fn parse_command(line: &str) -> Option<Impulse> {
    let mut parts = line.split_whitespace();
    let cmd = parts.next()?;
    let pattern = parts.next()?;
    let strength = parts
        .next()
        .and_then(|s| s.parse::<f32>().ok())
        .unwrap_or(0.6);

    let kind = match cmd.to_lowercase().as_str() {
        "q" => ImpulseKind::Query,
        "w" => ImpulseKind::Write,
        "a" => ImpulseKind::Affect,
        _ => return None,
    };

    Some(Impulse {
        kind,
        pattern: pattern.to_string(),
        strength: strength.clamp(0.0, 1.0),
        ttl_ms: 1_500,
        tags: vec!["cli".into()],
    })
}
