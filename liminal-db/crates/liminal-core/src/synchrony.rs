use std::collections::{HashMap, HashSet};
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::cluster_field::ClusterField;
use crate::dream_engine::PairStat;
use crate::types::NodeId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncConfig {
    pub phase_len_ms: u32,
    pub phase_gap_ms: u32,
    pub cooccur_threshold: f32,
    pub max_groups: u16,
    pub share_top_k: u16,
    pub weight_xfer: f32,
}

impl Default for SyncConfig {
    fn default() -> Self {
        SyncConfig {
            phase_len_ms: 4_000,
            phase_gap_ms: 1_000,
            cooccur_threshold: 0.6,
            max_groups: 4,
            share_top_k: 8,
            weight_xfer: 0.2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SyncGroup {
    pub id: u64,
    pub nodes: Vec<NodeId>,
    pub tokens: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SyncReport {
    pub groups: u16,
    pub shared: u32,
    pub aligned: u32,
    pub protected: u32,
    pub took_ms: u32,
}

fn score_pair(pair: &PairStat) -> f32 {
    (pair.freq.max(0.0) + 1.0) * (pair.avg_strength.max(0.0) + 0.5)
}

fn build_adjacency<'a>(
    pairs: impl Iterator<Item = &'a PairStat>,
    threshold: f32,
) -> HashMap<NodeId, Vec<NodeId>> {
    let mut adjacency: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for pair in pairs {
        if pair.avg_strength < threshold {
            continue;
        }
        adjacency.entry(pair.u).or_default().push(pair.v);
        adjacency.entry(pair.v).or_default().push(pair.u);
    }
    adjacency
}

fn extract_components(adjacency: &HashMap<NodeId, Vec<NodeId>>) -> Vec<Vec<NodeId>> {
    let mut seen: HashSet<NodeId> = HashSet::new();
    let mut groups = Vec::new();
    for &node in adjacency.keys() {
        if seen.contains(&node) {
            continue;
        }
        let mut queue = vec![node];
        let mut component = Vec::new();
        while let Some(current) = queue.pop() {
            if !seen.insert(current) {
                continue;
            }
            component.push(current);
            if let Some(neighbors) = adjacency.get(&current) {
                for &next in neighbors {
                    if !seen.contains(&next) {
                        queue.push(next);
                    }
                }
            }
        }
        if component.len() > 1 {
            groups.push(component);
        }
    }
    groups
}

pub fn detect_sync_groups(field: &ClusterField, cfg: &SyncConfig, now_ms: u64) -> Vec<SyncGroup> {
    let window = cfg.phase_len_ms.max(500);
    let pairs = field.recent_pairs(window);
    if pairs.is_empty() {
        return Vec::new();
    }
    let mut sorted: Vec<PairStat> = pairs
        .into_iter()
        .filter(|pair| pair.avg_strength >= cfg.cooccur_threshold)
        .collect();
    sorted.sort_by(|a, b| score_pair(b).partial_cmp(&score_pair(a)).unwrap());
    let adjacency = build_adjacency(sorted.iter(), cfg.cooccur_threshold);
    let mut components = extract_components(&adjacency);
    components.sort_by_key(|component| -(component.len() as isize));
    let mut groups = Vec::new();
    for (idx, nodes) in components.into_iter().enumerate() {
        if idx >= cfg.max_groups as usize {
            break;
        }
        let tokens = field.sync_top_tokens(&nodes, cfg.share_top_k as usize);
        if tokens.is_empty() {
            continue;
        }
        groups.push(SyncGroup {
            id: now_ms.wrapping_shl(16).wrapping_add(idx as u64),
            nodes,
            tokens,
        });
    }
    groups
}

pub fn run_collective_dream(
    field: &mut ClusterField,
    groups: &[SyncGroup],
    cfg: &SyncConfig,
    now_ms: u64,
) -> SyncReport {
    if groups.is_empty() {
        return SyncReport::default();
    }
    let start = Instant::now();
    let mut report = SyncReport {
        groups: groups.len() as u16,
        ..Default::default()
    };
    for group in groups {
        let (shared, aligned, protected) =
            field.share_tokens(&group.nodes, &group.tokens, cfg.weight_xfer);
        report.shared = report.shared.saturating_add(shared);
        report.aligned = report.aligned.saturating_add(aligned);
        report.protected = report.protected.saturating_add(protected);
    }
    report.took_ms = start.elapsed().as_millis().min(u32::MAX as u128) as u32;
    field.record_sync_report(now_ms, report.clone());
    report
}
