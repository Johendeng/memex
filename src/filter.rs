use std::path::Path;

use anyhow::Result;
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};

/// Global file filter. Stored as JSON inside `meta.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterRules {
    pub include_exts: Vec<String>,
    pub exclude_exts: Vec<String>,
    pub max_bytes: u64,
    pub exclude_globs: Vec<String>,
    pub skip_empty: bool,
    pub skip_hidden: bool,
    pub skip_system: bool,
}

impl Default for FilterRules {
    fn default() -> Self {
        FilterRules {
            include_exts: {
                let mut exts: Vec<&str> = vec![
                    "txt", "md", "markdown", "html", "htm", "csv", "json", "rst", "org",
                    "docx", "xlsx", "pptx", "pdf",
                ];
                #[cfg(feature = "ocr")]
                {
                    exts.extend_from_slice(&["png", "jpg", "jpeg", "bmp", "tiff", "tif", "webp"]);
                }
                exts.into_iter().map(String::from).collect()
            },
            exclude_exts: vec!["tmp", "bak", "exe", "dll", "so"]
                .into_iter()
                .map(String::from)
                .collect(),
            max_bytes: 50 * 1024 * 1024,
            exclude_globs: vec![
                "**/node_modules/**",
                "**/.git/**",
                "**/~$*",
                "**/Thumbs.db",
                "**/desktop.ini",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            skip_empty: true,
            skip_hidden: true,
            skip_system: true,
        }
    }
}

impl FilterRules {
    pub fn build_globset(&self) -> Result<GlobSet> {
        let mut b = GlobSetBuilder::new();
        for pat in &self.exclude_globs {
            b.add(Glob::new(pat)?);
        }
        Ok(b.build()?)
    }

    /// Decide whether a file should be indexed. `size` is the on-disk size.
    pub fn accepts(&self, path: &Path, size: u64) -> bool {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let name_lower = name.to_lowercase();

        if self.skip_hidden && name.starts_with('.') {
            return false;
        }
        if self.skip_system && (name_lower == "thumbs.db" || name_lower == "desktop.ini") {
            return false;
        }
        if self.skip_empty && size == 0 {
            return false;
        }
        if size > self.max_bytes {
            return false;
        }
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if self.exclude_exts.iter().any(|e| *e == ext) {
            return false;
        }
        // Empty whitelist = no extension restriction (index any extension not otherwise
        // excluded). This is the intuitive "cleared the list" behavior; a populated list
        // acts as a strict whitelist.
        self.include_exts.is_empty() || self.include_exts.iter().any(|e| *e == ext)
    }
}
