use std::sync::{Arc as StdArc, Mutex as StdMutex};
use std::time::Duration;

use anyhow::{anyhow, Result};
use hex::encode as encode_hex;
use hex::FromHex;
use liminal_bridge_abi::ffi::{liminal_init, liminal_pull, liminal_push};
use liminal_bridge_abi::protocol::BridgeConfig;
use liminal_core::life_loop::run_loop;
use liminal_core::types::{Hint, Impulse, ImpulseKind};
use liminal_core::ClusterField;
use liminal_sensor::start_host_sensors;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{mpsc, Mutex};
use tokio::task;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--pipe-cbor") {
        run_pipe_cbor().await
    } else {
        run_interactive().await
    }
}

async fn run_interactive() -> Result<()> {
    let mut field = ClusterField::new();
    field.add_root("liminal/root");
    let field = StdArc::new(Mutex::new(field));

    let (tx, mut rx) = mpsc::channel::<Impulse>(128);

    tokio::spawn(start_host_sensors(tx.clone()));

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
    let input_task = tokio::spawn(async move {
        let stdin = BufReader::new(tokio::io::stdin());
        let mut lines = stdin.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let trimmed = line.trim();
            if trimmed.is_empty() {
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

async fn run_pipe_cbor() -> Result<()> {
    let config = BridgeConfig { tick_ms: 200 };
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
