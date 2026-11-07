//! Single-instance guard using a lock file.
//!
//! The lock file lives at `{data_dir}/memex.lock` and stores the PID of
//! the running instance. A second invocation of `memex serve` (regardless
//! of port) will detect the live PID, report which port it's on, and exit.
//!
//! Stale locks (process already dead) are cleaned up automatically.

use std::fs;
use std::io::Write;

use anyhow::{anyhow, Result};

/// Acquire the single-instance lock. Returns the PID of the existing
/// instance if one is already running, `Ok(())` if we are the sole instance.
///
/// The lock file is created atomically (`create_new`); a second process
/// reading the same file will see a live PID and bail out.
pub fn acquire_lock() -> Result<Option<u32>> {
    let lock_path = lock_path()?;

    // Try to atomically create the lock file
    match fs::OpenOptions::new().create_new(true).write(true).open(&lock_path) {
        Ok(mut file) => {
            // Write our PID
            let pid = std::process::id();
            file.write_all(&pid.to_be_bytes())?;
            file.sync_all()?;
            eprintln!(
                "single-instance lock acquired (PID {pid}, lock: {})",
                lock_path.display()
            );
            Ok(None) // we are the first instance
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            // Lock file exists → check if that process is still alive
            let raw = fs::read(&lock_path).unwrap_or_default();
            let pid: u32 = if raw.len() >= 4 {
                u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]])
            } else {
                return Ok(Some(0)); // corrupt lock, assume stale
            };

            if process_alive(pid) {
                eprintln!(
                    "another Memex instance is already running (PID {pid}).\n\
                     If you need a second instance on a different port, \
                     stop the first one first (see tasklist / 任务管理器)."
                );
                Ok(Some(pid))
            } else {
                // Stale lock — process is dead, remove and retry
                eprintln!("stale lock file detected (PID {pid} not running), removing …");
                fs::remove_file(&lock_path)?;
                // Retry: create_new should now succeed
                acquire_lock()
            }
        }
        Err(e) => Err(anyhow!("failed to acquire lock file at {}: {e}", lock_path.display())),
    }
}

/// Release the lock (called on clean exit).
pub fn release_lock() {
    if let Ok(p) = lock_path() {
        let _ = fs::remove_file(&p);
    }
}

fn lock_path() -> Result<std::path::PathBuf> {
    Ok(crate::config::data_dir().join("memex.lock"))
}

/// Cheap liveness check: `tasklist` on Windows, `/proc/<pid>` on Unix.
#[cfg(windows)]
fn process_alive(pid: u32) -> bool {
    let out = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            text.contains(&pid.to_string())
        }
        _ => true, // conservative: assume alive if we can't check
    }
}

#[cfg(not(windows))]
fn process_alive(pid: u32) -> bool {
    std::path::Path::new(&format!("/proc/{pid}")).exists()
}
