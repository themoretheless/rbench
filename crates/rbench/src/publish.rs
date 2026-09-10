//! Crash-durable publication of JSON artifacts and run directories.
//!
//! Pattern: write to a sibling temporary path, `sync_all`, then `rename` into
//! place. On POSIX, rename within a directory is atomic. Callers that need a
//! whole directory published create it under `*.publishing` and rename once
//! complete; interrupted publishes leave the temporary name visible.
use crate::{error, Result};
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

/// Write `value` so readers never observe a partial JSON document at `path`.
/// Existing files are replaced only after the new bytes are durable.
pub fn write_atomic(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let tmp = temp_sibling(path, "tmp")?;
    {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        serde_json::to_writer_pretty(&mut f, value)?;
        f.write_all(b"\n")?;
        f.sync_all()?;
    }
    // Best-effort directory sync so the rename itself is durable on crash.
    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }
    fs::rename(&tmp, path)?;
    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

/// Create-only variant used for immutable first-write artifacts.
pub fn write_new_atomic(path: &Path, value: &impl Serialize) -> Result<()> {
    if path.exists() {
        return Err(error(format!(
            "refusing to overwrite existing {}",
            path.display()
        )));
    }
    write_atomic(path, value)
}

/// Prepare a staging directory `final.publishing` (or next free sibling).
pub fn staging_dir(final_path: &Path) -> Result<PathBuf> {
    if final_path.exists() {
        return Err(error(format!(
            "output already exists: {}",
            final_path.display()
        )));
    }
    if let Some(parent) = final_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let staging = temp_sibling(final_path, "publishing")?;
    fs::create_dir(&staging)?;
    Ok(staging)
}

/// Atomically publish a completed staging directory as `final_path`.
pub fn commit_dir(staging: &Path, final_path: &Path) -> Result<()> {
    if final_path.exists() {
        return Err(error(format!(
            "output already exists: {}",
            final_path.display()
        )));
    }
    if !staging.is_dir() {
        return Err(error(format!(
            "staging is not a directory: {}",
            staging.display()
        )));
    }
    let parent = final_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if let Ok(dir) = File::open(staging) {
        let _ = dir.sync_all();
    }
    fs::rename(staging, final_path)?;
    if let Ok(dir) = File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

/// Inspect a run directory that may have been interrupted before `status-final.json`.
#[derive(Debug, Clone, Serialize)]
pub struct RecoverReport {
    pub path: PathBuf,
    pub state: &'static str,
    pub has_run: bool,
    pub has_status_final: bool,
    pub has_progress: bool,
    pub partial_processes: Vec<u32>,
    pub note: String,
}

pub fn recover_report(path: &Path) -> Result<RecoverReport> {
    if !path.is_dir() {
        return Err(error(format!("not a run directory: {}", path.display())));
    }
    let has_run = path.join("run.json").is_file();
    let has_status_final = path.join("status-final.json").is_file();
    let has_progress = path.join("progress.json").is_file();
    let mut partial_processes = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if let Some(rest) = name.strip_prefix("partial-") {
            if let Some(num) = rest.strip_suffix(".json") {
                if let Ok(p) = num.parse::<u32>() {
                    partial_processes.push(p);
                }
            }
        }
    }
    partial_processes.sort_unstable();
    let (state, note) = if has_status_final && has_run {
        (
            "complete_or_terminal",
            "status-final.json present; treat as finished (check status for Complete/Failed/Cancelled).",
        )
    } else if has_progress || !partial_processes.is_empty() {
        (
            "incomplete",
            "Interrupted or still running. Use `resume` for allowlisted continuation; do not treat partial-*.json as a Complete run.",
        )
    } else if path
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.contains(".publishing"))
    {
        (
            "abandoned_publish",
            "Staging directory left behind; safe to delete after confirming no writer holds it.",
        )
    } else {
        (
            "empty_or_unknown",
            "No progress or final status; directory may be preflight residue.",
        )
    };
    Ok(RecoverReport {
        path: path.to_path_buf(),
        state,
        has_run,
        has_status_final,
        has_progress,
        partial_processes,
        note: note.into(),
    })
}

fn temp_sibling(path: &Path, kind: &str) -> Result<PathBuf> {
    let file_name = path
        .file_name()
        .ok_or_else(|| error("path has no file name"))?
        .to_string_lossy();
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    for n in 0..1000 {
        let candidate = parent.join(format!("{file_name}.{kind}{n}"));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(error("could not allocate temporary sibling path"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    #[test]
    fn atomic_replace_leaves_valid_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("doc.json");
        write_atomic(&path, &json!({"a": 1})).unwrap();
        write_atomic(&path, &json!({"a": 2})).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(v["a"], 2);
        assert!(write_new_atomic(&path, &json!({"a": 3})).is_err());
    }

    #[test]
    fn staging_commit_is_all_or_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let final_path = dir.path().join("run-out");
        let staging = staging_dir(&final_path).unwrap();
        fs::write(staging.join("marker"), b"ok").unwrap();
        commit_dir(&staging, &final_path).unwrap();
        assert!(final_path.join("marker").is_file());
        assert!(!staging.exists());
    }

    #[test]
    fn recover_detects_partials() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("progress.json"), b"{}").unwrap();
        fs::write(dir.path().join("partial-3.json"), b"{}").unwrap();
        let r = recover_report(dir.path()).unwrap();
        assert_eq!(r.state, "incomplete");
        assert_eq!(r.partial_processes, vec![3]);
    }
}
