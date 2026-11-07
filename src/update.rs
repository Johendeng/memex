//! Version update checking + automatic download.
//!
//! Sources (in priority order):
//!   1. Gitee (fast in China)
//!   2. GitHub (fallback)
//!
//! Flow: check_update() → compare versions → if newer, download_update() → launch installer.

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Repository paths — set these after creating the repos.
const GITEE_API: &str = "https://gitee.com/api/v5/repos/__OWNER__/memex/releases/latest";
const GITHUB_API: &str = "https://api.github.com/repos/__OWNER__/memex/releases/latest";

/// Progress tracking for download (polled via cmd_update_progress).
pub static DL_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
pub static DL_TOTAL: AtomicU64 = AtomicU64::new(0);
pub static DL_DONE: AtomicU64 = AtomicU64::new(0);

/// Path to the downloaded installer (set by download_update thread).
pub static DOWNLOADED_PATH: std::sync::Mutex<Option<PathBuf>> = std::sync::Mutex::new(None);

/// Result of the startup auto-check (None = not checked yet, Some(None) = checked, no update).
pub static AUTO_CHECK_RESULT: std::sync::Mutex<Option<Option<UpdateInfo>>> = std::sync::Mutex::new(None);

#[derive(Serialize, Deserialize, Clone)]
pub struct UpdateInfo {
    pub current: String,
    pub latest: String,
    pub has_update: bool,
    pub notes: String,
    pub download_url: String,
    pub installer_filename: String,
    pub source: String,
}

#[derive(Deserialize)]
struct GiteeRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<GiteeAsset>,
}

#[derive(Deserialize)]
struct GiteeAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

/// Check for updates. Tries Gitee first, then GitHub.
/// `owner` is the username on both platforms (e.g. "JohenDeng").
///
/// Returns `Ok(UpdateInfo)` on success (has_update may be false = up to date).
/// Returns `Err` only when all sources are genuinely unreachable (network/DNS/timeout)
/// — a 404 (repo not yet created / no releases published) is treated as "no update"
/// and silently returns an up-to-date result instead of erroring.
pub fn check_update(owner: &str) -> Result<UpdateInfo> {
    let current = env!("CARGO_PKG_VERSION").to_string();

    // Try Gitee first (Ok(None) = 404, silent fallthrough; Err = real network error)
    match check_gitee(owner, &current) {
        Ok(Some(info)) => return Ok(info),
        Ok(None) => {}
        Err(e) => eprintln!("[update] gitee check failed: {e}"),
    }

    // Fallback to GitHub
    match check_github(owner, &current) {
        Ok(Some(info)) => return Ok(info),
        Ok(None) => {}
        Err(e) => eprintln!("[update] github check failed: {e}"),
    }

    // Both sources returned 404 (repo not created / no releases yet) —
    // treat as "up to date" rather than a scary error.
    Ok(UpdateInfo {
        current: current.clone(),
        latest: current,
        has_update: false,
        notes: String::new(),
        download_url: String::new(),
        installer_filename: String::new(),
        source: "none".into(),
    })
}

fn check_gitee(owner: &str, current: &str) -> Result<Option<UpdateInfo>> {
    let url = GITEE_API.replace("__OWNER__", owner);
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("Memex")
        .build()?;
    let resp = client.get(&url).send()?;
    // 404 = repo not created / no releases yet → treat as "no update" (silent).
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let resp: GiteeRelease = resp.error_for_status()?.json()?;

    let latest = resp.tag_name.trim_start_matches('v').to_string();
    let has_update = version_is_newer(&latest, current);
    let asset = resp
        .assets
        .iter()
        .find(|a| a.name.ends_with(".exe"))
        .or_else(|| resp.assets.first());

    let (download_url, installer_filename) = match asset {
        Some(a) => (a.browser_download_url.clone(), a.name.clone()),
        None => (String::new(), String::new()),
    };

    Ok(Some(UpdateInfo {
        current: current.to_string(),
        latest,
        has_update,
        notes: resp.body.unwrap_or_default(),
        download_url,
        installer_filename,
        source: "gitee".into(),
    }))
}

fn check_github(owner: &str, current: &str) -> Result<Option<UpdateInfo>> {
    let url = GITHUB_API.replace("__OWNER__", owner);
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("Memex")
        .build()?;
    let resp = client.get(&url).send()?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let resp: GithubRelease = resp.error_for_status()?.json()?;

    let latest = resp.tag_name.trim_start_matches('v').to_string();
    let has_update = version_is_newer(&latest, current);
    let asset = resp
        .assets
        .iter()
        .find(|a| a.name.ends_with(".exe"))
        .or_else(|| resp.assets.first());

    let (download_url, installer_filename) = match asset {
        Some(a) => (a.browser_download_url.clone(), a.name.clone()),
        None => (String::new(), String::new()),
    };

    Ok(Some(UpdateInfo {
        current: current.to_string(),
        latest,
        has_update,
        notes: resp.body.unwrap_or_default(),
        download_url,
        installer_filename,
        source: "github".into(),
    }))
}

