use std::collections::HashMap;

use anyhow::Result;
use liminal_core::{CellSnapshot, ClusterField, NodeCell, SeedParams};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotEnvelope {
    now_ms: u64,
    next_id: u64,
    cells: Vec<CellSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterFieldSeed {
    pub now_ms: u64,
    pub next_id: u64,
    pub cells: Vec<CellSnapshot>,
}

impl ClusterFieldSeed {
    pub fn into_field(self) -> ClusterField {
        let mut field = ClusterField::new();
        let mut cells = HashMap::new();
        for snapshot in &self.cells {
            let cell = snapshot_to_node(snapshot);
            cells.insert(cell.id, cell);
        }
        let index = rebuild_index(&cells);
        field.cells = cells;
        field.index = index;
        field.now_ms = self.now_ms;
        field.next_id = self.next_id;
        field
    }
}

pub fn create_snapshot(cluster: &ClusterField) -> Result<Vec<u8>> {
    let cells = cluster
        .cells
        .values()
        .map(CellSnapshot::from)
        .collect::<Vec<_>>();
    let envelope = SnapshotEnvelope {
        now_ms: cluster.now_ms,
        next_id: cluster.next_id,
        cells,
    };
    Ok(serde_cbor::to_vec(&envelope)?)
}

pub fn load_snapshot(bytes: &[u8]) -> Result<ClusterFieldSeed> {
    let envelope: SnapshotEnvelope = serde_cbor::from_slice(bytes)?;
    Ok(ClusterFieldSeed {
        now_ms: envelope.now_ms,
        next_id: envelope.next_id,
        cells: envelope.cells,
    })
}

fn snapshot_to_node(snapshot: &CellSnapshot) -> NodeCell {
    let mut cell = NodeCell::from_seed(
        snapshot.id,
        SeedParams {
            affinity: snapshot.seed_affinity,
            base_metabolism: snapshot.seed_metabolism,
            core_pattern: snapshot.core_pattern.clone(),
        },
    );
    cell.links = snapshot.links.iter().copied().collect();
    cell.metabolism = snapshot.metabolism;
    cell.affinity = snapshot.affinity;
    cell.last_response_ms = snapshot.last_response_ms;
    cell.energy = snapshot.energy;
    cell.state = snapshot.state;
    cell
}

fn rebuild_index(cells: &HashMap<u64, NodeCell>) -> HashMap<String, Vec<u64>> {
    let mut index: HashMap<String, Vec<u64>> = HashMap::new();
    for (id, cell) in cells {
        let tokens = tokenize(&cell.seed.core_pattern);
        for token in tokens {
            index.entry(token).or_default().push(*id);
        }
    }
    index
}

fn tokenize(input: &str) -> Vec<String> {
    input
        .split(|c| matches!(c, '/' | ':' | '.'))
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect()
}
