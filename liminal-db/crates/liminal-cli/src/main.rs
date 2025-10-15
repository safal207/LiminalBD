use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc as StdArc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use hex::encode as encode_hex;
use hex::FromHex;
use liminal_bridge_abi::ffi::{liminal_init, liminal_pull, liminal_push};
use liminal_bridge_abi::protocol::BridgeConfig;
use liminal_bridge_net::stream_codec::StreamFormat;
use liminal_bridge_net::ws_client::{connect_ws, WsHandle};
use liminal_bridge_net::{
    format_clients as ws_format_clients, list_clients as ws_list_clients,
    publish_event as ws_publish_event, publish_metrics as ws_publish_metrics,
    set_default_format as ws_set_default_format, take_command_receiver as ws_take_command_receiver,
    ws_server, IncomingCommand,
};
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
use tokio::sync::{mpsc, Mutex};
use tokio::task;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    let mut args = std::env::args().skip(1);
    let mut pipe_cbor = false;
    let mut store_path: Option<PathBuf> = None;
    let mut snap_interval_secs: u64 = 60;
    let mut snap_maxwal: u64 = 5_000;
    let mut ws_port: u16 = 8787;
    let mut ws_format = StreamFormat::Json;
    let mut nexus_client: Option<String> = None;
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
                    return Err(anyhow!("--ws-port requires a port"));
                };
                ws_port = value
                    .parse::<u16>()
                    .map_err(|_| anyhow!("--ws-port expects a valid port, got {value}"))?;
            }
            "--ws-format" => {
                let Some(value) = args.next() else {
                    return Err(anyhow!("--ws-format requires a value"));
                };
                ws_format = match value.to_lowercase().as_str() {
                    "json" => StreamFormat::Json,
                    "cbor" => StreamFormat::Cbor,
                    other => return Err(anyhow!("unsupported ws format: {other}")),
                };
            }
            "--nexus-client" => {
                let Some(url) = args.next() else {
                    return Err(anyhow!("--nexus-client requires a url"));
                };
                nexus_client = Some(url);
            }
            other => return Err(anyhow!("unknown argument: {other}")),
        }
    }

    let store_config = store_path.map(|path| StoreRuntimeConfig {
        path,
        snap_interval: Duration::from_secs(snap_interval_secs),
        max_wal_events: snap_maxwal,
    });

    let ws_runtime = WsRuntimeConfig {
        port: ws_port,
        format: ws_format,
        nexus_client,
    };

    if pipe_cbor {
        run_pipe_cbor(store_config).await
    } else {
        run_interactive(store_config, ws_runtime).await
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
    format: StreamFormat,
    nexus_client: Option<String>,
}

