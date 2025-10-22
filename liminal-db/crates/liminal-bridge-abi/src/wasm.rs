use std::cell::RefCell;
use std::cmp::Ordering;

use js_sys::Uint8Array;
use liminal_core::morph_mind::{analyze, hints as gather_hints};
use liminal_core::types::Hint;
use liminal_core::ClusterField;
use liminal_core::{
    detect_sync_groups, parse_lql, run_collective_dream, AwakeningConfig, AwakeningReport,
    Influence, ResonantModel, Tension,
};
use serde_json::{json, Value as JsonValue};
use wasm_bindgen::prelude::*;

use crate::protocol::{
    adjust_tick, event_from_field_log, event_from_hint, event_from_impulse_log, event_from_metrics,
    BridgeConfig, IntrospectRequest, IntrospectTarget, Outbox, ProtocolCommand, ProtocolMetrics,
    ProtocolPackage, ProtocolPush,
};

thread_local! {
    static STATE: RefCell<Option<WasmState>> = RefCell::new(None);
}

struct WasmState {
    field: ClusterField,
    outbox: Outbox,
    tick_ms: u32,
    elapsed_since_metrics: u32,
    last_tick_ms: f64,
}

impl WasmState {
    fn new(config: BridgeConfig) -> Result<Self, String> {
        if config.tick_ms == 0 {
            return Err("tick_ms must be greater than zero".into());
        }
        let mut field = ClusterField::new();
        field.add_root("liminal/root");
        Ok(WasmState {
            field,
            outbox: Outbox::default(),
            tick_ms: config.tick_ms,
            elapsed_since_metrics: 0,
            last_tick_ms: js_sys::Date::now(),
        })
    }

