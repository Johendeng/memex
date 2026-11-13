//! HTTP/JSON bridge + embedded static file server.
//!
//! - `POST /api` — JSON command dispatch (frontend ↔ core).
//! - `GET /` + `GET /assets/*` — serves the embedded Vue build from
//!   `frontend/dist/` (via rust-embed, zero disk dependency in release).
//! - CORS open for localhost dev. No extra crates beyond rust-embed.

use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use anyhow::Result;

use crate::filter;
use crate::indexer;
use crate::embed;
use crate::mcp;
use crate::search;
use crate::vector;
use crate::store::{self, BigramIndex, Meta};

use rust_embed::RustEmbed;

/// Embeds `frontend/dist/` at compile time.
/// `debug-embed` feature reads from disk in dev builds (no rebuild per change).
#[derive(RustEmbed)]
#[folder = "frontend/dist/"]
struct Assets;

/// Port the HTTP server is currently bound to (set in `serve`, read by MCP config prompt).
static SERVE_PORT: AtomicU16 = AtomicU16::new(18321);

// ---------------------------------------------------------------- serve

pub fn serve(port: u16, open: bool) -> Result<()> {
    // Lower process priority so background indexing/embedding/OCR don't
    // compete with the user's foreground work. ONNX Runtime's thread pool
    // inherits this — when the user is active, the OS scheduler gives CPU
    // to foreground apps; when idle, Memex uses spare cycles at full speed.
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn SetPriorityClass(h: isize, c: u32) -> i32;
            fn GetCurrentProcess() -> isize;
        }
        const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x00004000;
        unsafe {
            let _ = SetPriorityClass(GetCurrentProcess(), BELOW_NORMAL_PRIORITY_CLASS);
        }
    }

    let meta = Arc::new(Mutex::new(Meta::load()?));
    let idx = Arc::new(RwLock::new(BigramIndex::load()?));

    // Start file watcher for auto-incremental indexing (Q3)
    let _watcher = match indexer::start_watcher(meta.clone(), idx.clone()) {
        Ok(w) => {
            eprintln!("file watcher started (auto-incremental indexing)");
            Some(w)
        }
        Err(e) => {
            eprintln!("file watcher failed to start (non-fatal): {e:#}");
            None
        }
    };

    let listener = TcpListener::bind(("127.0.0.1", port))?;
    SERVE_PORT.store(port, Ordering::Relaxed);
    let url = format!("http://127.0.0.1:{port}");
    eprintln!("Memex serving on {url}  (Ctrl+C to stop)");
    if open {
        #[cfg(windows)]
        std::process::Command::new("cmd").args(["/C", "start", "", &url]).spawn()?;
        #[cfg(target_os = "macos")]
        std::process::Command::new("open").arg(&url).spawn()?;
        #[cfg(all(not(windows), not(target_os = "macos")))]
        {
            let _ = open; // suppress unused warning on non-Windows
            std::process::Command::new("xdg-open").arg(&url).spawn()?;
        }
    }
    for stream in listener.incoming() {
        let stream = stream?;
        let meta = meta.clone();
        let idx = idx.clone();
        std::thread::spawn(move || {
            // Restore normal priority for HTTP handler threads so
            // search/UI requests stay responsive even when background
            // embedding is saturating CPU at BELOW_NORMAL priority.
            #[cfg(windows)]
            {
                #[link(name = "kernel32")]
                extern "system" {
                    fn SetThreadPriority(h: isize, p: i32) -> i32;
                    fn GetCurrentThread() -> isize;
                }
                const THREAD_PRIORITY_NORMAL: i32 = 0;
                unsafe {
                    let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_NORMAL);
                }
            }
            if let Err(e) = handle(stream, meta, idx) {
                eprintln!("http: {e:#}");
            }
        });
    }
    Ok(())
}

// ---------------------------------------------------------------- handle

fn handle(
    mut stream: std::net::TcpStream,
    meta: Arc<Mutex<Meta>>,
    idx: Arc<RwLock<BigramIndex>>,
) -> Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);

    let mut req_line = String::new();
    reader.read_line(&mut req_line)?;
    let parts: Vec<&str> = req_line.split_whitespace().collect();
    let method = parts.first().copied().unwrap_or("");
    let path = parts.get(1).copied().unwrap_or("/");

    let mut content_len = 0usize;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line)?;
        if n == 0 || line.trim().is_empty() {
            break;
        }
        if let Some(v) = line
            .to_ascii_lowercase()
            .strip_prefix("content-length:")
        {
            content_len = v.trim().parse().unwrap_or(0);
        }
    }

    let mut body = vec![0u8; content_len];
    if content_len > 0 {
        reader.read_exact(&mut body)?;
    }

    // CORS preflight
    if method == "OPTIONS" {
        stream.write_all(
            b"HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\n\
             Access-Control-Allow-Methods: POST, GET, OPTIONS\r\n\
             Access-Control-Allow-Headers: Content-Type\r\nContent-Length: 0\r\n\r\n",
        )?;
        return Ok(());
    }

    if method == "POST" && (path == "/api" || path == "/api/") {
        match serde_json::from_slice::<serde_json::Value>(&body) {
            Ok(req) => {
                let cmd = req.get("cmd").and_then(|v| v.as_str()).unwrap_or("");
                let args = req.get("args").cloned().unwrap_or(serde_json::Value::Null);
                match dispatch(meta.clone(), idx.clone(), cmd, args) {
                    Ok(val) => write_json(&mut stream, 200, &serde_json::to_vec(&val)?)?,
                    Err(e) => write_json(&mut stream, 500, &err_body(&format!("{e:#}")))?,
                }
            }
            Err(e) => write_json(&mut stream, 400, &err_body(&format!("bad json: {e}")))?,
        }
        return Ok(());
    }

    // MCP JSON-RPC 2.0 over HTTP (streamable-HTTP transport, POST-only).
    // Hermes Agent / any HTTP MCP client connects to http://127.0.0.1:<port>/mcp
    if method == "POST" && (path == "/mcp" || path == "/mcp/") {
        let ix = idx.read().unwrap();
        match serde_json::from_slice::<serde_json::Value>(&body) {
            Ok(req) => match mcp::handle_request(&req, Some(&ix)) {
                Some(resp) => write_json(&mut stream, 200, &serde_json::to_vec(&resp)?)?,
                None => {
                    // notification (no reply expected) — 202 No Content
                    stream.write_all(
                        b"HTTP/1.1 202 Accepted\r\nAccess-Control-Allow-Origin: *\r\n\
                         Content-Length: 0\r\n\r\n",
                    )?;
                }
            },
            Err(e) => write_json(&mut stream, 400, &err_body(&format!("bad json: {e}")))?,
        }
        return Ok(());
    }

    // Static file serving (GET /, /assets/*, etc.)
    let norm = path
        .trim_start_matches('/')
        .to_string();
    let lookup = if norm.is_empty() { "index.html" } else { norm.as_str() };
    match Assets::get(lookup) {
        Some(data) => {
            let ct = content_type(lookup);
            let head = format!(
                "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\n\
                 Content-Type: {ct}\r\nConnection: close\r\n\
                 Content-Length: {}\r\n\r\n",
                data.data.len()
            );
            stream.write_all(head.as_bytes())?;
            stream.write_all(&data.data)?;
        }
        None => {
            // SPA fallback: serve index.html for any non-asset route
            if let Some(data) = Assets::get("index.html") {
                let head = format!(
                    "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\n\
                     Content-Type: text/html; charset=utf-8\r\nConnection: close\r\n\
                     Content-Length: {}\r\n\r\n",
                    data.data.len()
                );
                stream.write_all(head.as_bytes())?;
                stream.write_all(&data.data)?;
            } else {
                let body = err_body("404");
                write_json(&mut stream, 404, &body)?;
            }
        }
    }
    Ok(())
}

