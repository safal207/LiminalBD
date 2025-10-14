use std::collections::{HashMap, HashSet};

use rand::Rng;

use crate::node_cell::NodeCell;
use crate::seed::create_seed;
use crate::types::{Impulse, NodeId, NodeState};

pub type FieldEvents = Vec<String>;

pub struct ClusterField {
    pub cells: HashMap<NodeId, NodeCell>,
    pub index: HashMap<String, Vec<NodeId>>,
    pub now_ms: u64,
    pub next_id: NodeId,
}

impl ClusterField {
    pub fn new() -> Self {
        ClusterField {
            cells: HashMap::new(),
            index: HashMap::new(),
            now_ms: 0,
            next_id: 1,
        }
    }

    fn attach_cell(&mut self, cell: NodeCell) {
        let id = cell.id;
        self.cells.insert(id, cell);
        self.update_index_for(id);
    }

    fn update_index_for(&mut self, id: NodeId) {
        let Some(cell) = self.cells.get(&id) else {
            return;
        };
        for ids in self.index.values_mut() {
            ids.retain(|existing| *existing != id);
        }
        for token in tokenize(&cell.seed.core_pattern) {
            self.index.entry(token).or_default().push(id);
        }
    }

    fn rebuild_index(&mut self) {
        self.index.clear();
        for id in self.cells.keys().copied().collect::<Vec<_>>() {
            self.update_index_for(id);
        }
    }

    pub fn add_root(&mut self, seed: &str) -> NodeId {
        let params = create_seed(seed);
        let id = self.next_id;
        self.next_id += 1;
        let cell = NodeCell::from_seed(id, params);
        self.attach_cell(cell);
        id
    }

    pub fn route_impulse(&mut self, imp: Impulse) -> Vec<String> {
        let tokens = tokenize(&imp.pattern);
        let mut candidates: HashSet<NodeId> = HashSet::new();
        for token in &tokens {
            if let Some(ids) = self.index.get(token) {
                candidates.extend(ids.iter().copied());
            }
        }
        if candidates.is_empty() {
            candidates.extend(self.cells.keys().copied());
        }
        let mut scored: Vec<(f32, NodeId)> = candidates
            .into_iter()
            .filter_map(|id| self.cells.get(&id).map(|cell| (score_cell(cell, &imp), id)))
            .collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut logs = Vec::new();
        for (_score, id) in scored.into_iter().take(3) {
            if let Some(cell) = self.cells.get_mut(&id) {
                if let Some(log) = cell.ingest(&imp) {
                    cell.last_response_ms = self.now_ms;
                    logs.push(log);
                }
            }
        }
        logs
    }

    pub fn tick_all(&mut self, dt_ms: u64) -> FieldEvents {
        self.now_ms = self.now_ms.saturating_add(dt_ms);
        let ids: Vec<NodeId> = self.cells.keys().copied().collect();
        for id in &ids {
            if let Some(cell) = self.cells.get_mut(id) {
                cell.tick(dt_ms);
            }
        }
        let mut events: FieldEvents = Vec::new();
        let mut new_cells: Vec<(NodeId, NodeCell)> = Vec::new();
        for id in &ids {
            if let Some(cell) = self.cells.get_mut(id) {
                let parent_affinity = cell.affinity;
                if let Some(mut child) = cell.maybe_divide() {
                    let child_affinity = child.affinity;
                    let new_id = self.next_id;
                    self.next_id += 1;
                    child.id = new_id;
                    child.last_response_ms = self.now_ms;
                    events.push(format!(
                        "DIVIDE parent=n{} -> child=n{} (aff {:.2}->{:.2})",
                        id, new_id, parent_affinity, child_affinity
                    ));
                    new_cells.push((*id, child));
                }
            }
        }
        for (parent, child) in new_cells {
            if let Some(parent_cell) = self.cells.get_mut(&parent) {
                parent_cell.links.insert(child.id);
            }
            self.attach_cell(child);
        }

        let mut needs_reindex = false;
        for id in ids {
            if let Some(cell) = self.cells.get_mut(&id) {
                let prev_state = cell.state;
                let died = cell.maybe_sleep_or_die(self.now_ms);
                if prev_state != NodeState::Sleep && cell.state == NodeState::Sleep {
                    events.push(format!("SLEEP n{}", id));
                }
                if died && prev_state != NodeState::Dead {
                    events.push(format!("DEAD n{}", id));
                    needs_reindex = true;
                }
            }
        }
        self.cells.retain(|_, cell| cell.state != NodeState::Dead);
        if needs_reindex {
            self.rebuild_index();
        }
        events
    }

    pub fn trim_low_energy(&mut self) {
        let before = self.cells.len();
        self.cells
            .retain(|_, cell| cell.energy > 0.1 && cell.state != NodeState::Dead);
        if self.cells.len() != before {
            self.rebuild_index();
        }
    }

    pub fn inject_seed_variation(&mut self, base_seed: &str) {
        let mut rng = rand::thread_rng();
        let params = create_seed(base_seed);
        let mut cell = NodeCell::from_seed(self.next_id, params.clone());
        cell.affinity = (params.affinity + rng.gen_range(-0.1..0.1)).clamp(0.0, 1.0);
        cell.energy = 0.5 + rng.gen_range(0.0..0.3);
        cell.last_response_ms = self.now_ms;
        let id = cell.id;
        self.next_id += 1;
        self.attach_cell(cell);
        for other in self.cells.values_mut() {
            if other.id != id && rng.gen_bool(0.1) {
                other.links.insert(id);
            }
        }
    }

    pub fn metrics_snapshot(&self) -> Vec<(NodeId, f32, NodeState, u64, f32)> {
        self.cells
            .values()
            .map(|c| {
                (
                    c.id,
                    c.metabolism,
                    c.state,
                    self.now_ms.saturating_sub(c.last_response_ms),
                    c.energy,
                )
            })
            .collect()
    }
}

fn tokenize(input: &str) -> Vec<String> {
    input
        .split(|c: char| !c.is_alphanumeric() && c != '/' && c != '_')
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect()
}

fn score_cell(cell: &NodeCell, imp: &Impulse) -> f32 {
    let affinity_score = 1.0 - (cell.affinity - imp.strength).abs();
    let state_bonus = match cell.state {
        NodeState::Active => 0.2,
        NodeState::Idle => 0.0,
        NodeState::Sleep => -0.4,
        NodeState::Dead => -1.0,
    };
    affinity_score + state_bonus
}
