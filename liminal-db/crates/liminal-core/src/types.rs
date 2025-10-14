use serde::{Deserialize, Serialize};

pub type NodeId = u64;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Hint {
    SlowTick,
    FastTick,
    TrimField,
    WakeSeeds,
}
