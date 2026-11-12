//! Post-build glue: copies `WebView2Loader.dll` (from webview2-com-sys build
//! output) next to the final binary. Windows resolves this DLL from the
//! import table at process startup, so it must live beside the exe
//! (no PATH tinkering can fix a load-time import).

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    // Embed application icon into the .exe (Windows PE resource).
    // This makes Explorer / shortcuts show the correct icon.
    // Uses #[cfg(windows)] so the winres crate (Windows-only build-dep)
    // is not referenced on Linux/macOS.
    #[cfg(windows)]
    {
        if let Err(e) = winres::WindowsResource::new()
            .set_icon("assets/memex_icon.ico")
            .compile()
        {
            println!("cargo:warning=winres failed to embed icon: {e}");
        }
    }

    // `winres` only works for MSVC: it emits `cargo:rustc-link-lib=dylib=resource`
    // and lets the linker pull the `.lib` in by name. The GNU (MinGW) backend
    // instead archives the object into `libresource.a` and emits
    // `cargo:rustc-link-lib=static=resource`; because a `.rsrc`-only object
    // exports no symbols, `ld` never extracts that archive member, so the
    // resource section is silently dropped and the built exe gets no icon and
    // no version info. Link the object explicitly to force it in.
    #[cfg(all(windows, target_env = "gnu"))]
    {
        let out_dir = env::var("OUT_DIR").unwrap_or_default();
        let obj = PathBuf::from(&out_dir).join("resource.o");
        if obj.is_file() {
            println!("cargo:rustc-link-arg={}", obj.display());
        } else {
            println!(
                "cargo:warning=resource.o not found in OUT_DIR — icon will be missing from the exe"
            );
        }
    }

    // WebView2Loader.dll copy — Windows only.
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
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