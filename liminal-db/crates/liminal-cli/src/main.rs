use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc as StdArc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use hex::encode as encode_hex;
use hex::FromHex;
use liminal_bridge_abi::ffi::{liminal_init, liminal_pull, liminal_push};
use liminal_bridge_abi::protocol::BridgeConfig;
use liminal_core::life_loop::run_loop;
use liminal_core::types::{Hint, Impulse, ImpulseKind};
use liminal_core::{ClusterField, ReflexAction, ReflexRule, ReflexWhen};
use liminal_sensor::start_host_sensors;
use liminal_store::{decode_delta, DiskJournal, Offset, SnapshotInfo, StoreStats};
use serde::Deserialize;
use serde_json::json;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{mpsc, Mutex};
use tokio::task;

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut pipe_cbor = false;
    let mut store_path: Option<PathBuf> = None;
    let mut snap_interval_secs: u64 = 60;
    let mut snap_maxwal: u64 = 5_000;
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
            other => return Err(anyhow!("unknown argument: {other}")),
        }
    }

    let store_config = store_path.map(|path| StoreRuntimeConfig {
        path,
        snap_interval: Duration::from_secs(snap_interval_secs),
        max_wal_events: snap_maxwal,
    });

    if pipe_cbor {
        run_pipe_cbor(store_config).await
    } else {
        run_interactive(store_config).await
    }
}

#[derive(Clone)]
struct StoreRuntimeConfig {
    path: PathBuf,
    snap_interval: Duration,
    max_wal_events: u64,
}

async fn run_interactive(store: Option<StoreRuntimeConfig>) -> Result<()> {
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
                    "METRICS cells={} sleeping={:.2} avgMet={:.2} avgLat={:.1} | HINTS: {:?}",
                    metrics.cells,
                    metrics.sleeping_pct,
                    metrics.avg_metabolism,
                    metrics.avg_latency_ms,
                    hints
                );
            },
            move |event| {
                println!("{}", event);
            },
            move |hint| {
                let mut store = hint_buffer.lock().unwrap();
                store.push(hint.clone());
            },
        )
        .await;
    });

    let tx_cli = tx.clone();
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
                    other => {
                        eprintln!("Unknown command: :{}", other);
                    }
                }
                continue;
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
