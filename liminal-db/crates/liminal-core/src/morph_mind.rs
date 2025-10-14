use rand::Rng;

use crate::cluster_field::ClusterField;
use crate::types::{Hint, Metrics, NodeState};

pub fn analyze(field: &ClusterField) -> Metrics {
    let snapshot = field.metrics_snapshot();
    if snapshot.is_empty() {
        return Metrics::default();
    }
    let cells = snapshot.len();
    let mut sleeping = 0usize;
    let mut active = 0usize;
    let mut metabolism_sum = 0.0f32;
    let mut latency_sum = 0.0f32;
    for (_id, metabolism, state, latency_ms, _energy) in snapshot {
        if matches!(state, NodeState::Sleep) {
            sleeping += 1;
        }
        if matches!(state, NodeState::Active) {
            active += 1;
        }
        metabolism_sum += metabolism;
        latency_sum += latency_ms as f32;
    }
    let sleeping_pct = sleeping as f32 / cells as f32;
    let active_pct = active as f32 / cells as f32;
    let avg_metabolism = metabolism_sum / cells as f32;
    let avg_latency_ms = latency_sum / cells as f32;
    let latency_norm = (avg_latency_ms / 320.0).clamp(0.0, 1.0);
    let metabolism_norm = avg_metabolism.clamp(0.0, 1.0);
    let vitality = (1.0 - sleeping_pct).clamp(0.0, 1.0);
    let live_load = ((active_pct) + (1.0 - latency_norm) + metabolism_norm + vitality) / 4.0;
    Metrics {
        cells,
        sleeping_pct,
        avg_metabolism,
        avg_latency_ms,
        active_pct,
        live_load: live_load.clamp(0.0, 1.0),
    }
}

pub fn hints(metrics: &Metrics) -> Vec<Hint> {
    let mut advice = Vec::new();
    if metrics.sleeping_pct > 0.7 {
        advice.push(Hint::SlowTick);
    }
    if metrics.avg_metabolism > 0.8 {
        advice.push(Hint::TrimField);
    }
    if metrics.avg_latency_ms > 150.0 {
        advice.push(Hint::FastTick);
    }
    let mut rng = rand::thread_rng();
    if metrics.cells < 3 {
        if rng.gen_bool(0.6) {
            advice.push(Hint::WakeSeeds);
        }
    } else if metrics.sleeping_pct > 0.6 && rng.gen_bool(0.25) {
        advice.push(Hint::WakeSeeds);
    }
    advice
}
