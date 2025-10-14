use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rand::seq::SliceRandom;
use rand::Rng;

use crate::journal::{
    AffinityDelta, CellSnapshot, DivideDelta, EnergyDelta, EventDelta, Journal, LinkDelta,
    StateDelta, TickDelta,
};
use crate::node_cell::NodeCell;
use crate::seed::{create_seed, SeedParams};
use crate::types::{Impulse, NodeId, NodeState};

pub type FieldEvents = Vec<String>;

pub struct ClusterField {
    pub cells: HashMap<NodeId, NodeCell>,
    pub index: HashMap<String, Vec<NodeId>>,
    pub now_ms: u64,
    pub next_id: NodeId,
    journal: Option<Arc<dyn Journal + Send + Sync>>,
    ticks_since_impulse: u64,
}

impl ClusterField {
    pub fn new() -> Self {
        ClusterField {
            cells: HashMap::new(),
            index: HashMap::new(),
            now_ms: 0,
            next_id: 1,
            journal: None,
            ticks_since_impulse: 0,
        }
    }

    pub fn with_journal(mut self, journal: Arc<dyn Journal + Send + Sync>) -> Self {
        self.journal = Some(journal);
        self
    }

    pub fn set_journal(&mut self, journal: Arc<dyn Journal + Send + Sync>) {
        self.journal = Some(journal);
    }

    pub fn clear_journal(&mut self) {
        self.journal = None;
    }

