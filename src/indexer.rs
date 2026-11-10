use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicU8, AtomicU64, AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::UNIX_EPOCH;

use anyhow::Result;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use walkdir::WalkDir;

use crate::parser;
use crate::store::{BigramIndex, FileRec, Meta};

// ---- progress tracking (polled by cmd_index_progress) ----
pub static PROGRESS_TOTAL: AtomicU64 = AtomicU64::new(0);
pub static PROGRESS_DONE: AtomicU64 = AtomicU64::new(0);
pub static PROGRESS_SCANNED: AtomicU64 = AtomicU64::new(0);
pub static PROGRESS_CURRENT: Mutex<Option<String>> = Mutex::new(None);

/// 0=idle, 1=title-phase, 2=content-phase
pub static PROGRESS_PHASE: AtomicU8 = AtomicU8::new(0);
pub static PROGRESS_CONTENT_TOTAL: AtomicU64 = AtomicU64::new(0);
pub static PROGRESS_CONTENT_DONE: AtomicU64 = AtomicU64::new(0);

/// OCR-specific progress (subset of content phase, tracked for UI).
pub static PROGRESS_OCR_TOTAL: AtomicU64 = AtomicU64::new(0);
pub static PROGRESS_OCR_DONE: AtomicU64 = AtomicU64::new(0);

/// Set by background thread when Meta changed on disk; serve thread checks
/// this and reloads Meta before the next API call.
pub static META_DIRTY: AtomicBool = AtomicBool::new(false);

/// ~500-char chunks with 80-char overlap (CJK-friendly, see tech-design).
pub const CHUNK_TARGET: usize = 500;
pub const CHUNK_OVERLAP: usize = 80;

/// How many files to parse before saving index+meta to disk in Phase 2.
const SAVE_BATCH: usize = 50;

#[derive(Debug, Default, serde::Serialize)]
pub struct IndexStats {
    pub scanned: u64,
    pub indexed: u64,
    pub skipped: u64,
    pub failed: u64,
    pub chunks: u64,
    /// 文档指纹未变而跳过解析的文件数（增量模式）
    #[serde(default)]
    pub unchanged: u64,
    /// 仅标题索引（等待 Phase 2）的文件数
    #[serde(default)]
    pub title_only: u64,
}

/// Split text into overlapping chunks on char boundaries.
pub fn chunk_text(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut start = 0usize;
    while start < chars.len() {
        let end = (start + CHUNK_TARGET).min(chars.len());
        out.push(chars[start..end].iter().collect());
        if end == chars.len() {
            break;
        }
        start = end.saturating_sub(CHUNK_OVERLAP);
    }
    out
}

// ===================================================== Phase 1: title index

