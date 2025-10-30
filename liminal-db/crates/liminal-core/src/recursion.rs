use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::cluster_field::ClusterField;
use crate::mirror::{rebuild_timeline, MirrorTimeline};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayConfig {
    #[serde(default = "ReplayConfig::default_mode")]
    pub mode: String,
    #[serde(default = "ReplayConfig::default_scale")]
    pub scale: f32,
    #[serde(default = "ReplayConfig::default_max_ops")]
    pub max_ops: u32,
    #[serde(default = "ReplayConfig::default_protect_salience")]
    pub protect_salience: f32,
}

impl ReplayConfig {
    fn default_mode() -> String {
        "dry".to_string()
    }

    fn default_scale() -> f32 {
        1.0
    }

    fn default_max_ops() -> u32 {
        128
    }

    fn default_protect_salience() -> f32 {
        0.75
    }
}

impl Default for ReplayConfig {
    fn default() -> Self {
        ReplayConfig {
            mode: Self::default_mode(),
            scale: Self::default_scale(),
            max_ops: Self::default_max_ops(),
            protect_salience: Self::default_protect_salience(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayReport {
    pub epoch_id: u64,
    pub mode: String,
    pub applied: u32,
    pub predicted_gain: f32,
    pub took_ms: u32,
}

pub fn replay_epoch(
    field: &mut ClusterField,
    timeline: &MirrorTimeline,
    epoch_id: u64,
    cfg: &ReplayConfig,
) -> ReplayReport {
    let start = Instant::now();
    let Some(epoch) = timeline.epochs.iter().find(|epoch| epoch.id == epoch_id) else {
        return ReplayReport {
            epoch_id,
            mode: cfg.mode.clone(),
            applied: 0,
            predicted_gain: 0.0,
            took_ms: 0,
        };
    };
    let scale = cfg.scale.clamp(0.0, 4.0);
    let predicted_gain = epoch.impact * scale.abs();
    let mut applied = 0u32;
    let mode = cfg.mode.to_lowercase();
    if mode == "apply" && cfg.max_ops > 0 {
        applied = field.apply_mirror_epoch(epoch, scale, cfg.max_ops, cfg.protect_salience);
    }
    let took_ms = start.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;
    let report = ReplayReport {
        epoch_id: epoch.id,
        mode: cfg.mode.clone(),
        applied,
        predicted_gain,
        took_ms,
    };
    field.record_mirror_replay(report.clone());
    report
}

pub fn replay_epoch_by_id(
    field: &mut ClusterField,
    epoch_id: u64,
    cfg: &ReplayConfig,
) -> ReplayReport {
    let timeline = rebuild_timeline(field);
    replay_epoch(field, &timeline, epoch_id, cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mirror::{record_epoch, EpochKind, MirrorTimeline};
    use crate::symmetry::HarmonyMetrics;

    #[test]
    fn dry_run_has_no_side_effects() {
        let before = HarmonyMetrics {
            avg_strength: 0.4,
            avg_latency: 150.0,
            entropy: 0.55,
        };
        let after = HarmonyMetrics {
            avg_strength: 0.5,
            avg_latency: 120.0,
            entropy: 0.45,
        };
        let epoch = record_epoch(
            EpochKind::Dream,
            (10, 20),
            10,
            20,
            before,
            after,
            0.6,
            0.4,
            150.0,
            110.0,
        );
        let mut timeline = MirrorTimeline::default();
        timeline.epochs.push(epoch.clone());
        let mut field = ClusterField::new();
        let cfg = ReplayConfig::default();
        let report = replay_epoch(&mut field, &timeline, epoch.id, &cfg);
        assert_eq!(report.mode, "dry");
        assert_eq!(report.applied, 0);
        assert!(report.predicted_gain > 0.0);
    }
}