    fn handle_push(&mut self, message: ProtocolPush) {
        match message {
            ProtocolPush::Impulse(impulse) => {
                let logs = self.field.route_impulse(impulse.to_core());
                for log in logs {
                    if let Some(event) = event_from_impulse_log(&log) {
                        self.outbox.push_event(event);
                    }
                }
            }
            ProtocolPush::Command(command) => match command {
                ProtocolCommand::TrsSet { cfg } => {
                    let event_string = self.field.set_trs_config(cfg);
                    if let Some(event) = event_from_field_log(&event_string, self.tick_ms) {
                        self.outbox.push_event(event);
                    }
                }
                ProtocolCommand::TrsTarget { value } => {
                    let event_string = self.field.set_trs_target(value);
                    if let Some(event) = event_from_field_log(&event_string, self.tick_ms) {
                        self.outbox.push_event(event);
                    }
                }
                ProtocolCommand::Lql { query } => {
                    let mut events: Vec<String> = Vec::new();
                    match parse_lql(&query) {
                        Ok(ast) => match self.field.exec_lql(ast) {
                            Ok(result) => events.extend(result.events),
                            Err(err) => events.push(
                                json!({
                                    "ev": "lql",
                                    "meta": {
                                        "error": err.to_string(),
                                        "query": query,
                                    }
                                })
                                .to_string(),
                            ),
                        },
                        Err(err) => {
                            events.push(
                                json!({
                                    "ev": "lql",
                                    "meta": {
                                        "error": err.to_string(),
                                        "query": query,
                                    }
                                })
                                .to_string(),
                            );
                        }
                    }
                    for event_string in events {
                        if let Some(event) = event_from_field_log(&event_string, self.tick_ms) {
                            self.outbox.push_event(event);
                        }
                    }
                }
                ProtocolCommand::DreamNow => match self.field.run_dream_cycle() {
                    Some(report) => {
                        let summary = format!(
                                "DREAM strengthened={} weakened={} pruned={} rewired={} protected={} took={}ms",
                                report.strengthened,
                                report.weakened,
                                report.pruned,
                                report.rewired,
                                report.protected,
                                report.took_ms
                            );
                        if let Some(event) = event_from_field_log(&summary, self.tick_ms) {
                            self.outbox.push_event(event);
                        }
                        let payload = json!({
                            "ev": "dream",
                            "meta": {
                                "strengthened": report.strengthened,
                                "weakened": report.weakened,
                                "pruned": report.pruned,
                                "rewired": report.rewired,
                                "protected": report.protected,
                                "took_ms": report.took_ms
                            }
                        })
                        .to_string();
                        if let Some(event) = event_from_field_log(&payload, self.tick_ms) {
                            self.outbox.push_event(event);
                        }
                    }
                    None => {
                        let msg = "DREAM no-op (not enough activity)";
                        if let Some(event) = event_from_field_log(msg, self.tick_ms) {
                            self.outbox.push_event(event);
                        }
                    }
                },
                ProtocolCommand::DreamSet { cfg } => {
                    self.field.set_dream_config(cfg.clone());
                    let payload = json!({
                        "ev": "dream_config",
                        "meta": {
                            "min_idle_s": cfg.min_idle_s,
                            "window_ms": cfg.window_ms,
                            "strengthen_top_pct": cfg.strengthen_top_pct,
                            "weaken_bottom_pct": cfg.weaken_bottom_pct,
                            "protect_salience": cfg.protect_salience,
                            "adreno_protect": cfg.adreno_protect,
                            "max_ops_per_cycle": cfg.max_ops_per_cycle
                        }
                    })
                    .to_string();
                    if let Some(event) = event_from_field_log(&payload, self.tick_ms) {
                        self.outbox.push_event(event);
                    }
                }
                ProtocolCommand::DreamGet => {
                    let cfg = self.field.dream_config();
                    let payload = json!({
                        "ev": "dream_config",
                        "meta": {
                            "min_idle_s": cfg.min_idle_s,
                            "window_ms": cfg.window_ms,
                            "strengthen_top_pct": cfg.strengthen_top_pct,
                            "weaken_bottom_pct": cfg.weaken_bottom_pct,
                            "protect_salience": cfg.protect_salience,
                            "adreno_protect": cfg.adreno_protect,
                            "max_ops_per_cycle": cfg.max_ops_per_cycle
                        }
                    })
                    .to_string();
                    if let Some(event) = event_from_field_log(&payload, self.tick_ms) {
                        self.outbox.push_event(event);
                    }
                }
                ProtocolCommand::SyncNow => {
                    let cfg = self.field.sync_config();
                    let groups = detect_sync_groups(&self.field, &cfg, self.field.now_ms);
                    if groups.is_empty() {
                        if let Some(event) = event_from_field_log(
                            "COLLECTIVE_DREAM no-op (no qualifying groups)",
                            self.tick_ms,
                        ) {
                            self.outbox.push_event(event);
                        }
                    } else {
                        let report =
                            run_collective_dream(&mut self.field, &groups, &cfg, self.field.now_ms);
                        let summary = format!(
                            "COLLECTIVE_DREAM groups={} shared={} aligned={} protected={} took={}ms",
                            report.groups, report.shared, report.aligned, report.protected, report.took_ms
                        );
                        if let Some(event) = event_from_field_log(&summary, self.tick_ms) {
                            self.outbox.push_event(event);
                        }
                        let payload = json!({
                            "ev": "collective_dream",
                            "meta": {
                                "groups": report.groups,
                                "shared": report.shared,
                                "aligned": report.aligned,
                                "protected": report.protected,
                                "took_ms": report.took_ms
                            }
                        })
                        .to_string();
                        if let Some(event) = event_from_field_log(&payload, self.tick_ms) {
                            self.outbox.push_event(event);
                        }
                    }
                }
                ProtocolCommand::SyncSet { cfg } => {
                    self.field.set_sync_config(cfg.clone());
                    let payload = json!({
                        "ev": "sync_config",
                        "meta": {
                            "phase_len_ms": cfg.phase_len_ms,
                            "phase_gap_ms": cfg.phase_gap_ms,
                            "cooccur_threshold": cfg.cooccur_threshold,
                            "max_groups": cfg.max_groups,
                            "share_top_k": cfg.share_top_k,
                            "weight_xfer": cfg.weight_xfer
                        }
                    })
                    .to_string();
                    if let Some(event) = event_from_field_log(&payload, self.tick_ms) {
                        self.outbox.push_event(event);
                    }
                }
                ProtocolCommand::SyncGet => {
                    let cfg = self.field.sync_config();
                    let payload = json!({
                        "ev": "sync_config",
                        "meta": {
                            "phase_len_ms": cfg.phase_len_ms,
                            "phase_gap_ms": cfg.phase_gap_ms,
                            "cooccur_threshold": cfg.cooccur_threshold,
                            "max_groups": cfg.max_groups,
                            "share_top_k": cfg.share_top_k,
                            "weight_xfer": cfg.weight_xfer
                        }
                    })
                    .to_string();
                    if let Some(event) = event_from_field_log(&payload, self.tick_ms) {
                        self.outbox.push_event(event);
                    }
                }
                ProtocolCommand::AwakenSet { cfg } => {
                    self.field.set_awakening_config(cfg.clone());
                    let payload = awaken_config_event(&cfg, "set");
                    push_json_event(&mut self.outbox, payload, self.tick_ms);
                }
                ProtocolCommand::AwakenGet => {
                    let cfg = self.field.awakening_config();
                    let payload = awaken_config_event(&cfg, "cfg");
                    push_json_event(&mut self.outbox, payload, self.tick_ms);
                }
                ProtocolCommand::Introspect(IntrospectRequest { target, top }) => {
                    let payload = build_introspect_event(&mut self.field, target, top);
                    push_json_event(&mut self.outbox, payload, self.tick_ms);
                }
            },
        }
    }

