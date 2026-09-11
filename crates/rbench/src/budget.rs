//! Declarative exact-case budgets. Missing metrics and uncertainty never silently pass.
//!
//! Optional `groups` require **all** member budgets to pass (wall ∩ throughput ∩ RSS).
use crate::{analysis, error, Result, Run, Status};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetFile {
    pub budgets: Vec<Budget>,
    /// Conjunctive gates over already-declared budgets (AND). Empty by default.
    #[serde(default)]
    pub groups: Vec<BudgetGroup>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub case: String,
    pub metric: String,
    pub unit: String,
    pub max: Option<f64>,
    pub min: Option<f64>,
    pub max_regression_percent: Option<f64>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetGroup {
    pub id: String,
    /// Only `"all"` is supported (AND). Present for forward-compatible schemas.
    #[serde(default = "require_all")]
    pub require: String,
    pub members: Vec<BudgetRef>,
}

fn require_all() -> String {
    "all".into()
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetRef {
    pub case: String,
    pub metric: String,
}

#[derive(Debug, Serialize)]
pub struct Outcome {
    pub case: String,
    pub metric: String,
    pub decision: String,
    pub detail: String,
}

fn select(run: &Run, case: &str, metric: &str) -> Result<Run> {
    let mut r = run.clone();
    r.cases.retain(|c| c.id == case);
    for c in &mut r.cases {
        c.metrics.retain(|m| m.id == metric);
    }
    r.observations
        .retain(|o| o.case == case && o.metric == metric);
    r.validate()?;
    Ok(r)
}

pub fn evaluate(config: &BudgetFile, run: &Run, baseline: Option<&Run>) -> Result<Vec<Outcome>> {
    run.validate()?;
    if run.status != Status::Complete {
        return Err(error("budget requires complete run"));
    }
    if config.budgets.is_empty() {
        return Err(error("budget configuration is empty"));
    }
    let mut seen = std::collections::BTreeSet::new();
    let family = config
        .budgets
        .iter()
        .filter(|b| b.max_regression_percent.is_some())
        .count()
        .max(1);
    let mut out = vec![];
    for b in &config.budgets {
        if !seen.insert((&b.case, &b.metric))
            || [b.max, b.min, b.max_regression_percent]
                .into_iter()
                .flatten()
                .any(|v| !v.is_finite() || v < 0.)
            || (b.max.is_none() && b.min.is_none() && b.max_regression_percent.is_none())
            || b.min.zip(b.max).is_some_and(|(a, z)| a > z)
        {
            return Err(error(
                "invalid/duplicate budget; use finite nonnegative bounds",
            ));
        }
        let check = || -> Result<(String, String)> {
            let r = select(run, &b.case, &b.metric)?;
            let m = r
                .cases
                .first()
                .and_then(|c| c.metrics.first())
                .ok_or_else(|| error("required case/metric missing"))?;
            if m.unit != b.unit {
                return Err(error(format!(
                    "unit mismatch: expected {}, found {}",
                    b.unit, m.unit
                )));
            }
            let mut count = 0;
            let mut violated = false;
            for o in r.observations.iter().filter(|o| o.variant == "candidate") {
                let n = o
                    .number()?
                    .ok_or_else(|| error("required candidate metric unavailable"))?;
                let n = if m.statistic == "batch_total" {
                    n / o.operations as f64
                } else {
                    n
                };
                violated |= b.max.is_some_and(|v| n > v) || b.min.is_some_and(|v| n < v);
                count += 1;
            }
            if count == 0 {
                return Err(error("candidate observations missing"));
            }
            if violated {
                return Ok((
                    "failed".into(),
                    format!("absolute bound exceeded among {count} candidate observations"),
                ));
            }
            if let Some(threshold) = b.max_regression_percent {
                let a = baseline
                    .map(|v| select(v, &b.case, &b.metric))
                    .transpose()?;
                let rows = if let Some(a) = a.as_ref() {
                    analysis::compare(a, Some(&r), threshold, 0.05 / family as f64)?
                } else {
                    analysis::compare(&r, None, threshold, 0.05 / family as f64)?
                };
                let result = &rows[0];
                use analysis::Decision::*;
                return Ok((
                    match result.decision {
                        Regression => "failed",
                        Improvement | WithinMargin => "passed",
                        _ => "inconclusive",
                    }
                    .into(),
                    format!(
                        "{:?}; interval {:?}; Bonferroni across {family} relative budgets",
                        result.decision, result.interval_percent
                    ),
                ));
            }
            Ok((
                "passed".into(),
                format!("{count} candidate observations satisfy absolute bounds"),
            ))
        };
        let (decision, detail) = match check() {
            Ok(v) => v,
            Err(e) => ("unavailable".into(), e.to_string()),
        };
        out.push(Outcome {
            case: b.case.clone(),
            metric: b.metric.clone(),
            decision,
            detail,
        });
    }
    out.extend(evaluate_groups(config, &out)?);
    Ok(out)
}

fn evaluate_groups(config: &BudgetFile, rows: &[Outcome]) -> Result<Vec<Outcome>> {
    let mut group_ids = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for g in &config.groups {
        if !group_ids.insert(g.id.as_str()) {
            return Err(error(format!("duplicate budget group id '{}'", g.id)));
        }
        if g.require != "all" {
            return Err(error(format!(
                "budget group '{}': only require=\"all\" (AND) is supported",
                g.id
            )));
        }
        if g.members.is_empty() {
            return Err(error(format!(
                "budget group '{}' has no members",
                g.id
            )));
        }
        let mut member_rows = Vec::new();
        for m in &g.members {
            let row = rows
                .iter()
                .find(|r| r.case == m.case && r.metric == m.metric)
                .ok_or_else(|| {
                    error(format!(
                        "budget group '{}': member {}/{} is not declared in budgets",
                        g.id, m.case, m.metric
                    ))
                })?;
            member_rows.push(row);
        }
        let failed = member_rows.iter().any(|r| r.decision == "failed");
        let unavailable = member_rows.iter().any(|r| r.decision == "unavailable");
        let inconclusive = member_rows.iter().any(|r| r.decision == "inconclusive");
        let (decision, detail) = if failed {
            (
                "failed".into(),
                format!(
                    "AND group failed; members: {}",
                    member_summary(&member_rows)
                ),
            )
        } else if unavailable {
            (
                "unavailable".into(),
                format!(
                    "AND group unavailable; members: {}",
                    member_summary(&member_rows)
                ),
            )
        } else if inconclusive {
            (
                "inconclusive".into(),
                format!(
                    "AND group inconclusive; members: {}",
                    member_summary(&member_rows)
                ),
            )
        } else {
            (
                "passed".into(),
                format!(
                    "AND group passed; members: {}",
                    member_summary(&member_rows)
                ),
            )
        };
        out.push(Outcome {
            case: g.id.clone(),
            metric: "group".into(),
            decision,
            detail,
        });
    }
    Ok(out)
}

fn member_summary(rows: &[&Outcome]) -> String {
    rows.iter()
        .map(|r| format!("{}/{}={}", r.case, r.metric, r.decision))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn exit_code(rows: &[Outcome]) -> i32 {
    if rows.iter().any(|r| r.decision == "failed") {
        1
    } else if rows.iter().any(|r| r.decision != "passed") {
        2
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Availability, Case, Direction, Metric, Observation, Run, Status,
    };
    use std::collections::BTreeMap;

    fn run_with(wall: f64, thr: f64, rss: f64) -> Run {
        let mut run = Run::new();
        run.status = Status::Complete;
        run.cases.push(Case {
            id: "ship".into(),
            contract: BTreeMap::new(),
            metrics: vec![
                Metric {
                    id: "wall".into(),
                    unit: "ns".into(),
                    scope: "test".into(),
                    phase: "measurement".into(),
                    statistic: "sample".into(),
                    direction: Direction::Lower,
                },
                Metric {
                    id: "throughput".into(),
                    unit: "ops/s".into(),
                    scope: "test".into(),
                    phase: "measurement".into(),
                    statistic: "sample".into(),
                    direction: Direction::Higher,
                },
                Metric {
                    id: "os.rss_peak".into(),
                    unit: "bytes".into(),
                    scope: "test".into(),
                    phase: "measurement".into(),
                    statistic: "sample".into(),
                    direction: Direction::Lower,
                },
            ],
        });
        for (metric, value) in [
            ("wall", wall),
            ("throughput", thr),
            ("os.rss_peak", rss),
        ] {
            run.observations.push(Observation {
                case: "ship".into(),
                metric: metric.into(),
                variant: "candidate".into(),
                process: 0,
                pair: None,
                sequence: 0,
                value: Some(value.to_string()),
                operations: 1,
                availability: Availability::Available,
            });
        }
        run
    }

    #[test]
    fn and_group_requires_all_members() {
        let config: BudgetFile = serde_json::from_str(
            r#"{
              "budgets": [
                {"case":"ship","metric":"wall","unit":"ns","max":100},
                {"case":"ship","metric":"throughput","unit":"ops/s","min":10},
                {"case":"ship","metric":"os.rss_peak","unit":"bytes","max":1000}
              ],
              "groups": [{
                "id": "ship_gate",
                "require": "all",
                "members": [
                  {"case":"ship","metric":"wall"},
                  {"case":"ship","metric":"throughput"},
                  {"case":"ship","metric":"os.rss_peak"}
                ]
              }]
            }"#,
        )
        .unwrap();
        let ok = evaluate(&config, &run_with(50.0, 20.0, 500.0), None).unwrap();
        let group = ok.iter().find(|r| r.metric == "group").unwrap();
        assert_eq!(group.decision, "passed");
        assert_eq!(exit_code(&ok), 0);

        let bad = evaluate(&config, &run_with(50.0, 2.0, 500.0), None).unwrap();
        let group = bad.iter().find(|r| r.metric == "group").unwrap();
        assert_eq!(group.decision, "failed");
        assert_eq!(exit_code(&bad), 1);
    }
}
