use anyhow::Result;
use liminal_core::{CellSnapshot, ClusterField, NodeCell, ReflexRule, SeedParams, TrsState};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SnapshotEnvelope {
    now_ms: u64,
    next_id: u64,
    cells: Vec<CellSnapshot>,
    #[serde(default)]
    rules: Vec<ReflexRule>,
    #[serde(default)]
    next_reflex_id: u64,
    #[serde(default)]
    trs: Option<TrsState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterFieldSeed {
    pub now_ms: u64,
    pub next_id: u64,
    pub cells: Vec<CellSnapshot>,
    pub rules: Vec<ReflexRule>,
    pub next_reflex_id: u64,
    pub trs: TrsState,
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
        for rule in &self.rules {
            field.rules.insert(rule.id, rule.clone());
        }
        if self.next_reflex_id == 0 {
            field.next_reflex_id = field
                .rules
                .keys()
                .max()
                .copied()
                .unwrap_or(0)
                .saturating_add(1);
        } else {
            field.next_reflex_id = self.next_reflex_id;
        }
        field.trs = self.trs.clone();
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
        rules: cluster.list_reflex(),
        next_reflex_id: cluster.next_reflex_id,
        trs: Some(cluster.trs.clone()),
    };
    Ok(serde_cbor::to_vec(&envelope)?)
}

pub fn load_snapshot(bytes: &[u8]) -> Result<ClusterFieldSeed> {
    let envelope: SnapshotEnvelope = serde_cbor::from_slice(bytes)?;
    Ok(ClusterFieldSeed {
        now_ms: envelope.now_ms,
        next_id: envelope.next_id,
        cells: envelope.cells,
        rules: envelope.rules,
        next_reflex_id: envelope.next_reflex_id,
        trs: envelope.trs.unwrap_or_default(),
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
    use liminal_core::types::ImpulseKind;
    use liminal_core::{ReflexAction, ReflexWhen};

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

    #[test]
    fn reflex_rules_round_trip() {
        let mut field = ClusterField::new();
        field.add_root("liminal/reflex");
        field.add_reflex(ReflexRule {
            id: 0,
            when: ReflexWhen {
                token: "cpu/load".into(),
                kind: ImpulseKind::Affect,
                min_strength: 0.6,
                window_ms: 800,
                min_count: 3,
            },
            then: ReflexAction::WakeSleeping { count: 1 },
            enabled: true,
        });
        let bytes = create_snapshot(&field).expect("create snapshot");
        let seed = load_snapshot(&bytes).expect("load snapshot");
        assert_eq!(seed.rules.len(), 1);
        let restored = seed.into_field();
        let restored_rules = restored.list_reflex();
        assert_eq!(restored_rules.len(), 1);
        assert_eq!(restored_rules[0].when.kind, ImpulseKind::Affect);
        assert_eq!(restored_rules[0].when.token, "cpu/load");
        assert_eq!(restored.next_reflex_id, field.next_reflex_id);
    }
}
