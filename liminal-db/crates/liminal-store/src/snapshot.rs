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
        for snapshot in &self.cells {
            let cell = snapshot_to_node(snapshot);
            field.cells.insert(cell.id, cell);
        }
        field.now_ms = self.now_ms;
        field.next_id = self.next_id;
        field.rebuild_caches();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_round_trip() {
        let mut field = ClusterField::new();
        field.add_root("liminal/root");
        field.add_root("liminal/aux");
        field.tick_all(200);
        let before = field.cells.len();
        let bytes = create_snapshot(&field).expect("create snapshot");
        let seed = load_snapshot(&bytes).expect("load snapshot");
        let restored = seed.into_field();
        assert_eq!(restored.cells.len(), before);
        assert_eq!(restored.now_ms, field.now_ms);
        assert_eq!(restored.next_id, field.next_id);
    }
}
