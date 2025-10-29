use std::collections::HashMap;
use std::hash::Hash;

use serde::{Deserialize, Serialize};

use crate::cluster_field::{ClusterField, Hit};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub struct HarmonyMetrics {
    pub avg_strength: f32,
    pub avg_latency: f32,
    pub entropy: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SymmetryStatus {
    Ok,
    Drift,
    Overload,
}

impl SymmetryStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SymmetryStatus::Ok => "ok",
            SymmetryStatus::Drift => "drift",
            SymmetryStatus::Overload => "overload",
        }
    }
}

impl Default for SymmetryStatus {
    fn default() -> Self {
        SymmetryStatus::Ok
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MirrorImpulse {
    #[serde(rename = "k")]
    pub kind: &'static str,
    #[serde(rename = "p")]
    pub pattern: String,
    #[serde(rename = "s")]
    pub strength: f32,
    #[serde(rename = "t")]
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct HarmonySnapshot {
    pub metrics: HarmonyMetrics,
    pub delta_strength: f32,
    pub delta_latency: f32,
    pub entropy_ratio: f32,
    pub dominant_pattern: Option<String>,
    pub status: SymmetryStatus,
    pub mirror: Option<MirrorImpulse>,
}

impl Default for HarmonySnapshot {
    fn default() -> Self {
        HarmonySnapshot {
            metrics: HarmonyMetrics::default(),
            delta_strength: 0.0,
            delta_latency: 0.0,
            entropy_ratio: 0.0,
            dominant_pattern: None,
            status: SymmetryStatus::Ok,
            mirror: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SymmetryLoop {
    pub last: HarmonyMetrics,
    pub decay: f32,
    window_ms: u32,
    current: HarmonyMetrics,
    delta_strength: f32,
    delta_latency: f32,
    entropy_ratio: f32,
    dominant_pattern: Option<String>,
    status: SymmetryStatus,
    last_mirror: Option<MirrorImpulse>,
}

impl SymmetryLoop {
    pub fn new(decay: f32, window_ms: u32) -> Self {
        let decay = decay.clamp(0.0, 0.999);
        SymmetryLoop {
            last: HarmonyMetrics::default(),
            decay,
            window_ms: window_ms.max(200),
            current: HarmonyMetrics::default(),
            delta_strength: 0.0,
            delta_latency: 0.0,
            entropy_ratio: 0.0,
            dominant_pattern: None,
            status: SymmetryStatus::Ok,
            last_mirror: None,
        }
    }

    pub fn update(&mut self, field: &ClusterField) -> Option<MirrorImpulse> {
        let samples = field.symmetry_samples(self.window_ms);
        self.update_with_samples(field.now_ms, &samples)
    }

    pub fn update_with_samples(
        &mut self,
        now_ms: u64,
        samples: &[(String, Hit)],
    ) -> Option<MirrorImpulse> {
        if samples.is_empty() {
            self.current = HarmonyMetrics::default();
            self.delta_strength = 0.0;
            self.delta_latency = 0.0;
            self.entropy_ratio = 0.0;
            self.dominant_pattern = None;
            self.status = SymmetryStatus::Ok;
            self.last_mirror = None;
            self.last.avg_strength *= self.decay;
            self.last.avg_latency *= self.decay;
            self.last.entropy *= self.decay;
            return None;
        }

        let mut total_strength = 0.0f32;
        let mut total_latency = 0.0f32;
        let mut total_count = 0usize;
        let mut patterns: Vec<String> = Vec::new();
        let mut pattern_hits: HashMap<String, usize> = HashMap::new();
        let mut dominant_pattern: Option<String> = None;
        let mut dominant_count = 0usize;

        for (pattern, hit) in samples.iter() {
            total_strength += hit.strength;
            total_latency += hit.latency_ms as f32;
            total_count += 1;
            patterns.push(pattern.clone());
            let entry = pattern_hits.entry(pattern.clone()).or_insert(0);
            *entry += 1;
            if *entry > dominant_count {
                dominant_count = *entry;
                dominant_pattern = Some(pattern.clone());
            }
        }

        let avg_strength = if total_count > 0 {
            total_strength / total_count as f32
        } else {
            0.0
        };
        let avg_latency = if total_count > 0 {
            total_latency / total_count as f32
        } else {
            0.0
        };
        let entropy = entropy_of(&patterns);

        let metrics = HarmonyMetrics {
            avg_strength,
            avg_latency,
            entropy,
        };

        let delta_strength = metrics.avg_strength - self.last.avg_strength;
        let delta_latency = metrics.avg_latency - self.last.avg_latency;

        let status = if metrics.entropy > 0.85 || delta_strength.abs() > 0.35 {
            SymmetryStatus::Overload
        } else if metrics.entropy > 0.7 || delta_strength.abs() > 0.2 {
            SymmetryStatus::Drift
        } else {
            SymmetryStatus::Ok
        };

        self.current = metrics;
        self.delta_strength = delta_strength;
        self.delta_latency = delta_latency;
        self.entropy_ratio = metrics.entropy;
        self.dominant_pattern = dominant_pattern.clone();
        self.status = status;

        let mirror = if delta_strength.abs() > 0.2 || metrics.entropy > 0.7 {
            Some(MirrorImpulse {
                kind: "mirror",
                pattern: dominant_pattern.unwrap_or_else(|| "*".to_string()),
                strength: -delta_strength,
                timestamp_ms: now_ms,
            })
        } else {
            None
        };

        self.last_mirror = mirror.clone();

        self.last.avg_strength =
            self.last.avg_strength * self.decay + metrics.avg_strength * (1.0 - self.decay);
        self.last.avg_latency =
            self.last.avg_latency * self.decay + metrics.avg_latency * (1.0 - self.decay);
        self.last.entropy = self.last.entropy * self.decay + metrics.entropy * (1.0 - self.decay);

        mirror
    }

    pub fn snapshot(&self) -> HarmonySnapshot {
        HarmonySnapshot {
            metrics: self.current,
            delta_strength: self.delta_strength,
            delta_latency: self.delta_latency,
            entropy_ratio: self.entropy_ratio,
            dominant_pattern: self.dominant_pattern.clone(),
            status: self.status,
            mirror: self.last_mirror.clone(),
        }
    }

    pub fn set_window(&mut self, window_ms: u32) {
        self.window_ms = window_ms.max(200);
    }

    pub fn window(&self) -> u32 {
        self.window_ms
    }
}

pub fn entropy_of<T>(data: &[T]) -> f32
where
    T: Eq + Hash + Clone,
{
    if data.is_empty() {
        return 0.0;
    }
    let mut counts: HashMap<T, usize> = HashMap::new();
    for item in data.iter().cloned() {
        *counts.entry(item).or_insert(0) += 1;
    }
    let total = data.len() as f32;
    if total <= f32::EPSILON {
        return 0.0;
    }
    let mut entropy = 0.0f32;
    for count in counts.values() {
        let p = *count as f32 / total;
        if p > 0.0 {
            entropy -= p * p.ln();
        }
    }
    let max_entropy = (counts.len() as f32).ln();
    if max_entropy > 0.0 {
        (entropy / max_entropy).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cluster_field::ClusterField;
    use crate::types::{Impulse, ImpulseKind};

    #[test]
    fn entropy_reaches_one_for_uniform_distribution() {
        let data = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let entropy = entropy_of(&data);
        assert!((entropy - 1.0).abs() < 1e-6, "entropy={entropy}");
    }

    #[test]
    fn mirror_triggers_on_strength_delta() {
        let mut field = ClusterField::new();
        field.add_root("cpu/load");
        for i in 0..4 {
            field.now_ms = (i * 200) as u64;
            let _ = field.route_impulse(Impulse {
                kind: ImpulseKind::Query,
                pattern: "cpu/load".into(),
                strength: 0.9,
                ttl_ms: 1_000,
                tags: Vec::new(),
            });
        }
        let mut loop_ctrl = SymmetryLoop::new(0.6, 2_000);
        let samples = field.symmetry_samples(loop_ctrl.window());
        let mirror = loop_ctrl.update_with_samples(field.now_ms, &samples);
        assert!(mirror.is_some(), "expected mirror impulse to trigger");
        let snapshot = loop_ctrl.snapshot();
        assert!(snapshot.delta_strength > 0.2);
    }

    #[test]
    fn entropy_grows_with_pattern_diversity() {
        let mut field = ClusterField::new();
        field.add_root("cpu/load");
        field.add_root("disk/load");
        let impulses = [
            ("cpu/load", 0.7f32),
            ("cpu/load", 0.65f32),
            ("disk/load", 0.7f32),
            ("disk/load", 0.72f32),
        ];
        for (idx, (pattern, strength)) in impulses.iter().enumerate() {
            field.now_ms = (idx * 250) as u64;
            let _ = field.route_impulse(Impulse {
                kind: ImpulseKind::Query,
                pattern: (*pattern).into(),
                strength: *strength,
                ttl_ms: 1_000,
                tags: Vec::new(),
            });
        }
        let mut loop_ctrl = SymmetryLoop::new(0.5, 2_000);
        let samples = field.symmetry_samples(loop_ctrl.window());
        let _ = loop_ctrl.update_with_samples(field.now_ms, &samples);
        let entropy_mixed = loop_ctrl.snapshot().entropy_ratio;
        assert!(
            entropy_mixed > 0.5,
            "entropy should increase with diversity"
        );
    }
}