    fn maybe_tick(&mut self) {
        let now = js_sys::Date::now();
        let mut elapsed = now - self.last_tick_ms;
        if elapsed < self.tick_ms as f64 {
            return;
        }
        while elapsed >= self.tick_ms as f64 {
            self.tick_once();
            self.last_tick_ms += self.tick_ms as f64;
            elapsed = now - self.last_tick_ms;
        }
    }

    fn tick_once(&mut self) {
        let events = self.field.tick_all(self.tick_ms as u64);
        for event in events {
            if let Some(proto) = event_from_field_log(&event, self.tick_ms) {
                self.outbox.push_event(proto);
            }
        }
        self.elapsed_since_metrics = self.elapsed_since_metrics.saturating_add(self.tick_ms);
        if self.elapsed_since_metrics >= 1000 {
            let metrics_core = analyze(&self.field);
            let hints = gather_hints(&metrics_core);
            for hint in hints {
                self.apply_hint(&hint);
            }
            let proto = ProtocolMetrics::from_core(&metrics_core);
            let event = event_from_metrics(&proto, 1_000);
            self.outbox.set_metrics(proto.clone());
            self.outbox.push_event(event);
            self.elapsed_since_metrics = 0;
        }
    }

    fn apply_hint(&mut self, hint: &Hint) {
        match hint {
            Hint::SlowTick | Hint::FastTick => {
                let atomic = std::sync::atomic::AtomicU32::new(self.tick_ms);
                let new_tick = adjust_tick(&atomic, hint);
                self.tick_ms = new_tick;
            }
            Hint::TrimField => {
                self.field.trim_low_energy();
            }
            Hint::WakeSeeds => {
                self.field.inject_seed_variation("liminal/wake");
            }
        }
        let event = event_from_hint(hint, self.tick_ms);
        self.outbox.push_event(event);
    }

    fn take_package(&mut self) -> Option<ProtocolPackage> {
        self.outbox.take()
    }

    fn restore_package(&mut self, package: ProtocolPackage) {
        self.outbox.restore(package);
    }
}

fn push_json_event(outbox: &mut Outbox, payload: JsonValue, tick_ms: u32) {
    let serialized = payload.to_string();
    if let Some(event) = event_from_field_log(&serialized, tick_ms) {
        outbox.push_event(event);
    }
}

fn awaken_config_event(cfg: &AwakeningConfig, action: &str) -> JsonValue {
    let cfg_json = serde_json::to_value(cfg).unwrap_or(JsonValue::Null);
    json!({
        "ev": "awaken",
        "meta": {
            "action": action,
            "cfg": cfg_json
        }
    })
}

fn awaken_report_event(report: &AwakeningReport) -> JsonValue {
    json!({
        "ev": "awaken",
        "meta": {
            "action": "now",
            "applied": report.applied,
            "protected": report.protected,
            "avg_tension": report.avg_tension,
            "tick_adjust_ms": report.tick_adjust_ms,
            "energy_delta": report.energy_delta
        }
    })
}

fn ensure_resonant_model(field: &mut ClusterField) -> ResonantModel {
    field
        .rebuild_resonant_model()
        .or_else(|| field.resonant_model().cloned())
        .unwrap_or_default()
}

