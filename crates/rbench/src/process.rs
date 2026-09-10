//! OS process residency and CPU time sampled **outside** timed workloads.
//!
//! Scope: current process only. On unified memory (Apple Silicon / UMA) RSS
//! overlaps GPU/driver residency and must never be added to GPU payload bytes.
//! Unavailable platforms return [`crate::Availability::Unsupported`] rather
//! than fabricating zeroes.
use crate::{error, Availability, Direction, Metric, Observation, Result};
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sample {
    /// Resident set size in bytes when the platform reports it.
    pub rss_bytes: Option<u64>,
    /// Cumulative user-mode CPU time in nanoseconds.
    pub user_cpu_ns: Option<u64>,
    /// Cumulative system/kernel CPU time in nanoseconds.
    pub system_cpu_ns: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Delta {
    pub rss_start_bytes: Option<u64>,
    pub rss_end_bytes: Option<u64>,
    pub rss_peak_bytes: Option<u64>,
    pub user_cpu_ns: Option<u64>,
    pub system_cpu_ns: Option<u64>,
    pub wall_ns: u128,
    pub availability: Availability,
}

/// One-shot sample of the calling process. Call between cases or after a phase
/// boundary — never inside a timed microbench batch.
pub fn sample() -> Result<Sample> {
    sample_impl()
}

/// Tracks RSS high-water and CPU deltas across an untimed phase.
pub struct Tracker {
    start: Sample,
    peak_rss: Option<u64>,
    started: Instant,
    availability: Availability,
}

impl Tracker {
    pub fn begin() -> Self {
        match sample() {
            Ok(s) => Self {
                peak_rss: s.rss_bytes,
                start: s,
                started: Instant::now(),
                availability: Availability::Available,
            },
            Err(e) => Self {
                start: Sample::default(),
                peak_rss: None,
                started: Instant::now(),
                availability: Availability::Unsupported(e.to_string()),
            },
        }
    }

    /// Optional mid-phase poll for long scenarios — not for microbench hot loops.
    pub fn poll(&mut self) {
        if self.availability != Availability::Available {
            return;
        }
        if let Ok(s) = sample() {
            match (self.peak_rss, s.rss_bytes) {
                (Some(p), Some(r)) => self.peak_rss = Some(p.max(r)),
                (None, Some(r)) => self.peak_rss = Some(r),
                _ => {}
            }
        }
    }

    pub fn finish(mut self) -> Delta {
        self.poll();
        let end = if self.availability == Availability::Available {
            sample().unwrap_or_default()
        } else {
            Sample::default()
        };
        if let (Some(p), Some(r)) = (self.peak_rss, end.rss_bytes) {
            self.peak_rss = Some(p.max(r));
        } else if end.rss_bytes.is_some() {
            self.peak_rss = end.rss_bytes;
        }
        let sub = |a: Option<u64>, b: Option<u64>| match (a, b) {
            (Some(after), Some(before)) => Some(after.saturating_sub(before)),
            _ => None,
        };
        Delta {
            rss_start_bytes: self.start.rss_bytes,
            rss_end_bytes: end.rss_bytes,
            rss_peak_bytes: self.peak_rss,
            user_cpu_ns: sub(end.user_cpu_ns, self.start.user_cpu_ns),
            system_cpu_ns: sub(end.system_cpu_ns, self.start.system_cpu_ns),
            wall_ns: self.started.elapsed().as_nanos(),
            availability: self.availability,
        }
    }
}

/// Metric descriptors for process-scope OS samples.
pub fn metrics() -> Vec<Metric> {
    vec![
        Metric {
            id: "os.rss_peak".into(),
            unit: "bytes".into(),
            scope: "whole process RSS highwater; UMA overlaps GPU; not additive to GPU bytes"
                .into(),
            phase: "process".into(),
            statistic: "process_total".into(),
            direction: Direction::Lower,
        },
        Metric {
            id: "os.cpu_user".into(),
            unit: "ns".into(),
            scope: "process user CPU delta outside timed batches".into(),
            phase: "process".into(),
            statistic: "process_total".into(),
            direction: Direction::Lower,
        },
        Metric {
            id: "os.cpu_system".into(),
            unit: "ns".into(),
            scope: "process system CPU delta outside timed batches".into(),
            phase: "process".into(),
            statistic: "process_total".into(),
            direction: Direction::Lower,
        },
    ]
}

/// Emit peak RSS / CPU observations for one case/process.
pub fn observations(case: &str, variant: &str, process: u32, delta: &Delta) -> Vec<Observation> {
    let mk = |metric: &str, value: Option<u64>| Observation {
        case: case.into(),
        metric: metric.into(),
        variant: variant.into(),
        process,
        pair: None,
        sequence: 0,
        value: value.map(|v| v.to_string()),
        operations: 1,
        availability: if delta.availability == Availability::Available && value.is_some() {
            Availability::Available
        } else if delta.availability != Availability::Available {
            delta.availability.clone()
        } else {
            Availability::Unsupported("platform omitted this counter".into())
        },
    };
    vec![
        mk("os.rss_peak", delta.rss_peak_bytes),
        mk("os.cpu_user", delta.user_cpu_ns),
        mk("os.cpu_system", delta.system_cpu_ns),
    ]
}

#[cfg(target_os = "linux")]
fn sample_impl() -> Result<Sample> {
    let status = std::fs::read_to_string("/proc/self/status").map_err(|e| error(e.to_string()))?;
    let mut rss_bytes = None;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kib: u64 = rest
                .split_whitespace()
                .next()
                .ok_or_else(|| error("VmRSS missing value"))?
                .parse()
                .map_err(|e| error(format!("VmRSS: {e}")))?;
            rss_bytes = Some(kib.saturating_mul(1024));
            break;
        }
    }
    let stat = std::fs::read_to_string("/proc/self/stat").map_err(|e| error(e.to_string()))?;
    // Fields after `comm`: 14=utime, 15=stime (1-based in man proc).
    let after = stat
        .rfind(')')
        .map(|i| &stat[i + 2..])
        .ok_or_else(|| error("malformed /proc/self/stat"))?;
    let fields: Vec<_> = after.split_whitespace().collect();
    if fields.len() < 13 {
        return Err(error("/proc/self/stat too short"));
    }
    let ticks = ticks_per_second()?;
    let user = fields[11]
        .parse::<u64>()
        .map_err(|e| error(format!("utime: {e}")))?;
    let system = fields[12]
        .parse::<u64>()
        .map_err(|e| error(format!("stime: {e}")))?;
    Ok(Sample {
        rss_bytes,
        user_cpu_ns: Some(ticks_to_ns(user, ticks)),
        system_cpu_ns: Some(ticks_to_ns(system, ticks)),
    })
}

