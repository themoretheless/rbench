//! Host isolation helpers for confirmatory runs on Linux.
//!
//! These do **not** claim BenchExec-grade sandboxing. They capture scheduler
//! noise factors, optionally pin the current process to one CPU, and can
//! best-effort enter a writable cgroup v2 subtree for CPU/memory limits.
use crate::{error, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub loadavg_1: Option<f64>,
    pub loadavg_5: Option<f64>,
    pub loadavg_15: Option<f64>,
    pub nprocs: Option<u32>,
    pub cpu_governor: Option<String>,
    pub cpu_freq_khz: Option<u64>,
    pub pinned_cpu: Option<u32>,
    /// Absolute path of the current process cgroup (v2 unified hierarchy when available).
    pub cgroup_path: Option<String>,
    pub cgroup_cpu_max: Option<String>,
    pub cgroup_memory_max: Option<String>,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CgroupReport {
    pub applied: bool,
    pub path: Option<String>,
    pub cpus: Option<String>,
    pub memory_max: Option<String>,
    pub note: String,
}

/// Read load average, optional governor/freq, affinity, and cgroup controllers.
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
    let cgroup_path = current_cgroup_path();
    let (cpu_max, mem_max) = cgroup_path
        .as_ref()
        .map(|p| {
            (
                read_first_line(&format!("{p}/cpu.max")),
                read_first_line(&format!("{p}/memory.max")),
            )
        })
        .unwrap_or((None, None));
    if cgroup_path.is_none() {
        notes.push("cgroup v2 path unavailable (legacy hierarchy, container, or non-Linux)".into());
    }
    Snapshot {
        loadavg_1: a,
        loadavg_5: b,
        loadavg_15: c,
        nprocs,
        cpu_governor: governor,
        cpu_freq_khz: freq,
        pinned_cpu: current_affinity().ok().and_then(|v| v.first().copied()),
        cgroup_path,
        cgroup_cpu_max: cpu_max,
        cgroup_memory_max: mem_max,
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

/// Resolve the current process cgroup directory under the v2 mount.
pub fn current_cgroup_path() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let text = fs::read_to_string("/proc/self/cgroup").ok()?;
        // v2: "0::/path"
        let rel = text.lines().find_map(|l| l.strip_prefix("0::"))?;
        let mount = cgroup_v2_mount()?;
        let path = if rel == "/" {
            mount
        } else {
            format!("{}{}", mount.trim_end_matches('/'), rel)
        };
        fs::metadata(&path).ok()?;
        Some(path)
    }
    #[cfg(not(target_os = "linux"))]
    {
        None
    }
}

fn cgroup_v2_mount() -> Option<String> {
    let mounts = fs::read_to_string("/proc/self/mountinfo").ok()?;
    for line in mounts.lines() {
        let mut parts = line.split(" - ");
        let _ = parts.next()?;
        let rest = parts.next()?;
        let mut fields = rest.split_whitespace();
        let fstype = fields.next()?;
        let mount = fields.next()?;
        if fstype == "cgroup2" {
            return Some(mount.to_string());
        }
    }
    let fallback = "/sys/fs/cgroup";
    if fs::metadata(format!("{fallback}/cgroup.controllers")).is_ok() {
        Some(fallback.into())
    } else {
        None
    }
}

/// Best-effort enter a writable cgroup v2 leaf.
///
/// Env:
/// - `RBENCH_CGROUP` — absolute path or name under the current cgroup
/// - `RBENCH_CGROUP_CPUS` — written to `cpuset.cpus` when present (e.g. `0`)
/// - `RBENCH_CGROUP_MEMORY_MAX` — written to `memory.max` (bytes or `max`)
pub fn apply_env_cgroup() -> Result<CgroupReport> {
    let path = match std::env::var("RBENCH_CGROUP") {
        Ok(v) if !v.is_empty() => v,
        _ => {
            return Ok(CgroupReport {
                applied: false,
                path: current_cgroup_path(),
                cpus: None,
                memory_max: None,
                note: "RBENCH_CGROUP unset; no cgroup enter attempted".into(),
            });
        }
    };
    let cpus = std::env::var("RBENCH_CGROUP_CPUS")
        .ok()
        .filter(|s| !s.is_empty());
    let memory_max = std::env::var("RBENCH_CGROUP_MEMORY_MAX")
        .ok()
        .filter(|s| !s.is_empty());
    enter_cgroup(&path, cpus.as_deref(), memory_max.as_deref())
}

