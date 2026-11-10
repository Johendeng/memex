//! Simple file logger — appends to `<data_dir>/memex.log`.
//! Used to debug startup failures when the console is hidden
//! (CREATE_NO_WINDOW / FreeConsole suppresses stderr).

use std::fs::OpenOptions;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

/// Append a timestamped line to `data/memex.log`.
pub fn log(msg: impl AsRef<str>) {
    let path = crate::config::data_dir().join("memex.log");
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        // Local time CST (UTC+8)
        let local = now + 8 * 3600;
        let h = (local % 86400) / 3600;
        let m = (local % 3600) / 60;
        let s = local % 60;
        let _ = writeln!(f, "[{h:02}:{m:02}:{s:02}] {}", msg.as_ref());
    }
}

/// Path to the log file (for redirecting child process stdout/stderr).
pub fn log_path() -> std::path::PathBuf {
    crate::config::data_dir().join("memex.log")
}