    fn emit(&self, delta: EventDelta) {
        if let Some(journal) = &self.journal {
            journal.append_delta(&delta);
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
        if let Some(cell) = self.cells.get(&id) {
            self.emit(EventDelta::Spawn(CellSnapshot::from(cell)));
        }
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
                    let delta = EventDelta::Energy(EnergyDelta {
                        id,
                        energy: cell.energy,
                        metabolism: cell.metabolism,
                        state: cell.state,
                        last_response_ms: cell.last_response_ms,
                    });
                    logs.push(log);
                    self.emit(delta);
                }
            }
        }
        self.ticks_since_impulse = 0;
        logs
    }

    pub fn tick_all(&mut self, dt_ms: u64) -> FieldEvents {
        self.now_ms = self.now_ms.saturating_add(dt_ms);
        self.emit(EventDelta::Tick(TickDelta {
            now_ms: self.now_ms,
        }));
        let ids: Vec<NodeId> = self.cells.keys().copied().collect();
        let mut pending_events: Vec<EventDelta> = Vec::new();
        for id in &ids {
            if let Some(cell) = self.cells.get_mut(id) {
                cell.tick(dt_ms);
                pending_events.push(EventDelta::Energy(EnergyDelta {
                    id: *id,
                    energy: cell.energy,
                    metabolism: cell.metabolism,
                    state: cell.state,
                    last_response_ms: cell.last_response_ms,
                }));
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
                    child.seed.core_pattern = format!("{}:n{}", child.seed.core_pattern, new_id);
                    child.last_response_ms = self.now_ms;
                    events.push(format!(
                        "DIVIDE parent=n{} -> child=n{} (aff {:.2}->{:.2})",
                        id, new_id, parent_affinity, child_affinity
                    ));
                    pending_events.push(EventDelta::Affinity(AffinityDelta {
                        id: *id,
                        affinity: cell.affinity,
                    }));
                    pending_events.push(EventDelta::Energy(EnergyDelta {
                        id: *id,
                        energy: cell.energy,
                        metabolism: cell.metabolism,
                        state: cell.state,
                        last_response_ms: cell.last_response_ms,
                    }));
                    new_cells.push((*id, child));
                }
            }
        }
        for (parent, child) in new_cells {
            if let Some(parent_cell) = self.cells.get_mut(&parent) {
                if parent_cell.links.insert(child.id) {
                    pending_events.push(EventDelta::Link(LinkDelta {
                        from: parent,
                        to: child.id,
                    }));
                }
            }
            let snapshot = CellSnapshot::from(&child);
            self.attach_cell(child);
            pending_events.push(EventDelta::Divide(DivideDelta {
                parent,
                child: snapshot,
            }));
        }

        let mut needs_reindex = false;
        for id in ids {
            if let Some(cell) = self.cells.get_mut(&id) {
                let prev_state = cell.state;
                let died = cell.maybe_sleep_or_die(self.now_ms);
                if prev_state != NodeState::Sleep && cell.state == NodeState::Sleep {
                    events.push(format!("SLEEP n{}", id));
                    pending_events.push(EventDelta::Sleep(StateDelta { id }));
                }
                if died && prev_state != NodeState::Dead {
                    events.push(format!("DEAD n{}", id));
                    pending_events.push(EventDelta::Dead(StateDelta { id }));
                    needs_reindex = true;
                }
                pending_events.push(EventDelta::Energy(EnergyDelta {
                    id,
                    energy: cell.energy,
                    metabolism: cell.metabolism,
                    state: cell.state,
                    last_response_ms: cell.last_response_ms,
                }));
            }
        }
        let before_trim = self.cells.len();
        self.cells.retain(|_, cell| cell.state != NodeState::Dead);
        if self.cells.len() != before_trim {
            needs_reindex = true;
        }
        if needs_reindex {
            self.rebuild_index();
        }
        self.ticks_since_impulse = self.ticks_since_impulse.saturating_add(1);
        if self.ticks_since_impulse > 10 {
            self.dream();
            self.ticks_since_impulse = 0;
        }
        for event in pending_events {
            self.emit(event);
        }
        events
    }

    pub fn trim_low_energy(&mut self) {
        let before = self.cells.len();
        let victims: Vec<NodeId> = self
            .cells
            .iter()
            .filter_map(|(id, cell)| {
                if cell.energy <= 0.1 || cell.state == NodeState::Dead {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();
        self.cells
            .retain(|_, cell| cell.energy > 0.1 && cell.state != NodeState::Dead);
        if self.cells.len() != before {
            self.rebuild_index();
        }
        for id in victims {
            self.emit(EventDelta::Dead(StateDelta { id }));
        }
    }

    pub fn inject_seed_variation(&mut self, base_seed: &str) {
        if self.cells.len() > 64 {
            return;
        }
        let mut rng = rand::thread_rng();
        let params = create_seed(base_seed);
        let mut cell = NodeCell::from_seed(self.next_id, params.clone());
        cell.affinity = (params.affinity + rng.gen_range(-0.1..0.1)).clamp(0.0, 1.0);
        cell.seed.affinity = cell.affinity;
        cell.energy = 0.5 + rng.gen_range(0.0..0.3);
        cell.last_response_ms = self.now_ms;
        cell.seed.core_pattern = format!("{}:bud{}", params.core_pattern, self.next_id);
        let id = cell.id;
        self.next_id += 1;
        let snapshot = CellSnapshot::from(&cell);
        self.attach_cell(cell);
        self.emit(EventDelta::Spawn(snapshot));
        let mut link_events: Vec<EventDelta> = Vec::new();
        for other in self.cells.values_mut() {
            if other.id != id && rng.gen_bool(0.1) {
                if other.links.insert(id) {
                    link_events.push(EventDelta::Link(LinkDelta {
                        from: other.id,
                        to: id,
                    }));
                }
            }
        }
        for event in link_events {
            self.emit(event);
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

    fn dream(&mut self) {
        if self.cells.len() < 2 {
            return;
        }
        let mut rng = rand::thread_rng();
        let ids: Vec<NodeId> = self.cells.keys().copied().collect();
        for _ in 0..3 {
            let Some(&from) = ids.choose(&mut rng) else {
                continue;
            };
            let Some(&to) = ids.choose(&mut rng) else {
                continue;
            };
            if from == to {
                continue;
            }
            if let Some(cell) = self.cells.get_mut(&from) {
                if cell.links.contains(&to) {
                    cell.links.remove(&to);
                    self.emit(EventDelta::Unlink(LinkDelta { from, to }));
                } else if cell.links.len() < 8 {
                    cell.links.insert(to);
                    self.emit(EventDelta::Link(LinkDelta { from, to }));
                }
            }
        }
    }

    pub fn apply_delta(&mut self, delta: &EventDelta) {
        match delta {
            EventDelta::Tick(tick) => {
                self.now_ms = tick.now_ms;
                self.ticks_since_impulse = 0;
            }
            EventDelta::Spawn(snapshot) => {
                let cell = snapshot_to_node(snapshot);
                self.attach_cell(cell);
                self.next_id = self.next_id.max(snapshot.id + 1);
            }
            EventDelta::Divide(divide) => {
                let cell = snapshot_to_node(&divide.child);
                if let Some(parent) = self.cells.get_mut(&divide.parent) {
                    parent.links.insert(divide.child.id);
                }
                self.attach_cell(cell);
                self.next_id = self.next_id.max(divide.child.id + 1);
            }
            EventDelta::Sleep(state) => {
                if let Some(cell) = self.cells.get_mut(&state.id) {
                    cell.state = NodeState::Sleep;
                }
            }
            EventDelta::Dead(state) => {
                self.cells.remove(&state.id);
                self.rebuild_index();
            }
            EventDelta::Link(link) => {
                if let Some(cell) = self.cells.get_mut(&link.from) {
                    cell.links.insert(link.to);
                }
            }
            EventDelta::Unlink(link) => {
                if let Some(cell) = self.cells.get_mut(&link.from) {
                    cell.links.remove(&link.to);
                }
            }
            EventDelta::Affinity(delta) => {
                if let Some(cell) = self.cells.get_mut(&delta.id) {
                    cell.affinity = delta.affinity;
                    cell.seed.affinity = delta.affinity;
                }
            }
            EventDelta::Energy(delta) => {
                if let Some(cell) = self.cells.get_mut(&delta.id) {
                    cell.energy = delta.energy;
                    cell.metabolism = delta.metabolism;
                    cell.state = delta.state;
                    cell.last_response_ms = delta.last_response_ms;
                }
            }
        }
    }
}

fn tokenize(input: &str) -> Vec<String> {
    input
        .split(|c| matches!(c, '/' | ':' | '.'))
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