fn content_type(name: &str) -> &'static str {
    if name.ends_with(".html") { "text/html; charset=utf-8" }
    else if name.ends_with(".js") { "application/javascript; charset=utf-8" }
    else if name.ends_with(".css") { "text/css; charset=utf-8" }
    else if name.ends_with(".json") { "application/json; charset=utf-8" }
    else if name.ends_with(".png") { "image/png" }
    else if name.ends_with(".svg") { "image/svg+xml" }
    else if name.ends_with(".woff2") { "font/woff2" }
    else { "application/octet-stream" }
}

fn write_json(stream: &mut std::net::TcpStream, status: u16, body: &[u8]) -> Result<()> {
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nAccess-Control-Allow-Origin: *\r\n\
         Content-Type: application/json; charset=utf-8\r\n\
         Connection: close\r\nContent-Length: {}\r\n\r\n",
        body.len(),
        reason = reason(status)
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    Ok(())
}

/// Acquire a read lock on the BigramIndex, recovering from poisoned locks
/// (caused by a panic in a writer thread) and retrying briefly if a write
/// lock is currently held (e.g. during batch indexing).
fn recover_read(idx: &std::sync::RwLock<crate::store::BigramIndex>) -> std::sync::RwLockReadGuard<'_, crate::store::BigramIndex> {
    // First attempt: try non-blocking
    for _ in 0..60 {
        match idx.try_read() {
            Ok(g) => return g,
            Err(std::sync::TryLockError::Poisoned(e)) => return e.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
    // Last resort: blocking read (will block until writer releases)
    idx.read().unwrap_or_else(|e| e.into_inner())
}

fn reason(c: u16) -> &'static str {
    match c {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "Error",
    }
}

fn err_body(msg: &str) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({ "error": msg })).unwrap_or_default()
}

// ---------------------------------------------------------------- dispatch

