use serde::{Deserialize, Serialize};

use crate::types::{Hint, ImpulseKind};

pub type ReflexId = u64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflexWhen {
    pub token: String,
    pub kind: ImpulseKind,
    pub min_strength: f32,
    pub window_ms: u32,
    pub min_count: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReflexAction {
    EmitHint { hint: Hint },
    SpawnSeed { seed: String, affinity_shift: f32 },
    WakeSleeping { count: u16 },
    BoostLinks { factor: f32, top: u16 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflexRule {
    pub id: ReflexId,
    pub when: ReflexWhen,
    pub then: ReflexAction,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflexFire {
    pub id: ReflexId,
    pub at_ms: u64,
    pub matched: u16,
}
