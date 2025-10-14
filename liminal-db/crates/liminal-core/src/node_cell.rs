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

        if !pattern_match(
            self.affinity,
            &self.seed.core_pattern,
            &imp.pattern,
            imp.strength,
            &imp.tags,
        ) {
            self.energy = (self.energy - 0.01).clamp(0.0, 1.0);
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
        self.energy = (self.energy + delta).clamp(0.0, 1.0);
        Some(response)
    }

    pub fn tick(&mut self, dt_ms: u64, metabolism_scale: f32, sleep_shift: f32) {
        if self.state == NodeState::Dead {
            return;
        }
        let target = self.seed.base_metabolism * metabolism_scale.clamp(0.5, 1.5);
        self.metabolism = self.metabolism + (target - self.metabolism) * 0.05;
        let decay = self.metabolism * dt_ms as f32 * 0.0003;
        self.energy = (self.energy - decay).clamp(0.0, 1.0);
        let shift = sleep_shift.clamp(-0.2, 0.2);
        let idle_threshold = (0.2 - shift).clamp(0.05, 0.45);
        if self.energy < idle_threshold && self.state == NodeState::Active {
            self.state = NodeState::Idle;
        }
        let wake_threshold = (0.6 - shift).clamp(0.35, 0.95);
        if self.energy > wake_threshold && self.state == NodeState::Idle {
            self.state = NodeState::Active;
        }
        let wake_sleep_threshold = (0.5 - shift).clamp(0.2, 0.85);
        if self.state == NodeState::Sleep && self.energy > wake_sleep_threshold {
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

    pub fn drift_affinity(&mut self, drive: f32) {
        let target = self.seed.affinity;
        let t = (drive * 0.05).clamp(0.0, 0.25);
        self.affinity = self.affinity + (target - self.affinity) * t;
    }

    pub fn maybe_sleep_or_die(&mut self, now_ms: u64, sleep_shift: f32) -> bool {
        if self.state == NodeState::Dead {
            return false;
        }
        let idle_for = now_ms.saturating_sub(self.last_response_ms);
        let shift = sleep_shift.clamp(-0.2, 0.2);
        let sleep_threshold = (0.15 - shift).clamp(0.01, 0.3);
        if self.energy < sleep_threshold && idle_for > 3_000 {
            self.state = NodeState::Sleep;
        }
        if self.energy < 0.05 && idle_for > 6_000 {
            self.state = NodeState::Dead;
            return true;
        }
        false
    }
}

fn pattern_match(affinity: f32, core: &str, pattern: &str, strength: f32, tags: &[String]) -> bool {
    let core_tokens = tokenize(core);
    let mut pattern_tokens = tokenize(pattern);
    if pattern_tokens.is_empty() {
        pattern_tokens = tags.iter().map(|t| t.to_lowercase()).collect();
    } else {
        pattern_tokens.extend(tags.iter().map(|t| t.to_lowercase()));
    }
    if pattern_tokens
        .iter()
        .any(|token| core_tokens.iter().any(|core| core == token))
    {
        return true;
    }
    let closeness = 1.0 - (affinity - strength).abs();
    closeness > 0.45
}

fn tokenize(input: &str) -> Vec<String> {
    input
        .split(|c| matches!(c, '/' | ':' | '.'))
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect()
}