/// Compare semantic versions: returns true if `remote` > `local`.
fn version_is_newer(remote: &str, local: &str) -> bool {
    let parse = |s: &str| -> Vec<u64> {
        s.split('.')
            .filter_map(|p| p.split('-').next().unwrap_or(p).parse().ok())
            .collect()
    };
    let r = parse(remote);
    let l = parse(local);
    for i in 0..r.len().max(l.len()) {
        let rv = r.get(i).copied().unwrap_or(0);
        let lv = l.get(i).copied().unwrap_or(0);
        if rv > lv {
            return true;
        }
        if rv < lv {
            return false;
        }
    }
    false
}

/// Download the installer to data/update_cache/ with progress tracking.
/// Returns the local file path on success.
pub fn download_update(url: &str, filename: &str) -> Result<PathBuf> {
    let dest_dir = crate::config::data_dir().join("update_cache");
    std::fs::create_dir_all(&dest_dir)?;
    let dest = dest_dir.join(filename);

    DL_IN_PROGRESS.store(true, Ordering::Relaxed);
    DL_DONE.store(0, Ordering::Relaxed);

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(300))
        .user_agent("Memex")
        .build()?;

    let resp = client.get(url).send()?.error_for_status()?;
    let total = resp.content_length().unwrap_or(0);
    DL_TOTAL.store(total, Ordering::Relaxed);

    let mut file = std::fs::File::create(&dest)?;
    let mut done = 0u64;
    use std::io::Read;
    let mut stream = resp;
    let mut buf = [0u8; 8192];
    loop {
        let n = stream.read(&mut buf)?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        done += n as u64;
        DL_DONE.store(done, Ordering::Relaxed);
    }
    file.flush()?;

    DL_IN_PROGRESS.store(false, Ordering::Relaxed);
    eprintln!("[update] downloaded {} bytes → {}", done, dest.display());
    Ok(dest)
}

/// Launch the downloaded installer and exit the current process.
/// Inno Setup will handle the in-place upgrade (data/ is preserved).
pub fn install_update(installer_path: &PathBuf) -> Result<()> {
    eprintln!("[update] launching installer: {}", installer_path.display());

    // On Windows, launch the installer in a detached process.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x00000008;
        std::process::Command::new(installer_path)
            .creation_flags(DETACHED_PROCESS)
            .spawn()?;
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new(installer_path).spawn()?;
    }

    // Caller is responsible for releasing the single-instance lock and exiting.
    Ok(())
}

/// Background auto-check on startup. Silently logs result.
/// Stores result in AUTO_CHECK_RESULT for frontend to read.
///
/// Logging policy:
/// - 404 (repos not created / no releases): completely silent.
/// - Up to date (repos exist, same version): one info line.
/// - New version available: logged.
/// - Real network error: logged.
pub fn auto_check(owner: &str) {
    let result = match check_update(owner) {
        Ok(info) => {
            if info.has_update {
                eprintln!(
                    "[update] new version available: {} → {} (source: {})",
                    info.current, info.latest, info.source
                );
                Some(info)
            } else {
                // Up to date OR repos not yet created (source="none") — silent.
                if info.source != "none" {
                    eprintln!("[update] up to date ({})", info.current);
                }
                None
            }
        }
        Err(e) => {
            eprintln!("[update] auto-check failed: {e}");
            None
        }
    };
    *AUTO_CHECK_RESULT.lock().unwrap() = Some(result);
}

/// Get download progress as a JSON-serializable struct.
pub fn download_progress() -> serde_json::Value {
    let in_progress = DL_IN_PROGRESS.load(Ordering::Relaxed);
    let total = DL_TOTAL.load(Ordering::Relaxed);
    let done = DL_DONE.load(Ordering::Relaxed);
    let percent = if total > 0 {
        (done * 100 / total) as u64
    } else {
        0
    };
    let downloaded = DOWNLOADED_PATH
        .lock()
        .unwrap()
        .as_ref()
        .map(|p| p.exists())
        .unwrap_or(false);
    serde_json::json!({
        "in_progress": in_progress,
        "total": total,
        "done": done,
        "percent": percent,
        "downloaded": downloaded,
    })
}
