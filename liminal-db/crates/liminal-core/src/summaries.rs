use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::{ClusterField, NodeId};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Status {
    pub harmony: f32,
    pub emotional_load: f32,
    pub top_salient: Vec<String>,
    pub last_dream: Option<u64>,
    pub last_awaken: Option<u64>,
    pub target_load: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Short {
    pub token: String,
    pub occurrences: usize,
    pub avg_latency_ms: Option<f32>,
    pub resonance: Vec<String>,
    pub salience: f32,
}

pub fn make_status(field: &ClusterField) -> Status {
    let mut total_affinity = 0.0f32;
    let mut total_energy = 0.0f32;
    let mut count = 0f32;
    for cell in field.cells.values() {
        total_affinity += cell.affinity;
        total_energy += cell.energy;
        count += 1.0;
    }
    let harmony = if count > 0.0 {
        total_affinity / count
    } else {
        0.0
    };
    let emotional = if count > 0.0 {
        total_energy / count
    } else {
        0.0
    };

    let mut top_tokens: Vec<(String, usize)> = field
        .index
        .iter()
        .map(|(token, ids)| (token.clone(), ids.len()))
        .collect();
    top_tokens.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    let top_salient = top_tokens
        .into_iter()
        .take(3)
        .map(|(token, _)| token)
        .collect();

    Status {
        harmony,
        emotional_load: emotional,
        top_salient,
        last_dream: field.last_dream_reports(1).first().map(|(ts, _)| *ts),
        last_awaken: Some(field.last_awaken_tick()),
        target_load: field.trs.target_load,
    }
}

pub fn explain_token(field: &ClusterField, token: &str) -> Short {
    let key = token.to_lowercase();
    let mut occurrences = 0usize;
    let mut salience = 0.0f32;
    let mut seen: HashMap<NodeId, ()> = HashMap::new();
    if let Some(ids) = field.index.get(&key) {
        occurrences = ids.len();
        for id in ids {
            if let Some(cell) = field.cells.get(id) {
                salience += cell.salience;
                seen.insert(*id, ());
            }
        }
    }
    let avg_salience = if occurrences > 0 {
        salience / occurrences as f32
    } else {
        0.0
    };

    Short {
        token: token.to_string(),
        occurrences,
        avg_latency_ms: None,
        resonance: Vec::new(),
        salience: avg_salience,
    }
}
