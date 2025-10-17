use std::collections::HashSet;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::cluster_field::ClusterField;
use crate::types::NodeId;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DreamConfig {
    pub min_idle_s: u32,
    pub window_ms: u32,
    pub strengthen_top_pct: f32,
    pub weaken_bottom_pct: f32,
    pub protect_salience: f32,
    pub adreno_protect: bool,
    pub max_ops_per_cycle: u32,
}

impl Default for DreamConfig {
    fn default() -> Self {
        DreamConfig {
            min_idle_s: 15,
            window_ms: 3_000,
            strengthen_top_pct: 0.15,
            weaken_bottom_pct: 0.10,
            protect_salience: 0.6,
            adreno_protect: true,
            max_ops_per_cycle: 256,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DreamReport {
    pub strengthened: u32,
    pub weakened: u32,
    pub pruned: u32,
    pub protected: u32,
    pub rewired: u32,
    pub took_ms: u32,
}

pub fn run_dream(field: &mut ClusterField, cfg: &DreamConfig, now_ms: u64) -> DreamReport {
    let start = Instant::now();
    let mut report = DreamReport::default();

    let mut ops_remaining = cfg.max_ops_per_cycle;
    if ops_remaining == 0 {
        return report;
    }

    let mut pairs = field.recent_pairs(cfg.window_ms);
    if pairs.is_empty() {
        report.took_ms = start.elapsed().as_millis().min(u32::MAX as u128) as u32;
        return report;
    }

    pairs.sort_by(|a, b| b.freq.partial_cmp(&a.freq).unwrap_or(std::cmp::Ordering::Equal));

    let total_pairs = pairs.len();
    let strengthen_count = ((total_pairs as f32) * cfg.strengthen_top_pct)
        .round()
        .clamp(0.0, total_pairs as f32) as usize;
    let weaken_count = ((total_pairs as f32) * cfg.weaken_bottom_pct)
        .round()
        .clamp(0.0, total_pairs as f32) as usize;

    let mut touched: HashSet<(NodeId, NodeId)> = HashSet::new();

    for entry in pairs.iter().take(strengthen_count) {
        if ops_remaining == 0 {
            break;
        }
        if should_protect(field, entry.u, entry.v, cfg) {
            report.protected += 1;
            continue;
        }
        if touched.insert((entry.u, entry.v)) {
            if let Some((from, to)) = pick_direction(field, entry.u, entry.v) {
                if field.adjust_link_score(from, to, 0.15) {
                    ops_remaining = ops_remaining.saturating_sub(1);
                    report.strengthened += 1;
                    field.emit_dream_strengthen(from, to, entry.freq, entry.avg_strength);
                }
            }
        }
    }

    if ops_remaining > 0 && weaken_count > 0 {
        let mut bottom = pairs;
        bottom.sort_by(|a, b| a.freq.partial_cmp(&b.freq).unwrap_or(std::cmp::Ordering::Equal));
        for entry in bottom.iter().take(weaken_count) {
            if ops_remaining == 0 {
                break;
            }
            if should_protect(field, entry.u, entry.v, cfg) {
                report.protected += 1;
                continue;
            }
            if touched.insert((entry.u, entry.v)) {
                if let Some((from, to)) = pick_direction(field, entry.u, entry.v) {
                    if field.adjust_link_score(from, to, -0.2) {
                        ops_remaining = ops_remaining.saturating_sub(1);
                        report.weakened += 1;
                        field.emit_dream_weaken(from, to, entry.freq, entry.avg_strength);
                        if field.link_score(from, to) <= 0.25 {
                            if field.prune_link(from, to) {
                                report.pruned += 1;
                                field.emit_dream_prune(from, to);
                                ops_remaining = ops_remaining.saturating_sub(1);
                                if ops_remaining == 0 {
                                    break;
                                }
                                let rewired_a = field.rewire_lonely(from).unwrap_or(0);
                                let rewired_b = field.rewire_lonely(to).unwrap_or(0);
                                report.rewired += rewired_a + rewired_b;
                                let total = rewired_a.saturating_add(rewired_b);
                                if total > 0 {
                                    ops_remaining = ops_remaining.saturating_sub(total);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    report.took_ms = start.elapsed().as_millis().min(u32::MAX as u128) as u32;
    field.record_dream_report(now_ms, report.clone());
    report
}

fn should_protect(field: &ClusterField, u: NodeId, v: NodeId, cfg: &DreamConfig) -> bool {
    let protect = |id: NodeId| -> bool {
        if let Some(cell) = field.cell(id) {
            if cell.salience >= cfg.protect_salience {
                return true;
            }
            if cfg.adreno_protect && cell.adreno_tag {
                return true;
            }
        }
        false
    };
    protect(u) || protect(v)
}

fn pick_direction(field: &ClusterField, a: NodeId, b: NodeId) -> Option<(NodeId, NodeId)> {
    if field
        .cell(a)
        .map(|cell| cell.links.contains(&b))
        .unwrap_or(false)
    {
        return Some((a, b));
    }
    if field
        .cell(b)
        .map(|cell| cell.links.contains(&a))
        .unwrap_or(false)
    {
        return Some((b, a));
    }
    None
}

#[derive(Debug, Clone)]
pub struct PairStat {
    pub u: NodeId,
    pub v: NodeId,
    pub freq: f32,
    pub avg_strength: f32,
}

impl ClusterField {
    pub fn recent_pairs(&self, window_ms: u32) -> Vec<PairStat> {
        self.collect_recent_pairs(window_ms)
    }
}
