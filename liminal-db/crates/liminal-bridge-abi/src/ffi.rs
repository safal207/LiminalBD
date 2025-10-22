use std::cmp::Ordering;
use std::slice;
use std::sync::atomic::AtomicU32;
use std::sync::{Arc, Mutex as StdMutex, OnceLock};
use std::time::Duration;

use liminal_core::life_loop::run_loop;
use liminal_core::types::Hint;
use liminal_core::ClusterField;
use liminal_core::{
    detect_sync_groups, parse_lql, run_collective_dream, AwakeningConfig, AwakeningReport,
    Influence, ResonantModel, Tension,
};
use liminal_store::{decode_delta, DiskJournal, Offset};
use serde_json::{json, Value as JsonValue};
use tokio::runtime::Builder;
use tokio::sync::Mutex;

use crate::protocol::{
    adjust_tick, event_from_field_log, event_from_hint, event_from_impulse_log, event_from_metrics,
    event_from_snapshot, BridgeConfig, IntrospectRequest, IntrospectTarget, Outbox,
    ProtocolCommand, ProtocolMetrics, ProtocolPush,
};

struct BridgeState {
    runtime: tokio::runtime::Runtime,
    field: Arc<Mutex<ClusterField>>,
    outbox: Arc<StdMutex<Outbox>>,
    #[allow(dead_code)]
    journal: Option<Arc<DiskJournal>>,
    snapshot_cfg: Option<SnapshotConfig>,
}

#[derive(Clone)]
struct SnapshotConfig {
    interval: Duration,
    max_wal_events: u64,
}

static STATE: OnceLock<BridgeState> = OnceLock::new();

impl BridgeState {
    fn new(config: BridgeConfig) -> Result<Self, String> {
        if config.tick_ms == 0 {
            return Err("tick_ms must be greater than zero".into());
        }
        let store_path = config.store_path.clone();
        let interval_secs = config.snap_interval.unwrap_or(60).max(1);
        let max_wal = config.snap_maxwal.unwrap_or(5_000).max(1);
        let snapshot_cfg = store_path.as_ref().map(|_| SnapshotConfig {
            interval: Duration::from_secs(interval_secs as u64),
            max_wal_events: max_wal as u64,
        });
        let runtime = Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let (field, journal) = if let Some(path) = store_path.clone() {
            let journal = Arc::new(DiskJournal::open(path).map_err(|e| e.to_string())?);
            let (mut field, replay_offset) = if let Some((seed, offset)) =
                journal.load_latest_snapshot().map_err(|e| e.to_string())?
            {
                (seed.into_field(), offset)
            } else {
                (ClusterField::new(), Offset::start())
            };
            let mut stream = journal
                .stream_from(replay_offset)
                .map_err(|e| e.to_string())?;
            while let Some(record) = stream.next() {
                let bytes = record.map_err(|e| e.to_string())?;
                let delta = decode_delta(&bytes).map_err(|e| e.to_string())?;
                field.apply_delta(&delta);
            }
            if field.cells.is_empty() {
                field.add_root("liminal/root");
            }
            field.set_journal(journal.clone());
            (field, Some(journal))
        } else {
            let mut field = ClusterField::new();
            field.add_root("liminal/root");
            (field, None)
        };
        let field = Arc::new(Mutex::new(field));
        let outbox = Arc::new(StdMutex::new(Outbox::default()));
        let tick_ms = Arc::new(AtomicU32::new(config.tick_ms));

        let state = BridgeState {
            runtime,
            field: field.clone(),
            outbox: outbox.clone(),
            journal,
            snapshot_cfg,
        };

        state.start_life_loop(config.tick_ms, outbox.clone(), tick_ms);

        if let (Some(journal), Some(cfg)) = (state.journal.clone(), state.snapshot_cfg.clone()) {
            state.spawn_snapshot_loop(journal, cfg, outbox);
        }

        Ok(state)
    }