/// Phase 1 — 快速标题索引（同步，秒级）。
///
/// Walk 全部文件夹，为每个文件注册 FileRec（稳定 ID：按路径复用旧 ID），
/// 仅用 file_stem 创建 chunk#0，不解析文件内容。
/// 完成后保存 Meta + Index，用户立即可按文件名搜索。
///
/// 增量逻辑：
/// - mtime+size 未变 + content_indexed=true → 完全跳过（零开销）
/// - mtime+size 未变 + content_indexed=false → 保留，等 Phase 2
/// - mtime+size 变化 → 移除旧 chunk，重新注册标题 chunk，content_indexed=false
/// - 新文件 → 注册 + content_indexed=false
/// - 已消失 → 从 Index 移除 chunk
pub fn index_titles(meta: &mut Meta, idx: &mut BigramIndex) -> Result<IndexStats> {
    let rules = meta.filter_rules.clone();
    let globset = rules.build_globset()?;

    // old_files: rel_path → (file_id, mtime, size, content_indexed)
    let old_files: HashMap<String, (i64, u64, u64, bool)> = meta
        .files
        .iter()
        .map(|f| (f.rel_path.clone(), (f.id, f.mtime, f.size, f.content_indexed)))
        .collect();

    // Use the shared index (already loaded by caller)
    if idx.file_chunks.is_empty() && !idx.chunks.is_empty() {
        idx.rebuild_file_chunks_map();
    }

    let mut stats = IndexStats::default();
    let mut new_files: Vec<FileRec> = Vec::new();
    let mut next_file_id = meta.next_file_id;

    // Track which old file_ids are still present
    let mut seen_old_ids: HashSet<i64> = HashSet::new();

    // Pre-count total files
    let mut total: u64 = 0;
    for folder in &meta.folders {
        let root = Path::new(&folder.path);
        if !root.exists() {
            continue;
        }
        for entry in WalkDir::new(root).follow_links(true).into_iter().flatten() {
            if entry.file_type().is_file() {
                total += 1;
            }
        }
    }
    PROGRESS_TOTAL.store(total, Ordering::Relaxed);
    PROGRESS_DONE.store(0, Ordering::Relaxed);
    PROGRESS_SCANNED.store(0, Ordering::Relaxed);
    PROGRESS_PHASE.store(1, Ordering::Relaxed);
    *PROGRESS_CURRENT.lock().unwrap() = None;

    for folder in &meta.folders {
        let root = Path::new(&folder.path);
        if !root.exists() {
            continue;
        }

        for entry in WalkDir::new(root).follow_links(true).into_iter() {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            stats.scanned += 1;
            PROGRESS_SCANNED.fetch_add(1, Ordering::Relaxed);

            let path = entry.path();
            let norm = path.to_string_lossy().replace('\\', "/");
            if globset.is_match(&norm) {
                stats.skipped += 1;
                continue;
            }

            let m = entry.metadata()?;
            let size = m.len();
            if !rules.accepts(path, size) {
                stats.skipped += 1;
                continue;
            }

            let rel = match path.strip_prefix(root) {
                Ok(r) => r.to_string_lossy().replace('\\', "/"),
                Err(_) => {
                    stats.skipped += 1;
                    continue;
                }
            };
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let mtime = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs())
                .unwrap_or(0);

            *PROGRESS_CURRENT.lock().unwrap() = Some(norm.clone());

            // Check if file is unchanged
            let file_id = if let Some((old_id, old_mtime, old_size, was_indexed)) =
                old_files.get(&rel)
            {
                if *old_mtime == mtime && *old_size == size && *was_indexed {
                    // Completely unchanged — keep existing FileRec
                    stats.unchanged += 1;
                    seen_old_ids.insert(*old_id);
                    if let Some(f) = meta.files.iter().find(|f| f.id == *old_id) {
                        new_files.push(f.clone());
                    }
                    PROGRESS_DONE.fetch_add(1, Ordering::Relaxed);
                    continue;
                } else if *old_mtime == mtime && *old_size == size && !*was_indexed {
                    // Same file, content not yet indexed — keep as title-only
                    stats.title_only += 1;
                    seen_old_ids.insert(*old_id);
                    if let Some(f) = meta.files.iter().find(|f| f.id == *old_id) {
                        new_files.push(f.clone());
                    }
                    PROGRESS_DONE.fetch_add(1, Ordering::Relaxed);
                    continue;
                } else {
                    // File changed — reuse ID, remove old chunks, re-register title
                    seen_old_ids.insert(*old_id);
                    idx.remove_file(*old_id);
                    *old_id
                }
            } else {
                // New file
                let id = next_file_id;
                next_file_id += 1;
                id
            };

            // Create title-only chunk#0 (just the file stem)
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();

            idx.add_file(file_id, &[stem]);

            new_files.push(FileRec {
                id: file_id,
                folder_id: folder.id,
                rel_path: rel,
                ext,
                size,
                mtime,
                content_indexed: false,
            });
            stats.indexed += 1;
            stats.title_only += 1;
            stats.chunks += 1;
            PROGRESS_DONE.fetch_add(1, Ordering::Relaxed);
        }
    }

    // Remove deleted files from index
    for (old_id, _, _, _) in old_files.values() {
        if !seen_old_ids.contains(old_id) {
            idx.remove_file(*old_id);
        }
    }

    *PROGRESS_CURRENT.lock().unwrap() = None;
    PROGRESS_PHASE.store(0, Ordering::Relaxed);

    meta.files = new_files;
    meta.next_file_id = next_file_id;
    idx.save_postings()?;
    meta.save()?;
    Ok(stats)
}

