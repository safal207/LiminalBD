use std::cell::RefCell;

use js_sys::Uint8Array;
use liminal_core::morph_mind::{analyze, hints as gather_hints};
use liminal_core::parse_lql;
use liminal_core::types::Hint;
use liminal_core::ClusterField;
use serde_json::json;
use wasm_bindgen::prelude::*;

use crate::protocol::{
    adjust_tick, event_from_field_log, event_from_hint, event_from_impulse_log, event_from_metrics,
    BridgeConfig, Outbox, ProtocolCommand, ProtocolMetrics, ProtocolPackage, ProtocolPush,
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
                ProtocolCommand::DreamNow => {
                    match self.field.run_dream_cycle() {
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
                    }
                }
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
