use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// Identifier type for variants tracked by the manifold.
pub type VariantId = String;

/// Representation of an actionable variant in the manifold.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Variant {
    pub id: VariantId,
    pub title: String,
    pub evidence_score: f32,
    pub cost_est: f32,
    pub time_est_ms: u32,
    pub risk: f32,
    pub expected_gain: f32,
    #[serde(default)]
    pub path_hint: Vec<String>,
    #[serde(default)]
    pub resonance_hint: Option<f32>,
}

impl Variant {
    pub fn effort(&self) -> f32 {
        let time = self.time_est_ms as f32 / 1_000.0;
        (self.cost_est + time).max(1e-3)
    }
}

/// Collection of desired attractor states.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Slide {
    pub id: String,
    #[serde(default)]
    pub variant_ids: Vec<VariantId>,
    #[serde(default)]
    pub desired_metrics: HashMap<String, f64>,
    #[serde(default)]
    pub ethics_guard: Option<String>,
}

/// Directional intent supplied by the operator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Intention {
    #[serde(default)]
    pub focus_tokens: Vec<String>,
    #[serde(default)]
    pub weights: Vec<f32>,
    pub horizon_ms: Option<u64>,
    pub budget: Option<f32>,
}

impl Intention {
    pub fn normalized_weights(&self) -> Vec<f32> {
        if self.weights.is_empty() {
            return Vec::new();
        }
        let sum: f32 = self.weights.iter().copied().filter(|v| *v > 0.0).sum();
        if sum <= 0.0 {
            return self.weights.iter().map(|_| 0.0).collect();
        }
        self.weights.iter().map(|w| (w.max(0.0)) / sum).collect()
    }
}

/// Counter-force competing for attention.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Pendulum {
    pub id: String,
    #[serde(default)]
    pub signature_tokens: Vec<String>,
    pub drain_rate: f32,
    #[serde(default)]
    pub counter_policy: Option<String>,
}

impl Pendulum {
    pub fn penalty_for(&self, tokens: &[String]) -> f32 {
        if self.signature_tokens.is_empty() {
            return 0.0;
        }
        let mut hits = 0usize;
        for token in tokens {
            if self
                .signature_tokens
                .iter()
                .any(|needle| needle.eq_ignore_ascii_case(token))
            {
                hits += 1;
            }
        }
        if hits == 0 {
            0.0
        } else {
            (hits as f32 / self.signature_tokens.len() as f32) * self.drain_rate.max(0.0)
        }
    }
}

/// Weights applied when computing variant probabilities.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ProbabilityWeights {
    pub evidence: f32,
    pub cost: f32,
    pub gain: f32,
    pub risk: f32,
    pub resonance: f32,
}

impl Default for ProbabilityWeights {
    fn default() -> Self {
        ProbabilityWeights {
            evidence: 2.1,
            cost: 1.3,
            gain: 1.4,
            risk: 1.1,
            resonance: 1.6,
        }
    }
}

/// Component responsible for translating variant state into probabilities.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProbabilityFlow {
    pub weights: ProbabilityWeights,
    pub min_effort: f32,
}

impl Default for ProbabilityFlow {
    fn default() -> Self {
        ProbabilityFlow {
            weights: ProbabilityWeights::default(),
            min_effort: 0.2,
        }
    }
}

impl ProbabilityFlow {
    pub fn probability(
        &self,
        variant: &Variant,
        intention: Option<&Intention>,
        resonance: f32,
    ) -> f32 {
        let cost_norm = match intention.and_then(|i| i.budget) {
            Some(budget) if budget > 0.0 => variant.cost_est / budget.max(1e-3),
            _ => variant.cost_est,
        };
        let x = self.weights.evidence * variant.evidence_score - self.weights.cost * cost_norm
            + self.weights.gain * variant.expected_gain
            - self.weights.risk * variant.risk
            + self.weights.resonance * resonance;
        sigmoid(x)
    }

    pub fn effort(&self, variant: &Variant) -> f32 {
        variant.effort().max(self.min_effort)
    }

