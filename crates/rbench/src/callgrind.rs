//! Callgrind / Cachegrind deterministic counters for CI (iai-class).
//!
//! Runs a program under `valgrind --tool=callgrind` (or parses an existing
//! `callgrind.out*` file) and emits typed metrics. When Valgrind is missing or
//! the tool is denied, probes return [`Availability::Unsupported`] /
//! [`Availability::PermissionDenied`] — never fabricated zeroes.
//!
//! Counts include Valgrind instrumentation overhead and are not wall-time.
use crate::{error, Availability, Direction, Metric, Observation, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    pub instructions: Option<u64>,
    pub data_reads: Option<u64>,
    pub data_writes: Option<u64>,
    pub l1i_misses: Option<u64>,
    pub l1d_misses: Option<u64>,
    pub lli_misses: Option<u64>,
    pub lld_misses: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Probe {
    pub availability: Availability,
    pub note: String,
    pub valgrind: Option<String>,
}

/// Probe whether `valgrind` is usable for Callgrind runs.
pub fn probe() -> Probe {
    match Command::new("valgrind").arg("--version").output() {
        Ok(out) if out.status.success() => {
            let ver = String::from_utf8_lossy(&out.stdout).trim().to_string();
            Probe {
                availability: Availability::Available,
                note: format!("valgrind available for Callgrind ({ver})"),
                valgrind: Some(ver),
            }
        }
        Ok(out) => Probe {
            availability: Availability::PermissionDenied(format!(
                "valgrind exited {}",
                out.status
            )),
            note: "valgrind present but failed --version".into(),
            valgrind: None,
        },
        Err(e) => Probe {
            availability: Availability::Unsupported(format!("valgrind not runnable: {e}")),
            note: "install valgrind to enable Callgrind deterministic CI counters".into(),
            valgrind: None,
        },
    }
}

pub fn metrics() -> Vec<Metric> {
    let mk = |id: &str, unit: &str, scope: &str| Metric {
        id: id.into(),
        unit: unit.into(),
        scope: scope.into(),
        phase: "measurement".into(),
        statistic: "process_total".into(),
        direction: Direction::Lower,
    };
    vec![
        mk(
            "callgrind.instructions",
            "Ir",
            "Callgrind Ir (instruction fetches); Valgrind-instrumented, not wall time",
        ),
        mk(
            "callgrind.data_reads",
            "Dr",
            "Callgrind Dr (data reads); Valgrind-instrumented",
        ),
        mk(
            "callgrind.data_writes",
            "Dw",
            "Callgrind Dw (data writes); Valgrind-instrumented",
        ),
        mk(
            "callgrind.l1i_misses",
            "I1mr",
            "Callgrind I1mr (L1 instruction misses)",
        ),
        mk(
            "callgrind.l1d_misses",
            "D1mr",
            "Callgrind D1mr (L1 data read misses)",
        ),
        mk(
            "callgrind.lli_misses",
            "ILmr",
            "Callgrind ILmr (LL instruction misses)",
        ),
        mk(
            "callgrind.lld_misses",
            "DLmr",
            "Callgrind DLmr (LL data read misses)",
        ),
    ]
}

pub fn observations(
    case: &str,
    variant: &str,
    process: u32,
    sequence: u64,
    counts: &Counts,
    availability: &Availability,
) -> Vec<Observation> {
    let mk = |metric: &str, value: Option<u64>| Observation {
        case: case.into(),
        metric: metric.into(),
        variant: variant.into(),
        process,
        pair: None,
        sequence,
        value: value.map(|v| v.to_string()),
        operations: 1,
        availability: if *availability == Availability::Available && value.is_some() {
            Availability::Available
        } else if *availability != Availability::Available {
            availability.clone()
        } else {
            Availability::Unsupported("counter value missing".into())
        },
    };
    vec![
        mk("callgrind.instructions", counts.instructions),
        mk("callgrind.data_reads", counts.data_reads),
        mk("callgrind.data_writes", counts.data_writes),
        mk("callgrind.l1i_misses", counts.l1i_misses),
        mk("callgrind.l1d_misses", counts.l1d_misses),
        mk("callgrind.lli_misses", counts.lli_misses),
        mk("callgrind.lld_misses", counts.lld_misses),
    ]
}

/// Parse Callgrind / Cachegrind summary events from file contents or stderr.
///
/// Accepts lines like `summary: Ir Dr Dw ...` followed by totals, or
/// `events: Ir Dr` / `summary: N M` pairs, and `==N==  I   refs:` Cachegrind style.
pub fn parse_summary(text: &str) -> Result<Counts> {
    if let Some(c) = parse_callgrind_summary(text) {
        return Ok(c);
    }
    if let Some(c) = parse_cachegrind_refs(text) {
        return Ok(c);
    }
    Err(error(
        "no Callgrind/Cachegrind summary found in output (need events/summary or I refs)",
    ))
}

fn parse_callgrind_summary(text: &str) -> Option<Counts> {
    let mut events: Vec<String> = Vec::new();
    let mut summary: Option<Vec<u64>> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("events:") {
            events = rest.split_whitespace().map(|s| s.to_string()).collect();
        } else if let Some(rest) = line.strip_prefix("summary:") {
            let nums: std::result::Result<Vec<u64>, _> = rest
                .split_whitespace()
                .map(|s| s.parse::<u64>())
                .collect();
            if let Ok(n) = nums {
                summary = Some(n);
            }
        }
    }
    let summary = summary?;
    if events.is_empty() || events.len() != summary.len() {
        // Some Callgrind files only print `summary: <Ir>` with events earlier.
        if summary.len() == 1 {
            return Some(Counts {
                instructions: Some(summary[0]),
                ..Counts::default()
            });
        }
        return None;
    }
    let map: BTreeMap<_, _> = events.into_iter().zip(summary).collect();
    Some(Counts {
        instructions: map.get("Ir").copied(),
        data_reads: map.get("Dr").copied(),
        data_writes: map.get("Dw").copied(),
        l1i_misses: map.get("I1mr").copied(),
        l1d_misses: map.get("D1mr").copied().or_else(|| map.get("D1mw").copied()),
        lli_misses: map.get("ILmr").copied(),
        lld_misses: map.get("DLmr").copied().or_else(|| map.get("DLmw").copied()),
    })
}

