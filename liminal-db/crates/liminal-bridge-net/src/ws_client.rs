use anyhow::{anyhow, Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::Value as JsonValue;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::protocol::Message;

use crate::stream_codec::{decode_cbor_json, encode_cbor_json, StreamFormat};

#[derive(Clone)]
pub struct WsHandle {
    sender: mpsc::Sender<JsonValue>,
    receiver: Arc<Mutex<mpsc::Receiver<JsonValue>>>,
}

impl WsHandle {
    pub async fn send(&self, value: JsonValue) -> Result<()> {
        self.sender
            .send(value)
            .await
            .map_err(|_| anyhow!("ws client disconnected"))
    }

    pub async fn recv(&self) -> Option<JsonValue> {
        self.receiver.lock().await.recv().await
    }
}

pub async fn connect_ws(url: &str) -> Result<WsHandle> {
    let (ws_stream, _) = connect_async(url)
        .await
        .with_context(|| format!("failed to connect to websocket {url}"))?;
    let (sink, mut stream) = ws_stream.split();
    let sink = Arc::new(Mutex::new(sink));
    let (out_tx, mut out_rx) = mpsc::channel::<JsonValue>(64);
    let (in_tx, in_rx) = mpsc::channel::<JsonValue>(64);

    let sink_writer = sink.clone();
    tokio::spawn(async move {
        while let Some(value) = out_rx.recv().await {
            match encode_cbor_json(&value, StreamFormat::Json) {
                Ok(message) => {
                    let mut guard = sink_writer.lock().await;
                    if guard.send(message).await.is_err() {
                        break;
                    }
                }
                Err(err) => {
                    tracing::warn!(error = %err, "ws_client.encode_failed");
                }
            }
        }
    });

    let sink_pong = sink.clone();
    tokio::spawn(async move {
        while let Some(item) = stream.next().await {
            match item {
                Ok(Message::Ping(payload)) => {
                    let mut guard = sink_pong.lock().await;
                    if guard.send(Message::Pong(payload)).await.is_err() {
                        break;
                    }
                }
                Ok(Message::Pong(_)) => {}
                Ok(msg) => match decode_cbor_json(msg) {
                    Ok((value, _)) => {
                        if in_tx.send(value).await.is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        tracing::warn!(error = %err, "ws_client.decode_failed");
                    }
                },
                Err(err) => {
                    tracing::warn!(error = %err, "ws_client.recv_error");
                    break;
                }
            }
        }
    });

    Ok(WsHandle {
        sender: out_tx,
        receiver: Arc::new(Mutex::new(in_rx)),
    })
}
