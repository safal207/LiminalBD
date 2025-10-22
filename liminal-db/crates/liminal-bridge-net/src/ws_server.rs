use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use liminal_core::ClusterField;
use once_cell::sync::Lazy;
use serde_json::Value as JsonValue;
use tokio::net::TcpListener;
use tokio::sync::{mpsc, Mutex};
use tokio::time;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{error, info, warn};

use crate::stream_codec::{decode_cbor_json, encode_cbor_json};
use crate::{
    command_sender, global_hub, register_client_snapshot, remove_client_snapshot,
    update_client_format, update_client_ping, ClientSnapshot, IncomingCommand, IntrospectTarget,
    StreamEvent, StreamMetrics,
};

static NEXT_CLIENT_ID: Lazy<std::sync::atomic::AtomicU64> =
    Lazy::new(|| std::sync::atomic::AtomicU64::new(1));

pub async fn start_ws_server(field: Arc<Mutex<ClusterField>>, addr: &str) -> Result<()> {
    let _ = field; // placeholder to retain ownership for lifetime
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("failed to bind websocket server to {addr}"))?;
    info!(addr = %addr, "ws_server.listening");

    let latest_harmony = Arc::new(Mutex::new(None::<JsonValue>));

    loop {
        let (stream, peer) = listener
            .accept()
            .await
            .with_context(|| "failed to accept websocket connection")?;
        let peer_addr = peer.to_string();
        let hub = global_hub();
        let default_format = hub.default_format();
        let client_id = NEXT_CLIENT_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let harmony_cache = latest_harmony.clone();

        tokio::spawn(async move {
            let ws_stream = match accept_async(stream).await {
                Ok(stream) => stream,
                Err(err) => {
                    error!(peer = %peer_addr, error = %err, "ws_server.accept_failed");
                    return;
                }
            };
            let (mut sink, mut source) = ws_stream.split();
            let format_state = Arc::new(Mutex::new(default_format));

            let snapshot = ClientSnapshot {
                id: client_id,
                last_ping: Instant::now(),
                connected_at: Instant::now(),
                addr: peer_addr.clone(),
                format: default_format,
            };
            register_client_snapshot(snapshot);

            let (out_tx, mut out_rx) = mpsc::channel::<Message>(256);
            let mut event_rx = crate::subscribe_events();
            let mut metrics_rx = crate::subscribe_metrics();
            let cmd_tx = command_sender();

            if let Some(payload) = { harmony_cache.lock().await.clone() } {
                let fmt = {
                    let guard = format_state.lock().await;
                    *guard
                };
                if let Ok(msg) = encode_cbor_json(&payload, fmt) {
                    let _ = out_tx.send(msg).await;
                }
            }

            let out_events_tx = out_tx.clone();
            let format_for_events = format_state.clone();
            let latest_for_events = harmony_cache.clone();
            let pump_events = async move {
                loop {
                    match event_rx.recv().await {
                        Ok(StreamEvent { payload }) => {
                            let is_harmony = payload
                                .get("ev")
                                .and_then(|ev| ev.as_str())
                                .map(|ev| ev == "harmony")
                                .unwrap_or(false);
                            if is_harmony {
                                *latest_for_events.lock().await = Some(payload.clone());
                            }
                            let fmt = *format_for_events.lock().await;
                            if let Ok(msg) = encode_cbor_json(&payload, fmt) {
                                if out_events_tx.send(msg).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            };

            let out_metrics_tx = out_tx.clone();
            let format_for_metrics = format_state.clone();
            let pump_metrics = async move {
                loop {
                    match metrics_rx.recv().await {
                        Ok(StreamMetrics { payload }) => {
                            let fmt = *format_for_metrics.lock().await;
                            if let Ok(msg) = encode_cbor_json(&payload, fmt) {
                                if out_metrics_tx.send(msg).await.is_err() {
                                    break;
                                }
                            }
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    }
                }
            };

            let mut ping_interval = time::interval(Duration::from_secs(30));
            let out_ping_tx = out_tx.clone();

            let send_loop = async move {
                while let Some(msg) = out_rx.recv().await {
                    if sink.send(msg).await.is_err() {
                        break;
                    }
                }
            };

            let mut send_task = tokio::spawn(send_loop);
            let mut events_task = tokio::spawn(pump_events);
            let mut metrics_task = tokio::spawn(pump_metrics);

            loop {
                tokio::select! {
                    biased;
                    _ = ping_interval.tick() => {
                        update_client_ping(client_id);
                        if out_ping_tx.send(Message::Ping(Vec::new())).await.is_err() {
                            break;
                        }
                    }
                    message = source.next() => {
                        match message {
                            Some(Ok(msg)) => {
                                match decode_cbor_json(msg) {
                                    Ok((value, detected_format)) => {
                                        {
                                            let mut guard = format_state.lock().await;
                                            if detected_format != *guard {
                                                *guard = detected_format;
                                                update_client_format(client_id, detected_format);
                                            }
                                        }
                                        if let Some(cmd) = value.get("cmd").and_then(|v| v.as_str()) {
                                            match cmd {
                                                "impulse" => {
                                                    let data = value.get("data").cloned().unwrap_or(JsonValue::Null);
                                                    let _ = cmd_tx.send(IncomingCommand::Impulse { data }).await;
                                                }
                                                "lql" => {
                                                    if let Some(query) = value.get("q").and_then(|v| v.as_str()) {
                                                        let _ = cmd_tx.send(IncomingCommand::Lql { query: query.to_string() }).await;
                                                    }
                                                }
                                                "policy.set" => {
                                                    let data = value.get("data").cloned().unwrap_or(JsonValue::Null);
                                                    let _ = cmd_tx.send(IncomingCommand::PolicySet { data }).await;
                                                }
                                                "subscribe" => {
                                            if let Some(pattern) = value.get("pattern").and_then(|v| v.as_str()) {
                                                let query = format!("SUBSCRIBE {}", pattern);
                                                let _ = cmd_tx.send(IncomingCommand::Lql { query }).await;
                                            }
                                        }
                                        "dream.now" => {
                                            let _ = cmd_tx.send(IncomingCommand::DreamNow).await;
                                        }
                                        "dream.set" => {
                                            let cfg = value.get("cfg").cloned().unwrap_or(JsonValue::Null);
                                            let _ = cmd_tx
                                                .send(IncomingCommand::DreamSet { cfg })
                                                .await;
                                        }
                                        "dream.get" => {
                                            let _ = cmd_tx.send(IncomingCommand::DreamGet).await;
                                        }
                                        "sync.now" => {
                                            let _ = cmd_tx.send(IncomingCommand::SyncNow).await;
                                        }
                                        "sync.set" => {
                                            let cfg = value.get("cfg").cloned().unwrap_or(JsonValue::Null);
                                            let _ = cmd_tx
                                                .send(IncomingCommand::SyncSet { cfg })
                                                .await;
                                        }
                                        "sync.get" => {
                                            let _ = cmd_tx.send(IncomingCommand::SyncGet).await;
                                        }
                                        "awaken.set" => {
                                            let cfg = value.get("cfg").cloned().unwrap_or(JsonValue::Null);
                                            let _ = cmd_tx
                                                .send(IncomingCommand::AwakenSet { cfg })
                                                .await;
                                        }
                                        "awaken.get" => {
                                            let _ = cmd_tx.send(IncomingCommand::AwakenGet).await;
                                        }
                                        "awaken.now" => {
                                            let _ = cmd_tx
                                                .send(IncomingCommand::Introspect {
                                                    target: IntrospectTarget::Awaken,
                                                    top: None,
                                                })
                                                .await;
                                        }
                                        "introspect" => {
                                            let target = value
                                                .get("target")
                                                .and_then(|v| v.as_str())
                                                .and_then(parse_introspect_target);
                                            if let Some(target) = target {
                                                let top = value
                                                    .get("top")
                                                    .and_then(|v| v.as_u64())
                                                    .map(|v| v.min(u32::MAX as u64) as u32);
                                                let _ = cmd_tx
                                                    .send(IncomingCommand::Introspect { target, top })
                                                    .await;
                                            } else {
                                                warn!("ws_server.invalid_introspect_target");
                                            }
                                        }
                                        other => {
                                            warn!(command = %other, "ws_server.unknown_cmd");
                                            let _ = cmd_tx.send(IncomingCommand::Raw(value.clone())).await;
                                        }
                                    }
                                        } else {
                                            let _ = cmd_tx.send(IncomingCommand::Raw(value.clone())).await;
                                        }
                                    }
                                    Err(err) => {
                                        warn!(error = %err, "ws_server.decode_failed");
                                    }
                                }
                            }
                            Some(Err(err)) => {
                                warn!(error = %err, "ws_server.recv_error");
                                break;
                            }
                            None => break,
                        }
                    }
                    _ = &mut send_task => break,
                    _ = &mut events_task => break,
                    _ = &mut metrics_task => break,
                }
            }

            let _ = send_task.await;
            let _ = events_task.await;
            let _ = metrics_task.await;
            remove_client_snapshot(client_id);
        });
    }
}

fn parse_introspect_target(label: &str) -> Option<IntrospectTarget> {
    match label.to_ascii_lowercase().as_str() {
        "awaken" => Some(IntrospectTarget::Awaken),
        "model" => Some(IntrospectTarget::Model),
        "influence" => Some(IntrospectTarget::Influence),
        "tension" => Some(IntrospectTarget::Tension),
        _ => None,
    }
}
