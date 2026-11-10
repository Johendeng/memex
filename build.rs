//! Post-build glue: copies `WebView2Loader.dll` (from webview2-com-sys build
//! output) next to the final binary. Windows resolves this DLL from the
//! import table at process startup, so it must live beside the exe
//! (no PATH tinkering can fix a load-time import).

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    // Only needed on Windows (wry uses WebView2 there).
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        // Embed application icon into the .exe (Windows PE resource).
        // This makes Explorer / shortcuts show the correct icon.
        if let Err(e) = winres::WindowsResource::new()
            .set_icon("assets/memex_icon.ico")
            .compile()
        {
            println!("cargo:warning=winres failed to embed icon: {e}");
        }
    }
    if target_os != "windows" {
        return;
    }

    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let arch_dir = match target_arch.as_str() {
        "x86_64" => "x64",
        "x86" => "x86",
        "aarch64" => "arm64",
        _ => return, // unsupported arch, skip silently
    };

    // Locate the DLL inside the webview2-com-sys crate (its build script
    // extracts it to <OUT_DIR>/<arch_dir>/WebView2Loader.dll). Search each
    // dependency OUT_DIR under target/<profile>/build/*/out/.
    let target_dir = env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("target"));
    let profile = env::var("PROFILE").unwrap_or_else(|_| "release".into());
    let build_dir = target_dir.join(&profile).join("build");

    let mut found: Option<PathBuf> = None;
    if let Ok(entries) = std::fs::read_dir(&build_dir) {
        for entry in entries.flatten() {
            let out = entry.path().join("out").join(arch_dir).join("WebView2Loader.dll");
            if out.is_file() {
                found = Some(out);
                break;
            }
        }
    }

    if let Some(src) = found {
        let dest = target_dir.join(&profile).join("WebView2Loader.dll");
        if src != dest {
            let _ = std::fs::copy(&src, &dest);
            // Also emit a rerun-if-changed so future rebuilds re-check.
            println!("cargo:rerun-if-changed={}", src.display());
        }
    } else {
        println!("cargo:warning=WebView2Loader.dll not found — native window mode may fail at runtime");
    }

    // Make sure we never cache-stale this script across profile differences.
    println!("cargo:rerun-if-env-changed=PROFILE");
    println!("cargo:rerun-if-env-changed=CARGO_CFG_TARGET_ARCH");

    let _ = Path::new(""); // keep Path import used if above branches drop it
}