async fn run_interactive(store: Option<StoreRuntimeConfig>, ws: WsRuntimeConfig) -> Result<()> {
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

    tokio::spawn(start_host_sensors(tx.clone()));

    ws_set_default_format(ws.format);

    if ws.nexus_client.is_none() {
        let listen_addr = format!("127.0.0.1:{}", ws.port);
        let field_for_ws = field.clone();
        let log_addr = listen_addr.clone();
        tokio::spawn(async move {
            if let Err(err) = ws_server::start_ws_server(field_for_ws, &listen_addr).await {
                eprintln!("websocket server error: {err}");
            }
        });
        info!(addr = %log_addr, "ws.local_listening");
    }

    let remote_ws: StdArc<Mutex<Option<WsHandle>>> = StdArc::new(Mutex::new(None));
    if let Some(url) = ws.nexus_client.clone() {
        let holder = remote_ws.clone();
        tokio::spawn(async move {
            match connect_ws(&url).await {
                Ok(handle) => {
                    info!(url = %url, "ws.remote_connected");
                    {
                        let mut guard = holder.lock().await;
                        *guard = Some(handle.clone());
                    }
                    tokio::spawn(async move {
                        loop {
                            match handle.recv().await {
                                Some(value) => ws_publish_event(value),
                                None => break,
                            }
                        }
                    });
                }
                Err(err) => {
                    eprintln!("failed to connect to nexus client {url}: {err}");
                }
            }
        });
    }

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
    tokio::spawn(async move {
        while let Some(impulse) = rx.recv().await {
            let logs = {
                let mut guard = field_for_impulses.lock().await;
                guard.route_impulse(impulse)
            };
            for log in logs {
                println!("IMPULSE {}", log);
            }
        }
    });

    let hint_buffer: StdArc<StdMutex<Vec<Hint>>> = StdArc::new(StdMutex::new(Vec::new()));

    let loop_field = field.clone();
    let hints_for_metrics = hint_buffer.clone();
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
                if let Ok(metrics_json) = serde_json::to_value(metrics) {
                    ws_publish_metrics(json!({"ev": "metrics", "metrics": metrics_json}));
                }
            },
            move |event| {
                print_event_line(event);
                if let Ok(value) = serde_json::from_str::<JsonValue>(event) {
                    ws_publish_event(value);
                }
            },
            move |hint| {
                let mut store = hint_buffer.lock().unwrap();
                store.push(hint.clone());
            },
        )
        .await;
    });

    if let Some(mut cmd_rx) = ws_take_command_receiver() {
        let field_for_cmds = field.clone();
        let tx_for_cmds = tx.clone();
        tokio::spawn(async move {
            while let Some(cmd) = cmd_rx.recv().await {
                if let Err(err) =
                    handle_network_command(cmd.clone(), &field_for_cmds, &tx_for_cmds).await
                {
                    eprintln!("ws command failed: {err}");
                }
            }
        });
    }

    let tx_cli = tx.clone();
    let journal_for_cli = journal.clone();
    let field_for_cli = field.clone();
    let remote_for_cli = remote_ws.clone();
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
                    command if command.starts_with("ws") => {
                        if let Err(err) = handle_ws_command(
                            command.trim_start_matches("ws").trim(),
                            &remote_for_cli,
                        )
                        .await
                        {
                            eprintln!("ws command failed: {err}");
                        }
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
        ws_port: None,
        ws_format: None,
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

async fn handle_network_command(
    command: IncomingCommand,
    field: &StdArc<Mutex<ClusterField>>,
    impulse_tx: &mpsc::Sender<Impulse>,
) -> Result<()> {
    match command {
        IncomingCommand::Impulse { data } => {
            let impulse = parse_impulse_json(&data)?;
            impulse_tx
                .send(impulse)
                .await
                .map_err(|_| anyhow!("impulse channel closed"))?;
        }
        IncomingCommand::Lql { query } => {
            let ast = parse_lql(&query)?;
            let outcome = {
                let mut guard = field.lock().await;
                guard.exec_lql(ast)
            }?;
            if let Some(response) = outcome.response {
                print_lql_response(&response);
            }
            for event in outcome.events {
                print_event_line(&event);
                if let Ok(value) = serde_json::from_str::<JsonValue>(&event) {
                    ws_publish_event(value);
                }
            }
        }
        IncomingCommand::PolicySet { data } => {
            println!("POLICY_SET {:?}", data);
        }
        IncomingCommand::Subscribe { .. } => {}
        IncomingCommand::Raw(value) => {
            println!("WS RAW {}", value);
        }
    }
    Ok(())
}

async fn handle_ws_command(command: &str, remote: &StdArc<Mutex<Option<WsHandle>>>) -> Result<()> {
    let mut parts = command.splitn(2, ' ');
    let sub = parts.next().unwrap_or("").trim();
    match sub {
        "" | "help" => {
            println!("WS commands: info | send <json> | broadcast <json>");
        }
        "info" => {
            let clients = ws_list_clients();
            if clients.is_empty() {
                println!("WS no clients connected");
            } else {
                for line in ws_format_clients(&clients) {
                    println!("WS {}", line);
                }
            }
            if remote.lock().await.is_some() {
                println!("WS nexus-client connected");
            }
        }
        "send" => {
            let payload = parts.next().unwrap_or("").trim();
            if payload.is_empty() {
                return Err(anyhow!("usage: :ws send <json>"));
            }
            let value: JsonValue = serde_json::from_str(payload)?;
            let handle = {
                let guard = remote.lock().await;
                guard.clone()
            };
            let Some(client) = handle else {
                return Err(anyhow!("no nexus client connection"));
            };
            client.send(value).await?;
            println!("WS send queued");
        }
        "broadcast" => {
            let payload = parts.next().unwrap_or("").trim();
            if payload.is_empty() {
                return Err(anyhow!("usage: :ws broadcast <json>"));
            }
            let value: JsonValue = serde_json::from_str(payload)?;
            ws_publish_event(value);
            println!("WS broadcast queued");
        }
        other => {
            return Err(anyhow!("unknown ws subcommand: {}", other));
        }
    }
    Ok(())
}

fn parse_impulse_json(data: &JsonValue) -> Result<Impulse> {
    let pattern = data
        .get("pattern")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("impulse requires pattern"))?;
    let strength = data.get("strength").and_then(|v| v.as_f64()).unwrap_or(0.6) as f32;
    let ttl_ms = data.get("ttl_ms").and_then(|v| v.as_u64()).unwrap_or(1_500);
    let kind = data.get("kind").and_then(|v| v.as_str()).unwrap_or("query");
    let impulse_kind = match kind.to_lowercase().as_str() {
        "affect" | "a" => ImpulseKind::Affect,
        "write" | "w" => ImpulseKind::Write,
        _ => ImpulseKind::Query,
    };
    let tags = data
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| vec!["ws".into()]);
    Ok(Impulse {
        kind: impulse_kind,
        pattern: pattern.to_string(),
        strength,
        ttl_ms,
        tags,
    })
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