fn parse_cachegrind_refs(text: &str) -> Option<Counts> {
    let mut instructions = None;
    let mut data_reads = None;
    let mut data_writes = None;
    for line in text.lines() {
        let l = line.trim();
        // "==123== I   refs:      1,234,567"
        if let Some(rest) = l.split_once("I   refs:").or_else(|| l.split_once("I refs:")) {
            instructions = parse_commas(rest.1);
        } else if let Some(rest) = l.split_once("D   refs:") {
            // D refs line often includes rd/wr breakdown later; take first number.
            data_reads = parse_commas(rest.1);
        } else if let Some(rest) = l.split_once("D1  misses:") {
            let _ = rest;
        } else if let Some(rest) = l.split_once(" wr:") {
            if data_writes.is_none() {
                data_writes = parse_commas(rest.1);
            }
        }
    }
    if instructions.is_some() {
        Some(Counts {
            instructions,
            data_reads,
            data_writes,
            ..Counts::default()
        })
    } else {
        None
    }
}

fn parse_commas(s: &str) -> Option<u64> {
    let cleaned: String = s
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == ',' || *c == ' ')
        .filter(|c| c.is_ascii_digit())
        .collect();
    if cleaned.is_empty() {
        None
    } else {
        cleaned.parse().ok()
    }
}