    pub fn score(
        &self,
        variant: &Variant,
        intention: Option<&Intention>,
        resonance: f32,
    ) -> (f32, f32, f32) {
        let probability = self.probability(variant, intention, resonance);
        let effort = self.effort(variant);
        let score = probability / effort.max(self.min_effort);
        (probability, effort, score)
    }
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

/// Ranking record for a variant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariantScore {
    pub id: VariantId,
    pub title: String,
    pub probability: f32,
    pub effort: f32,
    pub score: f32,
    pub evidence: f32,
    pub risk: f32,
    pub expected_gain: f32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
struct EvidenceEma {
    value: f32,
    alpha: f32,
    seen: bool,
}

impl EvidenceEma {
    fn update(&mut self, sample: f32) -> f32 {
        if !self.seen {
            self.value = sample;
            self.seen = true;
        } else {
            self.value = self.alpha * sample + (1.0 - self.alpha) * self.value;
        }
        self.value
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitState {
    pub variant_id: VariantId,
    pub started_ms: u64,
    pub accumulated_effort: f32,
}

/// Outcome of committing to a variant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitOutcome {
    pub variant_id: VariantId,
    #[serde(default)]
    pub planted_seeds: Vec<u64>,
}

/// Outcome of pivoting from one variant to another.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PivotOutcome {
    pub from: VariantId,
    pub to: VariantId,
    pub inertia: f32,
}

/// Outcome of defusing a pendulum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DefuseOutcome {
    pub pendulum_id: String,
    pub previous_drain: f32,
    pub new_drain: f32,
}

/// Maintains the manifold of variants and their supporting actors.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VariantManifold {
    #[serde(default)]
    variants: HashMap<VariantId, Variant>,
    #[serde(default)]
    slides: HashMap<String, Slide>,
    #[serde(default)]
    pendulums: HashMap<String, Pendulum>,
    #[serde(default)]
    evidence: HashMap<VariantId, EvidenceEma>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    active_slide: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    active_intention: Option<Intention>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    active_commit: Option<CommitState>,
    #[serde(default)]
    flow: ProbabilityFlow,
}

impl Default for VariantManifold {
    fn default() -> Self {
        VariantManifold {
            variants: HashMap::new(),
            slides: HashMap::new(),
            pendulums: HashMap::new(),
            evidence: HashMap::new(),
            active_slide: None,
            active_intention: None,
            active_commit: None,
            flow: ProbabilityFlow::default(),
        }
    }
}

impl VariantManifold {
    pub fn upsert_variant(&mut self, variant: Variant) {
        if !self.evidence.contains_key(&variant.id) {
            self.evidence.insert(
                variant.id.clone(),
                EvidenceEma {
                    alpha: 0.35,
                    ..EvidenceEma::default()
                },
            );
        }
        self.variants.insert(variant.id.clone(), variant);
    }

    pub fn set_slide(&mut self, slide: Slide) {
        let id = slide.id.clone();
        self.slides.insert(id.clone(), slide);
        self.active_slide = Some(id);
    }

    pub fn get_variant(&self, id: &str) -> Option<&Variant> {
        self.variants.get(id)
    }

    pub fn register_pendulum(&mut self, pendulum: Pendulum) {
        self.pendulums.insert(pendulum.id.clone(), pendulum);
    }

    pub fn set_intention(&mut self, intention: Intention) {
        self.active_intention = Some(intention);
    }

    pub fn intention(&self) -> Option<&Intention> {
        self.active_intention.as_ref()
    }

    pub fn observe_variant(&mut self, variant_id: &str, evidence: f32) -> Option<f32> {
        let ema = self.evidence.get_mut(variant_id)?;
        Some(ema.update(evidence.clamp(0.0, 1.0)))
    }

    pub fn active_variant(&self) -> Option<&CommitState> {
        self.active_commit.as_ref()
    }

    pub fn commit(&mut self, variant_id: &str, now_ms: u64) -> Option<CommitOutcome> {
        let variant = self.variants.get(variant_id)?.clone();
        self.active_commit = Some(CommitState {
            variant_id: variant_id.to_string(),
            started_ms: now_ms,
            accumulated_effort: variant.effort(),
        });
        Some(CommitOutcome {
            variant_id: variant_id.to_string(),
            planted_seeds: Vec::new(),
        })
    }

    pub fn pivot(&mut self, from: &str, to: &str, now_ms: u64) -> Option<PivotOutcome> {
        let current = self.active_commit.as_ref()?;
        if current.variant_id != from {
            return None;
        }
        let Some(next_variant) = self.variants.get(to) else {
            return None;
        };
        let elapsed = now_ms.saturating_sub(current.started_ms) as f32 / 1_000.0;
        let inertia = (current.accumulated_effort / next_variant.effort()) * (1.0 + elapsed * 0.05);
        self.active_commit = Some(CommitState {
            variant_id: to.to_string(),
            started_ms: now_ms,
            accumulated_effort: next_variant.effort(),
        });
        Some(PivotOutcome {
            from: from.to_string(),
            to: to.to_string(),
            inertia,
        })
    }

    pub fn defuse(&mut self, pendulum_id: &str, reduction: f32) -> Option<DefuseOutcome> {
        let pendulum = self.pendulums.get_mut(pendulum_id)?;
        let previous = pendulum.drain_rate;
        pendulum.drain_rate = (pendulum.drain_rate - reduction).max(0.0);
        Some(DefuseOutcome {
            pendulum_id: pendulum_id.to_string(),
            previous_drain: previous,
            new_drain: pendulum.drain_rate,
        })
    }