// ===================================================== Phase 2: content index

/// Phase 2 — 后台内容索引（在独立线程中运行）。
///
/// 遍历 `content_indexed == false` 的文件，逐个解析内容，
/// 用 `file_stem + "\n" + content` 重新分 chunk，替换该文件的标题 chunk。
/// 每 SAVE_BATCH 个文件存盘一次。完成后自动触发向量嵌入（Q1=A）。
pub fn index_content_background(meta: Arc<Mutex<Meta>>, idx: Arc<RwLock<BigramIndex>>) {
    if let Err(e) = index_content_inner(&meta, &idx) {
        eprintln!("content indexing failed: {e:#}");
    }
    PROGRESS_PHASE.store(0, Ordering::Relaxed);
    *PROGRESS_CURRENT.lock().unwrap() = None;
}

fn index_content_inner(meta: &Arc<Mutex<Meta>>, idx: &Arc<RwLock<BigramIndex>>) -> Result<()> {
    // Collect files needing content indexing (hold lock briefly)
    let (folder_paths, to_index): (HashMap<i64, String>, Vec<FileRec>) = {
        let m = meta.lock().unwrap();
        let fp: HashMap<i64, String> =
            m.folders.iter().map(|f| (f.id, f.path.clone())).collect();
        let files: Vec<FileRec> = m
            .files
            .iter()
            .filter(|f| !f.content_indexed)
            .cloned()
            .collect();
        (fp, files)
    };

    let total = to_index.len() as u64;
    PROGRESS_PHASE.store(2, Ordering::Relaxed);
    PROGRESS_CONTENT_TOTAL.store(total, Ordering::Relaxed);
    PROGRESS_CONTENT_DONE.store(0, Ordering::Relaxed);

    // OCR progress: count image files among the pending set
    let ocr_total = to_index
        .iter()
        .filter(|f| {
            Path::new(&f.rel_path)
                .extension()
                .map(|e| parser::is_image_ext(&e.to_string_lossy().to_lowercase()))
                .unwrap_or(false)
        })
        .count() as u64;
    PROGRESS_OCR_TOTAL.store(ocr_total, Ordering::Relaxed);
    PROGRESS_OCR_DONE.store(0, Ordering::Relaxed);

    if total == 0 {
        return Ok(());
    }

    let mut done = 0u64;
    let mut failed = 0u64;
    let mut processed_ids: HashSet<i64> = HashSet::new();
    let mut pending_replacements: Vec<(i64, Vec<String>)> = Vec::with_capacity(SAVE_BATCH);

    for file in &to_index {
        let Some(folder_path) = folder_paths.get(&file.folder_id) else {
            failed += 1;
            continue;
        };
        let full_path = format!("{}/{}", folder_path, file.rel_path);
        *PROGRESS_CURRENT.lock().unwrap() = Some(full_path.clone());

        let is_img = Path::new(&file.rel_path)
            .extension()
            .map(|e| parser::is_image_ext(&e.to_string_lossy().to_lowercase()))
            .unwrap_or(false);

        let text = match parser::extract_text(Path::new(&full_path)) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("skip {full_path}: {e:#}");
                failed += 1;
                // Mark as indexed with title-only to avoid endless retry
                let stem = Path::new(&full_path)
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                pending_replacements.push((file.id, vec![stem]));
                done += 1;
                processed_ids.insert(file.id);
                PROGRESS_CONTENT_DONE.store(done, Ordering::Relaxed);
                if is_img {
                    PROGRESS_OCR_DONE.fetch_add(1, Ordering::Relaxed);
                }
                if (done as usize) % SAVE_BATCH == 0 {
                flush_batch(&mut pending_replacements, idx, meta, &processed_ids);
                }
                continue;
            }
        };

        // Prepend file stem as title line
        let stem = Path::new(&full_path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let full_text = format!("{}\n{}", stem, text);
        let chunks = chunk_text(&full_text);

        pending_replacements.push((file.id, chunks));
        done += 1;
        processed_ids.insert(file.id);
        PROGRESS_CONTENT_DONE.store(done, Ordering::Relaxed);
        if is_img {
            PROGRESS_OCR_DONE.fetch_add(1, Ordering::Relaxed);
        }

        // Save every SAVE_BATCH files
        if (done as usize) % SAVE_BATCH == 0 {
            flush_batch(&mut pending_replacements, idx, meta, &processed_ids);
        }
    }

    // Final flush
    flush_batch(&mut pending_replacements, idx, meta, &processed_ids);

    eprintln!("content indexing done: {} files ({} failed)", done, failed);
    if ocr_total > 0 {
        eprintln!(
            "ocr progress: {}/{} images processed",
            PROGRESS_OCR_DONE.load(Ordering::Relaxed),
            ocr_total
        );
    }

    // Auto-trigger embedding after content indexing completes (Q1=A)
    if done > 0 {
        eprintln!("auto-triggering embedding...");
        let ix = idx.read().unwrap();
        if let Err(e) = crate::embed::embed_all_with_idx(&ix) {
            eprintln!("embedding failed: {e:#}");
        }
    }

    Ok(())
}

