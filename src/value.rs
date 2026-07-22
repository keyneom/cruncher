//! First-class value types for curves and distributions (MVP stubs).
//!
//! These types are the foundation for Monte Carlo / scenario evaluation.
//! Sampling fills columnar `f64` buffers that plug into [`crate::Expr::eval_batch`]
//! and [`crate::CompiledExpr`].

use std::f64::consts::PI;

/// Dense columnar curve (timeseries, scenario index, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct Curve {
    pub values: Vec<f64>,
}

impl Curve {
    #[must_use]
    pub fn new(values: Vec<f64>) -> Self {
        Self { values }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    #[must_use]
    pub fn as_slice(&self) -> &[f64] {
        &self.values
    }

    /// Pointwise mean (empty → `NaN`).
    #[must_use]
    pub fn mean(&self) -> f64 {
        if self.values.is_empty() {
            return f64::NAN;
        }
        self.values.iter().sum::<f64>() / self.values.len() as f64
    }
}

/// Runtime value returned by future typed evaluators.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Scalar(f64),
    Curve(Curve),
}

/// Something that can draw columnar Monte Carlo samples.
pub trait Distribution {
    /// Write `out.len()` independent samples into `out`.
    fn sample_into(&self, out: &mut [f64], rng: &mut dyn RngCore);
}

/// Minimal RNG trait so the MVP does not pull in `rand` yet.
///
/// Adapters can wrap `rand::Rng` later; tests use [`Lcg64`].
pub trait RngCore {
    fn next_u64(&mut self) -> u64;

    fn next_f64(&mut self) -> f64 {
        // (0, 1] open-closed via bit twiddle
        let bits = (self.next_u64() >> 11) | 1;
        (bits as f64) * (1.0 / ((1u64 << 53) as f64))
    }
}

/// Tiny LCG for reproducible stub sampling (not cryptographic).
#[derive(Debug, Clone)]
pub struct Lcg64 {
    state: u64,
}

impl Lcg64 {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed | 1, // avoid zero lock
        }
    }
}

impl RngCore for Lcg64 {
    fn next_u64(&mut self) -> u64 {
        // Numerical Recipes LCG
        self.state = self.state.wrapping_mul(6364136223846793005).wrapping_add(1);
        self.state
    }
}

/// Parametric distributions used in scenario equations.
#[derive(Debug, Clone, PartialEq)]
pub enum Dist {
    Normal { mean: f64, sd: f64 },
    Uniform { lo: f64, hi: f64 },
    Empirical { samples: Curve },
}

impl Dist {
    #[must_use]
    pub fn normal(mean: f64, sd: f64) -> Self {
        Self::Normal { mean, sd }
    }

    #[must_use]
    pub fn uniform(lo: f64, hi: f64) -> Self {
        Self::Uniform { lo, hi }
    }

    /// Allocate and fill `n` samples.
    pub fn sample(&self, n: usize, rng: &mut dyn RngCore) -> Curve {
        let mut values = vec![0.0; n];
        self.sample_into(&mut values, rng);
        Curve::new(values)
    }
}

impl Distribution for Dist {
    fn sample_into(&self, out: &mut [f64], rng: &mut dyn RngCore) {
        match *self {
            Self::Uniform { lo, hi } => {
                let span = hi - lo;
                for x in out.iter_mut() {
                    *x = lo + span * rng.next_f64();
                }
            }
            Self::Normal { mean, sd } => {
                // Box–Muller in pairs
                let mut i = 0;
                while i < out.len() {
                    let u1 = rng.next_f64().max(f64::EPSILON);
                    let u2 = rng.next_f64();
                    let r = (-2.0 * u1.ln()).sqrt();
                    let theta = 2.0 * PI * u2;
                    out[i] = mean + sd * r * theta.cos();
                    i += 1;
                    if i < out.len() {
                        out[i] = mean + sd * r * theta.sin();
                        i += 1;
                    }
                }
            }
            Self::Empirical { ref samples } => {
                if samples.is_empty() {
                    for x in out.iter_mut() {
                        *x = f64::NAN;
                    }
                    return;
                }
                let n = samples.len();
                for x in out.iter_mut() {
                    let idx = (rng.next_u64() as usize) % n;
                    *x = samples.values[idx];
                }
            }
        }
    }
}

/// Illustrative Disney theme-park revenue scenario (see `docs/ROADMAP.md`).
///
/// ```text
/// stay_days ~ Uniform(1, 4)   // continuous stand-in for discrete stay
/// spend_per_day ~ Normal(mu, sigma)
/// revenue = stay_days * spend_per_day * guests
/// ```
pub fn theme_park_revenue_columns(
    n: usize,
    guests: f64,
    spend_mean: f64,
    spend_sd: f64,
    rng: &mut dyn RngCore,
) -> (Curve, Curve, Curve) {
    let stay = Dist::uniform(1.0, 4.0).sample(n, rng);
    let spend = Dist::normal(spend_mean, spend_sd).sample(n, rng);
    let mut revenue = Vec::with_capacity(n);
    for i in 0..n {
        revenue.push(stay.values[i] * spend.values[i] * guests);
    }
    (stay, spend, Curve::new(revenue))
}

#[cfg(test)]
mod tests {
    use super::{theme_park_revenue_columns, Dist, Distribution, Lcg64};

    #[test]
    fn uniform_in_range() {
        let mut rng = Lcg64::new(42);
        let mut out = [0.0; 1000];
        Dist::uniform(0.0, 1.0).sample_into(&mut out, &mut rng);
        assert!(out.iter().all(|&x| (0.0..=1.0).contains(&x)));
    }

    #[test]
    fn normal_mean_roughly() {
        let mut rng = Lcg64::new(7);
        let curve = Dist::normal(100.0, 5.0).sample(50_000, &mut rng);
        let mean = curve.mean();
        assert!((mean - 100.0).abs() < 0.5, "mean={mean}");
    }

    #[test]
    fn theme_park_scenario_runs() {
        let mut rng = Lcg64::new(1);
        let (_stay, _spend, revenue) = theme_park_revenue_columns(10_000, 4.0, 250.0, 40.0, &mut rng);
        assert_eq!(revenue.len(), 10_000);
        assert!(revenue.mean().is_finite());
    }
}
