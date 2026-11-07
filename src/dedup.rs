//! Duplicate file detection and cleanup.
//!
//! Scan: group files by size → compute SHA-256 for same-size groups →
//!   group by hash → groups with >1 copy are "duplicate groups"
//! Clean: for each group, keep the shallowest path, delete the rest,
//!   remove from index, tag the keeper with "重要度 / 高频·N份",
//!   set dup_count in Meta for search weight boost.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use sha2::{Digest, Sha256};

use crate::store::{BigramIndex, Meta, TagRec};

// ---- response types ----

#[derive(Debug, serde::Serialize)]
pub struct DupFile {
    pub file_id: i64,
    pub path: String,
    pub depth: usize,
    pub size: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct DupGroup {
    pub hash: String,
    pub file_name: String,
    pub copies: Vec<DupFile>,
    pub keep_file_id: i64,
    pub savings_bytes: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct ScanResult {
    pub groups: Vec<DupGroup>,
    pub total_groups: usize,
    pub total_dup_files: usize,
    pub total_savings_bytes: u64,
}

#[derive(Debug, serde::Serialize)]
pub struct CleanResult {
    pub deleted_files: usize,
    pub freed_bytes: u64,
    pub tagged_files: usize,
    pub errors: Vec<String>,
}

// ---- helpers ----

fn path_depth(path: &str) -> usize {
    path.chars().filter(|&c| c == '\\' || c == '/').count()
}

fn file_hash(path: &str) -> Result<String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let hash = hasher.finalize();
    // Full 128-bit hex (64 chars) — matches user spec: content-exact SHA-256
    Ok(format!("{:x}", hash))
}

fn full_path(meta: &Meta, file_id: i64) -> Option<String> {
    let file = meta.files.iter().find(|f| f.id == file_id)?;
    let folder = meta.folders.iter().find(|fo| fo.id == file.folder_id)?;
    Some(format!("{}/{}", folder.path, file.rel_path))
}

// ---- scan ----

/// Build (file_id, size, full_path) snapshot from Meta — call under lock, then drop lock.
pub fn snapshot(meta: &Meta) -> Vec<(i64, u64, String)> {
    meta.files
        .iter()
        .filter_map(|f| {
            if f.size == 0 {
                return None;
            }
            full_path(meta, f.id).map(|p| (f.id, f.size, p))
        })
        .collect()
}

/// Scan from a pre-built snapshot — NO lock needed, safe to call after dropping the Meta guard.
pub fn scan_files(files: &[(i64, u64, String)]) -> Result<ScanResult> {
    // Phase 1: group by file size (instant — no disk reads)
    let mut size_groups: HashMap<u64, Vec<usize>> = HashMap::new(); // size → indices into files
    for (i, (_, size, _)) in files.iter().enumerate() {
        size_groups.entry(*size).or_default().push(i);
    }

    // Phase 2: compute SHA-256 only for same-size groups (dedup candidates)
    let mut hash_groups: HashMap<String, Vec<i64>> = HashMap::new();
    for (_size, indices) in &size_groups {
        if indices.len() < 2 {
            continue;
        }
        for &i in indices {
            let (fid, _, path) = &files[i];
            match file_hash(path) {
                Ok(h) => {
                    hash_groups.entry(h).or_default().push(*fid);
                }
                Err(_) => continue,
            }
        }
    }

    // Phase 3: build DupGroups for groups with >1 copy
    let mut groups = Vec::new();
    let mut total_dup_files = 0usize;
    let mut total_savings_bytes = 0u64;

    // Build file_id → (size, path) lookup
    let file_map: HashMap<i64, (u64, &str)> =
        files.iter().map(|(fid, sz, p)| (*fid, (*sz, p.as_str()))).collect();

    for (hash, file_ids) in &hash_groups {
        if file_ids.len() < 2 {
            continue;
        }

        let mut copies: Vec<DupFile> = file_ids
            .iter()
            .filter_map(|&fid| {
                let (size, path) = *file_map.get(&fid)?;
                Some(DupFile {
                    file_id: fid,
                    depth: path_depth(path),
                    path: path.to_string(),
                    size,
                })
            })
            .collect();

        copies.sort_by(|a, b| a.depth.cmp(&b.depth).then(a.path.cmp(&b.path)));

        if copies.len() < 2 {
            continue;
        }

        let keep_file_id = copies[0].file_id;
        let savings_bytes = copies[1..].iter().map(|c| c.size).sum();

        let file_name = file_map
            .get(&keep_file_id)
            .map(|(_, p)| {
                Path::new(p)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| p.to_string())
            })
            .unwrap_or_default();

        total_dup_files += copies.len() - 1;
        total_savings_bytes += savings_bytes;

        groups.push(DupGroup {
            hash: hash.clone(),
            file_name,
            copies,
            keep_file_id,
            savings_bytes,
        });
    }

