pub mod stream_codec;
pub mod ws_client;
pub mod ws_server;

use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde_json::Value as JsonValue;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{broadcast, mpsc};

use stream_codec::StreamFormat;

#[derive(Debug, Clone)]
pub struct StreamEvent {
    pub payload: JsonValue,
}

#[derive(Debug, Clone)]
pub struct StreamMetrics {
    pub payload: JsonValue,
}

#[derive(Debug, Clone)]
pub enum IncomingCommand {
    Impulse { data: JsonValue },
    Lql { query: String },
    PolicySet { data: JsonValue },
    Subscribe { pattern: String },
    Raw(JsonValue),
}

#[derive(Debug, Clone)]
pub struct ClientSnapshot {
    pub id: u64,
    pub last_ping: Instant,
    pub connected_at: Instant,
    pub addr: String,
    pub format: StreamFormat,
}

struct BridgeHubInner {
    events_tx: broadcast::Sender<StreamEvent>,
    metrics_tx: broadcast::Sender<StreamMetrics>,
    command_tx: mpsc::Sender<IncomingCommand>,
    command_rx: Mutex<Option<mpsc::Receiver<IncomingCommand>>>,
    clients: Mutex<Vec<ClientSnapshot>>,
    default_format: Mutex<StreamFormat>,
}

impl BridgeHubInner {
    fn new() -> Self {
        let (events_tx, _) = broadcast::channel(1024);
        let (metrics_tx, _) = broadcast::channel(32);
        let (command_tx, command_rx) = mpsc::channel(128);
        BridgeHubInner {
            events_tx,
            metrics_tx,
            command_tx,
            command_rx: Mutex::new(Some(command_rx)),
            clients: Mutex::new(Vec::new()),
            default_format: Mutex::new(StreamFormat::Json),
        }
    }
}

static HUB: Lazy<Arc<BridgeHubInner>> = Lazy::new(|| Arc::new(BridgeHubInner::new()));

pub(crate) fn global_hub() -> Arc<BridgeHubInner> {
    HUB.clone()
}

impl BridgeHubInner {
    pub fn publish_event(&self, mut payload: JsonValue) {
        if let Some(obj) = payload.as_object_mut() {
            obj.entry("meta".to_string())
                .or_insert_with(|| JsonValue::Object(Default::default()));
            if let Some(meta) = obj.get_mut("meta").and_then(|m| m.as_object_mut()) {
                meta.insert("source".into(), JsonValue::String("ws".into()));
            }
        }
        let _ = self.events_tx.send(StreamEvent { payload });
    }

    pub fn publish_metrics(&self, payload: JsonValue) {
        let _ = self.metrics_tx.send(StreamMetrics { payload });
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<StreamEvent> {
        self.events_tx.subscribe()
    }

    pub fn subscribe_metrics(&self) -> broadcast::Receiver<StreamMetrics> {
        self.metrics_tx.subscribe()
    }

    pub fn command_sender(&self) -> mpsc::Sender<IncomingCommand> {
        self.command_tx.clone()
    }

    pub fn take_command_receiver(&self) -> Option<mpsc::Receiver<IncomingCommand>> {
        self.command_rx.lock().take()
    }

    pub fn list_clients(&self) -> Vec<ClientSnapshot> {
        self.clients.lock().clone()
    }

    pub fn register_client(&self, snapshot: ClientSnapshot) {
        self.clients.lock().push(snapshot);
    }

    pub fn update_ping(&self, id: u64) {
        let mut guard = self.clients.lock();
        if let Some(client) = guard.iter_mut().find(|c| c.id == id) {
            client.last_ping = Instant::now();
        }
    }

    pub fn remove_client(&self, id: u64) {
        let mut guard = self.clients.lock();
        guard.retain(|c| c.id != id);
    }

    pub fn default_format(&self) -> StreamFormat {
        *self.default_format.lock()
    }

    pub fn set_default_format(&self, format: StreamFormat) {
        *self.default_format.lock() = format;
    }

    pub fn update_client_format(&self, id: u64, format: StreamFormat) {
        let mut guard = self.clients.lock();
        if let Some(client) = guard.iter_mut().find(|c| c.id == id) {
            client.format = format;
        }
    }
}

pub fn register_client_snapshot(snapshot: ClientSnapshot) {
    HUB.register_client(snapshot);
}

pub fn update_client_ping(id: u64) {
    HUB.update_ping(id);
}

pub fn remove_client_snapshot(id: u64) {
    HUB.remove_client(id);
}

pub fn update_client_format(id: u64, format: StreamFormat) {
    HUB.update_client_format(id, format);
}

pub fn set_default_format(format: StreamFormat) {
    HUB.set_default_format(format);
}

pub fn list_clients() -> Vec<ClientSnapshot> {
    HUB.list_clients()
}

pub async fn next_command(
    mut receiver: mpsc::Receiver<IncomingCommand>,
) -> Option<IncomingCommand> {
    receiver.recv().await
}

pub fn command_sender() -> mpsc::Sender<IncomingCommand> {
    HUB.command_sender()
}

pub fn subscribe_events() -> broadcast::Receiver<StreamEvent> {
    HUB.subscribe_events()
}

pub fn subscribe_metrics() -> broadcast::Receiver<StreamMetrics> {
    HUB.subscribe_metrics()
}

pub fn publish_event(payload: JsonValue) {
    HUB.publish_event(payload);
}

pub fn publish_metrics(payload: JsonValue) {
    HUB.publish_metrics(payload);
}

pub fn take_command_receiver() -> Option<mpsc::Receiver<IncomingCommand>> {
    HUB.take_command_receiver()
}

pub fn now_instant() -> Instant {
    Instant::now()
}

pub fn age_since(instant: Instant) -> Duration {
    instant.elapsed()
}

pub fn format_clients(clients: &[ClientSnapshot]) -> Vec<String> {
    clients
        .iter()
        .map(|client| {
            format!(
                "id={} addr={} connected={}s ago format={:?}",
                client.id,
                client.addr,
                age_since(client.connected_at).as_secs(),
                client.format
            )
        })
        .collect()
}