/// Parse an on-disk Callgrind output file.
pub fn parse_file(path: &Path) -> Result<Counts> {
    let text = fs::read_to_string(path).map_err(|e| error(format!("read {}: {e}", path.display())))?;
    parse_summary(&text)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RunReport {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub callgrind_out: PathBuf,
    pub counts: Counts,
    pub availability: Availability,
    pub note: String,
    pub valgrind_stderr: String,
}

/// Execute `program` under Callgrind and parse the resulting counters.
pub fn run_program(
    program: &Path,
    args: &[String],
    out_dir: &Path,
    extra_valgrind_args: &[String],
) -> Result<RunReport> {
    let probe = probe();
    if probe.availability != Availability::Available {
        return Ok(RunReport {
            program: program.to_path_buf(),
            args: args.to_vec(),
            callgrind_out: out_dir.join("callgrind.out"),
            counts: Counts::default(),
            availability: probe.availability,
            note: probe.note,
            valgrind_stderr: String::new(),
        });
    }
    fs::create_dir_all(out_dir).map_err(|e| error(format!("create {}: {e}", out_dir.display())))?;
    let out_file = out_dir.join("callgrind.out");
    let mut cmd = Command::new("valgrind");
    cmd.arg("--tool=callgrind")
        .arg(format!("--callgrind-out-file={}", out_file.display()))
        .arg("--instr-atstart=yes")
        .arg("--collect-jumps=yes")
        .arg("--quiet");
    for a in extra_valgrind_args {
        cmd.arg(a);
    }
    cmd.arg(program);
    for a in args {
        cmd.arg(a);
    }
    let output = cmd
        .output()
        .map_err(|e| error(format!("spawn valgrind: {e}")))?;
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        let availability = if stderr.to_lowercase().contains("permission") {
            Availability::PermissionDenied(stderr.clone())
        } else {
            Availability::Invalid(format!(
                "valgrind/callgrind failed: {}; stderr: {}",
                output.status, stderr
            ))
        };
        return Ok(RunReport {
            program: program.to_path_buf(),
            args: args.to_vec(),
            callgrind_out: out_file,
            counts: Counts::default(),
            availability,
            note: "Callgrind execution failed".into(),
            valgrind_stderr: stderr,
        });
    }
    // Prefer the out file; fall back to stderr summaries.
    let counts = if out_file.exists() {
        parse_file(&out_file).or_else(|_| parse_summary(&stderr))?
    } else {
        parse_summary(&stderr)?
    };
    if counts.instructions.is_none() {
        return Ok(RunReport {
            program: program.to_path_buf(),
            args: args.to_vec(),
            callgrind_out: out_file,
            counts,
            availability: Availability::Invalid(
                "Callgrind finished but Ir summary missing".into(),
            ),
            note: "parse incomplete".into(),
            valgrind_stderr: stderr,
        });
    }
    Ok(RunReport {
        program: program.to_path_buf(),
        args: args.to_vec(),
        callgrind_out: out_file,
        counts,
        availability: Availability::Available,
        note: "Callgrind Ir/Dr/Dw collected under Valgrind".into(),
        valgrind_stderr: stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_events_summary() {
        let text = "\
events: Ir Dr Dw I1mr D1mr
summary: 1000 200 50 3 7
";
        let c = parse_summary(text).unwrap();
        assert_eq!(c.instructions, Some(1000));
        assert_eq!(c.data_reads, Some(200));
        assert_eq!(c.data_writes, Some(50));
        assert_eq!(c.l1i_misses, Some(3));
        assert_eq!(c.l1d_misses, Some(7));
    }

    #[test]
    fn parse_cachegrind_style() {
        let text = "==1== I   refs:      12,345\n==1== D   refs:       6,789\n";
        let c = parse_summary(text).unwrap();
        assert_eq!(c.instructions, Some(12345));
        assert_eq!(c.data_reads, Some(6789));
    }

    #[test]
    fn probe_does_not_panic() {
        let _ = probe();
    }
}