fn dispatch(
    meta: Arc<Mutex<Meta>>,
    idx: Arc<RwLock<BigramIndex>>,
    cmd: &str,
    args: serde_json::Value,
) -> Result<serde_json::Value> {
    // Reload Meta if background thread flagged it dirty — but ONLY when
    // indexing is NOT running. During indexing, the in-memory Meta is more
    // up-to-date than the on-disk copy (which is saved every ~5000 files).
    // Reloading during indexing would LOSE the in-memory additions.
    use std::sync::atomic::Ordering;
    if indexer::META_DIRTY.load(Ordering::Relaxed)
        && indexer::PROGRESS_PHASE.load(Ordering::Relaxed) == 0
    {
        if let Ok(fresh) = Meta::load() {
            *meta.lock().unwrap() = fresh;
            indexer::META_DIRTY.store(false, Ordering::Relaxed);
        }
    }
    Ok(match cmd {
        "cmd_info" => {
            // Acquire idx first (consistent lock ordering: idx → meta)
            let ix = recover_read(&idx);
            let m = meta.lock().unwrap();
            // file type distribution
            let mut ext_map: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
            let mut total_bytes: u64 = 0;
            for f in &m.files {
                let ext = if f.ext.is_empty() { "(无扩展名)".to_string() } else { f.ext.clone() };
                *ext_map.entry(ext).or_default() += 1;
                total_bytes += f.size;
            }
            // vector status
            let (has_vec, vec_count, vec_dim) = match vector::VectorIndex::load() {
                Ok(vi) => (true, vi.vectors.len(), embed::DIM),
                _ => (false, 0, embed::DIM),
            };
            serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "folders": m.folders.len(),
                "files": m.files.len(),
                "chunks": ix.chunks.len(),
                "bigrams": ix.bigram_count(),
                "total_bytes": total_bytes,
                "ext_dist": ext_map,
                "has_vectors": has_vec,
                "vector_count": vec_count,
                "vector_dim": vec_dim,
            })
        }
        "cmd_folders_list" => serde_json::to_value(&meta.lock().unwrap().folders)?,
        "cmd_folders_add" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let mut m = meta.lock().unwrap();
            let id = store::add_folder(&mut m, path)?;
            m.save()?;
            serde_json::json!({ "id": id })
        }
        "cmd_folders_remove" => {
            let id = args.get("id").and_then(|v| v.as_i64()).unwrap_or(0);

            // 1. Remove from meta + reset content_indexed on all remaining files
            //    (index files are deleted, so they need to be rebuilt from scratch)
            let mut m = meta.lock().unwrap();
            store::remove_folder(&mut m, id);
            for f in m.files.iter_mut() {
                f.content_indexed = false;
            }
            m.save()?;
            drop(m);

            // 2. Delete all index files (will be rebuilt from remaining folders)
            if let Ok(dir) = store::index_dir() {
                for name in &[
                    "index.bin",
                    "chunks.text",
                    "postings.bin",
                    "postings.idx",
                    "postings.delta",
                    "vectors.bin",
                ] {
                    let p = dir.join(name);
                    if p.exists() {
                        let _ = std::fs::remove_file(&p);
                    }
                }
            }

            // 3. Reset in-memory BigramIndex to empty
            {
                let mut ix = idx.write().unwrap();
                *ix = BigramIndex::default();
            }

            // 4. Trigger full reindex in background (scans remaining folders)
            let meta_clone = meta.clone();
            let idx_clone = idx.clone();
            std::thread::spawn(move || {
                indexer::reindex_full_background(meta_clone, idx_clone);
            });

            serde_json::json!({ "ok": true, "reindex": true })
        }
        "cmd_index_run" => {
            // Check if indexing is already running
            let phase = indexer::PROGRESS_PHASE.load(Ordering::Relaxed);
            if phase != 0 {
                serde_json::json!({
                    "error": "indexing already in progress",
                    "phase": phase
                })
            } else {
                // Reload Meta if background thread flagged it dirty
                if indexer::META_DIRTY.load(Ordering::Relaxed) {
                    if let Ok(fresh) = Meta::load() {
                        *meta.lock().unwrap() = fresh;
                        indexer::META_DIRTY.store(false, Ordering::Relaxed);
                    }
                }
                let mode = args.get("mode").and_then(|v| v.as_str()).unwrap_or("full");
                let meta_clone = meta.clone();
                let idx_clone = idx.clone();
                match mode {
                    "incremental" => {
                        std::thread::spawn(move || {
                            indexer::reindex_incremental_background(meta_clone, idx_clone);
                        });
                        serde_json::json!({"status": "started", "mode": "incremental"})
                    }
                    _ => {
                        std::thread::spawn(move || {
                            indexer::reindex_full_background(meta_clone, idx_clone);
                        });
                        serde_json::json!({"status": "started", "mode": "full"})
                    }
                }
            }
        }
        "cmd_search" => {
            // Acquire idx first (consistent lock ordering: idx → meta)
            let ix = recover_read(&idx);
            let m = meta.lock().unwrap();
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;
            let hybrid = args.get("hybrid").and_then(|v| v.as_bool()).unwrap_or(false);

            // Tag pre-filter: when dim+tag are specified, build a file_id set
            // and over-fetch to compensate for filtered-out results.
            let filter_dim = args.get("dim").and_then(|v| v.as_str());
            let filter_tag = args.get("tag").and_then(|v| v.as_str());
            let allowed: Option<HashSet<i64>> = match (filter_dim, filter_tag) {
                (Some(d), Some(t)) => Some(
                    m.tags
                        .iter()
                        .filter(|tag| tag.dim == d && tag.value == t)
                        .map(|tag| tag.file_id)
                        .collect(),
                ),
                _ => None,
            };
            // Ext / folder post-filters (also need over-fetch).
            let ext_filter = args.get("ext_filter").and_then(|v| v.as_str()).unwrap_or("");
            let folder_filter = args.get("folder_filter").and_then(|v| v.as_str()).unwrap_or("");
            let has_post_filter = allowed.is_some() || !ext_filter.is_empty() || !folder_filter.is_empty();
            let search_limit = if has_post_filter {
                (limit * 10).max(200)
            } else {
                limit
            };

            let hits = if hybrid {
                let vec_idx = vector::VectorIndex::load()?;
                if vec_idx.vectors.is_empty() {
                    search::search(&m, &ix, query, search_limit)?
                } else {
                    vector::hybrid_search(&m, &ix, &vec_idx, query, search_limit)?
                }
            } else {
                search::search(&m, &ix, query, search_limit)?
            };

            // Apply tag / ext / folder filters and trim to original limit
            let hits = if has_post_filter {
                hits.into_iter()
                    .filter(|h| {
                        if let Some(ref allowed_set) = allowed {
                            if !allowed_set.contains(&h.file_id) { return false; }
                        }
                        if !ext_filter.is_empty() {
                            let exts: Vec<&str> = ext_filter.split(',').collect();
                            let h_ext = h.ext.strip_prefix('.').unwrap_or(&h.ext).to_lowercase();
                            if !exts.iter().any(|e| {
                                let e = e.strip_prefix('.').unwrap_or(e).to_lowercase();
                                h_ext == e
                            }) { return false; }
                        }
                        if !folder_filter.is_empty() {
                            if !h.folder_path.to_lowercase().contains(&folder_filter.to_lowercase()) {
                                return false;
                            }
                        }
                        true
                    })
                    .take(limit)
                    .collect::<Vec<_>>()
            } else {
                hits
            };
            serde_json::to_value(&hits)?
        }
        "cmd_filter_get" => serde_json::to_value(&meta.lock().unwrap().filter_rules)?,
        "cmd_filter_save" => {
            let rules: filter::FilterRules = serde_json::from_value(
                args.get("rules").cloned().unwrap_or(serde_json::Value::Null),
            )?;
            let mut m = meta.lock().unwrap();
            m.filter_rules = rules;
            m.save()?;
            serde_json::json!({ "ok": true })
        }
        "cmd_open_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                return Err(anyhow::anyhow!("empty path"));
            }
            #[cfg(windows)]
            std::process::Command::new("cmd")
                .args(["/C", "start", "", path])
                .spawn()?;
            #[cfg(target_os = "macos")]
            std::process::Command::new("open").arg(path).spawn()?;
            #[cfg(all(not(windows), not(target_os = "macos")))]
            std::process::Command::new("xdg-open").arg(path).spawn()?;
            serde_json::json!({ "ok": true })
        }
        "cmd_open_folder" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                return Err(anyhow::anyhow!("empty path"));
            }
            #[cfg(windows)]
            std::process::Command::new("explorer")
                .args([format!("/select,{}", path)])
                .spawn()?;
            #[cfg(target_os = "macos")]
            std::process::Command::new("open")
                .args(["-R", path])
                .spawn()?;
            #[cfg(all(not(windows), not(target_os = "macos")))]
            {
                let parent = std::path::Path::new(path)
                    .parent()
                    .unwrap_or(std::path::Path::new("."));
                std::process::Command::new("xdg-open").arg(parent).spawn()?;
            }
            serde_json::json!({ "ok": true })
        }
        "cmd_pick_folder" => {
            let folder = rfd::FileDialog::new().pick_folder();
            match folder {
                Some(p) => serde_json::json!({ "path": p.display().to_string() }),
                None => serde_json::json!({ "path": "" }),
            }
        }
        // ---- M3: embedding (hybrid search support) ----
        "cmd_vector_status" => {
            let has = std::path::Path::new(&vector::vector_path()?).exists();
            let n = if has {
                vector::VectorIndex::load()?.vectors.len()
            } else {
                0
            };
            serde_json::json!({ "has_vectors": has, "count": n, "dim": embed::DIM })
        }
        "cmd_embed" => {
            if embed::EMBED_IN_PROGRESS.load(Ordering::Relaxed) {
                serde_json::json!({ "ok": false, "error": "embedding already in progress" })
            } else {
                embed::EMBED_IN_PROGRESS.store(true, Ordering::Relaxed);
                let idx_arc = idx.clone();
                std::thread::spawn(move || {
                    if let Err(e) = embed::embed_all_with_idx(&idx_arc) {
                        eprintln!("embedding failed: {e}");
                    }
                    embed::EMBED_IN_PROGRESS.store(false, Ordering::Relaxed);
                });
                serde_json::json!({ "ok": true, "started": true })
            }
        }
        "cmd_embed_progress" => {
            let in_progress = embed::EMBED_IN_PROGRESS.load(Ordering::Relaxed);
            let total = embed::EMBED_TOTAL.load(Ordering::Relaxed);
            let done = embed::EMBED_DONE.load(Ordering::Relaxed);
            let percent = if total > 0 { (done * 100 / total) as u8 } else { 0 };
            serde_json::json!({
                "in_progress": in_progress,
                "total": total,
                "done": done,
                "percent": percent,
            })
        }
        // ---- tags / dims / confidence (M4 → M5) ----
        "cmd_tags" => {
            let m = meta.lock().unwrap();
            let tags: Vec<serde_json::Value> = m.tags.iter().map(|t| {
                let file = m.files.iter().find(|f| f.id == t.file_id);
                let folder = file.and_then(|f| m.folders.iter().find(|fo| fo.id == f.folder_id));
                let path = file.map(|f| format!("{}/{}", folder.map(|fo| fo.path.as_str()).unwrap_or(""), f.rel_path)).unwrap_or_default();
                let filename = file.map(|f| f.rel_path.split('/').next_back().unwrap_or(&f.rel_path).to_string()).unwrap_or_default();
                serde_json::json!({
                    "file_id": t.file_id, "dim": t.dim, "value": t.value,
                    "source": t.source, "note": t.note, "at": t.at,
                    "path": path, "filename": filename,
                })
            }).collect();
            serde_json::json!({ "tags": tags, "count": m.tags.len() })
        }
        "cmd_dims" => {
            let m = meta.lock().unwrap();
            let mut dims: Vec<String> = m
                .tags
                .iter()
                .map(|t| t.dim.clone())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();
            dims.sort();
            serde_json::json!({ "dims": dims })
        }
        // ---- tag summary: dim → [{value, count}] ----
        "cmd_tag_summary" => {
            let m = meta.lock().unwrap();
            let dim = args.get("dim").and_then(|v| v.as_str()).unwrap_or("");
            let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
            for t in &m.tags {
                if t.dim == dim {
                    *counts.entry(t.value.clone()).or_insert(0) += 1;
                }
            }
            let mut summary: Vec<(String, usize)> = counts.into_iter().collect();
            summary.sort_by(|a, b| b.1.cmp(&a.1));
            let tags: Vec<serde_json::Value> = summary
                .into_iter()
                .map(|(v, c)| serde_json::json!({ "value": v, "count": c }))
                .collect();
            serde_json::json!({ "dim": dim, "tags": tags })
        }
        // ---- files by tag: dim+tag → paginated file list ----
        "cmd_files_by_tag" => {
            let m = meta.lock().unwrap();
            let dim = args.get("dim").and_then(|v| v.as_str()).unwrap_or("");
            let tag = args.get("tag").and_then(|v| v.as_str()).unwrap_or("");
            let page = args.get("page").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
            let page_size = args.get("page_size").and_then(|v| v.as_u64()).unwrap_or(50) as usize;

            // Collect file_ids for this dim+tag
            let file_ids: Vec<i64> = m
                .tags
                .iter()
                .filter(|t| t.dim == dim && t.value == tag)
                .map(|t| t.file_id)
                .collect();

            let total = file_ids.len();
            let start = ((page - 1) * page_size).min(total);
            let end = (start + page_size).min(total);

            let files: Vec<serde_json::Value> = file_ids[start..end]
                .iter()
                .filter_map(|&fid| {
                    let file = m.files.iter().find(|f| f.id == fid)?;
                    let folder = m.folders.iter().find(|fo| fo.id == file.folder_id)?;
                    let path = format!("{}/{}", folder.path, file.rel_path);
                    let filename = file.rel_path.split('/').next_back().unwrap_or(&file.rel_path).to_string();
                    Some(serde_json::json!({
                        "file_id": fid,
                        "path": path,
                        "filename": filename,
                        "ext": file.ext,
                        "rel_path": file.rel_path,
                    }))
                })
                .collect();

            let pages = if page_size > 0 { (total + page_size - 1) / page_size } else { 1 };
            serde_json::json!({
                "files": files,
                "total": total,
                "page": page,
                "page_size": page_size,
                "pages": pages,
            })
        }
        "cmd_confidence" => {
            let m = meta.lock().unwrap();
            let records: Vec<serde_json::Value> = m.confidence.iter().map(|c| {
                let file = m.files.iter().find(|f| f.id == c.file_id);
                let folder = file.and_then(|f| m.folders.iter().find(|fo| fo.id == f.folder_id));
                let path = file.map(|f| format!("{}/{}", folder.map(|fo| fo.path.as_str()).unwrap_or(""), f.rel_path)).unwrap_or_default();
                let filename = file.map(|f| f.rel_path.split('/').next_back().unwrap_or(&f.rel_path).to_string()).unwrap_or_default();
                let ext = file.map(|f| f.ext.clone()).unwrap_or_default();
                serde_json::json!({
                    "file_id": c.file_id, "score": c.score, "status": c.status,
                    "at": c.at, "note": c.note,
                    "path": path, "filename": filename, "ext": ext,
                })
            }).collect();
            let verified = m.confidence.iter().filter(|c| c.status == "verified").count();
            let below = m.confidence.iter().filter(|c| c.status == "below").count();
            let reverify = m.confidence.iter().filter(|c| c.status == "reverify").count();
            let unverified = m.files.len().saturating_sub(m.confidence.len());
            serde_json::json!({
                "records": records,
                "stats": {
                    "verified": verified,
                    "below": below,
                    "reverify": reverify,
                    "unverified": unverified,
                    "total_files": m.files.len(),
                }
            })
        }
        // ---- duplicate file scan + cleanup ----
        "cmd_dedup_scan" => {
            // Snapshot under lock, hash off-lock — avoids blocking other API calls
            // during the ~60s SHA-256 computation phase.
            let snap = {
                let m = meta.lock().unwrap();
                crate::dedup::snapshot(&m)
            };
            let result = crate::dedup::scan_files(&snap)?;
            serde_json::to_value(&result)?
        }
        "cmd_dedup_clean" => {
            // Args: { hashes: [hashString, ...] }
            let hashes: Vec<String> = args
                .get("hashes")
                .and_then(|v| v.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|x| x.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            if hashes.is_empty() {
                serde_json::json!({ "error": "no hashes provided", "deleted_files": 0, "freed_bytes": 0, "tagged_files": 0, "errors": [] })
            } else {
                // Phase 1: snapshot under lock (instant), then drop lock
                let snap = {
                    let m = meta.lock().unwrap();
                    crate::dedup::snapshot(&m)
                };
                // Phase 2: scan off-lock (SHA-256 hashing, ~3s, no blocking)
                let scan_result = crate::dedup::scan_files(&snap)?;
                // Phase 3: acquire locks only for deletion (fast: batch retain + chunk mark)
                let mut m = meta.lock().unwrap();
                let mut ix = idx.write().unwrap();
                let result = crate::dedup::clean_from_scan(&mut m, &mut ix, &scan_result, &hashes)?;
                serde_json::to_value(&result)?
            }
        }
        "cmd_mcp_prompt" => {
            let exe = std::env::current_exe()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| "memex.exe".to_string());
            let prompt = format!(
                r#"请帮我注册一个 MCP server，配置如下：

name: memex
command: {exe}
args: ["mcp", "serve"]
description: Memex 本地知识库与 AI 记忆体 — 全文检索、记忆存储、笔记管理、文件打标。13 个工具：memex_search / memex_read / memex_tag / memex_report_confidence / memex_find_similar / memex_remember / memex_recall / memex_forget / memex_write_note / memex_list_notes / memex_read_note / memex_delete_note / memex_stats。
"#
            );
            serde_json::json!({ "prompt": prompt })
        }
        "cmd_mcp_http_prompt" => {
            let port = SERVE_PORT.load(Ordering::Relaxed);
            let url = format!("http://127.0.0.1:{port}/mcp");
            let prompt = format!(
                r#"请帮我注册一个 MCP server（HTTP 远程模式），配置如下：

name: memex-http
url: {url}
transport: http
description: Memex 本地知识库（HTTP 模式）— 全文检索、记忆存储、笔记管理。需要 Memex serve 正在运行。
"#
            );
            serde_json::json!({ "prompt": prompt })
        }
        "cmd_gen_agent_prompt" => {
            let exe = std::env::current_exe()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| "memex.exe".to_string());
            let port = SERVE_PORT.load(Ordering::Relaxed);
            let http_url = format!("http://127.0.0.1:{port}/mcp");
            let exe_fwd = exe.replace('\\', "/");
            let prompt = format!(
r#"# Memex — 本地知识库与 AI 记忆体

## 简介

Memex 是一个本地运行的知识库系统，为 AI Agent 提供持久化记忆和文档检索能力。它自动索引你电脑上的文档（PDF/Word/PPT/Markdown/图片 OCR 等），提供 BM25 + 语义向量混合检索，并通过 MCP 协议向 Agent 暴露 13 个工具，实现记忆存储、笔记管理、文档检索等能力。

Memex 的设计理念：Agent 是用户的"记忆扩展器"——主动判断什么值得记住，什么时候该回忆，用户零操作。

## 核心能力

1. **文档索引与检索**：自动解析 PDF/DOCX/PPTX/XLSX/MD/TXT/图片（OCR），bigram 关键词 + BGE 语义向量 + RRF 融合
2. **重复文件检测**：SHA-256 全文去重，路径最浅保留
3. **多维分类归档**：维度+值标签体系，Agent 和用户均可打标
4. **KV 记忆存储**：轻量级键值记忆，Agent 的"长期记忆"
5. **Markdown 笔记**：结构化笔记，可搜索可读取
6. **OCR 识别**：PaddleOCR PP-OCRv6 模型，图片文字提取
7. **PDF 渲染**：pdfium 引擎，复杂 PDF 文本提取
8. **数据导出**：JSON/CSV 导出，便于迁移

## MCP 注册

### 方式一：stdio（推荐，所有 MCP client 通用）

Agent 的 MCP 配置中添加：

```json
{{
  "mcpServers": {{
    "memex": {{
      "command": "{exe}",
      "args": ["mcp", "serve"]
    }}
  }}
}}
```

#### Claude Desktop 配置示例

编辑 `~/AppData/Roaming/Claude/claude_desktop_config.json`：

```json
{{
  "mcpServers": {{
    "memex": {{
      "command": "{exe}",
      "args": ["mcp", "serve"]
    }}
  }}
}}
```

#### Cursor 配置示例

Settings → MCP → Add New MCP Server：
- Name: `memex`
- Type: `stdio`
- Command: `{exe_fwd} mcp serve`

#### Hermes Agent 配置示例

在 MCP servers 配置中添加：
```yaml
- name: memex
  command: {exe}
  args: ["mcp", "serve"]
```

### 方式二：HTTP（适用于支持 HTTP transport 的 agent）

MCP endpoint: `POST {http_url}`
协议：JSON-RPC 2.0

需要 Memex 以 serve 模式运行（`memex serve` 或桌面应用已启动）。

### 选择建议

- 单一 Agent、本地使用 → **方式一（stdio）**
- 多 Agent 共享、Memex 已常驻 → **方式二（HTTP）**
- 拿不准 → 方式一

## 工具列表（13 个）

### 检索类

| 工具 | 说明 |
|------|------|
| `memex_search` | 在已索引文件中检索（BM25 + 语义向量 + RRF 融合）。hybrid=true 启用语义检索 |
| `memex_find_similar` | 查找与指定文件语义相似的文档（向量余弦相似度） |
| `memex_read` | 读取已索引文件的解析文本。verified 文件直接返回缓存 |

### 记忆类

| 工具 | 说明 |
|------|------|
| `memex_remember` | 存储键值记忆（Agent 长期记忆）。同一 key+namespace 更新而非重复 |
| `memex_recall` | 检索记忆。按 key 精确或关键词模糊搜索 |
| `memex_forget` | 删除一条记忆 |

### 笔记类

| 工具 | 说明 |
|------|------|
| `memex_write_note` | 写入 Markdown 笔记到 `data/notes/<title>.md` |
| `memex_list_notes` | 列出所有笔记（标题、大小、修改时间、预览） |
| `memex_read_note` | 读取笔记完整内容 |
| `memex_delete_note` | 删除一条笔记 |

### 管理类

| 工具 | 说明 |
|------|------|
| `memex_tag` | 为文件打标签（维度+值），用于多维分类 |
| `memex_report_confidence` | 报告文件解析置信度（0-1），≥0.85 标记 verified |
| `memex_stats` | 知识库统计概览（文件数、chunk 数、标签分布等） |

## 调用时机说明

### 何时主动检索（无需用户指示）

- **任务开始时**：`memex_recall` 查相关上下文（用户偏好、项目背景、历史决策）
- **用户问的问题可能已有记录时**：`memex_search` 搜本地知识库
- **做决策前**：`memex_recall` 查有没有相关约束或先前的决策
- **用户提到项目/人名/技术名词时**：`memex_search` 看知识库有没有相关文档

### 何时不要检索

- 翻译、格式化、算术等通用任务
- 常识性问题
- 与用户领域完全无关的任务

### 何时主动保存（无需用户指示）

- **用户表达了偏好、约束或工作习惯** → `memex_remember`
- **做了决策及理由**（选 A 不选 B，为什么）→ `memex_remember`
- **解决了非平凡问题**（记录方案结论，不是排查过程）→ `memex_remember` 或 `memex_write_note`
- **用户透露了环境、项目、团队的关键事实** → `memex_remember`
- **下次对话需要知道才能有效的上下文** → `memex_remember`

### 何时不要保存

- 闲聊、临时调试信息、一次性操作
- 知识库中已有的内容
- 用户能轻易重新推导的信息

### 两层存储分工

| 存储 | 工具 | 粒度 | 触发 |
|------|------|------|------|
| KV 记忆 | `memex_remember` | 一句话事实/偏好/决策 | Agent 自主，高频 |
| Markdown 笔记 | `memex_write_note` | 详细内容/方案/纪要 | 用户显式为主，低频 |

### 显式指令

用户说"记住"/"保存这个"时，Agent 无条件执行 `memex_remember`。
用户说"写笔记"/"记录方案"时，Agent 使用 `memex_write_note`。

### 会话末回顾

对话结束前，Agent 简要列出本轮保存的记忆和笔记，供用户清理。
事后清理比事前确认体验更好——不打断当前流程。
"#,
                exe = exe,
                exe_fwd = exe_fwd,
                http_url = http_url,
            );
            serde_json::json!({ "prompt": prompt })
        }
        "cmd_index_progress" => {
            use std::sync::atomic::Ordering;
            // Reload Meta if background thread flagged it dirty — but ONLY
            // when indexing is NOT running (same reason as dispatch()).
            if indexer::META_DIRTY.load(Ordering::Relaxed)
                && indexer::PROGRESS_PHASE.load(Ordering::Relaxed) == 0
            {
                if let Ok(fresh) = Meta::load() {
                    *meta.lock().unwrap() = fresh;
                    indexer::META_DIRTY.store(false, Ordering::Relaxed);
                }
            }
            let total = indexer::PROGRESS_TOTAL.load(Ordering::Relaxed);
            let done = indexer::PROGRESS_DONE.load(Ordering::Relaxed);
            let scanned = indexer::PROGRESS_SCANNED.load(Ordering::Relaxed);
            let current = indexer::PROGRESS_CURRENT.lock().unwrap().clone();
            let percent = if total > 0 { done * 100 / total } else { 0 };
            let phase = indexer::PROGRESS_PHASE.load(Ordering::Relaxed);
            let content_total = indexer::PROGRESS_CONTENT_TOTAL.load(Ordering::Relaxed);
            let content_done = indexer::PROGRESS_CONTENT_DONE.load(Ordering::Relaxed);
            let content_percent = if content_total > 0 {
                content_done * 100 / content_total
            } else {
                0
            };
            let ocr_total = indexer::PROGRESS_OCR_TOTAL.load(Ordering::Relaxed);
            let ocr_done = indexer::PROGRESS_OCR_DONE.load(Ordering::Relaxed);
            let ocr_percent = if ocr_total > 0 {
                ocr_done * 100 / ocr_total
            } else {
                0
            };
            // Embedding progress (phase 3)
            let embed_in_progress = embed::EMBED_IN_PROGRESS.load(Ordering::Relaxed);
            let embed_total = embed::EMBED_TOTAL.load(Ordering::Relaxed);
            let embed_done = embed::EMBED_DONE.load(Ordering::Relaxed);
            let embed_percent = if embed_total > 0 {
                embed_done * 100 / embed_total
            } else {
                0
            };
            serde_json::json!({
                "total": total,
                "done": done,
                "scanned": scanned,
                "current": current,
                "percent": percent,
                "phase": phase,
                "content_total": content_total,
                "content_done": content_done,
                "content_percent": content_percent,
                "ocr_total": ocr_total,
                "ocr_done": ocr_done,
                "ocr_percent": ocr_percent,
                "embed_in_progress": embed_in_progress,
                "embed_total": embed_total,
                "embed_done": embed_done,
                "embed_percent": embed_percent,
            })
        }
        // ---- update check / download / install ----
        "cmd_check_update" => {
            match crate::update::check_update() {
                Ok(info) => serde_json::json!({
                    "ok": true,
                    "current": info.current,
                    "latest": info.latest,
                    "has_update": info.has_update,
                    "notes": info.notes,
                    "download_url": info.download_url,
                    "installer_filename": info.installer_filename,
                    "source": info.source,
                }),
                Err(e) => serde_json::json!({ "ok": false, "error": format!("{e}") }),
            }
        }
        "cmd_download_update" => {
            let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let filename = args.get("filename").and_then(|v| v.as_str()).unwrap_or("Memex-Setup.exe").to_string();
            if url.is_empty() {
                serde_json::json!({ "ok": false, "error": "missing url" })
            } else if crate::update::DL_IN_PROGRESS.load(Ordering::Relaxed) {
                serde_json::json!({ "ok": false, "error": "download already in progress" })
            } else {
                std::thread::spawn(move || {
                    crate::update::DL_IN_PROGRESS.store(true, std::sync::atomic::Ordering::Relaxed);
                    match crate::update::download_update(&url, &filename) {
                        Ok(path) => {
                            *crate::update::DOWNLOADED_PATH.lock().unwrap() = Some(path);
                        }
                        Err(e) => {
                            eprintln!("[update] download failed: {e}");
                        }
                    }
                    crate::update::DL_IN_PROGRESS.store(false, std::sync::atomic::Ordering::Relaxed);
                });
                serde_json::json!({ "ok": true, "started": true })
            }
        }
        "cmd_update_progress" => {
            crate::update::download_progress()
        }
        "cmd_auto_check_result" => {
            let guard = crate::update::AUTO_CHECK_RESULT.lock().unwrap();
            match &*guard {
                None => serde_json::json!({ "checked": false }),
                Some(None) => serde_json::json!({ "checked": true, "has_update": false }),
                Some(Some(info)) => serde_json::json!({
                    "checked": true,
                    "has_update": true,
                    "current": info.current,
                    "latest": info.latest,
                    "notes": info.notes,
                    "download_url": info.download_url,
                    "installer_filename": info.installer_filename,
                    "source": info.source,
                }),
            }
        }
        "cmd_install_update" => {
            let path = crate::update::DOWNLOADED_PATH.lock().unwrap().clone();
            match path {
                Some(p) if p.exists() => {
                    let p_clone = p.clone();
                    // Launch installer in a delayed thread so we can return the HTTP response first.
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(500));
                        let _ = crate::update::install_update(&p_clone);
                    });
                    serde_json::json!({ "ok": true, "path": p.display().to_string() })
                }
                Some(_) => {
                    serde_json::json!({ "ok": false, "error": "installer file not found" })
                }
                None => {
                    serde_json::json!({ "ok": false, "error": "no update downloaded" })
                }
            }
        }
        // ---- MD preview / edit: read & save text files ----
        "cmd_read_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                return Err(anyhow::anyhow!("empty path"));
            }
            let ext = std::path::Path::new(path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            const TEXT_EXTS: &[&str] = &[
                "md", "markdown", "txt", "csv", "json", "html", "htm",
                "xml", "log", "yml", "yaml", "toml", "ini", "conf",
                "rs", "py", "js", "ts", "go", "java", "c", "cpp", "h",
                "sh", "bat", "ps1",
            ];
            if !TEXT_EXTS.contains(&ext.as_str()) {
                serde_json::json!({
                    "ok": false,
                    "error": format!("不支持预览 .{} 文件，仅支持文本文件", ext),
                    "content": "", "ext": ext, "size": 0,
                })
            } else {
                match std::fs::read(path) {
                    Ok(bytes) => {
                        let size = bytes.len();
                        if size > 2_097_152 {
                            serde_json::json!({
                                "ok": false, "error": "文件过大（超过 2MB）",
                                "content": "", "ext": ext, "size": size,
                            })
                        } else {
                            let content = String::from_utf8_lossy(&bytes).to_string();
                            serde_json::json!({ "ok": true, "content": content, "ext": ext, "size": size })
                        }
                    }
                    Err(e) => serde_json::json!({
                        "ok": false, "error": format!("读取失败: {e}"),
                        "content": "", "ext": ext, "size": 0,
                    }),
                }
            }
        }
        "cmd_save_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
            if path.is_empty() {
                return Err(anyhow::anyhow!("empty path"));
            }
            std::fs::write(path, content)?;
            serde_json::json!({ "ok": true })
        }
        // ---- Find Similar: vector similarity search by file_id ----
        "cmd_find_similar" => {
            let file_id = args.get("file_id").and_then(|v| v.as_i64()).unwrap_or(0);
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
            if file_id == 0 {
                return Err(anyhow::anyhow!("missing file_id"));
            }
            let vec_idx = match vector::VectorIndex::load() {
                Ok(vi) => vi,
                Err(_) => {
                    return Ok(serde_json::json!({ "hits": [], "error": "向量索引不可用" }));
                }
            };
            if vec_idx.vectors.is_empty() {
                serde_json::json!({ "hits": [], "error": "向量索引为空，请先在搜索页生成向量索引" })
            } else {
                // Phase 1: briefly lock idx to collect this file's chunk IDs
                let file_chunks: Vec<u32> = {
                    let ix = recover_read(&idx);
                    ix.chunks.iter()
                        .enumerate()
                        .filter(|(_, c)| c.file_id == file_id)
                        .map(|(i, _)| i as u32)
                        .collect()
                };
                // release idx lock here — heavy computation runs lock-free

                if file_chunks.is_empty() {
                    serde_json::json!({ "hits": [], "error": "文件无索引数据" })
                } else {
                    // Phase 2: compute centroid (no locks needed)
                    // Use first non-empty vector for dim (vectors[0] may be empty)
                    let dim = vec_idx.vectors.iter()
                        .find(|v| !v.is_empty())
                        .map(|v| v.len())
                        .unwrap_or(0);
                    if dim == 0 {
                        serde_json::json!({ "hits": [], "error": "向量维度异常" })
                    } else {
                        let mut centroid = vec![0.0f32; dim];
                        let mut count = 0u32;
                        for cid in &file_chunks {
                            if let Some(v) = vec_idx.vectors.get(*cid as usize) {
                                if !v.is_empty() && v.len() == dim {
                                    for (i, &x) in v.iter().enumerate() { centroid[i] += x; }
                                    count += 1;
                                }
                            }
                        }
                        if count == 0 {
                            serde_json::json!({ "hits": [], "error": "文件无向量数据" })
                        } else {
                            for x in &mut centroid { *x /= count as f32; }
                            let file_chunk_set: HashSet<u32> = file_chunks.into_iter().collect();

                            // Phase 3: brute-force cosine similarity (no locks)
                            let mut scored: Vec<(u32, f32)> = vec_idx.vectors.iter()
                                .enumerate()
                                .filter_map(|(i, v)| {
                                    if v.is_empty() || v.len() != dim || file_chunk_set.contains(&(i as u32)) {
                                        None
                                    } else {
                                        Some((i as u32, vector::cosine(&centroid, v)))
                                    }
                                })
                                .collect();
                            scored.sort_by(|a, b| b.1.total_cmp(&a.1));
                            scored.truncate(limit * 5);

                            // Phase 4: briefly lock idx + meta to resolve top-K metadata
                            let ix = recover_read(&idx);
                            let m = meta.lock().unwrap();
                            let mut seen_files: HashSet<i64> = HashSet::new();
                            let hits: Vec<serde_json::Value> = scored.iter()
                                .filter_map(|(cid, score)| {
                                    let chunk = ix.chunks.get(*cid as usize)?;
                                    if seen_files.contains(&chunk.file_id) { return None; }
                                    seen_files.insert(chunk.file_id);
                                    let file = m.files.iter().find(|f| f.id == chunk.file_id)?;
                                    let folder = m.folders.iter().find(|fo| fo.id == file.folder_id)?;
                                    let snippet: String = ix.get_chunk_text(*cid).chars().take(200).collect();
                                    Some(serde_json::json!({
                                        "file_id": chunk.file_id,
                                        "folder_path": folder.path,
                                        "rel_path": file.rel_path,
                                        "ext": file.ext,
                                        "chunk_seq": chunk.seq,
                                        "snippet": snippet,
                                        "rank": *score as f64,
                                    }))
                                })
                                .take(limit)
                                .collect();
                            serde_json::json!({ "hits": hits, "count": hits.len() })
                        }
                    }
                }
            }
        }
        // ---- Data export: metadata as JSON ----
        "cmd_export" => {
            let m = meta.lock().unwrap();
            let export = serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "exported_at": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs()).unwrap_or(0),
                "folders": m.folders,
                "files": m.files.iter().map(|f| {
                    let folder = m.folders.iter().find(|fo| fo.id == f.folder_id);
                    serde_json::json!({
                        "id": f.id,
                        "folder": folder.map(|fo| fo.path.as_str()).unwrap_or(""),
                        "rel_path": f.rel_path,
                        "ext": f.ext,
                        "size": f.size,
                        "mtime": f.mtime,
                    })
                }).collect::<Vec<_>>(),
                "tags": m.tags,
                "confidence": m.confidence,
            });
            serde_json::json!({ "ok": true, "data": export })
        }
        // ---- Data export: file list as CSV ----
        "cmd_export_csv" => {
            let m = meta.lock().unwrap();
            let mut csv = String::from("id,folder,rel_path,ext,size,mtime,tags,confidence\n");
            for f in &m.files {
                let folder = m.folders.iter().find(|fo| fo.id == f.folder_id);
                let folder_path = folder.map(|fo| fo.path.as_str()).unwrap_or("");
                let tags: Vec<String> = m.tags.iter()
                    .filter(|t| t.file_id == f.id)
                    .map(|t| format!("{}={}", t.dim, t.value))
                    .collect();
                let conf = m.confidence.iter()
                    .find(|c| c.file_id == f.id)
                    .map(|c| format!("{}({})", c.status, c.score))
                    .unwrap_or_default();
                // Escape commas in fields
                let esc_folder = folder_path.replace(',', ";");
                let esc_path = f.rel_path.replace(',', ";");
                let esc_tags = tags.join("|").replace(',', ";");
                csv.push_str(&format!("{},{},{},{},{},{},{},{}\n",
                    f.id, esc_folder, esc_path, f.ext, f.size, f.mtime, esc_tags, conf));
            }
            serde_json::json!({ "ok": true, "csv": csv, "rows": m.files.len() })
        }
        // ---- Import tags from JSON ----
        "cmd_import_tags" => {
            let tags_json = args.get("tags").cloned().unwrap_or(serde_json::json!([]));
            let tags: Vec<store::TagRec> = serde_json::from_value(tags_json)
                .map_err(|e| anyhow::anyhow!("invalid tags JSON: {e}"))?;
            let mut m = meta.lock().unwrap();
            let before = m.tags.len();
            for t in tags {
                // Only add if file_id exists
                if m.files.iter().any(|f| f.id == t.file_id) {
                    m.tags.push(t);
                }
            }
            let added = m.tags.len() - before;
            m.save()?;
            serde_json::json!({ "ok": true, "added": added, "total_tags": m.tags.len() })
        }
        // ---- Export memories (KV store) ----
        "cmd_export_memories" => {
            let store = crate::memories::MemoryStore::load()
                .map_err(|e| anyhow::anyhow!("load memories: {e}"))?;
            serde_json::json!({
                "ok": true,
                "memories": store.memories,
                "count": store.memories.len(),
            })
        }
        // ---- List notes ----
        "cmd_list_notes" => {
            let notes = crate::memories::list_notes()
                .map_err(|e| anyhow::anyhow!("list notes: {e}"))?;
            serde_json::json!({ "ok": true, "notes": notes })
        }
        // ---- Read a single note ----
        "cmd_read_note" => {
            let title = args.get("title").and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing title"))?;
            let content = crate::memories::read_note(title)
                .map_err(|e| anyhow::anyhow!("read note: {e}"))?;
            serde_json::json!({ "ok": true, "content": content })
        }
        // ---- Delete a note ----
        "cmd_delete_note" => {
            let title = args.get("title").and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing title"))?;
            let deleted = crate::memories::delete_note(title)
                .map_err(|e| anyhow::anyhow!("delete note: {e}"))?;
            serde_json::json!({ "ok": true, "deleted": deleted })
        }
        // ---- Delete a KV memory ----
        "cmd_delete_memory" => {
            let key = args.get("key").and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing key"))?;
            let namespace = args.get("namespace").and_then(|v| v.as_str()).unwrap_or("");
            let mut store = crate::memories::MemoryStore::load()
                .map_err(|e| anyhow::anyhow!("load memories: {e}"))?;
            let deleted = store.forget(key, namespace);
            if deleted {
                store.save().map_err(|e| anyhow::anyhow!("save memories: {e}"))?;
            }
            serde_json::json!({ "ok": true, "deleted": deleted, "count": store.memories.len() })
        }
        // ---- Upsert a KV memory (manual add/edit) ----
        "cmd_save_memory" => {
            let key = args.get("key").and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing key"))?;
            let value = args.get("value").and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing value"))?;
            let namespace = args.get("namespace").and_then(|v| v.as_str()).unwrap_or("default");
            let note = args.get("note").and_then(|v| v.as_str());
            let mut store = crate::memories::MemoryStore::load()
                .map_err(|e| anyhow::anyhow!("load memories: {e}"))?;
            let updated = store.upsert(key, value, namespace, note);
            store.save().map_err(|e| anyhow::anyhow!("save memories: {e}"))?;
            serde_json::json!({ "ok": true, "updated": updated, "count": store.memories.len() })
        }
        other => serde_json::json!({ "error": format!("unknown cmd: {other}") }),
    })
}