#[cfg(target_os = "linux")]
fn ticks_per_second() -> Result<u64> {
    let t = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if t <= 0 {
        return Err(error("sysconf(_SC_CLK_TCK) failed"));
    }
    Ok(t as u64)
}

#[cfg(target_os = "linux")]
fn ticks_to_ns(ticks: u64, hz: u64) -> u64 {
    ticks.saturating_mul(1_000_000_000 / hz.max(1))
}

#[cfg(target_os = "macos")]
fn sample_impl() -> Result<Sample> {
    use std::mem::MaybeUninit;
    let mut usage = MaybeUninit::<libc::rusage>::uninit();
    let rc = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
    if rc != 0 {
        return Err(error("getrusage failed"));
    }
    let usage = unsafe { usage.assume_init() };
    let timeval_ns = |t: libc::timeval| {
        (t.tv_sec as u64)
            .saturating_mul(1_000_000_000)
            .saturating_add((t.tv_usec as u64).saturating_mul(1_000))
    };
    // ru_maxrss is bytes on macOS.
    Ok(Sample {
        rss_bytes: Some(usage.ru_maxrss as u64),
        user_cpu_ns: Some(timeval_ns(usage.ru_utime)),
        system_cpu_ns: Some(timeval_ns(usage.ru_stime)),
    })
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn sample_impl() -> Result<Sample> {
    Err(error(format!(
        "OS process metrics unsupported on {}",
        std::env::consts::OS
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_current_process_on_supported_os() {
        if cfg!(any(target_os = "linux", target_os = "macos")) {
            let s = sample().expect("sample");
            assert!(s.rss_bytes.unwrap_or(0) > 0);
            let mut t = Tracker::begin();
            t.poll();
            let d = t.finish();
            assert_eq!(d.availability, Availability::Available);
            assert!(d.rss_peak_bytes.unwrap_or(0) > 0);
        }
    }
}