    fn start_life_loop(
        &self,
        initial_tick: u32,
        outbox: Arc<StdMutex<Outbox>>,
        tick_ms: Arc<AtomicU32>,
    ) {
        let metrics_box = outbox.clone();
        let events_box = outbox.clone();
        let hints_box = outbox.clone();
        let events_tick = tick_ms.clone();
        let hints_tick = tick_ms.clone();
        let field = self.field.clone();

        self.runtime.spawn(run_loop(
            field,
            initial_tick as u64,
            move |metrics| {
                let mut guard = metrics_box.lock().unwrap();
                let proto = ProtocolMetrics::from_core(metrics);
                let event = event_from_metrics(&proto, 1_000);
                guard.set_metrics(proto.clone());
                guard.push_event(event);
            },
            move |event| {
                if let Some(proto) = event_from_field_log(
                    event,
                    events_tick.load(std::sync::atomic::Ordering::Relaxed),
                ) {
                    let mut guard = events_box.lock().unwrap();
                    guard.push_event(proto);
                }
            },
            move |hint: &Hint| {
                let new_tick = adjust_tick(&hints_tick, hint);
                let event = event_from_hint(hint, new_tick);
                let mut guard = hints_box.lock().unwrap();
                guard.push_event(event);
            },
        ));
    }

    fn spawn_snapshot_loop(
        &self,
        journal: Arc<DiskJournal>,
        cfg: SnapshotConfig,
        outbox: Arc<StdMutex<Outbox>>,
    ) {
        let field = self.field.clone();
        self.runtime.spawn(async move {
            let mut ticker = tokio::time::interval(Duration::from_secs(5));
            let mut last_snapshot = tokio::time::Instant::now();
            loop {
                ticker.tick().await;
                if journal.delta_since_snapshot() >= cfg.max_wal_events
                    || last_snapshot.elapsed() >= cfg.interval
                {
                    let snapshot_result = {
                        let guard = field.lock().await;
                        journal.write_snapshot(&*guard)
                    };
                    match snapshot_result {
                        Ok(info) => {
                            last_snapshot = tokio::time::Instant::now();
                            if let Err(err) = journal.run_gc(info.offset) {
                                eprintln!("snapshot GC failed: {err}");
                            }
                            if let Ok(mut out) = outbox.lock() {
                                out.push_event(event_from_snapshot(info.id));
                            }
                        }
                        Err(err) => eprintln!("snapshot failed: {err}"),
                    }
                }
            }
        });
    }
}

#[no_mangle]
pub extern "C" fn liminal_init(cfg_cbor: *const u8, len: usize) -> bool {
    if cfg_cbor.is_null() || len == 0 {
        return false;
    }
    let bytes = unsafe { slice::from_raw_parts(cfg_cbor, len) };
    let config: BridgeConfig = match serde_cbor::from_slice(bytes) {
        Ok(cfg) => cfg,
        Err(_) => return false,
    };
    let state = match BridgeState::new(config) {
        Ok(state) => state,
        Err(_) => return false,
    };
    STATE.set(state).is_ok()
}

