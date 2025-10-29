use serde::{Deserialize, Serialize};

use crate::cluster_field::ClusterField;
use crate::resonant::{ResonantModel, Tension};
use crate::trs::TrsState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AwakeningConfig {
    pub max_nodes: usize,
    pub energy_floor: f32,
    pub energy_boost: f32,
    pub salience_boost: f32,
    pub protect_salience: f32,
    pub tick_bias_ms: i32,
    pub target_gain: f32,
}

impl Default for AwakeningConfig {
    fn default() -> Self {
        AwakeningConfig {
            max_nodes: 4,
            energy_floor: 0.45,
            energy_boost: 0.35,
            salience_boost: 0.12,
            protect_salience: 0.75,
            tick_bias_ms: 18,
            target_gain: 0.08,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct AwakeningReport {
    pub applied: u32,
    pub protected: u32,
    pub tick_adjust_ms: i32,
    pub avg_tension: f32,
    pub energy_delta: f32,
}

pub fn awaken(
    field: &mut ClusterField,
    model: &ResonantModel,
    cfg: &AwakeningConfig,
    trs: &mut TrsState,
) -> AwakeningReport {
    if model.tensions.is_empty() {
        return AwakeningReport::default();
    }

    let mut report = AwakeningReport::default();
    let mut total_tension = 0.0f32;
    let mut counted = 0u32;
    let mut energy_delta = 0.0f32;

    let mut consider: Vec<&Tension> = model.tensions.iter().collect();
    consider.sort_by(|a, b| {
        b.magnitude
            .partial_cmp(&a.magnitude)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    for tension in consider.into_iter().take(cfg.max_nodes) {
        counted = counted.saturating_add(1);
        total_tension += tension.magnitude;
        let Some(cell_view) = field.cells.get(&tension.node) else {
            continue;
        };
        if cell_view.salience >= cfg.protect_salience {
            report.protected = report.protected.saturating_add(1);
            continue;
        }
        let mut delta = 0.0f32;
        {
            let Some(cell) = field.cells.get_mut(&tension.node) else {
                continue;
            };
            let before = cell.energy;
            let floor = cfg.energy_floor.max(before);
            let boost = (tension.magnitude * cfg.energy_boost).min(1.0 - floor);
            let after = (floor + boost).clamp(0.0, 1.0);
            if (after - before).abs() > f32::EPSILON {
                cell.energy = after;
                cell.bump_salience(cfg.salience_boost * (1.0 + tension.magnitude));
                delta = after - before;
            }
        }
        if delta.abs() > f32::EPSILON {
            energy_delta += delta;
            report.applied = report.applied.saturating_add(1);
            field.emit_energy_state(tension.node);
        }
    }

    if counted > 0 {
        report.avg_tension = total_tension / counted as f32;
    }

    report.energy_delta = energy_delta;

    let tension_delta = report.avg_tension - 0.5;
    if cfg.tick_bias_ms != 0 {
        let adjust = ((cfg.tick_bias_ms as f32) * tension_delta)
            .round()
            .clamp(-64.0, 64.0) as i32;
        report.tick_adjust_ms = adjust;
    }

    if cfg.target_gain > 0.0 {
        let delta = (cfg.target_gain * tension_delta).clamp(-0.15, 0.15);
        let next_target = (trs.target_load + delta).clamp(0.3, 0.95);
        trs.set_target(next_target);
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster_field::ClusterField;

    #[test]
    fn awaken_elevates_low_energy_and_respects_protection() {
        let mut field = ClusterField::new();
        let low = field.add_root("low");
        let protected = field.add_root("protected");
        field.cells.get_mut(&low).unwrap().energy = 0.2;
        field.cells.get_mut(&low).unwrap().salience = 0.4;
        field.cells.get_mut(&protected).unwrap().energy = 0.55;
        field.cells.get_mut(&protected).unwrap().salience = 0.8;

        let mut model = ResonantModel::default();
        model.coherence = 0.6;
        model.tensions = vec![
            Tension {
                node: low,
                magnitude: 0.9,
                relief: 0.2,
            },
            Tension {
                node: protected,
                magnitude: 0.7,
                relief: 0.2,
            },
        ];

        let cfg = AwakeningConfig {
            max_nodes: 3,
            energy_floor: 0.5,
            energy_boost: 0.4,
            salience_boost: 0.2,
            protect_salience: 0.7,
            tick_bias_ms: 20,
            target_gain: 0.1,
        };

        let mut trs = TrsState::default();
        let report = awaken(&mut field, &model, &cfg, &mut trs);

        assert_eq!(report.applied, 1);
        assert_eq!(report.protected, 1);
        assert!(report.avg_tension > 0.0);
        assert!(field.cells.get(&low).unwrap().energy >= cfg.energy_floor);
        assert_eq!(field.cells.get(&protected).unwrap().energy, 0.55);
        assert!(field.cells.get(&low).unwrap().salience > 0.4);
        assert!(report.tick_adjust_ms != 0);
        assert!((trs.target_load - 0.6).abs() > f32::EPSILON);
    }
}
