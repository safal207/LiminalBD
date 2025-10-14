use serde::{Deserialize, Serialize};

pub type NodeId = u64;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum ImpulseKind {
    Query,
    Write,
    Affect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Impulse {
    pub kind: ImpulseKind,
    pub pattern: String,
    pub strength: f32,
    pub ttl_ms: u64,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum NodeState {
    Active,
    Idle,
    Sleep,
    Dead,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Metrics {
    pub cells: usize,
    pub sleeping_pct: f32,
    pub avg_metabolism: f32,
    pub avg_latency_ms: f32,
    pub active_pct: f32,
    pub live_load: f32,
}

impl Metrics {
    pub fn observed_load(&self) -> f32 {
        self.live_load
    }

    pub fn suggest_target(&self) -> f32 {
        let mut target = self.live_load;
        if target < 0.55 {
            target = (target + 0.55) * 0.5;
        }
        if target > 0.7 {
            target = (target + 0.7) * 0.5;
        }
        target.clamp(0.3, 0.8)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Hint {
    SlowTick,
    FastTick,
    TrimField,
    WakeSeeds,
}
