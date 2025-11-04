use serde::{Deserialize, Serialize};

use crate::cluster_field::ClusterField;
use crate::seeds::Seed;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SeedTelemetry {
    pub hits: u64,
    pub avg_latency: f32,
    pub errors: u32,
    pub energy_delta: f32,
    pub salience_gain: f32,
}

pub fn evaluate(_field: &ClusterField, _seed: &Seed) -> SeedTelemetry {
    // Placeholder implementation: in v1.3 we will surface real telemetry.
    // For now, keep the structure wired so seeds can evolve alongside
    // other subsystems without panicking.
    SeedTelemetry {
        hits: 0,
        avg_latency: 0.0,
        errors: 0,
        energy_delta: 0.0,
        salience_gain: 0.0,
    }
}

pub fn score(telemetry: &SeedTelemetry) -> f32 {
    let latency_penalty = if telemetry.avg_latency <= 0.0 {
        0.0
    } else {
        -telemetry.avg_latency
    };
    let hit_reward = telemetry.hits as f32 * 0.5;
    let salience_reward = telemetry.salience_gain;
    let error_penalty = telemetry.errors as f32 * 1.5;
    let energy_bonus = telemetry.energy_delta;
    latency_penalty + hit_reward + salience_reward + energy_bonus - error_penalty
}
