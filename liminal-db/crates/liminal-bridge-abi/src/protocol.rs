use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicU32, Ordering};

use liminal_core::types::{Hint, Impulse as CoreImpulse, ImpulseKind, Metrics as CoreMetrics};
use liminal_core::{AwakeningConfig, DreamConfig, SyncConfig, TrsConfig};
use serde::{Deserialize, Serialize};
use serde_cbor::Value;
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BridgeConfig {
    #[serde(rename = "tick_ms")]
    pub tick_ms: u32,
    #[serde(rename = "store_path", skip_serializing_if = "Option::is_none")]
    pub store_path: Option<String>,
    #[serde(rename = "snap_interval", skip_serializing_if = "Option::is_none")]
    pub snap_interval: Option<u32>,
    #[serde(rename = "snap_maxwal", skip_serializing_if = "Option::is_none")]
    pub snap_maxwal: Option<u32>,
    #[serde(rename = "ws_port", skip_serializing_if = "Option::is_none")]
    pub ws_port: Option<u16>,
    #[serde(rename = "ws_format", skip_serializing_if = "Option::is_none")]
    pub ws_format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProtocolImpulse {
    #[serde(rename = "k")]
    pub kind: u8,
    #[serde(rename = "p")]
    pub pattern: String,
    #[serde(rename = "s")]
    pub strength: f32,
    #[serde(rename = "t")]
    pub ttl_ms: u32,
    #[serde(rename = "tg", default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "cmd")]
pub enum ProtocolCommand {
    #[serde(rename = "trs_set")]
    TrsSet { cfg: TrsConfig },
    #[serde(rename = "trs_target")]
    TrsTarget { value: f32 },
    #[serde(rename = "lql")]
    Lql {
        #[serde(rename = "q")]
        query: String,
    },
    #[serde(rename = "dream.now")]
    DreamNow,
    #[serde(rename = "dream.set")]
    DreamSet { cfg: DreamConfig },
    #[serde(rename = "dream.get")]
    DreamGet,
    #[serde(rename = "sync.set")]
    SyncSet { cfg: SyncConfig },
    #[serde(rename = "sync.get")]
    SyncGet,
    #[serde(rename = "sync.now")]
    SyncNow,
    #[serde(rename = "awaken.set")]
    AwakenSet { cfg: AwakeningConfig },
    #[serde(rename = "awaken.get")]
    AwakenGet,
    #[serde(rename = "introspect")]
    Introspect(IntrospectRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IntrospectTarget {
    Awaken,
    Model,
    Influence,
    Tension,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IntrospectRequest {
    #[serde(rename = "target")]
    pub target: IntrospectTarget,
    #[serde(rename = "top", skip_serializing_if = "Option::is_none")]
    pub top: Option<u32>,
}

impl IntrospectRequest {
    pub fn new(target: IntrospectTarget) -> Self {
        IntrospectRequest { target, top: None }
    }

    pub fn with_top(mut self, value: Option<u32>) -> Self {
        self.top = value;
        self
    }
}

impl ProtocolCommand {
    pub fn awaken_set(cfg: AwakeningConfig) -> Self {
        ProtocolCommand::AwakenSet { cfg }
    }

    pub fn awaken_get() -> Self {
        ProtocolCommand::AwakenGet
    }

    pub fn introspect(target: IntrospectTarget, top: Option<u32>) -> Self {
        ProtocolCommand::Introspect(IntrospectRequest { target, top })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ProtocolPush {
    Impulse(ProtocolImpulse),
    Command(ProtocolCommand),
}

impl ProtocolImpulse {
    pub fn to_core(&self) -> CoreImpulse {
        CoreImpulse {
            kind: match self.kind {
                0 => ImpulseKind::Affect,
                1 => ImpulseKind::Query,
                2 => ImpulseKind::Write,
                _ => ImpulseKind::Query,
            },
            pattern: self.pattern.clone(),
            strength: self.strength,
            ttl_ms: self.ttl_ms as u64,
            tags: self.tags.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProtocolMetrics {
    #[serde(rename = "cells")]
    pub cells: u32,
    #[serde(rename = "sleeping")]
    pub sleeping: f32,
    #[serde(rename = "avgMet")]
    pub avg_met: f32,
    #[serde(rename = "avgLat")]
    pub avg_latency: u32,
}

impl ProtocolMetrics {
    pub fn from_core(metrics: &CoreMetrics) -> Self {
        ProtocolMetrics {
            cells: metrics.cells.min(u32::MAX as usize) as u32,
            sleeping: metrics.sleeping_pct,
            avg_met: metrics.avg_metabolism,
            avg_latency: metrics.avg_latency_ms.round().clamp(0.0, u32::MAX as f32) as u32,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum EventId {
    Num(u64),
    Text(String),
}

impl From<u64> for EventId {
    fn from(value: u64) -> Self {
        EventId::Num(value)
    }
}

impl From<&str> for EventId {
    fn from(value: &str) -> Self {
        EventId::Text(value.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProtocolEvent {
    #[serde(rename = "ev")]
    pub ev: String,
    #[serde(rename = "id")]
    pub id: EventId,
    #[serde(rename = "dt")]
    pub dt: u32,
    #[serde(rename = "meta", default)]
    pub meta: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ProtocolPackage {
    #[serde(rename = "events", default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<ProtocolEvent>,
    #[serde(rename = "metrics", skip_serializing_if = "Option::is_none")]
    pub metrics: Option<ProtocolMetrics>,
}

#[derive(Default)]
pub struct Outbox {
    events: VecDeque<ProtocolEvent>,
    metrics: Option<ProtocolMetrics>,
}

impl Outbox {
    pub fn push_event(&mut self, event: ProtocolEvent) {
        self.events.push_back(event);
    }

    pub fn set_metrics(&mut self, metrics: ProtocolMetrics) {
        self.metrics = Some(metrics);
    }

    pub fn take(&mut self) -> Option<ProtocolPackage> {
        if self.events.is_empty() && self.metrics.is_none() {
            return None;
        }
        let events: Vec<ProtocolEvent> = self.events.drain(..).collect();
        let metrics = self.metrics.take();
        Some(ProtocolPackage { events, metrics })
    }

    pub fn restore(&mut self, package: ProtocolPackage) {
        for event in package.events.into_iter().rev() {
            self.events.push_front(event);
        }
        if self.metrics.is_none() {
            self.metrics = package.metrics;
        }
    }
}

pub fn event_from_field_log(log: &str, tick_ms: u32) -> Option<ProtocolEvent> {
    let trimmed = log.trim();
    if trimmed.starts_with('{') {
        if let Ok(json) = serde_json::from_str::<JsonValue>(trimmed) {
            if let Some(event) = event_from_json(&json, tick_ms) {
                return Some(event);
            }
        }
    }
    if let Some(rest) = trimmed.strip_prefix("DIVIDE parent=n") {
        let (parent_part, rest) = rest.split_once(" -> child=n")?;
        let parent_id: u64 = parent_part.parse().ok()?;
        let (child_part, rest) = rest.split_once(" (aff ")?;
        let child_id: u64 = child_part.parse().ok()?;
        let (aff_part, _) = rest.split_once(')')?;
        let (aff_before, aff_after) = aff_part.split_once("->")?;
        let parent_aff: f32 = aff_before.trim().parse().ok()?;
        let child_aff: f32 = aff_after.trim().parse().ok()?;
        let mut meta = BTreeMap::new();
        meta.insert("parent".into(), Value::Integer(parent_id as i128));
        meta.insert("child".into(), Value::Integer(child_id as i128));
        meta.insert("aff_before".into(), Value::Float(parent_aff as f64));
        meta.insert("aff_after".into(), Value::Float(child_aff as f64));
        return Some(ProtocolEvent {
            ev: "divide".into(),
            id: child_id.into(),
            dt: tick_ms,
            meta,
        });
    }
    if let Some(rest) = trimmed.strip_prefix("SLEEP n") {
        let id: u64 = rest.parse().ok()?;
        let mut meta = BTreeMap::new();
        meta.insert("state".into(), Value::Text("sleep".into()));
        return Some(ProtocolEvent {
            ev: "sleep".into(),
            id: id.into(),
            dt: tick_ms,
            meta,
        });
    }
    if let Some(rest) = trimmed.strip_prefix("DEAD n") {
        let id: u64 = rest.parse().ok()?;
        let mut meta = BTreeMap::new();
        meta.insert("state".into(), Value::Text("dead".into()));
        return Some(ProtocolEvent {
            ev: "dead".into(),
            id: id.into(),
            dt: tick_ms,
            meta,
        });
    }
    if let Some(rest) = trimmed.strip_prefix("COLLECTIVE_DREAM ") {
        let mut meta = BTreeMap::new();
        for chunk in rest.split_whitespace() {
            if let Some((key, raw)) = chunk.split_once('=') {
                let cleaned = raw.trim_end_matches("ms");
                if let Ok(value) = cleaned.parse::<u64>() {
                    meta.insert(key.to_string(), Value::Integer(value as i128));
                }
            }
        }
        return Some(ProtocolEvent {
            ev: "collective_dream".into(),
            id: EventId::Text("collective_dream".into()),
            dt: tick_ms,
            meta,
        });
    }
    None
}

pub fn event_from_impulse_log(log: &str) -> Option<ProtocolEvent> {
    let trimmed = log.trim();
    if let Some(stripped) = trimmed.strip_prefix('n') {
        let (id_part, rest) = stripped.split_once(' ')?;
        let id: u64 = id_part.parse().ok()?;
        let mut meta = BTreeMap::new();
        meta.insert("kind".into(), Value::Text("impulse".into()));
        meta.insert("message".into(), Value::Text(rest.trim().into()));
        return Some(ProtocolEvent {
            ev: "hint".into(),
            id: id.into(),
            dt: 0,
            meta,
        });
    }
    None
}

fn event_from_json(json: &JsonValue, tick_ms: u32) -> Option<ProtocolEvent> {
    let ev = json.get("ev")?.as_str()?.to_string();
    let id = if let Some(id_val) = json.get("id") {
        json_value_to_event_id(id_val).unwrap_or(EventId::Text(ev.clone()))
    } else {
        EventId::Text(ev.clone())
    };
    let meta = match json.get("meta") {
        Some(JsonValue::Object(map)) => map
            .iter()
            .map(|(k, v)| (k.clone(), json_to_cbor(v)))
            .collect(),
        _ => BTreeMap::new(),
    };
    Some(ProtocolEvent {
        ev,
        id,
        dt: tick_ms,
        meta,
    })
}

fn json_value_to_event_id(value: &JsonValue) -> Option<EventId> {
    match value {
        JsonValue::Number(num) => {
            if let Some(u) = num.as_u64() {
                Some(EventId::Num(u))
            } else {
                None
            }
        }
        JsonValue::String(text) => Some(EventId::Text(text.clone())),
        _ => None,
    }
}

fn json_to_cbor(value: &JsonValue) -> Value {
    match value {
        JsonValue::Null => Value::Null,
        JsonValue::Bool(b) => Value::Bool(*b),
        JsonValue::Number(num) => {
            if let Some(i) = num.as_i64() {
                Value::Integer(i as i128)
            } else if let Some(u) = num.as_u64() {
                Value::Integer(u as i128)
            } else if let Some(f) = num.as_f64() {
                Value::Float(f)
            } else {
                Value::Null
            }
        }
        JsonValue::String(text) => Value::Text(text.clone()),
        JsonValue::Array(items) => {
            let converted = items.iter().map(json_to_cbor).collect();
            Value::Array(converted)
        }
        JsonValue::Object(map) => {
            let converted = map
                .iter()
                .map(|(k, v)| (Value::Text(k.clone()), json_to_cbor(v)))
                .collect();
            Value::Map(converted)
        }
    }
}

pub fn event_from_snapshot(id: u64) -> ProtocolEvent {
    let mut meta = BTreeMap::new();
    meta.insert("id".into(), Value::Integer(id as i128));
    ProtocolEvent {
        ev: "snapshot".into(),
        id: id.into(),
        dt: 0,
        meta,
    }
}

pub fn event_from_hint(hint: &Hint, tick_ms: u32) -> ProtocolEvent {
    let mut meta = BTreeMap::new();
    let label = match hint {
        Hint::SlowTick => "slow_tick",
        Hint::FastTick => "fast_tick",
        Hint::TrimField => "trim_field",
        Hint::WakeSeeds => "wake_seeds",
    };
    meta.insert("hint".into(), Value::Text(label.into()));
    meta.insert("tick_ms".into(), Value::Integer(tick_ms as i128));
    ProtocolEvent {
        ev: "hint".into(),
        id: EventId::Text("system".into()),
        dt: tick_ms,
        meta,
    }
}

pub fn event_from_metrics(metrics: &ProtocolMetrics, dt: u32) -> ProtocolEvent {
    let mut meta = BTreeMap::new();
    meta.insert("cells".into(), Value::Integer(metrics.cells as i128));
    meta.insert("sleeping".into(), Value::Float(metrics.sleeping as f64));
    meta.insert("avgMet".into(), Value::Float(metrics.avg_met as f64));
    meta.insert("avgLat".into(), Value::Integer(metrics.avg_latency as i128));
    ProtocolEvent {
        ev: "metrics".into(),
        id: EventId::Text("system".into()),
        dt,
        meta,
    }
}

pub fn adjust_tick(current: &AtomicU32, hint: &Hint) -> u32 {
    loop {
        let tick = current.load(Ordering::SeqCst);
        let updated = match hint {
            Hint::SlowTick => tick.saturating_add(50).min(400),
            Hint::FastTick => {
                if tick > 100 {
                    (tick.saturating_sub(10)).max(100)
                } else {
                    tick
                }
            }
            Hint::TrimField | Hint::WakeSeeds => tick,
        };
        if current
            .compare_exchange(tick, updated, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return updated;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn impulse_roundtrip() {
        let impulse = ProtocolImpulse {
            kind: 0,
            pattern: "cpu/load".into(),
            strength: 0.75,
            ttl_ms: 900,
            tags: vec!["cli".into(), "test".into()],
        };
        let bytes = serde_cbor::to_vec(&impulse).unwrap();
        let decoded: ProtocolImpulse = serde_cbor::from_slice(&bytes).unwrap();
        assert_eq!(decoded, impulse);
    }

    #[test]
    fn event_roundtrip() {
        let mut meta = BTreeMap::new();
        meta.insert("state".into(), Value::Text("sleep".into()));
        let event = ProtocolEvent {
            ev: "sleep".into(),
            id: EventId::Num(5),
            dt: 200,
            meta,
        };
        let bytes = serde_cbor::to_vec(&event).unwrap();
        let decoded: ProtocolEvent = serde_cbor::from_slice(&bytes).unwrap();
        assert_eq!(decoded, event);
    }

    #[test]
    fn metrics_roundtrip() {
        let metrics = ProtocolMetrics {
            cells: 12,
            sleeping: 0.25,
            avg_met: 0.66,
            avg_latency: 180,
        };
        let bytes = serde_cbor::to_vec(&metrics).unwrap();
        let decoded: ProtocolMetrics = serde_cbor::from_slice(&bytes).unwrap();
        assert_eq!(decoded, metrics);
    }

    #[test]
    fn command_introspect_roundtrip() {
        let command = ProtocolCommand::introspect(IntrospectTarget::Influence, Some(3));
        let bytes = serde_cbor::to_vec(&command).unwrap();
        let decoded: ProtocolCommand = serde_cbor::from_slice(&bytes).unwrap();
        assert_eq!(decoded, command);
    }

    #[test]
    fn command_awaken_set_roundtrip() {
        let cfg = AwakeningConfig {
            max_nodes: 6,
            energy_floor: 0.4,
            energy_boost: 0.2,
            salience_boost: 0.1,
            protect_salience: 0.8,
            tick_bias_ms: 24,
            target_gain: 0.05,
        };
        let command = ProtocolCommand::awaken_set(cfg.clone());
        let bytes = serde_cbor::to_vec(&command).unwrap();
        let decoded: ProtocolCommand = serde_cbor::from_slice(&bytes).unwrap();
        assert_eq!(decoded, command);
        if let ProtocolCommand::AwakenSet { cfg: decoded_cfg } = decoded {
            assert_eq!(decoded_cfg, cfg);
        } else {
            panic!("expected awaken.set command");
        }
    }
}
