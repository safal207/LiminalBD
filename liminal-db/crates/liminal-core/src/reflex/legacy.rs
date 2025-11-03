use std::fs::OpenOptions;
use std::io::Write;

use serde::{Deserialize, Serialize};

use crate::cluster_field::Hit;
use crate::symmetry::{HarmonySnapshot, MirrorImpulse, SymmetryLoop};
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

#[derive(Debug, Clone)]
pub struct ReflexEngine {
    symmetry: SymmetryLoop,
    interval_ms: u64,
    accum_ms: u64,
}

impl ReflexEngine {
    pub fn new(interval_ms: u64) -> Self {
        ReflexEngine {
            symmetry: SymmetryLoop::new(0.85, interval_ms as u32),
            interval_ms: interval_ms.max(200),
            accum_ms: 0,
        }
    }

    pub fn with_symmetry(symmetry: SymmetryLoop, interval_ms: u64) -> Self {
        ReflexEngine {
            symmetry,
            interval_ms: interval_ms.max(200),
            accum_ms: 0,
        }
    }

    pub fn tick(
        &mut self,
        now_ms: u64,
        dt_ms: u64,
        samples: &[(String, Hit)],
    ) -> Option<HarmonySnapshot> {
        self.accum_ms = self.accum_ms.saturating_add(dt_ms);
        if self.accum_ms < self.interval_ms {
            return None;
        }
        self.accum_ms = 0;
        let mirror = self.symmetry.update_with_samples(now_ms, samples);
        let mut snapshot = self.symmetry.snapshot();
        if let Some(ref signal) = mirror {
            self.log_mirror(&snapshot, signal);
        }
        snapshot.mirror = mirror;
        Some(snapshot)
    }

    fn log_mirror(&self, snapshot: &HarmonySnapshot, mirror: &MirrorImpulse) {
        if let Ok(mut file) = OpenOptions::new()
            .create(true)
            .append(true)
            .open("reflex.log")
        {
            let pattern = snapshot
                .dominant_pattern
                .as_deref()
                .unwrap_or(mirror.pattern.as_str());
            let _ = writeln!(
                file,
                "[mirror] t={} pattern={} delta_str={:.3} delta_lat={:.3} entropy={:.3} status={} strength={:.3}",
                mirror.timestamp_ms,
                pattern,
                snapshot.delta_strength,
                snapshot.delta_latency,
                snapshot.entropy_ratio,
                snapshot.status.as_str(),
                mirror.strength,
            );
        }
    }

    pub fn snapshot(&self) -> HarmonySnapshot {
        self.symmetry.snapshot()
    }

    pub fn set_interval(&mut self, interval_ms: u64) {
        let interval = interval_ms.max(200);
        self.interval_ms = interval;
        self.symmetry.set_window(interval as u32);
    }

    pub fn interval(&self) -> u64 {
        self.interval_ms
    }

    pub fn window(&self) -> u32 {
        self.symmetry.window()
    }
}
