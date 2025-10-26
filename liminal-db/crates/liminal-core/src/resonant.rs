use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::cluster_field::ClusterField;
use crate::dream_engine::DreamReport;
use crate::synchrony::SyncReport;
use crate::types::NodeId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ResonantEdge {
    pub from: NodeId,
    pub to: NodeId,
    pub weight: f32,
    pub coherence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Influence {
    pub source: NodeId,
    pub sink: NodeId,
    pub weight: f32,
    pub coherence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Tension {
    pub node: NodeId,
    pub magnitude: f32,
    pub relief: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ModelFrame {
    pub hash: String,
    pub cell_count: u32,
    pub created_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ResonantModel {
    pub edges: Vec<ResonantEdge>,
    pub influences: Vec<Influence>,
    pub tensions: Vec<Tension>,
    pub coherence: f32,
    #[serde(skip)]
    last_build: Option<ModelFrame>,
    #[serde(skip)]
    last_applied: Option<String>,
}

impl ResonantModel {
    pub fn record_build(&mut self, frame: ModelFrame) {
        self.last_build = Some(frame);
    }

    pub fn last_hash(&self) -> Option<&String> {
        self.last_build.as_ref().map(|frame| &frame.hash)
    }

    pub fn last_build(&self) -> Option<&ModelFrame> {
        self.last_build.as_ref()
    }

    pub fn set_last_applied(&mut self, hash: Option<String>) {
        self.last_applied = hash;
    }

    pub fn last_applied(&self) -> Option<&String> {
        self.last_applied.as_ref()
    }

    pub fn top_edges(&self, limit: usize) -> Vec<ResonantEdge> {
        take_top(&self.edges, limit)
    }

    pub fn top_influences(&self, limit: usize) -> Vec<Influence> {
        take_top(&self.influences, limit)
    }

    pub fn top_tensions(&self, limit: usize) -> Vec<Tension> {
        take_top(&self.tensions, limit)
    }

    pub fn truncated(&self, limit: usize) -> ResonantModel {
        let mut model = self.clone();
        model.edges = take_top(&model.edges, limit);
        model.influences = take_top(&model.influences, limit);
        model.tensions = take_top(&model.tensions, limit);
        model
    }
}

fn take_top<T: Clone>(items: &[T], limit: usize) -> Vec<T> {
    if limit == 0 {
        Vec::new()
    } else {
        items.iter().take(limit).cloned().collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncEvent {
    Dream {
        at_ms: u64,
        strengthened: u32,
        weakened: u32,
        protected: u32,
    },
    Collective {
        at_ms: u64,
        shared: u32,
        aligned: u32,
        protected: u32,
    },
    Build {
        at_ms: u64,
        hash: String,
        cell_count: u32,
    },
    Apply {
        at_ms: u64,
        hash: Option<String>,
        cell_count: u32,
    },
    Tick {
        at_ms: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct SyncLog {
    entries: Vec<SyncEvent>,
    #[serde(skip)]
    capacity: usize,
}

impl SyncLog {
    pub fn new() -> Self {
        SyncLog {
            entries: Vec::new(),
            capacity: 48,
        }
    }

    pub fn record_dream(&mut self, at_ms: u64, report: &DreamReport) {
        self.push(SyncEvent::Dream {
            at_ms,
            strengthened: report.strengthened,
            weakened: report.weakened,
            protected: report.protected,
        });
    }

    pub fn record_collective(&mut self, at_ms: u64, report: &SyncReport) {
        self.push(SyncEvent::Collective {
            at_ms,
            shared: report.shared,
            aligned: report.aligned,
            protected: report.protected,
        });
    }

    pub fn record_build(&mut self, hash: String, at_ms: u64, cell_count: u32) {
        self.push(SyncEvent::Build {
            at_ms,
            hash,
            cell_count,
        });
    }

    pub fn record_apply(&mut self, hash: Option<String>, at_ms: u64, cell_count: u32) {
        self.push(SyncEvent::Apply {
            at_ms,
            hash,
            cell_count,
        });
    }

    pub fn record_tick(&mut self, at_ms: u64) {
        self.push(SyncEvent::Tick { at_ms });
    }

    pub fn total_protected(&self) -> u32 {
        self.entries
            .iter()
            .map(|event| match event {
                SyncEvent::Dream { protected, .. } => *protected,
                SyncEvent::Collective { protected, .. } => *protected,
                _ => 0,
            })
            .sum()
    }

    pub fn total_aligned(&self) -> u32 {
        self.entries
            .iter()
            .map(|event| match event {
                SyncEvent::Dream { .. } => 0,
                SyncEvent::Collective { aligned, .. } => *aligned,
                _ => 0,
            })
            .sum()
    }

    pub fn total_shared(&self) -> u32 {
        self.entries
            .iter()
            .map(|event| match event {
                SyncEvent::Dream { .. } => 0,
                SyncEvent::Collective { shared, .. } => *shared,
                _ => 0,
            })
            .sum()
    }

    pub fn coherence_bias(&self) -> f32 {
        let mut best = 0.5f32;
        for event in &self.entries {
            match *event {
                SyncEvent::Dream { .. } => {
                    best = best.max(0.55);
                }
                SyncEvent::Collective {
                    aligned, shared, ..
                } => {
                    let total = aligned + shared;
                    if total > 0 {
                        let ratio = (aligned as f32 + 0.5 * shared as f32) / total as f32;
                        best = best.max(ratio);
                    }
                }
                _ => {}
            }
        }
        best.clamp(0.0, 1.0)
    }

    pub fn protection_pressure(&self) -> f32 {
        let protected = self.total_protected() as f32;
        let scaled = protected / (protected + 8.0);
        scaled.clamp(0.0, 1.0)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn truncated(&self, limit: usize) -> SyncLog {
        let mut clone = self.clone();
        if clone.entries.len() > limit {
            let drain = clone.entries.len() - limit;
            clone.entries.drain(0..drain);
        }
        clone
    }

    fn push(&mut self, event: SyncEvent) {
        if self.capacity == 0 {
            self.capacity = 48;
        }
        self.entries.push(event);
        if self.entries.len() > self.capacity {
            let drain = self.entries.len() - self.capacity;
            self.entries.drain(0..drain);
        }
    }
}

fn dream_balance(history: &[DreamReport]) -> f32 {
    let strengthened: u32 = history.iter().map(|r| r.strengthened).sum();
    let weakened: u32 = history.iter().map(|r| r.weakened).sum();
    let total = (strengthened + weakened) as f32;
    if total <= f32::EPSILON {
        0.5
    } else {
        (strengthened as f32 / total).clamp(0.0, 1.0)
    }
}

pub fn build_model(field: &ClusterField, log: &SyncLog, history: &[DreamReport]) -> ResonantModel {
    let mut model = ResonantModel::default();
    if field.cells.len() < 2 {
        return model;
    }

    let mut ranked: Vec<_> = field.cells.values().collect();
    ranked.sort_by(|a, b| {
        b.salience
            .partial_cmp(&a.salience)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(&b.id))
    });
    ranked.truncate(6);

    if ranked.len() < 2 {
        return model;
    }

    let balance = dream_balance(history);
    let coherence_bias = log.coherence_bias();
    let protection = log.protection_pressure();
    let mut tension_map: HashMap<NodeId, (f32, f32)> = HashMap::new();
    let mut total_coherence = 0.0f32;
    let mut edges = Vec::new();
    let mut influences = Vec::new();

    for (idx, left) in ranked.iter().enumerate() {
        for right in ranked.iter().skip(idx + 1) {
            let salience_avg = (left.salience + right.salience) * 0.5;
            let energy_avg = (left.energy + right.energy) * 0.5;
            let base_weight = (salience_avg + 0.1).clamp(0.0, 1.25);
            let base_coherence = (energy_avg + 0.05).clamp(0.0, 1.25);
            let weight = (base_weight * (0.5 + 0.5 * balance)).clamp(0.0, 2.5);
            let coherence = (base_coherence * (0.4 + 0.6 * coherence_bias)).clamp(0.0, 2.5);
            edges.push(ResonantEdge {
                from: left.id,
                to: right.id,
                weight,
                coherence,
            });
            total_coherence += coherence;

            let forward_weight = ((left.salience + 0.05) / (right.salience + 0.05)).clamp(0.2, 4.0);
            let backward_weight =
                ((right.salience + 0.05) / (left.salience + 0.05)).clamp(0.2, 4.0);
            let influence_coherence = (coherence * 0.8 + energy_avg).clamp(0.0, 3.0);
            influences.push(Influence {
                source: left.id,
                sink: right.id,
                weight: forward_weight,
                coherence: influence_coherence,
            });
            influences.push(Influence {
                source: right.id,
                sink: left.id,
                weight: backward_weight,
                coherence: (coherence * 0.8 + energy_avg * 0.5).clamp(0.0, 3.0),
            });

            let tension_left =
                ((1.0 - left.energy).max(0.0) + left.salience * 0.5) * (1.0 - protection);
            let tension_right =
                ((1.0 - right.energy).max(0.0) + right.salience * 0.5) * (1.0 - protection);
            let entry_left = tension_map.entry(left.id).or_insert((0.0, protection));
            entry_left.0 = entry_left.0.max(tension_left);
            entry_left.1 = protection;
            let entry_right = tension_map.entry(right.id).or_insert((0.0, protection));
            entry_right.0 = entry_right.0.max(tension_right);
            entry_right.1 = protection;
        }
    }

    let mut tensions: Vec<Tension> = tension_map
        .into_iter()
        .map(|(node, (magnitude, relief))| Tension {
            node,
            magnitude: magnitude.clamp(0.0, 2.0),
            relief: relief.clamp(0.0, 1.0),
        })
        .collect();
    tensions.sort_by(|a, b| {
        b.magnitude
            .partial_cmp(&a.magnitude)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let coherence = if edges.is_empty() {
        0.0
    } else {
        total_coherence / edges.len() as f32
    };

    model.edges = edges;
    model.influences = influences;
    model.tensions = tensions;
    model.coherence = coherence;
    model
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster_field::ClusterField;
    use crate::dream_engine::DreamReport;

    fn prepare_field() -> ClusterField {
        let mut field = ClusterField::new();
        let a = field.add_root("alpha");
        let b = field.add_root("beta");
        let c = field.add_root("gamma");
        field.cells.get_mut(&a).unwrap().salience = 0.8;
        field.cells.get_mut(&b).unwrap().salience = 0.6;
        field.cells.get_mut(&c).unwrap().salience = 0.5;
        field.cells.get_mut(&a).unwrap().energy = 0.35;
        field.cells.get_mut(&b).unwrap().energy = 0.45;
        field.cells.get_mut(&c).unwrap().energy = 0.55;
        field
    }

    #[test]
    fn weight_and_coherence_are_monotonic() {
        let field = prepare_field();
        let mut log = SyncLog::new();
        log.record_collective(
            10,
            &SyncReport {
                shared: 2,
                aligned: 3,
                protected: 0,
                ..Default::default()
            },
        );
        let history1 = vec![DreamReport {
            strengthened: 2,
            weakened: 2,
            protected: 1,
            ..Default::default()
        }];
        let model1 = build_model(&field, &log, &history1);
        assert!(!model1.edges.is_empty());

        log.record_collective(
            20,
            &SyncReport {
                shared: 6,
                aligned: 8,
                protected: 2,
                ..Default::default()
            },
        );
        let mut history2 = history1.clone();
        history2.push(DreamReport {
            strengthened: 5,
            weakened: 1,
            protected: 2,
            ..Default::default()
        });
        let model2 = build_model(&field, &log, &history2);
        assert!(!model2.edges.is_empty());

        let edge1 = &model1.edges[0];
        let edge2 = &model2.edges[0];
        assert!(edge2.weight + 1e-4 >= edge1.weight);
        assert!(edge2.coherence + 1e-4 >= edge1.coherence);
    }

    #[test]
    fn influence_weights_are_asymmetric() {
        let field = prepare_field();
        let log = SyncLog::new();
        let history = vec![DreamReport::default()];
        let model = build_model(&field, &log, &history);
        assert!(model.influences.len() >= 2);

        let mut paired: HashMap<(NodeId, NodeId), (f32, f32)> = HashMap::new();
        for influence in &model.influences {
            let key = (
                influence.source.min(influence.sink),
                influence.source.max(influence.sink),
            );
            let entry = paired.entry(key).or_insert((0.0, 0.0));
            if influence.source <= influence.sink {
                entry.0 = influence.weight;
            } else {
                entry.1 = influence.weight;
            }
        }
        assert!(!paired.is_empty());
        for (_key, (forward, backward)) in paired {
            if forward > 0.0 && backward > 0.0 {
                assert!((forward - backward).abs() > 1e-4);
            }
        }
    }

    #[test]
    fn tension_reflects_protection_pressure() {
        let field = prepare_field();
        let log_low = SyncLog::new();
        let history = vec![DreamReport::default()];
        let model_low = build_model(&field, &log_low, &history);

        let mut log_high = SyncLog::new();
        log_high.record_collective(
            1,
            &SyncReport {
                shared: 4,
                aligned: 6,
                protected: 6,
                ..Default::default()
            },
        );
        let model_high = build_model(&field, &log_high, &history);

        let to_map = |model: &ResonantModel| -> HashMap<NodeId, f32> {
            model
                .tensions
                .iter()
                .map(|t| (t.node, t.magnitude))
                .collect()
        };
        let low = to_map(&model_low);
        let high = to_map(&model_high);
        for (node, low_mag) in low {
            if let Some(high_mag) = high.get(&node) {
                assert!(high_mag <= &(low_mag + 1e-4));
            }
        }
    }
}