fn build_introspect_event(
    field: &mut ClusterField,
    target: IntrospectTarget,
    top: Option<u32>,
) -> JsonValue {
    let limit = top.unwrap_or(5).max(1).min(32) as usize;
    match target {
        IntrospectTarget::Awaken => awaken_report_event(&field.awaken_now()),
        IntrospectTarget::Model => {
            let model = ensure_resonant_model(field);
            json!({
                "ev": "introspect",
                "meta": {
                    "target": "model",
                    "coherence": model.coherence,
                    "edges": model.edges.len(),
                    "influences": model.influences.len(),
                    "tensions": model.tensions.len(),
                    "top_nodes": top_nodes(field, limit),
                    "requested_top": limit as u32
                }
            })
        }
        IntrospectTarget::Influence => {
            let model = ensure_resonant_model(field);
            json!({
                "ev": "introspect",
                "meta": {
                    "target": "influence",
                    "items": top_influences(&model, limit),
                    "requested_top": limit as u32
                }
            })
        }
        IntrospectTarget::Tension => {
            let model = ensure_resonant_model(field);
            json!({
                "ev": "introspect",
                "meta": {
                    "target": "tension",
                    "items": top_tensions(&model, limit),
                    "requested_top": limit as u32
                }
            })
        }
    }
}

fn top_nodes(field: &ClusterField, limit: usize) -> Vec<JsonValue> {
    let mut nodes: Vec<_> = field.cells.values().collect();
    nodes.sort_by(|a, b| {
        b.salience
            .partial_cmp(&a.salience)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    nodes.truncate(limit);
    nodes
        .into_iter()
        .map(|cell| {
            json!({
                "id": cell.id,
                "pattern": cell.seed.core_pattern,
                "salience": cell.salience,
                "energy": cell.energy
            })
        })
        .collect()
}

fn top_influences(model: &ResonantModel, limit: usize) -> Vec<JsonValue> {
    let mut influences: Vec<Influence> = model.influences.clone();
    influences.sort_by(|a, b| {
        b.weight
            .abs()
            .partial_cmp(&a.weight.abs())
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.source.cmp(&b.source))
    });
    influences.truncate(limit);
    influences
        .into_iter()
        .map(|inf| {
            json!({
                "source": inf.source,
                "sink": inf.sink,
                "weight": inf.weight,
                "coherence": inf.coherence
            })
        })
        .collect()
}

fn top_tensions(model: &ResonantModel, limit: usize) -> Vec<JsonValue> {
    let mut tensions: Vec<Tension> = model.tensions.clone();
    tensions.sort_by(|a, b| {
        b.magnitude
            .partial_cmp(&a.magnitude)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.node.cmp(&b.node))
    });
    tensions.truncate(limit);
    tensions
        .into_iter()
        .map(|ten| {
            json!({
                "node": ten.node,
                "magnitude": ten.magnitude,
                "relief": ten.relief
            })
        })
        .collect()
}

#[wasm_bindgen]
pub fn init(cfg_cbor: Uint8Array) -> bool {
    STATE.with(|state| {
        if state.borrow().is_some() {
            return false;
        }
        let data = cfg_cbor.to_vec();
        let config: BridgeConfig = match serde_cbor::from_slice(&data) {
            Ok(cfg) => cfg,
            Err(_) => return false,
        };
        match WasmState::new(config) {
            Ok(instance) => {
                *state.borrow_mut() = Some(instance);
                true
            }
            Err(_) => false,
        }
    })
}

#[wasm_bindgen]
pub fn push(msg_cbor: Uint8Array) -> u32 {
    STATE.with(|state| {
        let mut borrow = state.borrow_mut();
        let Some(instance) = borrow.as_mut() else {
            return 0;
        };
        let data = msg_cbor.to_vec();
        let message: ProtocolPush = match serde_cbor::from_slice(&data) {
            Ok(msg) => msg,
            Err(_) => return 0,
        };
        instance.handle_push(message);
        instance.maybe_tick();
        data.len() as u32
    })
}

#[wasm_bindgen]
pub fn pull(cap: u32) -> Uint8Array {
    STATE.with(|state| {
        let mut borrow = state.borrow_mut();
        let Some(instance) = borrow.as_mut() else {
            return Uint8Array::new_with_length(0);
        };
        instance.maybe_tick();
        let Some(package) = instance.take_package() else {
            return Uint8Array::new_with_length(0);
        };
        let encoded = match serde_cbor::to_vec(&package) {
            Ok(bytes) => bytes,
            Err(_) => {
                instance.restore_package(package);
                return Uint8Array::new_with_length(0);
            }
        };
        if encoded.len() > cap as usize {
            instance.restore_package(package);
            return Uint8Array::new_with_length(0);
        }
        let result = Uint8Array::new_with_length(encoded.len() as u32);
        result.copy_from(&encoded);
        result
    })
}
