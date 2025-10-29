use std::cmp::Ordering;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

use serde::{Deserialize, Serialize};

use crate::cluster_field::ClusterField;
use crate::symmetry::HarmonyMetrics;

static EPOCH_SEQ: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EpochKind {
    Dream,
    Collective,
    Awaken,
}

impl EpochKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EpochKind::Dream => "dream",
            EpochKind::Collective => "collective",
            EpochKind::Awaken => "awaken",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Epoch {
    pub id: u64,
    pub kind: EpochKind,
    pub start_ms: u64,
    pub end_ms: u64,
    pub cfg_hash: u64,
    pub report_hash: u64,
    pub harmony_before: HarmonyMetrics,
    pub harmony_after: HarmonyMetrics,
    pub tension_before: f32,
    pub tension_after: f32,
    pub latency_avg_before: f32,
    pub latency_avg_after: f32,
    pub impact: f32,
}

impl Epoch {
    pub fn duration(&self) -> u64 {
        self.end_ms.saturating_sub(self.start_ms)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MirrorTimeline {
    pub epochs: Vec<Epoch>,
    pub built_ms: u64,
}

pub fn record_epoch(
    kind: EpochKind,
    times: (u64, u64),
    cfg_hash: u64,
    report_hash: u64,
    harmony_before: HarmonyMetrics,
    harmony_after: HarmonyMetrics,
    tension_before: f32,
    tension_after: f32,
    latency_avg_before: f32,
    latency_avg_after: f32,
) -> Epoch {
    let (start_ms, end_ms) = times;
    let id = EPOCH_SEQ.fetch_add(1, AtomicOrdering::Relaxed);
    let tension_gain = tension_before - tension_after;
    let latency_gain = latency_avg_before - latency_avg_after;
    let stability_before = harmony_before.avg_strength - harmony_before.avg_latency;
    let stability_after = harmony_after.avg_strength - harmony_after.avg_latency;
    let stability_gain = stability_after - stability_before;
    let entropy_drift = harmony_after.entropy - harmony_before.entropy;
    let mut impact = 0.5 * tension_gain + 0.3 * latency_gain + 0.2 * stability_gain;
    if entropy_drift > 0.0 {
        impact -= entropy_drift * 0.15;
    }
    let impact = impact.clamp(-5.0, 5.0);
    Epoch {
        id,
        kind,
        start_ms,
        end_ms,
        cfg_hash,
        report_hash,
        harmony_before,
        harmony_after,
        tension_before,
        tension_after,
        latency_avg_before,
        latency_avg_after,
        impact,
    }
}

pub fn rebuild_timeline(field: &ClusterField) -> MirrorTimeline {
    let mut epochs: Vec<_> = field.mirror_epochs_iter().cloned().collect();
    epochs.sort_by(|a, b| match a.start_ms.cmp(&b.start_ms) {
        Ordering::Equal => a.id.cmp(&b.id),
        other => other,
    });
    MirrorTimeline {
        epochs,
        built_ms: field.now_ms,
    }
}

pub fn top_influencers(timeline: &MirrorTimeline, k: usize) -> Vec<Epoch> {
    if k == 0 {
        return Vec::new();
    }
    let mut epochs = timeline.epochs.clone();
    epochs.sort_by(|a, b| {
        b.impact
            .partial_cmp(&a.impact)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.start_ms.cmp(&b.start_ms))
            .then_with(|| a.id.cmp(&b.id))
    });
    epochs.truncate(k);
    epochs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_metrics() -> (HarmonyMetrics, HarmonyMetrics) {
        (
            HarmonyMetrics {
                avg_strength: 0.4,
                avg_latency: 180.0,
                entropy: 0.6,
            },
            HarmonyMetrics {
                avg_strength: 0.55,
                avg_latency: 140.0,
                entropy: 0.5,
            },
        )
    }

    #[test]
    fn impact_is_monotonic_with_improvement() {
        let (before, after) = sample_metrics();
        let better = record_epoch(
            EpochKind::Dream,
            (0, 1),
            1,
            2,
            before,
            after,
            0.6,
            0.4,
            180.0,
            120.0,
        )
        .impact;
        let worse = record_epoch(
            EpochKind::Dream,
            (0, 1),
            1,
            2,
            after,
            before,
            0.4,
            0.6,
            120.0,
            180.0,
        )
        .impact;
        assert!(
            better > worse,
            "impact not monotonic: {} vs {}",
            better,
            worse
        );
    }
}
