use std::collections::HashSet;

use rand::{rngs::SmallRng, Rng, SeedableRng};

use crate::{
    seed::SeedParams,
    types::{Impulse, ImpulseKind, NodeId, NodeState},
};

#[derive(Debug, Clone)]
pub struct NodeCell {
    pub id: NodeId,
    pub seed: SeedParams,
    pub state: NodeState,
    pub links: HashSet<NodeId>,
    pub metabolism: f32,
    pub affinity: f32,
    pub last_response_ms: u64,
    pub energy: f32,
}

impl NodeCell {
    pub fn from_seed(id: NodeId, seed: SeedParams) -> Self {
        NodeCell {
            id,
            affinity: seed.affinity,
            metabolism: seed.base_metabolism,
            state: NodeState::Active,
            links: HashSet::new(),
            last_response_ms: 0,
            energy: 0.6,
            seed,
        }
    }

    pub fn ingest(&mut self, imp: &Impulse) -> Option<String> {
        if self.state == NodeState::Dead {
            return None;
        }
        let (delta, response) = match imp.kind {
            ImpulseKind::Query => {
                if self.state == NodeState::Sleep && imp.strength > 0.6 {
                    self.state = NodeState::Idle;
                }
                (
                    -0.05 * imp.strength,
                    format!("n{} echoes '{}'?", self.id, imp.pattern),
                )
            }
            ImpulseKind::Write => {
                self.state = NodeState::Active;
                (
                    0.08 * imp.strength,
                    format!("n{} records '{}'.", self.id, imp.pattern),
                )
            }
            ImpulseKind::Affect => {
                if imp.strength > 0.8 {
                    self.state = NodeState::Active;
                } else if imp.strength < 0.3 {
                    self.state = NodeState::Idle;
                }
                (
                    0.05 * (imp.strength - 0.4),
                    format!("n{} senses '{}'.", self.id, imp.pattern),
                )
            }
        };
        self.energy = (self.energy + delta).clamp(0.0, 1.2);
        Some(response)
    }

    pub fn tick(&mut self, dt_ms: u64) {
        if self.state == NodeState::Dead {
            return;
        }
        let decay = self.metabolism * dt_ms as f32 * 0.0003;
        self.energy = (self.energy - decay).clamp(0.0, 1.0);
        let target = self.seed.base_metabolism;
        self.metabolism = self.metabolism + (target - self.metabolism) * 0.05;
        if self.energy < 0.2 && self.state == NodeState::Active {
            self.state = NodeState::Idle;
        }
        if self.energy > 0.6 && self.state == NodeState::Idle {
            self.state = NodeState::Active;
        }
        if self.state == NodeState::Sleep && self.energy > 0.5 {
            self.state = NodeState::Idle;
        }
    }

    pub fn maybe_divide(&mut self) -> Option<NodeCell> {
        if self.state != NodeState::Active || self.energy <= 0.8 {
            return None;
        }
        let mut rng = SmallRng::from_entropy();
        let child_seed = self.seed.clone();
        let variation = rng.gen_range(-0.05..0.05);
        let child_affinity = (self.affinity + variation).clamp(0.0, 1.0);
        let child_metabolism = (self.metabolism + child_seed.base_metabolism) / 2.0;
        self.energy *= 0.55;
        let mut child = NodeCell {
            id: 0,
            seed: child_seed,
            state: NodeState::Idle,
            links: HashSet::new(),
            metabolism: child_metabolism,
            affinity: child_affinity,
            last_response_ms: self.last_response_ms,
            energy: 0.45,
        };
        child.seed.affinity = child_affinity;
        Some(child)
    }

    pub fn maybe_sleep_or_die(&mut self, now_ms: u64) -> bool {
        if self.state == NodeState::Dead {
            return true;
        }
        let idle_for = now_ms.saturating_sub(self.last_response_ms);
        if self.energy < 0.15 && idle_for > 3_000 {
            self.state = NodeState::Sleep;
        }
        if self.energy < 0.05 && idle_for > 6_000 {
            self.state = NodeState::Dead;
            return true;
        }
        false
    }
}