/// Create/enter `path` and optionally set cpuset/memory controllers.
pub fn enter_cgroup(
    path_or_name: &str,
    cpus: Option<&str>,
    memory_max: Option<&str>,
) -> Result<CgroupReport> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (path_or_name, cpus, memory_max);
        return Ok(CgroupReport {
            applied: false,
            path: None,
            cpus: None,
            memory_max: None,
            note: format!("cgroup enter unsupported on {}", std::env::consts::OS),
        });
    }
    #[cfg(target_os = "linux")]
    {
        let target = resolve_cgroup_target(path_or_name)?;
        if let Err(e) = fs::create_dir_all(&target) {
            return Ok(CgroupReport {
                applied: false,
                path: Some(target.display().to_string()),
                cpus: cpus.map(str::to_string),
                memory_max: memory_max.map(str::to_string),
                note: format!("cannot create cgroup {}: {e}", target.display()),
            });
        }
        if let Some(cpus) = cpus {
            enable_subtree_controller(&target, "cpuset");
            if let Err(e) = fs::write(target.join("cpuset.cpus"), format!("{cpus}\n")) {
                return Ok(CgroupReport {
                    applied: false,
                    path: Some(target.display().to_string()),
                    cpus: Some(cpus.into()),
                    memory_max: memory_max.map(str::to_string),
                    note: format!("cannot write cpuset.cpus: {e}"),
                });
            }
            let _ = fs::write(target.join("cpuset.mems"), "0\n");
        }
        if let Some(mem) = memory_max {
            enable_subtree_controller(&target, "memory");
            if let Err(e) = fs::write(target.join("memory.max"), format!("{mem}\n")) {
                return Ok(CgroupReport {
                    applied: false,
                    path: Some(target.display().to_string()),
                    cpus: cpus.map(str::to_string),
                    memory_max: Some(mem.into()),
                    note: format!("cannot write memory.max: {e}"),
                });
            }
        }
        let pid = std::process::id().to_string();
        match fs::write(target.join("cgroup.procs"), format!("{pid}\n")) {
            Ok(()) => Ok(CgroupReport {
                applied: true,
                path: Some(target.display().to_string()),
                cpus: cpus.map(str::to_string),
                memory_max: memory_max.map(str::to_string),
                note: "entered cgroup v2 leaf; still not BenchExec-grade isolation".into(),
            }),
            Err(e) => Ok(CgroupReport {
                applied: false,
                path: Some(target.display().to_string()),
                cpus: cpus.map(str::to_string),
                memory_max: memory_max.map(str::to_string),
                note: format!("cannot move pid into cgroup.procs: {e}"),
            }),
        }
    }
}

#[cfg(target_os = "linux")]
fn resolve_cgroup_target(path_or_name: &str) -> Result<PathBuf> {
    let p = PathBuf::from(path_or_name);
    if p.is_absolute() {
        return Ok(p);
    }
    let current = current_cgroup_path()
        .ok_or_else(|| error("cannot resolve relative RBENCH_CGROUP without a cgroup v2 path"))?;
    Ok(PathBuf::from(current).join(path_or_name))
}

#[cfg(target_os = "linux")]
fn enable_subtree_controller(target: &std::path::Path, controller: &str) {
    let mut cur = target.parent().map(|p| p.to_path_buf());
    while let Some(dir) = cur {
        let ctl = dir.join("cgroup.subtree_control");
        if ctl.exists() {
            let _ = fs::write(&ctl, format!("+{controller}\n"));
        }
        if dir.as_os_str() == "/" {
            break;
        }
        cur = dir.parent().map(|p| p.to_path_buf());
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
    if let Some(mem) = &snap.cgroup_memory_max {
        if mem != "max" {
            w.push(format!(
                "cgroup memory.max={mem}; OOM/kill may abort confirmatory processes"
            ));
        }
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_runs() {
        let s = snapshot();
        if cfg!(target_os = "linux") {
            assert!(s.loadavg_1.is_some());
        }
        let _ = noise_warnings(&s);
    }

    #[test]
    fn env_cgroup_unset_is_noop() {
        // SAFETY: test process; we restore by removing the var.
        std::env::remove_var("RBENCH_CGROUP");
        let r = apply_env_cgroup().unwrap();
        assert!(!r.applied);
        assert!(r.note.contains("unset"));
    }
}