#[no_mangle]
pub extern "C" fn liminal_push(msg_cbor: *const u8, len: usize) -> usize {
    if msg_cbor.is_null() || len == 0 {
        return 0;
    }
    let bytes = unsafe { slice::from_raw_parts(msg_cbor, len) };
    let message: ProtocolPush = match serde_cbor::from_slice(bytes) {
        Ok(msg) => msg,
        Err(_) => return 0,
    };

    let Some(state) = STATE.get() else {
        return 0;
    };

    match message {
        ProtocolPush::Impulse(impulse) => {
            let core_impulse = impulse.to_core();
            let logs = state.runtime.block_on({
                let field = state.field.clone();
                async move {
                    let mut guard = field.lock().await;
                    guard.route_impulse(core_impulse)
                }
            });

            if let Ok(mut outbox) = state.outbox.lock() {
                for log in logs {
                    if let Some(event) = event_from_impulse_log(&log) {
                        outbox.push_event(event);
                    }
                }
            }
        }
        ProtocolPush::Command(command) => match command {
            ProtocolCommand::TrsSet { cfg } => {
                let event_string = state.runtime.block_on({
                    let field = state.field.clone();
                    async move {
                        let mut guard = field.lock().await;
                        guard.set_trs_config(cfg)
                    }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
                    if let Some(event) = event_from_field_log(&event_string, 1_000) {
                        outbox.push_event(event);
                    }
                }
            }
            ProtocolCommand::TrsTarget { value } => {
                let event_string = state.runtime.block_on({
                    let field = state.field.clone();
                    async move {
                        let mut guard = field.lock().await;
                        guard.set_trs_target(value)
                    }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
                    if let Some(event) = event_from_field_log(&event_string, 1_000) {
                        outbox.push_event(event);
                    }
                }
            }
            ProtocolCommand::Lql { query } => {
                let mut events: Vec<String> = Vec::new();
                match parse_lql(&query) {
                    Ok(ast) => {
                        let outcome = state.runtime.block_on({
                            let field = state.field.clone();
                            async move {
                                let mut guard = field.lock().await;
                                guard.exec_lql(ast)
                            }
                        });
                        match outcome {
                            Ok(result) => {
                                events.extend(result.events);
                            }
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
                    }
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
                if let Ok(mut outbox) = state.outbox.lock() {
                    for event_string in events {
                        if let Some(event) = event_from_field_log(&event_string, 1_000) {
                            outbox.push_event(event);
                        }
                    }
                }
            }
            ProtocolCommand::DreamNow => {
                let report_opt = state.runtime.block_on({
                    let field = state.field.clone();
                    async move { field.lock().await.run_dream_cycle() }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
                    match report_opt {
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
                            if let Some(event) = event_from_field_log(&summary, 1_000) {
                                outbox.push_event(event);
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
                            if let Some(event) = event_from_field_log(&payload, 1_000) {
                                outbox.push_event(event);
                            }
                        }
                        None => {
                            let msg = "DREAM no-op (not enough activity)";
                            if let Some(event) = event_from_field_log(msg, 1_000) {
                                outbox.push_event(event);
                            }
                        }
                    }
                }
            }
            ProtocolCommand::DreamSet { cfg } => {
                let updated = state.runtime.block_on({
                    let field = state.field.clone();
                    let cfg = cfg.clone();
                    async move {
                        let mut guard = field.lock().await;
                        guard.set_dream_config(cfg);
                        guard.dream_config()
                    }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
                    let payload = json!({
                        "ev": "dream_config",
                        "meta": {
                            "min_idle_s": updated.min_idle_s,
                            "window_ms": updated.window_ms,
                            "strengthen_top_pct": updated.strengthen_top_pct,
                            "weaken_bottom_pct": updated.weaken_bottom_pct,
                            "protect_salience": updated.protect_salience,
                            "adreno_protect": updated.adreno_protect,
                            "max_ops_per_cycle": updated.max_ops_per_cycle
                        }
                    })
                    .to_string();
                    if let Some(event) = event_from_field_log(&payload, 1_000) {
                        outbox.push_event(event);
                    }
                }
            }
            ProtocolCommand::DreamGet => {
                let cfg = state.runtime.block_on({
                    let field = state.field.clone();
                    async move { field.lock().await.dream_config() }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
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
                    if let Some(event) = event_from_field_log(&payload, 1_000) {
                        outbox.push_event(event);
                    }
                }
            }
            ProtocolCommand::SyncNow => {
                let report_opt = state.runtime.block_on({
                    let field = state.field.clone();
                    async move {
                        let mut guard = field.lock().await;
                        let cfg = guard.sync_config();
                        let now_ms = guard.now_ms;
                        let groups = detect_sync_groups(&*guard, &cfg, now_ms);
                        if groups.is_empty() {
                            None
                        } else {
                            Some(run_collective_dream(&mut *guard, &groups, &cfg, now_ms))
                        }
                    }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
                    match report_opt {
                        Some(report) => {
                            let summary = format!(
                                "COLLECTIVE_DREAM groups={} shared={} aligned={} protected={} took={}ms",
                                report.groups, report.shared, report.aligned, report.protected, report.took_ms
                            );
                            if let Some(event) = event_from_field_log(&summary, 1_000) {
                                outbox.push_event(event);
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
                            if let Some(event) = event_from_field_log(&payload, 1_000) {
                                outbox.push_event(event);
                            }
                        }
                        None => {
                            let msg = "COLLECTIVE_DREAM no-op (no qualifying groups)";
                            if let Some(event) = event_from_field_log(msg, 1_000) {
                                outbox.push_event(event);
                            }
                        }
                    }
                }
            }
            ProtocolCommand::SyncSet { cfg } => {
                let updated = state.runtime.block_on({
                    let field = state.field.clone();
                    let cfg = cfg.clone();
                    async move {
                        let mut guard = field.lock().await;
                        guard.set_sync_config(cfg);
                        guard.sync_config()
                    }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
                    let payload = json!({
                        "ev": "sync_config",
                        "meta": {
                            "phase_len_ms": updated.phase_len_ms,
                            "phase_gap_ms": updated.phase_gap_ms,
                            "cooccur_threshold": updated.cooccur_threshold,
                            "max_groups": updated.max_groups,
                            "share_top_k": updated.share_top_k,
                            "weight_xfer": updated.weight_xfer
                        }
                    })
                    .to_string();
                    if let Some(event) = event_from_field_log(&payload, 1_000) {
                        outbox.push_event(event);
                    }
                }
            }
            ProtocolCommand::SyncGet => {
                let cfg = state.runtime.block_on({
                    let field = state.field.clone();
                    async move { field.lock().await.sync_config() }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
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
                    if let Some(event) = event_from_field_log(&payload, 1_000) {
                        outbox.push_event(event);
                    }
                }
            }
            ProtocolCommand::AwakenSet { cfg } => {
                let event = state.runtime.block_on({
                    let field = state.field.clone();
                    let cfg = cfg.clone();
                    async move {
                        let mut guard = field.lock().await;
                        guard.set_awakening_config(cfg.clone());
                        awaken_config_event(&cfg, "set")
                    }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
                    push_json_event(&mut outbox, event);
                }
            }
            ProtocolCommand::AwakenGet => {
                let event = state.runtime.block_on({
                    let field = state.field.clone();
                    async move {
                        let cfg = field.lock().await.awakening_config();
                        awaken_config_event(&cfg, "cfg")
                    }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
                    push_json_event(&mut outbox, event);
                }
            }
            ProtocolCommand::Introspect(IntrospectRequest { target, top }) => {
                let event = state.runtime.block_on({
                    let field = state.field.clone();
                    async move {
                        let mut guard = field.lock().await;
                        build_introspect_event(&mut *guard, target, top)
                    }
                });
                if let Ok(mut outbox) = state.outbox.lock() {
                    push_json_event(&mut outbox, event);
                }
            }
        },
    }

    len
}

fn push_json_event(outbox: &mut Outbox, payload: JsonValue) {
    let serialized = payload.to_string();
    if let Some(event) = event_from_field_log(&serialized, 1_000) {
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

#[no_mangle]
pub extern "C" fn liminal_pull(out: *mut u8, cap: usize) -> usize {
    if out.is_null() || cap == 0 {
        return 0;
    }
    let Some(state) = STATE.get() else {
        return 0;
    };

    let mut guard = match state.outbox.lock() {
        Ok(guard) => guard,
        Err(_) => return 0,
    };

    let Some(package) = guard.take() else {
        return 0;
    };

    let encoded = match serde_cbor::to_vec(&package) {
        Ok(bytes) => bytes,
        Err(_) => {
            guard.restore(package);
            return 0;
        }
    };

    if encoded.len() > cap {
        guard.restore(package);
        return 0;
    }

    unsafe {
        std::ptr::copy_nonoverlapping(encoded.as_ptr(), out, encoded.len());
    }
    encoded.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{BridgeConfig, ProtocolImpulse, ProtocolPackage};

    #[test]
    fn ffi_flow() {
        let cfg = BridgeConfig {
            tick_ms: 200,
            store_path: None,
            snap_interval: None,
            snap_maxwal: None,
            ws_port: None,
            ws_format: None,
        };
        let cfg_bytes = serde_cbor::to_vec(&cfg).unwrap();
        assert!(liminal_init(cfg_bytes.as_ptr(), cfg_bytes.len()));

        let impulse = ProtocolImpulse {
            kind: 0,
            pattern: "cpu/load".into(),
            strength: 0.8,
            ttl_ms: 900,
            tags: vec!["test".into()],
        };
        let imp_bytes = serde_cbor::to_vec(&impulse).unwrap();
        assert_eq!(
            liminal_push(imp_bytes.as_ptr(), imp_bytes.len()),
            imp_bytes.len()
        );

        std::thread::sleep(std::time::Duration::from_millis(250));

        let mut buffer = vec![0u8; 1024];
        let written = liminal_pull(buffer.as_mut_ptr(), buffer.len());
        assert!(written > 0);
        let package: ProtocolPackage = serde_cbor::from_slice(&buffer[..written]).unwrap();
        assert!(package.metrics.is_some() || !package.events.is_empty());
    }
}
