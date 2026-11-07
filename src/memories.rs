//! Agent memory store: persistent key-value memories + markdown notes.
//!
//! Two storage layers:
//! 1. **KV memories** (`data/memories.json`): fast structured recall for
//!    facts, decisions, preferences. No indexing required — direct lookup.
//! 2. **Markdown notes** (`data/notes/<title>.md`): long-form agent notes
//!    that are human-readable and can be indexed by the watcher for search.
//!
//! Together these transform Memex from a read-only document search tool
//! into a **local memory body** that AI agents can write to and recall from.

use std::path::PathBuf;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::config;

// ------------------------------------------------------------ paths

fn memories_path() -> Result<PathBuf> {
    Ok(config::data_dir().join("memories.json"))
}

pub fn notes_dir() -> Result<PathBuf> {
    let dir = config::data_dir().join("notes");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

// ------------------------------------------------------------ KV store

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub key: String,
    pub value: String,
    pub namespace: String,
    pub at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct MemoryStore {
    #[serde(default)]
    pub memories: Vec<Memory>,
}

impl MemoryStore {
    pub fn load() -> Result<Self> {
        let p = memories_path()?;
        if p.exists() {
            let s = std::fs::read_to_string(&p)?;
            Ok(serde_json::from_str(&s).unwrap_or_default())
        } else {
            Ok(MemoryStore::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let p = memories_path()?;
        let s = serde_json::to_string_pretty(self)?;
        let tmp = p.with_extension("tmp");
        std::fs::write(&tmp, s)?;
        std::fs::rename(&tmp, &p)?;
        Ok(())
    }

    /// Upsert a memory by (namespace, key). Returns whether it was an update.
    pub fn upsert(&mut self, key: &str, value: &str, namespace: &str, note: Option<&str>) -> bool {
        let at = now();
        if let Some(m) = self.memories.iter_mut().find(|m| m.key == key && m.namespace == namespace) {
            m.value = value.to_string();
            m.at = at;
            m.note = note.map(String::from);
            true
        } else {
            self.memories.push(Memory {
                key: key.to_string(),
                value: value.to_string(),
                namespace: namespace.to_string(),
                at,
                note: note.map(String::from),
            });
            false
        }
    }

    /// Exact-match recall by key within a namespace.
    pub fn recall(&self, key: &str, namespace: &str) -> Vec<&Memory> {
        self.memories.iter()
            .filter(|m| m.key == key && m.namespace == namespace)
            .collect()
    }

    /// Fuzzy recall: substring match on key or value within namespace (or all).
    pub fn search(&self, query: &str, namespace: Option<&str>) -> Vec<&Memory> {
        let q = query.to_lowercase();
        self.memories.iter()
            .filter(|m| {
                (namespace.is_none() || m.namespace == namespace.unwrap_or(""))
                && (m.key.to_lowercase().contains(&q) || m.value.to_lowercase().contains(&q))
            })
            .collect()
    }

    /// Delete by key within namespace. Returns whether deleted.
    pub fn forget(&mut self, key: &str, namespace: &str) -> bool {
        let before = self.memories.len();
        self.memories.retain(|m| !(m.key == key && m.namespace == namespace));
        self.memories.len() < before
    }

    /// List all memories in a namespace (or all if None).
    pub fn list(&self, namespace: Option<&str>) -> Vec<&Memory> {
        self.memories.iter()
            .filter(|m| namespace.is_none() || m.namespace == namespace.unwrap_or(""))
            .collect()
    }
}

// ------------------------------------------------------------ notes

/// Sanitize a title into a safe filename (no path separators, no special chars).
fn sanitize_title(title: &str) -> String {
    title
        .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
        .trim()
        .to_string()
}

/// Write a markdown note to `data/notes/<title>.md`.
/// If overwrite=false and file exists, returns error.
pub fn write_note(title: &str, content: &str, tags: Option<&str>, overwrite: bool) -> Result<PathBuf> {
    let dir = notes_dir()?;
    let safe = sanitize_title(title);
    if safe.is_empty() {
        anyhow::bail!("note title cannot be empty");
    }
    let filename = format!("{}.md", safe);
    let path = dir.join(&filename);

    if !overwrite && path.exists() {
        anyhow::bail!("note already exists: {}. Use overwrite=true to replace.", filename);
    }

    let mut md = String::new();
    // Optional frontmatter with tags
    if let Some(tags) = tags {
        if !tags.is_empty() {
            md.push_str("---\n");
            md.push_str(&format!("title: {}\n", title));
            md.push_str(&format!("tags: [{}]\n", tags));
            md.push_str(&format!("created_by: agent\n"));
            md.push_str(&format!("created_at: {}\n", now()));
            md.push_str("---\n\n");
        }
    }
    md.push_str(&format!("# {}\n\n", title));
    md.push_str(content);

    std::fs::write(&path, md)?;
    Ok(path)
}

/// List all notes in the notes directory.
pub fn list_notes() -> Result<Vec<NoteInfo>> {
    let dir = notes_dir()?;
    let mut notes = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        let meta = std::fs::metadata(&path)?;
        let size = meta.len();
        let mtime = meta.modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs().to_string())
            .unwrap_or_default();
        // Read first line as title preview
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        let preview: String = content.lines()
            .skip_while(|l| l.starts_with("---") || l.starts_with("title:") || l.starts_with("tags:") || l.starts_with("created_"))
            .take(3)
            .collect::<Vec<_>>()
            .join(" ");
        let preview = if preview.len() > 200 { preview[..200].to_string() } else { preview };
        notes.push(NoteInfo {
            name,
            filename: path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string(),
            path: path.to_string_lossy().to_string(),
            size,
            mtime,
            preview,
        });
    }
    notes.sort_by(|a, b| b.mtime.cmp(&a.mtime));
    Ok(notes)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteInfo {
    pub name: String,
    pub filename: String,
    pub path: String,
    pub size: u64,
    pub mtime: String,
    pub preview: String,
}

/// Delete a note by title (or filename).
pub fn delete_note(title: &str) -> Result<bool> {
    let dir = notes_dir()?;
    let safe = sanitize_title(title);
    let path = dir.join(format!("{}.md", safe));
    if path.exists() {
        std::fs::remove_file(&path)?;
        Ok(true)
    } else {
        // Try direct filename match
        let path2 = dir.join(title);
        if path2.exists() {
            std::fs::remove_file(&path2)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// Read a note's full content.
pub fn read_note(title: &str) -> Result<String> {
    let dir = notes_dir()?;
    let safe = sanitize_title(title);
    let path = dir.join(format!("{}.md", safe));
    if !path.exists() {
        anyhow::bail!("note not found: {}", safe);
    }
    Ok(std::fs::read_to_string(&path)?)
}

// ------------------------------------------------------------ utils

fn now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}
