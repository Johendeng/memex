//! OCR module using rusto-rs (PaddleOCR PP-OCRv6 + pure Rust RTen inference).
//!
//! Only compiled when `ocr` feature is enabled.
//! Model files are loaded from `<home>/models/ocr/`:
//!   - det.rten (detection model)
//!   - rec.rten (recognition model)
//!   - dict.txt (character dictionary)
//!
//! PP-OCRv6 Tiny (~6MB total) supports 50+ languages including Chinese.

#![cfg(feature = "ocr")]

use std::path::Path;

use anyhow::{Context, Result};

use crate::config;

/// Initialize OCR engine with model files from models/ocr/.
/// Returns None if models are not found (graceful degradation).
pub fn try_init() -> Option<rusto::RustO> {
    let model_dir = config::model_dir().join("ocr");

    let det_model = model_dir.join("det.rten");
    let rec_model = model_dir.join("rec.rten");
    let dict = model_dir.join("dict.txt");

    if !det_model.exists() || !rec_model.exists() || !dict.exists() {
        eprintln!("[ocr] model files not found in {}, OCR disabled", model_dir.display());
        return None;
    }

    let cfg = rusto::InitializeConfig::ppv6(
        det_model.to_string_lossy().as_ref(),
        rec_model.to_string_lossy().as_ref(),
        dict.to_string_lossy().as_ref(),
    );

    match rusto::RustO::initialize(cfg) {
        Ok(engine) => {
            eprintln!("[ocr] engine initialized with PP-OCRv6 Tiny");
            Some(engine)
        }
        Err(e) => {
            eprintln!("[ocr] init failed: {e}");
            None
        }
    }
}

/// Extract text from an image file using OCR.
/// Returns Ok(text) on success, Ok(empty) if engine not available.
pub fn extract_text(path: &Path) -> Result<String> {
    let mut engine = match try_init() {
        Some(e) => e,
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