/// Flush a batch of (file_id, chunks) to the shared index and save to disk.
/// Called between batches to limit how long the write lock is held.
fn flush_batch(
    pending: &mut Vec<(i64, Vec<String>)>,
    idx: &Arc<RwLock<BigramIndex>>,
    meta: &Arc<Mutex<Meta>>,
    processed: &HashSet<i64>,
) {
    if pending.is_empty() {
        return;
    }
    let batch: Vec<(i64, Vec<String>)> = std::mem::take(pending);
    {
        let mut ix = idx.write().unwrap();
        for (file_id, chunks) in &batch {
            ix.replace_file(*file_id, chunks);
        }
        if let Err(e) = ix.save_postings() {
            eprintln!("content indexing: idx.save_postings() failed: {e:#}");
        }
    }
    update_content_flags(meta, processed);
    META_DIRTY.store(true, Ordering::Relaxed);
}

/// Update `content_indexed = true` for processed files in Meta.
fn update_content_flags(meta: &Arc<Mutex<Meta>>, ids: &HashSet<i64>) {
    let mut m = meta.lock().unwrap();
    for f in &mut m.files {
        if ids.contains(&f.id) {
            f.content_indexed = true;
        }
    }
    let _ = m.save();
}

// ===================================================== Full rebuild

/// Full rebuild — 从零重建全部索引（含内容解析，同步）。
/// 保留稳定 ID（按路径复用），所有文件都重新解析内容。
pub fn reindex_full(meta: &mut Meta, idx: &mut BigramIndex) -> Result<IndexStats> {
    // Phase 1: title index
    let mut stats = index_titles(meta, idx)?;

    // Phase 2: content index (inline, not background)
    let to_index: Vec<FileRec> =
        meta.files.iter().filter(|f| !f.content_indexed).cloned().collect();

    PROGRESS_PHASE.store(2, Ordering::Relaxed);
    PROGRESS_CONTENT_TOTAL.store(to_index.len() as u64, Ordering::Relaxed);
    PROGRESS_CONTENT_DONE.store(0, Ordering::Relaxed);

    let folder_paths: HashMap<i64, String> =
        meta.folders.iter().map(|f| (f.id, f.path.clone())).collect();

    if idx.file_chunks.is_empty() && !idx.chunks.is_empty() {
        idx.rebuild_file_chunks_map();
    }

    let mut done = 0u64;
    let mut failed = 0u64;
    let mut processed_ids: HashSet<i64> = HashSet::new();

    for file in &to_index {
        let Some(folder_path) = folder_paths.get(&file.folder_id) else {
            failed += 1;
            continue;
        };
        let full_path = format!("{}/{}", folder_path, file.rel_path);
        *PROGRESS_CURRENT.lock().unwrap() = Some(full_path.clone());

        let text = match parser::extract_text(Path::new(&full_path)) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("skip {full_path}: {e:#}");
                failed += 1;
                done += 1;
                processed_ids.insert(file.id);
                PROGRESS_CONTENT_DONE.store(done, Ordering::Relaxed);
                continue;
            }
        };

        let stem = Path::new(&full_path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        let full_text = format!("{}\n{}", stem, text);
        let chunks = chunk_text(&full_text);
        idx.replace_file(file.id, &chunks);

        done += 1;
        stats.chunks += chunks.len() as u64;
        processed_ids.insert(file.id);
        PROGRESS_CONTENT_DONE.store(done, Ordering::Relaxed);

        if (done as usize) % SAVE_BATCH == 0 {
            idx.save()?;
            for f in &mut meta.files {
                if processed_ids.contains(&f.id) {
                    f.content_indexed = true;
                }
            }
            let _ = meta.save();
        }
    }

    // Final save
    for f in &mut meta.files {
        if processed_ids.contains(&f.id) {
            f.content_indexed = true;
        }
    }
    idx.compact();
    idx.save_postings()?;
    meta.save()?;

    PROGRESS_PHASE.store(0, Ordering::Relaxed);
    *PROGRESS_CURRENT.lock().unwrap() = None;

    stats.failed += failed;
    stats.indexed += done;
    Ok(stats)
}

