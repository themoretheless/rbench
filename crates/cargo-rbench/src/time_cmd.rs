//! Hyperfine-class command wall timing without the full experiment stack.
//!
//! Blocks on `Child::wait` (no 1 ms polling) unless a timeout is set. Supports
//! warmup, shell mode, prepare/cleanup, and JSON/Markdown summaries. Optional
//! Run artifact for gates.
use rbench::{
    error, Availability, Case, Direction, Metric, Observation, Result, Run, Status,
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct Request {
    pub command: Vec<String>,
    pub shell: bool,
    pub runs: u32,
    pub warmup: u32,
    pub prepare: Option<String>,
    pub cleanup: Option<String>,
    pub timeout: Option<Duration>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Summary {
    pub command: String,
    pub runs: u32,
    pub warmup: u32,
    pub samples_ns: Vec<u128>,
    pub mean_ns: f64,
    pub stddev_ns: f64,
    pub median_ns: u128,
    pub min_ns: u128,
    pub max_ns: u128,
    pub note: String,
}

pub fn run(req: &Request) -> Result<(Summary, Run)> {
    if req.command.is_empty() {
        return Err(error("time requires a command"));
    }
    if req.runs == 0 || req.runs > 10_000 {
        return Err(error("runs must be 1..=10000"));
    }
    let display = if req.shell {
        req.command.join(" ")
    } else {
        req.command
            .iter()
            .map(|s| shell_escape(s))
            .collect::<Vec<_>>()
            .join(" ")
    };
    for _ in 0..req.warmup {
        if let Some(p) = &req.prepare {
            run_shell(p)?;
        }
        let status = spawn(req)?
            .wait()
            .map_err(|e| error(format!("wait: {e}")))?;
        if !status.success() {
            return Err(error(format!("warmup command failed with {status}")));
        }
        if let Some(c) = &req.cleanup {
            run_shell(c)?;
        }
    }
    let mut samples = Vec::with_capacity(req.runs as usize);
    for _ in 0..req.runs {
        if let Some(p) = &req.prepare {
            run_shell(p)?;
        }
        let start = Instant::now();
        let mut child = spawn(req)?;
        let status = if let Some(limit) = req.timeout {
            wait_timeout(&mut child, limit)?
        } else {
            child.wait().map_err(|e| error(format!("wait: {e}")))?
        };
        let elapsed = start.elapsed().as_nanos();
        if !status.success() {
            return Err(error(format!("timed command failed with {status}")));
        }
        samples.push(elapsed);
        if let Some(c) = &req.cleanup {
            run_shell(c)?;
        }
    }
    let summary = summarize(&display, req.runs, req.warmup, &samples);
    let run = to_run(&summary)?;
    Ok((summary, run))
}

fn spawn(req: &Request) -> Result<std::process::Child> {
    let mut cmd = if req.shell {
        let mut c = Command::new("sh");
        c.arg("-c").arg(req.command.join(" "));
        c
    } else {
        let mut c = Command::new(&req.command[0]);
        if req.command.len() > 1 {
            c.args(&req.command[1..]);
        }
        c
    };
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd.spawn().map_err(|e| error(format!("spawn: {e}")))
}

fn run_shell(script: &str) -> Result<()> {
    let status = Command::new("sh")
        .arg("-c")
        .arg(script)
        .status()
        .map_err(|e| error(format!("prepare/cleanup spawn: {e}")))?;
    if !status.success() {
        return Err(error(format!("prepare/cleanup failed with {status}")));
    }
    Ok(())
}

fn wait_timeout(
    child: &mut std::process::Child,
    limit: Duration,
) -> Result<std::process::ExitStatus> {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(e) => return Err(error(format!("try_wait: {e}"))),
        }
        if start.elapsed() >= limit {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error(format!(
                "command timed out after {}ms",
                limit.as_millis()
            )));
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn summarize(command: &str, runs: u32, warmup: u32, samples: &[u128]) -> Summary {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let n = sorted.len() as f64;
    let mean = samples.iter().map(|x| *x as f64).sum::<f64>() / n;
    let var = samples
        .iter()
        .map(|x| {
            let d = *x as f64 - mean;
            d * d
        })
        .sum::<f64>()
        / n;
    let median = if sorted.len() % 2 == 0 {
        (sorted[sorted.len() / 2 - 1] / 2) + (sorted[sorted.len() / 2] / 2)
    } else {
        sorted[sorted.len() / 2]
    };
    Summary {
        command: command.into(),
        runs,
        warmup,
        samples_ns: samples.to_vec(),
        mean_ns: mean,
        stddev_ns: var.sqrt(),
        median_ns: median,
        min_ns: *sorted.first().unwrap_or(&0),
        max_ns: *sorted.last().unwrap_or(&0),
        note: "Whole-process wall via blocking wait; includes process startup. Not Suite microbench timing.".into(),
    }
}

fn to_run(summary: &Summary) -> Result<Run> {
    let mut run = Run::new();
    run.cases.push(Case {
        id: "command".into(),
        contract: BTreeMap::from([("command".into(), summary.command.clone())]),
        metrics: vec![Metric {
            id: "wall".into(),
            unit: "ns".into(),
            scope: "whole process wall including startup; blocking wait".into(),
            phase: "measurement".into(),
            statistic: "process_total".into(),
            direction: Direction::Lower,
        }],
    });
    for (i, ns) in summary.samples_ns.iter().enumerate() {
        run.observations.push(Observation {
            case: "command".into(),
            metric: "wall".into(),
            variant: "candidate".into(),
            process: i as u32,
            pair: None,
            sequence: 0,
            value: Some(ns.to_string()),
            operations: 1,
            availability: Availability::Available,
        });
    }
    run.notes.push(summary.note.clone());
    run.status = Status::Complete;
    run.validate()?;
    Ok(run)
}

pub fn markdown(summary: &Summary) -> String {
    format!(
        "| Command | Mean | Stddev | Median | Min | Max | Runs |\n\
         |---|---:|---:|---:|---:|---:|---:|\n\
         | `{}` | {:.3} ms | {:.3} ms | {:.3} ms | {:.3} ms | {:.3} ms | {} |\n\n{}\n",
        summary.command.replace('|', "\\|"),
        summary.mean_ns / 1e6,
        summary.stddev_ns / 1e6,
        summary.median_ns as f64 / 1e6,
        summary.min_ns as f64 / 1e6,
        summary.max_ns as f64 / 1e6,
        summary.runs,
        summary.note
    )
}

pub fn write_run(run: &Run, path: &Path) -> Result<()> {
    rbench::publish::write_new_atomic(path, run)
}

fn shell_escape(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_ascii_alphanumeric() || "-_./=@:+,".contains(c))
    {
        s.into()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

pub fn parse_command_line(raw: &[String], shell: bool) -> Result<Vec<String>> {
    if raw.is_empty() {
        return Err(error(
            "pass a command after `--`, e.g. cargo rbench time --runs 20 -- /bin/echo ok",
        ));
    }
    if shell && raw.len() != 1 {
        return Err(error(
            "shell mode expects a single string after -- (quote the shell command)",
        ));
    }
    Ok(raw.to_vec())
}
