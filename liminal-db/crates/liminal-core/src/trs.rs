use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrsState {
    pub alpha: f32,
    pub beta: f32,
    pub target_load: f32,
    pub err_i: f32,
    pub k_p: f32,
    pub k_i: f32,
    pub k_d: f32,
    pub last_err: f32,
    pub last_ts: u64,
}

impl Default for TrsState {
    fn default() -> Self {
        TrsState {
            alpha: 0.22,
            beta: 0.6,
            target_load: 0.6,
            err_i: 0.0,
            k_p: 0.85,
            k_i: 0.18,
            k_d: 0.05,
            last_err: 0.0,
            last_ts: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrsConfig {
    pub alpha: f32,
    pub beta: f32,
    pub target_load: f32,
    pub k_p: f32,
    pub k_i: f32,
    pub k_d: f32,
}

impl TrsState {
    pub fn apply_config(&mut self, cfg: &TrsConfig) {
        self.alpha = cfg.alpha.clamp(0.01, 0.95);
        self.beta = cfg.beta.clamp(0.0, 0.95);
        self.k_p = cfg.k_p.max(0.0);
        self.k_i = cfg.k_i.max(0.0);
        self.k_d = cfg.k_d.max(0.0);
        self.target_load = cfg.target_load.clamp(0.0, 1.0);
        self.err_i = 0.0;
        self.last_err = 0.0;
        self.last_ts = 0;
    }

    pub fn to_config(&self) -> TrsConfig {
        TrsConfig {
            alpha: self.alpha,
            beta: self.beta,
            target_load: self.target_load,
            k_p: self.k_p,
            k_i: self.k_i,
            k_d: self.k_d,
        }
    }

    pub fn set_target(&mut self, target: f32) {
        self.target_load = target.clamp(0.0, 1.0);
    }

    pub fn step(&mut self, now_ms: u64, observed_load: f32) -> TrsOutput {
        let observed = observed_load.clamp(0.0, 1.0);
        let target = self.target_load.clamp(0.0, 1.0);
        let err = target - observed;
        let dt_ms = if self.last_ts == 0 {
            1_000
        } else {
            now_ms.saturating_sub(self.last_ts).max(1)
        };
        let dt = dt_ms as f32 / 1_000.0;

        let beta = self.beta.clamp(0.0, 0.95);
        self.err_i = (self.err_i * beta + err * dt).clamp(-3.0, 3.0);
        let err_d = if self.last_ts == 0 {
            0.0
        } else {
            (err - self.last_err) / dt
        };

        let mut control = self.k_p * err + self.k_i * self.err_i + self.k_d * err_d;
        control = softsign(control);
        let damping = (1.0 - beta).clamp(0.15, 1.0);
        control *= damping;

        self.alpha = (self.alpha + control * 0.06).clamp(0.05, 0.95);
        self.last_err = err;
        self.last_ts = now_ms;

        let tick_adjust_ms = (-control * 140.0).round().clamp(-80.0, 80.0) as i32;
        let affinity_scale = (1.0 + control * 0.25).clamp(0.7, 1.35);
        let metabolism_scale = (1.0 + control * 0.3).clamp(0.5, 1.5);
        let sleep_threshold_delta = (-control * 0.1).clamp(-0.2, 0.2);

        TrsOutput {
            alpha_new: self.alpha,
            tick_adjust_ms,
            affinity_scale,
            metabolism_scale,
            sleep_threshold_delta,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrsOutput {
    pub alpha_new: f32,
    pub tick_adjust_ms: i32,
    pub affinity_scale: f32,
    pub metabolism_scale: f32,
    pub sleep_threshold_delta: f32,
}

fn softsign(x: f32) -> f32 {
    x / (1.0 + x.abs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trs_converges_toward_target() {
        let mut trs = TrsState::default();
        trs.set_target(0.65);
        let mut observed = 0.25f32;
        for step in 0..10 {
            let out = trs.step((step as u64 + 1) * 600, observed);
            // model environment response with smooth easing
            let adjust = (trs.target_load - observed) * (0.15 + (out.affinity_scale - 1.0).abs());
            observed = (observed + adjust).clamp(0.0, 1.0);
            // ensure adjustment direction follows error sign
            if trs.last_err > 0.0 {
                assert!(
                    out.tick_adjust_ms <= 0,
                    "expected faster ticks to raise load"
                );
            } else if trs.last_err < 0.0 {
                assert!(
                    out.tick_adjust_ms >= 0,
                    "expected slower ticks to reduce load"
                );
            }
        }
        assert!((observed - trs.target_load).abs() < 0.1);
    }
}
