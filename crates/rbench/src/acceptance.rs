//! Seeded synthetic suites and Monte Carlo checks for the confirmatory median
//! interval. These are **acceptance aids**, not a claim that every real
//! workload is IID.
//!
//! Generators cover IID uniform, heteroscedastic noise, linear drift, and AR(1)
//! autocorrelation. Coverage and A/A false-positive estimates are reported with
//! Wilson-style uncertainty notes for finite trials.
use crate::analysis::{median_interval, Decision};
use crate::{error, Availability, Case, Direction, Metric, Observation, Result, Run, Status};
use serde::Serialize;
use std::collections::BTreeMap;

/// Deterministic xorshift64* — no external RNG dependency.
#[derive(Clone, Debug)]
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed | 1)
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    pub fn uniform01(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / ((1u64 << 53) as f64)
    }
    pub fn uniform(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.uniform01()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Regime {
    IidUniform,
    Heteroscedastic,
    LinearDrift,
    Ar1,
}

/// Generate `n` process-level observations under the chosen dependence regime.
pub fn generate(regime: Regime, n: usize, seed: u64) -> Vec<f64> {
    let mut rng = Rng::new(seed);
    match regime {
        Regime::IidUniform => (0..n).map(|_| rng.uniform(0.0, 1.0)).collect(),
        Regime::Heteroscedastic => (0..n)
            .map(|i| {
                let scale = 0.2 + (i as f64 / n.max(1) as f64);
                rng.uniform(0.0, 1.0) * scale
            })
            .collect(),
        Regime::LinearDrift => (0..n)
            .map(|i| {
                let drift = i as f64 / n.max(1) as f64;
                rng.uniform(0.0, 1.0) + drift
            })
            .collect(),
        Regime::Ar1 => {
            let phi = 0.8;
            let mut x = 0.0;
            (0..n)
                .map(|_| {
                    x = phi * x + rng.uniform(-1.0, 1.0);
                    x
                })
                .collect()
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CoverageReport {
    pub regime: Regime,
    pub n: usize,
    pub trials: usize,
    pub alpha: f64,
    pub covered: usize,
    pub coverage: f64,
    pub wilson_low: f64,
    pub wilson_high: f64,
    pub note: String,
}

/// Monte Carlo coverage of [`median_interval`] against the known population
/// median of Uniform(0,1) (=0.5) for IID; for other regimes the target is the
/// sample-generating process median of a large reference draw (diagnostic only).
pub fn monte_carlo_coverage(
    regime: Regime,
    n: usize,
    trials: usize,
    alpha: f64,
    seed: u64,
) -> Result<CoverageReport> {
    if n < 2 || trials == 0 || !(0.0..1.0).contains(&alpha) {
        return Err(error("n>=2, trials>=1, alpha in (0,1) required"));
    }
    let target = match regime {
        Regime::IidUniform => 0.5,
        other => {
            let big = generate(other, 50_000, seed ^ 0xC0FFEE);
            crate::analysis::median(&big)
        }
    };
    let mut covered = 0usize;
    let mut rng_seed = seed;
    for _ in 0..trials {
        let sample = generate(regime, n, rng_seed);
        rng_seed = rng_seed.wrapping_add(0x9E3779B97F4A7C15);
        if let Some((lo, hi)) = median_interval(&sample, alpha) {
            if lo <= target && target <= hi {
                covered += 1;
            }
        }
    }
    let coverage = covered as f64 / trials as f64;
    let (wilson_low, wilson_high) = wilson(covered, trials, 0.05);
    Ok(CoverageReport {
        regime,
        n,
        trials,
        alpha,
        covered,
        coverage,
        wilson_low,
        wilson_high,
        note: match regime {
            Regime::IidUniform => {
                "Target median 0.5 of Uniform(0,1). Exact binomial interval assumes IID continuous samples."
                    .into()
            }
            Regime::Heteroscedastic | Regime::LinearDrift | Regime::Ar1 => {
                "Target is a large reference median under the same generator — diagnostic when IID assumption fails; under-coverage is expected for Ar1/drift."
                    .into()
            }
        },
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct AaReport {
    pub pairs: usize,
    pub trials: usize,
    pub threshold_percent: f64,
    pub alpha: f64,
    pub false_regressions: usize,
    pub false_improvements: usize,
    pub inconclusive_or_within: usize,
    pub false_positive_rate: f64,
    pub wilson_low: f64,
    pub wilson_high: f64,
    pub note: String,
}

/// A/A false-positive rate under IID paired noise with the same confirmatory
/// [`crate::analysis::compare`] decision rule used in CI gates.
pub fn aa_false_positive_rate(
    pairs: usize,
    trials: usize,
    threshold_percent: f64,
    alpha: f64,
    seed: u64,
) -> Result<AaReport> {
    if pairs < 6 || trials == 0 {
        return Err(error("pairs>=6 and trials>=1 required for A/A probe"));
    }
    let mut false_reg = 0usize;
    let mut false_imp = 0usize;
    let mut ok = 0usize;
    let mut seed = seed;
    for _ in 0..trials {
        let run = synthetic_aa(pairs as u32, seed)?;
        seed = seed.wrapping_add(0xD1B54A32D192ED03);
        let rows = crate::analysis::compare(&run, None, threshold_percent, alpha)?;
        match rows[0].decision {
            Decision::Regression => false_reg += 1,
            Decision::Improvement => false_imp += 1,
            Decision::WithinMargin | Decision::Inconclusive | Decision::Neutral => ok += 1,
            Decision::Unavailable => {
                return Err(error("A/A probe produced Unavailable decision"));
            }
        }
    }
    let fp = false_reg + false_imp;
    let rate = fp as f64 / trials as f64;
    let (wilson_low, wilson_high) = wilson(fp, trials, 0.05);
    Ok(AaReport {
        pairs,
        trials,
        threshold_percent,
        alpha,
        false_regressions: false_reg,
        false_improvements: false_imp,
        inconclusive_or_within: ok,
        false_positive_rate: rate,
        wilson_low,
        wilson_high,
        note: "IID paired Uniform noise, identical distributions. Family size 1. Not a substitute for real A/A on hardware under thermal drift.".into(),
    })
}

fn synthetic_aa(pairs: u32, seed: u64) -> Result<Run> {
    let mut rng = Rng::new(seed);
    let mut r = Run::new();
    r.cases.push(Case {
        id: "aa".into(),
        contract: BTreeMap::from([("synthetic".into(), "iid_uniform_aa".into())]),
        metrics: vec![Metric {
            id: "latency".into(),
            unit: "ns".into(),
            scope: "synthetic".into(),
            phase: "measurement".into(),
            statistic: "process_total".into(),
            direction: Direction::Lower,
        }],
    });
    for p in 0..pairs {
        for (i, variant) in [(0u32, "baseline"), (1, "candidate")] {
            let value = 1000.0 + rng.uniform(0.0, 100.0);
            r.observations.push(Observation {
                case: "aa".into(),
                metric: "latency".into(),
                variant: variant.into(),
                process: 2 * p + i,
                pair: Some(p),
                sequence: 0,
                value: Some(format!("{value:.6}")),
                operations: 1,
                availability: Availability::Available,
            });
        }
    }
    r.status = Status::Complete;
    r.validate()?;
    Ok(r)
}

/// Wilson score interval for a binomial proportion (z≈1.96).
fn wilson(successes: usize, n: usize, alpha: f64) -> (f64, f64) {
    let _ = alpha;
    let z = 1.96;
    let n = n as f64;
    let p = successes as f64 / n.max(1.0);
    let denom = 1.0 + z * z / n;
    let centre = p + z * z / (2.0 * n);
    let margin = z * ((p * (1.0 - p) + z * z / (4.0 * n)) / n).sqrt();
    ((centre - margin) / denom, (centre + margin) / denom)
}

/// Run the default acceptance battery used by `cargo rbench accept`.
pub fn battery(seed: u64) -> Result<serde_json::Value> {
    let coverage_iid = monte_carlo_coverage(Regime::IidUniform, 40, 400, 0.05, seed)?;
    let coverage_ar1 = monte_carlo_coverage(Regime::Ar1, 40, 200, 0.05, seed ^ 1)?;
    let coverage_drift = monte_carlo_coverage(Regime::LinearDrift, 40, 200, 0.05, seed ^ 2)?;
    let aa = aa_false_positive_rate(12, 200, 5.0, 0.05, seed ^ 3)?;
    Ok(serde_json::json!({
        "schema": 1,
        "seed": seed,
        "coverage": [coverage_iid, coverage_ar1, coverage_drift],
        "aa_false_positive": aa,
        "interpretation": "IID coverage should sit near 1-alpha. Ar1/drift under-coverage demonstrates why confirmatory analysis assumes independent process pairs. A/A FPR is descriptive for this synthetic battery only."
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iid_coverage_near_nominal() {
        let r = monte_carlo_coverage(Regime::IidUniform, 40, 300, 0.05, 7).unwrap();
        assert!(
            r.coverage > 0.90,
            "expected coverage near 0.95, got {}",
            r.coverage
        );
    }

    #[test]
    fn ar1_often_undercovers_relative_to_iid() {
        let iid = monte_carlo_coverage(Regime::IidUniform, 30, 200, 0.05, 11).unwrap();
        let ar1 = monte_carlo_coverage(Regime::Ar1, 30, 200, 0.05, 11).unwrap();
        // Soft check: AR(1) should not look better-calibrated than IID here.
        assert!(ar1.coverage <= iid.coverage + 0.05);
    }

    #[test]
    fn aa_probe_runs() {
        let r = aa_false_positive_rate(12, 50, 5.0, 0.05, 99).unwrap();
        assert_eq!(r.false_regressions + r.false_improvements + r.inconclusive_or_within, 50);
    }
}