// ===================================================== Incremental re-index

/// 增量重建 — Phase 1 检测变更（秒级），返回统计。
/// Phase 2 由调用方（httpd）决定是否启动后台线程。
pub fn reindex_incremental(meta: &mut Meta, idx: &mut BigramIndex) -> Result<IndexStats> {
    index_titles(meta, idx)
}

// ===================================================== Legacy compat

/// 旧版全量索引（CLI `memex index` 向后兼容）。
pub fn index_all(meta: &mut Meta) -> Result<IndexStats> {
    let mut idx = BigramIndex::load()?;
    let stats = reindex_full(meta, &mut idx)?;
    Ok(stats)
}

// ===================================================== File watcher

/// 启动文件监听 — 自动增量更新索引（Q3）。
///
/// 监听所有已注册文件夹，文件变更时 debounce 2 秒后增量更新。
/// 返回 watcher（调用方需保持持有，否则停止监听）。
pub fn start_watcher(
    meta: Arc<Mutex<Meta>>,
    idx: Arc<RwLock<BigramIndex>>,
) -> Result<RecommendedWatcher> {
    let (tx, rx) = std::sync::mpsc::channel();

    let mut watcher: RecommendedWatcher = notify::recommended_watcher(tx)?;

    // Watch all registered folders
    let folders: Vec<String> = {
        let m = meta.lock().unwrap();
        m.folders.iter().map(|f| f.path.clone()).collect()
    };
    for path in &folders {
        let _ = watcher.watch(Path::new(path), RecursiveMode::Recursive);
    }

    // Spawn event handler thread with debounce
    let meta_clone = meta.clone();
    let idx_clone = idx.clone();
    std::thread::spawn(move || {
        let mut pending: HashMap<String, WatchAction> = HashMap::new();

        loop {
            match rx.recv_timeout(std::time::Duration::from_secs(2)) {
                Ok(event) => {
                    if let Ok(ev) = event {
                        if let Some(p) =
                            ev.paths.first().map(|p| p.to_string_lossy().replace('\\', "/"))
                        {
                            let action = match ev.kind {
                                notify::EventKind::Remove(_) => WatchAction::Delete,
                                _ => WatchAction::Update,
                            };
                            pending.insert(p, action);
                        }
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if pending.is_empty() {
                        continue;
                    }
                    let events: Vec<(String, WatchAction)> = pending.drain().collect();
                    eprintln!("watcher: processing {} changed files", events.len());
                    if let Err(e) = process_watcher_events(&meta_clone, &idx_clone, &events) {
                        eprintln!("watcher error: {e:#}");
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
    });

    Ok(watcher)
}

#[derive(Debug, Clone, Copy)]
enum WatchAction {
    Update,
    Delete,
}

fn process_watcher_events(
    meta: &Arc<Mutex<Meta>>,
    idx: &Arc<RwLock<BigramIndex>>,
    events: &[(String, WatchAction)],
) -> Result<()> {
    // Snapshot folder paths + filter rules (brief Meta lock)
    let (folder_paths, rules) = {
        let m = meta.lock().unwrap();
        let folders: Vec<(i64, String)> =
            m.folders.iter().map(|f| (f.id, f.path.clone())).collect();
        let rules = m.filter_rules.clone();
        (folders, rules)
    };
    let globset = rules.build_globset()?;

    // Ensure file_chunks map is populated (brief idx write lock)
    {
        let mut ix = idx.write().unwrap();
        if ix.file_chunks.is_empty() && !ix.chunks.is_empty() {
            ix.rebuild_file_chunks_map();
        }
    }

    let mut meta_changed = false;
    let mut need_embed = false;

    for (path_str, action) in events {
        let Some((folder_id, rel_path)) = find_folder(&folder_paths, path_str) else {
            continue;
        };

        let norm = path_str.replace('\\', "/");
        if globset.is_match(&norm) {
            continue;
        }

        match action {
            WatchAction::Delete => {
                // Step 1: find file_id (brief Meta lock)
                let file_id = {
                    let m = meta.lock().unwrap();
                    m.files.iter().find(|f| f.rel_path == rel_path).map(|f| f.id)
                };
                // Step 2: remove from index (brief idx write lock)
                if let Some(id) = file_id {
                    let mut ix = idx.write().unwrap();
                    ix.remove_file(id);
                }
                // Step 3: remove from Meta (brief Meta lock)
                if file_id.is_some() {
                    let mut m = meta.lock().unwrap();
                    m.files.retain(|ff| ff.id != file_id.unwrap());
                    meta_changed = true;
                }
            }
            WatchAction::Update => {
                let path = Path::new(path_str);
                if !path.exists() || !path.is_file() {
                    continue;
                }
                let meta_data = std::fs::metadata(path)?;
                let size = meta_data.len();
                if !rules.accepts(path, size) {
                    continue;
                }
                let ext = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                let mtime = meta_data
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);

                // Parse text (no locks held)
                let text = match parser::extract_text(path) {
                    Ok(t) => t,
                    Err(e) => {
                        eprintln!("watcher: skip {path_str}: {e:#}");
                        continue;
                    }
                };

                let stem = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                let full_text = format!("{}\n{}", stem, text);
                let chunks = chunk_text(&full_text);

                // Step 1: update Meta (brief Meta lock)
                let file_id = {
                    let mut m = meta.lock().unwrap();
                    let file_id = if let Some(f) = m.files.iter().find(|f| f.rel_path == rel_path) {
                        f.id
                    } else {
                        let id = m.next_file_id;
                        m.next_file_id += 1;
                        id
                    };
                    let exists = m.files.iter().any(|f| f.id == file_id);
                    if exists {
                        if let Some(f) = m.files.iter_mut().find(|f| f.id == file_id) {
                            f.size = size;
                            f.mtime = mtime;
                            f.ext = ext.clone();
                            f.content_indexed = true;
                        }
                    } else {
                        m.files.push(FileRec {
                            id: file_id,
                            folder_id,
                            rel_path: rel_path.clone(),
                            ext,
                            size,
                            mtime,
                            content_indexed: true,
                        });
                    }
                    file_id
                };

                // Step 2: replace in index (brief idx write lock)
                {
                    let mut ix = idx.write().unwrap();
                    ix.replace_file(file_id, &chunks);
                }
                meta_changed = true;
                need_embed = true;
            }
        }
    }

    if meta_changed {
        // Save index (index.bin + delta file) — stays in mmap mode, no memory spike
        {
            let mut ix = idx.write().unwrap();
            ix.save()?;
        }
        // Save Meta (brief Meta lock)
        {
            meta.lock().unwrap().save()?;
        }
        META_DIRTY.store(true, Ordering::Relaxed);
    }

    // Note: embed_all is disabled in watcher to avoid ~1.5GB memory spike
    // from loading ONNX model + processing all 698K chunks.
    // Embedding can be triggered manually via cmd_embed or full reindex.

    Ok(())
}

/// Find which registered folder a path belongs to → (folder_id, rel_path).
fn find_folder(folders: &[(i64, String)], path_str: &str) -> Option<(i64, String)> {
    let normalized = path_str.replace('\\', "/");
    for (id, folder_path) in folders {
        let fp = folder_path.replace('\\', "/");
        if normalized.starts_with(&fp) {
            let rel = normalized[fp.len()..].trim_start_matches('/').to_string();
            return Some((*id, rel));
        }
    }
    None
}