    pub fn rank_variants(
        &self,
        limit: Option<usize>,
        min_probability: Option<f32>,
    ) -> Vec<VariantScore> {
        let mut items: Vec<VariantScore> = self
            .variants
            .values()
            .map(|variant| {
                let resonance = self.resonance_for(variant);
                let (probability, effort, mut score) =
                    self.flow.score(variant, self.intention(), resonance);
                let penalty = self.pendulum_penalty(&variant.path_hint);
                if penalty > 0.0 {
                    score *= (1.0 - penalty).clamp(0.0, 1.0);
                }
                VariantScore {
                    id: variant.id.clone(),
                    title: variant.title.clone(),
                    probability,
                    effort,
                    score,
                    evidence: variant.evidence_score,
                    risk: variant.risk,
                    expected_gain: variant.expected_gain,
                }
            })
            .collect();
        items.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut filtered = Vec::new();
        for item in items.into_iter() {
            if let Some(threshold) = min_probability {
                if item.probability < threshold {
                    continue;
                }
            }
            filtered.push(item);
            if let Some(limit) = limit {
                if filtered.len() >= limit {
                    break;
                }
            }
        }
        filtered
    }

    fn pendulum_penalty(&self, tokens: &[String]) -> f32 {
        self.pendulums
            .values()
            .map(|pendulum| pendulum.penalty_for(tokens))
            .sum::<f32>()
            .min(0.95)
    }

    fn resonance_for(&self, variant: &Variant) -> f32 {
        let mut score = variant.resonance_hint.unwrap_or(0.0);
        if let Some(intention) = self.intention() {
            let weights = intention.normalized_weights();
            for (idx, token) in intention.focus_tokens.iter().enumerate() {
                if variant
                    .path_hint
                    .iter()
                    .any(|hint| hint.eq_ignore_ascii_case(token))
                    || variant.title.to_lowercase().contains(&token.to_lowercase())
                {
                    let weight = weights.get(idx).copied().unwrap_or(0.0);
                    score += weight;
                }
            }
        }
        if let Some(active_id) = &self.active_slide {
            if let Some(slide) = self.slides.get(active_id) {
                if slide.variant_ids.iter().any(|id| id == &variant.id) {
                    score += 0.2;
                }
            }
        }
        score.clamp(-1.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_variant(id: &str) -> Variant {
        Variant {
            id: id.into(),
            title: id.into(),
            evidence_score: 0.5,
            cost_est: 1.0,
            time_est_ms: 10_000,
            risk: 0.2,
            expected_gain: 0.4,
            path_hint: vec!["alpha".into()],
            resonance_hint: None,
        }
    }

    #[test]
    fn probability_increases_with_evidence_and_lower_cost() {
        let flow = ProbabilityFlow::default();
        let mut variant = simple_variant("v1");
        let base = flow.probability(&variant, None, 0.0);
        variant.evidence_score = 0.9;
        let higher = flow.probability(&variant, None, 0.0);
        variant.cost_est = 0.5;
        let lower_cost = flow.probability(&variant, None, 0.0);
        assert!(higher > base, "evidence should raise probability");
        assert!(
            lower_cost >= higher,
            "lower cost should not reduce probability"
        );
    }

    #[test]
    fn pivot_accounts_for_inertia() {
        let mut manifold = VariantManifold::default();
        let mut v1 = simple_variant("v1");
        v1.cost_est = 2.0;
        v1.time_est_ms = 20_000;
        let mut v2 = simple_variant("v2");
        v2.cost_est = 3.0;
        manifold.upsert_variant(v1.clone());
        manifold.upsert_variant(v2.clone());
        let commit = manifold.commit("v1", 1_000).expect("commit");
        assert_eq!(commit.variant_id, "v1");
        let pivot = manifold.pivot("v1", "v2", 11_000).expect("pivot");
        assert_eq!(pivot.from, "v1");
        assert_eq!(pivot.to, "v2");
        assert!(pivot.inertia > 0.0);
    }

    #[test]
    fn defuse_reduces_pendulum_drain_rate() {
        let mut manifold = VariantManifold::default();
        manifold.register_pendulum(Pendulum {
            id: "doomscroll".into(),
            signature_tokens: vec!["doomscroll".into()],
            drain_rate: 0.6,
            counter_policy: Some("focus hygiene".into()),
        });
        let outcome = manifold.defuse("doomscroll", 0.3).expect("defuse");
        assert_eq!(outcome.previous_drain, 0.6);
        assert!((outcome.new_drain - 0.3).abs() < f32::EPSILON);
    }
}
