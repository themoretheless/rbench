//! Linux `perf_event` hardware counters (instructions / cycles).
//!
//! These are **typed metrics**, not converted into time. When the kernel denies
//! access (common in containers without `perf_event_paranoid` relaxation),
//! probes return [`crate::Availability::Unsupported`] or
//! [`crate::Availability::PermissionDenied`] — never fabricated zeroes.
//!
//! Counter reads include whatever work the caller wraps; Suite sibling-batch
//! sampling documents that wall and perf may come from consecutive batches.
use crate::{error, Availability, Direction, Metric, Observation, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counts {
    pub instructions: Option<u64>,
    pub cycles: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Probe {
    pub availability: Availability,
    pub note: String,
}

/// Probe whether hardware counters are usable in this process.
pub fn probe() -> Probe {
    match Counters::open() {
        Ok(c) => {
            drop(c);
            Probe {
                availability: Availability::Available,
                note: "perf_event_open hardware instructions+cycles usable".into(),
            }
        }
        Err(e) => {
            let msg = e.to_string();
            let availability = if msg.contains("Permission") || msg.contains("EPERM") || msg.contains("EACCES") {
                Availability::PermissionDenied(msg.clone())
            } else {
                Availability::Unsupported(msg.clone())
            };
            Probe {
                availability,
                note: msg,
            }
        }
    }
}

pub fn metrics() -> Vec<Metric> {
    vec![
        Metric {
            id: "perf.instructions".into(),
            unit: "instructions".into(),
            scope: "hardware retired instructions for the wrapped batch; includes measurement bookkeeping when sampled in-process".into(),
            phase: "measurement".into(),
            statistic: "batch_total".into(),
            direction: Direction::Lower,
        },
        Metric {
            id: "perf.cycles".into(),
            unit: "cycles".into(),
            scope: "hardware CPU cycles for the wrapped batch; frequency scaling makes this not a pure time proxy".into(),
            phase: "measurement".into(),
            statistic: "batch_total".into(),
            direction: Direction::Lower,
        },
    ]
}

pub fn observations(case: &str, variant: &str, process: u32, sequence: u64, counts: &Counts, availability: &Availability) -> Vec<Observation> {
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
        mk("perf.instructions", counts.instructions),
        mk("perf.cycles", counts.cycles),
    ]
}

/// RAII pair of hardware counters for the current thread.
pub struct Counters {
    instructions: Fd,
    cycles: Fd,
}

struct Fd(i32);
impl Drop for Fd {
    fn drop(&mut self) {
        if self.0 >= 0 {
            #[cfg(target_os = "linux")]
            unsafe {
                libc::close(self.0);
            }
            #[cfg(not(target_os = "linux"))]
            {
                self.0 = -1;
            }
        }
    }
}

impl Counters {
    pub fn open() -> Result<Self> {
        #[cfg(target_os = "linux")]
        {
            Ok(Self {
                instructions: Fd(open_counter(PERF_COUNT_HW_INSTRUCTIONS)?),
                cycles: Fd(open_counter(PERF_COUNT_HW_CPU_CYCLES)?),
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err(error(format!(
                "perf_event hardware counters unsupported on {}",
                std::env::consts::OS
            )))
        }
    }

    pub fn enable(&self) -> Result<()> {
        ioctl(self.instructions.0, PERF_EVENT_IOC_RESET)?;
        ioctl(self.cycles.0, PERF_EVENT_IOC_RESET)?;
        ioctl(self.instructions.0, PERF_EVENT_IOC_ENABLE)?;
        ioctl(self.cycles.0, PERF_EVENT_IOC_ENABLE)?;
        Ok(())
    }

    pub fn disable_and_read(&self) -> Result<Counts> {
        ioctl(self.instructions.0, PERF_EVENT_IOC_DISABLE)?;
        ioctl(self.cycles.0, PERF_EVENT_IOC_DISABLE)?;
        Ok(Counts {
            instructions: Some(read_u64(self.instructions.0)?),
            cycles: Some(read_u64(self.cycles.0)?),
        })
    }

    /// Measure `f` under both counters. Timing is the caller's responsibility.
    pub fn measure_counts<T>(&self, f: impl FnOnce() -> T) -> Result<(T, Counts)> {
        self.enable()?;
        let out = f();
        let counts = self.disable_and_read()?;
        Ok((out, counts))
    }
}

#[cfg(target_os = "linux")]
const PERF_TYPE_HARDWARE: u64 = 0;
#[cfg(target_os = "linux")]
const PERF_COUNT_HW_CPU_CYCLES: u64 = 0;
#[cfg(target_os = "linux")]
const PERF_COUNT_HW_INSTRUCTIONS: u64 = 1;
#[cfg(target_os = "linux")]
const PERF_EVENT_IOC_ENABLE: u64 = 0x2400;
#[cfg(target_os = "linux")]
const PERF_EVENT_IOC_DISABLE: u64 = 0x2401;
#[cfg(target_os = "linux")]
const PERF_EVENT_IOC_RESET: u64 = 0x2403;

#[cfg(target_os = "linux")]
#[repr(C)]
struct PerfEventAttr {
    type_: u32,
    size: u32,
    config: u64,
    sample_period: u64,
    sample_type: u64,
    read_format: u64,
    flags: u64,
    wakeup_events: u32,
    bp_type: u32,
    bp_addr: u64,
    bp_len: u64,
    branch_sample_type: u64,
    sample_regs_user: u64,
    sample_stack_user: u32,
    clockid: i32,
    sample_regs_intr: u64,
    aux_watermark: u32,
    sample_max_stack: u16,
    __reserved_2: u16,
    aux_sample_size: u32,
    __reserved_3: u32,
}

#[cfg(target_os = "linux")]
fn open_counter(config: u64) -> Result<i32> {
    let mut attr: PerfEventAttr = unsafe { std::mem::zeroed() };
    attr.type_ = PERF_TYPE_HARDWARE as u32;
    attr.size = std::mem::size_of::<PerfEventAttr>() as u32;
    attr.config = config;
    // disabled | exclude_kernel | exclude_hv
    attr.flags = 1 | (1 << 5) | (1 << 6);
    let fd = unsafe {
        libc::syscall(
            libc::SYS_perf_event_open,
            &attr as *const PerfEventAttr,
            0,  // current process
            -1, // any CPU
            -1, // no group
            0,
        )
    };
    if fd < 0 {
        let err = std::io::Error::last_os_error();
        return Err(error(format!("perf_event_open failed: {err}")));
    }
    Ok(fd as i32)
}

#[cfg(target_os = "linux")]
fn ioctl(fd: i32, op: u64) -> Result<()> {
    let rc = unsafe { libc::ioctl(fd, op as _, 0) };
    if rc < 0 {
        return Err(error(format!(
            "perf ioctl failed: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn read_u64(fd: i32) -> Result<u64> {
    let mut buf = [0u8; 8];
    let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut _, 8) };
    if n != 8 {
        return Err(error(format!(
            "perf read failed: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(u64::from_ne_bytes(buf))
}

#[cfg(not(target_os = "linux"))]
fn ioctl(_fd: i32, _op: u64) -> Result<()> {
    Err(error("perf ioctl unsupported"))
}
#[cfg(not(target_os = "linux"))]
fn read_u64(_fd: i32) -> Result<u64> {
    Err(error("perf read unsupported"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_does_not_panic() {
        let p = probe();
        // In CI this may be Available or PermissionDenied/Unsupported — never silent zeros.
        match p.availability {
            Availability::Available
            | Availability::Unsupported(_)
            | Availability::PermissionDenied(_) => {}
            other => panic!("unexpected availability {other:?}"),
        }
    }

    #[test]
    fn counters_count_some_work_when_available() {
        let Ok(c) = Counters::open() else { return; };
        let (_, counts) = c
            .measure_counts(|| {
                let mut x = 1u64;
                for i in 0..10_000u64 {
                    x = x.wrapping_mul(i | 1).wrapping_add(i);
                }
                std::hint::black_box(x);
            })
            .unwrap();
        assert!(counts.instructions.unwrap_or(0) > 0);
        assert!(counts.cycles.unwrap_or(0) > 0);
    }
}
