use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::cluster_field::ClusterField;
use crate::security::NsId;
use crate::seeds_metrics::{evaluate, score, SeedTelemetry};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SeedKind {
    BoostToken,
    CooldownToken,
    SpawnCluster,
    ViewWatch,
    PolicyBundle,
    Script,
}

impl SeedKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SeedKind::BoostToken => "BoostToken",
            SeedKind::CooldownToken => "CooldownToken",
            SeedKind::SpawnCluster => "SpawnCluster",
            SeedKind::ViewWatch => "ViewWatch",
            SeedKind::PolicyBundle => "PolicyBundle",
            SeedKind::Script => "Script",
        }
    }
}

impl std::str::FromStr for SeedKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "BoostToken" => Ok(SeedKind::BoostToken),
            "CooldownToken" => Ok(SeedKind::CooldownToken),
            "SpawnCluster" => Ok(SeedKind::SpawnCluster),
            "ViewWatch" => Ok(SeedKind::ViewWatch),
            "PolicyBundle" => Ok(SeedKind::PolicyBundle),
            "Script" => Ok(SeedKind::Script),
            other => Err(format!("unknown seed kind: {other}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SeedStage {
    Planted,
    Sprouting,
    Blooming,
    Harvested,
    Aborted,
}

impl SeedStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            SeedStage::Planted => "Planted",
            SeedStage::Sprouting => "Sprouting",
            SeedStage::Blooming => "Blooming",
            SeedStage::Harvested => "Harvested",
            SeedStage::Aborted => "Aborted",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Seed {
    pub id: u64,
    pub ts_planted: u64,
    pub kind: SeedKind,
    pub target: String,
    #[serde(default)]
    pub args: Map<String, Value>,
    pub stage: SeedStage,
    pub ttl_ms: u32,
    pub vitality: f32,
    pub mutation: f32,
    pub yield_score: f32,
    pub last_tick_ms: u64,
    pub ns: NsId,
}

impl Seed {
    pub fn new(
        kind: SeedKind,
        target: String,
        args: Map<String, Value>,
        ttl_ms: u32,
        ns: NsId,
        now: u64,
    ) -> Self {
        Seed {
            id: 0,
            ts_planted: now,
            kind,
            target,
            args,
            stage: SeedStage::Planted,
            ttl_ms,
            vitality: 1.0,
            mutation: 0.0,
            yield_score: 0.0,
            last_tick_ms: now,
            ns,
        }
    }

    pub fn lifetime_ms(&self, now: u64) -> u64 {
        now.saturating_sub(self.ts_planted)
    }

    pub fn ttl_remaining(&self, now: u64) -> u64 {
        let elapsed = self.lifetime_ms(now);
        let ttl = self.ttl_ms as u64;
        if elapsed >= ttl {
            0
        } else {
            ttl - elapsed
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedLog {
    pub id: u64,
    pub stage: SeedStage,
    pub vitality: f32,
    pub yield_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeedGarden {
    #[serde(default)]
    pub seeds: HashMap<u64, Seed>,
    #[serde(default = "SeedGarden::default_next_id")]
    pub next_id: u64,
    #[serde(default)]
    pub recent: Vec<SeedLog>,
}

impl Default for SeedGarden {
    fn default() -> Self {
        SeedGarden {
            seeds: HashMap::new(),
            next_id: 1,
            recent: Vec::new(),
        }
    }
}

impl SeedGarden {
    fn default_next_id() -> u64 {
        1
    }

    pub fn logs(&self) -> &[SeedLog] {
        &self.recent
    }

    pub fn clear_logs(&mut self) {
        self.recent.clear();
    }
}

pub fn plant(garden: &mut SeedGarden, mut seed: Seed) -> u64 {
    let id = if seed.id == 0 {
        garden.next_id
    } else {
        seed.id
    };
    garden.next_id = garden.next_id.max(id.saturating_add(1));
    seed.id = id;
    garden.seeds.insert(id, seed);
    id
}

pub fn abort(garden: &mut SeedGarden, id: u64) -> Option<Seed> {
    if let Some(seed) = garden.seeds.get_mut(&id) {
        seed.stage = SeedStage::Aborted;
        garden.recent.push(SeedLog {
            id,
            stage: SeedStage::Aborted,
            vitality: seed.vitality,
            yield_score: seed.yield_score,
        });
        return Some(seed.clone());
    }
    None
}

pub fn tick_garden(field: &mut ClusterField, garden: &mut SeedGarden, now: u64) -> Vec<String> {
    let mut logs = Vec::new();
    garden.recent.clear();
    let ids: Vec<u64> = garden.seeds.keys().copied().collect();
    for id in ids {
        if let Some(seed) = garden.seeds.get_mut(&id) {
            if matches!(seed.stage, SeedStage::Harvested | SeedStage::Aborted) {
                continue;
            }
            let elapsed = seed.lifetime_ms(now);
            if matches!(seed.stage, SeedStage::Planted) && elapsed >= 1_000 {
                seed.stage = SeedStage::Sprouting;
            }
            if matches!(seed.stage, SeedStage::Sprouting) && elapsed >= 10_000 {
                seed.stage = SeedStage::Blooming;
            }
            let telemetry: SeedTelemetry = evaluate(field, seed);
            let new_score = score(&telemetry);
            // Smooth the yield/vitality updates so they remain stable across ticks.
            seed.yield_score = 0.6 * seed.yield_score + 0.4 * new_score;
            let vitality_delta = telemetry.salience_gain - telemetry.errors as f32 * 0.05;
            seed.vitality = (seed.vitality + vitality_delta).clamp(0.0, 1.0);
            seed.last_tick_ms = now;
            let ttl = seed.ttl_ms as u64;
            if elapsed >= ttl {
                if seed.yield_score >= 0.0 {
                    seed.stage = SeedStage::Harvested;
                } else {
                    seed.stage = SeedStage::Aborted;
                }
            }
            garden.recent.push(SeedLog {
                id,
                stage: seed.stage.clone(),
                vitality: seed.vitality,
                yield_score: seed.yield_score,
            });
            logs.push(format!(
                "SEED id={} stage={} vitality={:.3} yield={:.3}",
                id,
                seed.stage.as_str(),
                seed.vitality,
                seed.yield_score
            ));
        }
    }
    logs
}
