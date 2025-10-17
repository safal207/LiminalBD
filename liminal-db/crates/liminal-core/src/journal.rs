use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::reflex::{ReflexFire, ReflexId, ReflexRule};
use crate::trs::TrsConfig;
use crate::types::{NodeId, NodeState};

pub trait Journal: Send + Sync {
    fn append_delta(&self, delta: &EventDelta);
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload")]
pub enum EventDelta {
    #[serde(rename = "tick")]
    Tick(TickDelta),
    #[serde(rename = "spawn")]
    Spawn(CellSnapshot),
    #[serde(rename = "divide")]
    Divide(DivideDelta),
    #[serde(rename = "sleep")]
    Sleep(StateDelta),
    #[serde(rename = "dead")]
    Dead(StateDelta),
    #[serde(rename = "link")]
    Link(LinkDelta),
    #[serde(rename = "unlink")]
    Unlink(LinkDelta),
    #[serde(rename = "affinity")]
    Affinity(AffinityDelta),
    #[serde(rename = "energy")]
    Energy(EnergyDelta),
    #[serde(rename = "reflex_add")]
    ReflexAdd(ReflexRule),
    #[serde(rename = "reflex_remove")]
    ReflexRemove { id: ReflexId },
    #[serde(rename = "reflex_fire")]
    ReflexFire(ReflexFire),
    #[serde(rename = "trs_set")]
    TrsSet(TrsConfig),
    #[serde(rename = "trs_target")]
    TrsTarget(TrsTargetDelta),
    #[serde(rename = "trs_trace")]
    TrsTrace(TrsTraceDelta),
    #[serde(rename = "trs_harmony")]
    TrsHarmony(TrsHarmonyDelta),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickDelta {
    pub now_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CellSnapshot {
    pub id: NodeId,
    pub seed_affinity: f32,
    pub seed_metabolism: f32,
    pub core_pattern: String,
    pub state: NodeState,
    pub links: BTreeSet<NodeId>,
    pub metabolism: f32,
    pub affinity: f32,
    pub last_response_ms: u64,
    pub energy: f32,
    #[serde(default)]
    pub salience: f32,
    #[serde(default)]
    pub adreno_tag: bool,
    #[serde(default)]
    pub last_recall_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DivideDelta {
    pub parent: NodeId,
    pub child: CellSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateDelta {
    pub id: NodeId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkDelta {
    pub from: NodeId,
    pub to: NodeId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AffinityDelta {
    pub id: NodeId,
    pub affinity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnergyDelta {
    pub id: NodeId,
    pub energy: f32,
    pub metabolism: f32,
    pub state: NodeState,
    pub last_response_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrsTargetDelta {
    pub target_load: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrsTraceDelta {
    pub now_ms: u64,
    pub observed_load: f32,
    pub error: f32,
    pub tick_adjust_ms: i32,
    pub alpha: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrsHarmonyDelta {
    pub alpha: f32,
    pub affinity_scale: f32,
    pub metabolism_scale: f32,
    pub sleep_delta: f32,
}

impl From<&crate::node_cell::NodeCell> for CellSnapshot {
    fn from(cell: &crate::node_cell::NodeCell) -> Self {
        CellSnapshot {
            id: cell.id,
            seed_affinity: cell.seed.affinity,
            seed_metabolism: cell.seed.base_metabolism,
            core_pattern: cell.seed.core_pattern.clone(),
            state: cell.state,
            links: cell.links.iter().copied().collect(),
            metabolism: cell.metabolism,
            affinity: cell.affinity,
            last_response_ms: cell.last_response_ms,
            energy: cell.energy,
            salience: cell.salience,
            adreno_tag: cell.adreno_tag,
            last_recall_ms: cell.last_recall_ms,
        }
    }
}
