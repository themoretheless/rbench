//! Host isolation helpers for confirmatory runs on Linux.
//!
//! These do **not** claim BenchExec-grade sandboxing. They capture scheduler
//! noise factors and optionally pin the current process to one CPU so
//! process-paired A/B comparisons see less cross-core jitter.
use crate::{error, Result};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub loadavg_1: Option<f64>,
    pub loadavg_5: Option<f64>,
    pub loadavg_15: Option<f64>,
    pub nprocs: Option<u32>,
    pub cpu_governor: Option<String>,
    pub cpu_freq_khz: Option<u64>,
    pub pinned_cpu: Option<u32>,
    pub notes: Vec<String>,
}

/// Read load average, optional governor/freq, and current affinity.
pub fn snapshot() -> Snapshot {
    let mut notes = Vec::new();
    let (a, b, c) = loadavg();
    let nprocs = std::thread::available_parallelism()
        .ok()
        .map(|n| n.get() as u32);
    let governor = read_first_line("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor");
    let freq = read_first_line("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq")
        .and_then(|s| s.parse().ok());
    if governor.is_none() {
        notes.push("cpufreq governor unavailable (container/VM or no sysfs)".into());
    }
    Snapshot {
        loadavg_1: a,
        loadavg_5: b,
        loadavg_15: c,
        nprocs,
        cpu_governor: governor,
        cpu_freq_khz: freq,
        pinned_cpu: current_affinity().ok().and_then(|v| v.first().copied()),
        notes,
    }
}

fn loadavg() -> (Option<f64>, Option<f64>, Option<f64>) {
    let Ok(text) = fs::read_to_string("/proc/loadavg") else {
        return (None, None, None);
    };
    let mut it = text.split_whitespace();
    let parse = |s: Option<&str>| s.and_then(|x| x.parse().ok());
    (parse(it.next()), parse(it.next()), parse(it.next()))
}

fn read_first_line(path: &str) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Pin the current process to a single CPU. Returns the CPU id used.
pub fn pin_to_cpu(cpu: u32) -> Result<u32> {
    #[cfg(target_os = "linux")]
    {
        unsafe {
            let mut set: libc::cpu_set_t = std::mem::zeroed();
            libc::CPU_ZERO(&mut set);
            libc::CPU_SET(cpu as usize, &mut set);
            let rc = libc::sched_setaffinity(0, std::mem::size_of_val(&set), &set);
            if rc != 0 {
                return Err(error(format!(
                    "sched_setaffinity({cpu}) failed: {}",
                    std::io::Error::last_os_error()
                )));
            }
        }
        Ok(cpu)
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = cpu;
        Err(error(format!(
            "CPU pinning unsupported on {}",
            std::env::consts::OS
        )))
    }
}

/// Pin to `RBENCH_PIN_CPU` when set, else leave affinity unchanged.
pub fn apply_env_pin() -> Result<Option<u32>> {
    match std::env::var("RBENCH_PIN_CPU") {
        Ok(v) => {
            let cpu: u32 = v
                .parse()
                .map_err(|_| error("RBENCH_PIN_CPU must be an unsigned integer"))?;
            Ok(Some(pin_to_cpu(cpu)?))
        }
        Err(_) => Ok(None),
    }
}

pub fn current_affinity() -> Result<Vec<u32>> {
    #[cfg(target_os = "linux")]
    {
        unsafe {
            let mut set: libc::cpu_set_t = std::mem::zeroed();
            let rc = libc::sched_getaffinity(0, std::mem::size_of_val(&set), &mut set);
            if rc != 0 {
                return Err(error(format!(
                    "sched_getaffinity failed: {}",
                    std::io::Error::last_os_error()
                )));
            }
            let mut out = Vec::new();
            let max = libc::CPU_SETSIZE as u32;
            for cpu in 0..max {
                if libc::CPU_ISSET(cpu as usize, &set) {
                    out.push(cpu);
                }
            }
            Ok(out)
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        Err(error(format!(
            "CPU affinity unsupported on {}",
            std::env::consts::OS
        )))
    }
}

/// Warn when the host looks noisy for confirmatory work.
pub fn noise_warnings(snap: &Snapshot) -> Vec<String> {
    let mut w = snap.notes.clone();
    if let (Some(load), Some(n)) = (snap.loadavg_1, snap.nprocs) {
        if load > n as f64 * 0.7 {
            w.push(format!(
                "loadavg_1={load:.2} is high relative to nprocs={n}; confirmatory runs may be confounded"
            ));
        }
    }
    if snap.cpu_governor.as_deref() == Some("powersave") {
        w.push("cpufreq governor is powersave; prefer performance for confirmatory A/B".into());
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_runs() {
        let s = snapshot();
        // loadavg should exist on Linux CI
        if cfg!(target_os = "linux") {
            assert!(s.loadavg_1.is_some());
        }
        let _ = noise_warnings(&s);
    }
}
