//! Memex path configuration.
//!
//! Priority for resolving the Memex home directory:
//!   1. `MEMEX_HOME` environment variable (explicit override)
//!   2. The executable's own directory (`current_exe().parent()`)
//!   3. Fallback: `%LOCALAPPDATA%\Memex` (legacy / development)
//!
//! The installer (Inno Setup) places the exe at `C:\Memex\memex.exe`,
//! so the home directory becomes `C:\Memex\` automatically:
//!   - Index / meta / lock:   `C:\Memex\data\`
//!   - ONNX model files:      `C:\Memex\models\`

use std::path::PathBuf;

/// Resolve the Memex home directory.
///
/// ```text
/// MEMEX_HOME  →  <env value>
/// (otherwise)   →  exe's parent directory
/// (fallback)    →  %LOCALAPPDATA%\Memex
/// ```
pub fn home_dir() -> PathBuf {
    if let Ok(h) = std::env::var("MEMEX_HOME") {
        return PathBuf::from(h);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            return dir.to_path_buf();
        }
    }
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("./"))
        .join("Memex")
}

/// Index / cache data directory: `<home>/data`.
/// Created automatically if it doesn't exist.
pub fn data_dir() -> PathBuf {
    let dir = home_dir().join("data");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// ONNX model directory: `<home>/models`.
pub fn model_dir() -> PathBuf {
    home_dir().join("models")
}

/// bge-small-zh-v1.5 model subdirectory (relative to model_dir).
pub fn bge_model_dir() -> PathBuf {
    model_dir().join("bge-small-zh-v1.5")
}