    groups.sort_by(|a, b| b.savings_bytes.cmp(&a.savings_bytes));
    let total_groups = groups.len();

    Ok(ScanResult {
        groups,
        total_groups,
        total_dup_files,
        total_savings_bytes,
    })
}

/// Legacy entry — used by clean() which already holds the lock.
pub fn scan(meta: &Meta) -> Result<ScanResult> {
    let snap = snapshot(meta);
    scan_files(&snap)
}

// ---- clean ----

/// Clean using a pre-computed scan result — avoids re-scanning under lock.
/// Call with locks held; deletion phase is fast (batch retain + chunk marking).
pub fn clean_from_scan(
    meta: &mut Meta,
    idx: &mut BigramIndex,
    scan_result: &ScanResult,
    hashes: &[String],
) -> Result<CleanResult> {
    let mut deleted_files = 0usize;
    let mut freed_bytes = 0u64;
    let mut tagged_files = 0usize;
    let mut errors = Vec::new();
    let requested: std::collections::HashSet<&str> =
        hashes.iter().map(|s| s.as_str()).collect();

    // Collect all file_ids to delete for batch removal
    let mut ids_to_delete: std::collections::HashSet<i64> = std::collections::HashSet::new();

    for group in &scan_result.groups {
        if !requested.contains(group.hash.as_str()) {
            continue;
        }

        let copies = &group.copies;
        if copies.len() < 2 {
            continue;
        }

        let keep_file_id = copies[0].file_id;
        let dup_count = copies.len() as u32;

        // Delete redundant copies from disk (copies[1..])
        for dup in &copies[1..] {
            match std::fs::remove_file(&dup.path) {
                Ok(_) => {
                    freed_bytes += dup.size;
                    deleted_files += 1;
                    ids_to_delete.insert(dup.file_id);
                    idx.remove_file(dup.file_id);
                }
                Err(e) => {
                    errors.push(format!("删除失败 {}: {e}", dup.path));
                }
            }
        }

        // Tag keeper: dim="重要度", value="高频·N份"
        meta.tags.retain(|t| !(t.file_id == keep_file_id && t.dim == "重要度"));
        meta.tags.push(TagRec {
            file_id: keep_file_id,
            dim: "重要度".to_string(),
            value: format!("高频·{}份", dup_count),
            source: "dedup".to_string(),
            by_agent: None,
            note: Some(format!("原重复{}份，已清理{}份", dup_count, dup_count - 1)),
            at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs().to_string())
                .unwrap_or_default(),
        });

        // Set dup_count for search weight boost
        meta.dup_counts.insert(keep_file_id, dup_count);

        tagged_files += 1;
    }

    // Batch removal: single-pass retain instead of O(N) per file
    if !ids_to_delete.is_empty() {
        meta.files.retain(|f| !ids_to_delete.contains(&f.id));
        meta.tags.retain(|t| !ids_to_delete.contains(&t.file_id));
        meta.confidence.retain(|c| !ids_to_delete.contains(&c.file_id));
    }

    meta.save()?;
    idx.save()?;

    Ok(CleanResult {
        deleted_files,
        freed_bytes,
        tagged_files,
        errors,
    })
}
