//! OCR module using rusto-rs (PaddleOCR PP-OCRv6 + pure Rust RTen inference).
//!
//! Only compiled when `ocr` feature is enabled.
//! Model files are loaded from `<home>/models/ocr/`:
//!   - det.rten (detection model)
//!   - rec.rten (recognition model)
//!   - dict.txt (character dictionary)
//!
//! PP-OCRv6 Tiny (~6MB total) supports 50+ languages including Chinese.
//!
//! ## Threading
//!
//! RTen (the inference runtime) defaults to creating a Rayon thread pool sized
//! to the machine's **physical core count** (16 on this machine). Each
//! `detect_text()` call saturates all those threads with matrix math, leaving
//! no headroom for the OS or foreground apps.
//!
//! We limit RTen to **2 threads** via the `RTEN_NUM_THREADS` env var, set
//! before the engine singleton is initialized. The engine itself is also a
//! `OnceLock` singleton so model files are loaded only once and the thread
//! pool is created only once — not per-image as before.

#![cfg(feature = "ocr")]

use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};

use anyhow::Result;

use crate::config;

/// Max RTen inference threads. 2 keeps OCR as a lightweight background task
/// that won't freeze the user's machine. Users can override by setting
/// `RTEN_NUM_THREADS` in their environment before launching Memex.
const OCR_THREAD_LIMIT: &str = "2";

static ENGINE: OnceLock<Option<Mutex<rusto::RustO>>> = OnceLock::new();

/// Lazily initialize the global OCR engine (only on first image file).
/// Returns a locked guard that serializes OCR calls across threads.
pub fn engine() -> Option<MutexGuard<'static, rusto::RustO>> {
    ENGINE
        .get_or_init(|| {
            // Limit RTen threads BEFORE loading models. RTen reads this env
            // var when it creates its global Rayon thread pool on the first
            // Model::load_file() call. If already set by the user, respect it.
            if std::env::var("RTEN_NUM_THREADS").is_err() {
                std::env::set_var("RTEN_NUM_THREADS", OCR_THREAD_LIMIT);
            }

            let model_dir = config::model_dir().join("ocr");
            let det_model = model_dir.join("det.rten");
            let rec_model = model_dir.join("rec.rten");
            let dict = model_dir.join("dict.txt");

            if !det_model.exists() || !rec_model.exists() || !dict.exists() {
                eprintln!(
                    "[ocr] model files not found in {}, OCR disabled",
                    model_dir.display()
                );
                return None;
            }

            let cfg = rusto::InitializeConfig::ppv6(
                det_model.to_string_lossy().as_ref(),
                rec_model.to_string_lossy().as_ref(),
                dict.to_string_lossy().as_ref(),
            );

            match rusto::RustO::initialize(cfg) {
                Ok(engine) => {
                    let threads = std::env::var("RTEN_NUM_THREADS")
                        .unwrap_or_else(|_| "auto".into());
                    eprintln!(
                        "[ocr] engine initialized (PP-OCRv6, threads={})",
                        threads
                    );
                    Some(Mutex::new(engine))
                }
                Err(e) => {
                    eprintln!("[ocr] init failed: {e}");
                    None
                }
            }
        })
        .as_ref()
        .map(|m| m.lock().expect("OCR engine mutex poisoned"))
}

/// Extract text from an image file using OCR.
/// Returns Ok(text) on success, Ok(empty) if engine not available.
pub fn extract_text(path: &Path) -> Result<String> {
    let mut engine = match engine() {
        Some(g) => g,
        None => return Ok(String::new()),
    };

    let img = rusto::ImageSource::Path(path.to_path_buf());
    let opts = rusto::OcrRunOptions::default();

    match engine.detect_text(&img, &opts) {
        Ok(rusto::DetectTextResult::Structured(results)) => {
            let text: Vec<String> = results.iter()
                .map(|r| r.text.clone())
                .collect();
            Ok(text.join("\n"))
        }
        Ok(rusto::DetectTextResult::Spatial(text)) => {
            Ok(text)
        }
        Err(e) => {
            eprintln!("[ocr] failed for {}: {e}", path.display());
            Ok(String::new())
        }
    }
}

/// Check if a file extension is an image type that OCR can process.
pub fn is_image_ext(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "bmp" | "tiff" | "tif" | "webp"
    )
}
