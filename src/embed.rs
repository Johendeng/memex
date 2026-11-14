//! bge-small-zh-v1.5 语义嵌入（fastembed → ort load-dynamic → ONNX Runtime）。
//!
//! 模型文件放在 `models/bge-small-zh-v1.5/` 目录下（手动下载，不走 hf-hub）：
//!   - onnx/model.onnx (~91MB)
//!   - tokenizer.json
//!   - config.json
//!   - special_tokens_map.json
//!   - tokenizer_config.json
//!
//! ort 使用 `load-dynamic` 特性，运行时从 exe 同目录加载 `onnxruntime.dll`。
//!
//! 下载脚本（hf-mirror）：
//! ```sh
//! mkdir -p models/bge-small-zh-v1.5/onnx
//! BASE=https://hf-mirror.com/Xenova/bge-small-zh-v1.5/resolve/main
//! curl -L -o models/bge-small-zh-v1.5/onnx/model.onnx $BASE/onnx/model.onnx
//! for f in tokenizer.json config.json special_tokens_map.json tokenizer_config.json; do
//!   curl -L -o models/bge-small-zh-v1.5/$f $BASE/$f
//! done
//! ```

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use anyhow::{Context, Result};
use fastembed::{
    InitOptionsUserDefined, Pooling, QuantizationMode, TextEmbedding, TokenizerFiles,
    UserDefinedEmbeddingModel,
};

/// 模型目录（相对于项目根 / exe CWD）。
// MODEL_DIR is now resolved via config::bge_model_dir() — kept for docs.

/// bge-small-zh-v1.5 输出维度。
pub const DIM: usize = 512;

// ---- embedding progress (polled by cmd_embed_progress) ----
pub static EMBED_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
pub static EMBED_TOTAL: AtomicU64 = AtomicU64::new(0);
pub static EMBED_DONE: AtomicU64 = AtomicU64::new(0);

pub struct BgeEmbedder {
    model: TextEmbedding,
}

fn read_bytes(path: &PathBuf, label: &str) -> Result<Vec<u8>> {
    std::fs::read(path).with_context(|| format!("读取 {label} 失败: {}", path.display()))
}

impl BgeEmbedder {
    /// 从本地文件初始化 bge-small-zh-v1.5 嵌入模型。
    pub fn new() -> Result<Self> {
        let base = crate::config::bge_model_dir();
        let onnx_path = base.join("onnx").join("model.onnx");
        let tok_path = base.join("tokenizer.json");
        let cfg_path = base.join("config.json");
        let stm_path = base.join("special_tokens_map.json");
        let tc_path = base.join("tokenizer_config.json");

        // Check all files exist
        for (p, label) in [
            (&onnx_path, "ONNX 模型"),
            (&tok_path, "tokenizer.json"),
            (&cfg_path, "config.json"),
            (&stm_path, "special_tokens_map.json"),
            (&tc_path, "tokenizer_config.json"),
        ] {
            if !p.exists() {
                anyhow::bail!(
                    "{label} 不存在: {}\n\
                     请运行下载脚本（见 embed.rs 文档注释）从 hf-mirror 下载模型文件",
                    p.display()
                );
            }
        }

        let onnx_file = read_bytes(&onnx_path, "ONNX 模型")?;
        let tokenizer_files = TokenizerFiles {
            tokenizer_file: read_bytes(&tok_path, "tokenizer.json")?,
            config_file: read_bytes(&cfg_path, "config.json")?,
            special_tokens_map_file: read_bytes(&stm_path, "special_tokens_map.json")?,
            tokenizer_config_file: read_bytes(&tc_path, "tokenizer_config.json")?,
        };

        let user_model = UserDefinedEmbeddingModel::new(onnx_file, tokenizer_files)
            .with_pooling(Pooling::Cls)
            .with_quantization(QuantizationMode::None);

        let opts = InitOptionsUserDefined::new();
        let model = TextEmbedding::try_new_from_user_defined(user_model, opts)
            .context("ONNX 模型初始化失败（确认 onnxruntime.dll 在 exe 同目录或 PATH 中）")?;

        Ok(Self { model })
    }

    /// 批量嵌入文本，返回 512 维向量列表。
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let batch: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        let embeddings = self
            .model
            .embed(batch, None)
            .context("bge 嵌入推理失败")?;
        Ok(embeddings)
    }

    /// 嵌入单条文本。
    pub fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        let result = self.embed_batch(&[text.to_string()])?;
        result
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("embedding returned empty"))
    }
}

/// RAII guard: resets EMBED_IN_PROGRESS when dropped (success or error).
struct EmbedGuard;
impl Drop for EmbedGuard {
    fn drop(&mut self) {
        EMBED_IN_PROGRESS.store(false, Ordering::Relaxed);
    }
}

// ---- incremental-embed sidecar fingerprint ----
// vectors.bin.fp 记录写入时的 chunk 总数 + 若干抽样位置的 (file_id, char_len)。
// 下次增量运行前抽查这些位置：全部吻合才允许按位置复用旧向量，
// 防止 index.bin 被删重建后位置整体偏移而长度检查恰好通过。
fn fingerprint_path(vec_path: &std::path::Path) -> std::path::PathBuf {
    vec_path.with_extension("bin.fp")
}

/// 抽样位置：头、尾、1/4、1/2、3/4 + 前几个（新增尾部最易偏移）。
fn fingerprint_spots(count: u64) -> Vec<usize> {
    let mut spots = vec![0usize, 1, 2];
    if count > 3 {
        spots.push((count / 4) as usize);
        spots.push((count / 2) as usize);
        spots.push((3 * count / 4) as usize);
        spots.push((count - 1) as usize);
    }
    spots.sort_unstable();
    spots.dedup();
    spots
}

/// 读取指纹：(写入时的 chunk 总数, 抽样 (pos, file_id, char_len) 列表)。
fn read_vector_fingerprint() -> Option<(u64, Vec<(usize, i64, u32)>)> {
    let vec_path = crate::vector::vector_path().ok()?;
    let bytes = std::fs::read(fingerprint_path(&vec_path)).ok()?;
    let mut cursor = 0usize;
    let mut rd_u64 = |buf: &[u8]| -> Option<u64> {
        if cursor + 8 > buf.len() {
            return None;
        }
        let v = u64::from_le_bytes(buf[cursor..cursor + 8].try_into().ok()?);
        cursor += 8;
        Some(v)
    };
    let count = rd_u64(&bytes)?;
    let n = rd_u64(&bytes)? as usize;
    let mut spots = Vec::with_capacity(n);
    for _ in 0..n {
        let pos = rd_u64(&bytes)? as usize;
        let file_id = rd_u64(&bytes)? as i64;
        let char_len = rd_u64(&bytes)? as u32;
        spots.push((pos, file_id, char_len));
    }
    if cursor != bytes.len() {
        return None; // trailing garbage — treat as invalid
    }
    Some((count, spots))
}

/// 写入指纹（embed 成功 rename 之后调用）。
fn write_vector_fingerprint(chunks_meta: &[crate::store::ChunkRec]) {
    let Ok(vec_path) = crate::vector::vector_path() else {
        return;
    };
    let count = chunks_meta.len() as u64;
    let mut buf = Vec::with_capacity(8 + fingerprint_spots(count).len() * 24 + 8);
    buf.extend_from_slice(&count.to_le_bytes());
    let spots = fingerprint_spots(count);
    buf.extend_from_slice(&(spots.len() as u64).to_le_bytes());
    for pos in spots {
        let c = &chunks_meta[pos];
        buf.extend_from_slice(&(pos as u64).to_le_bytes());
        buf.extend_from_slice(&(c.file_id as u64).to_le_bytes());
        buf.extend_from_slice(&(c.char_len as u64).to_le_bytes());
    }
    let _ = std::fs::write(fingerprint_path(&vec_path), buf);
}

/// 对所有已索引 chunk 做嵌入，流式写入 vectors.bin。
/// 调用前提：index.bin 中已有 chunk（先跑 `memex index`）。
/// 使用传入的共享索引引用（避免重复加载 976MB index.bin）。
///
/// 优化要点（v1.2）：
/// - 流式读取 chunks.text（按 batch seek+read，不全量加载）
/// - 增量写入临时文件，最后 rename（内存峰值 ≈ 1 batch）
/// - ONNX 线程限制为 2，batch=16，每 batch sleep 10ms 让出 CPU
/// - 不在搜索栏暴露按钮，由 indexer 自动后台触发
///
/// 增量构建（v1.2.1）：
/// - vectors.bin 按位置与 BigramIndex.chunks 对齐，且位置稳定
///   （add_file 只追加、remove_file 只标记死，compact 无调用方）
/// - 位置 < 旧长度的非空向量 = 同一文本的同一向量，流式拷贝复用，不跑 ONNX
/// - 只嵌入新增尾部（位置 ≥ 旧长度）；新变死的位置改写为空标记
/// - 关闭软件重开、无变化时秒级完成，不再从头重算
pub fn embed_all_with_idx(idx_arc: &std::sync::Arc<std::sync::RwLock<crate::store::BigramIndex>>) -> Result<()> {
    use crate::vector;
    use std::io::Write;

    // Atomically claim the "in progress" flag FIRST. Callers check the flag
    // before spawning this function, but their check-then-spawn is racy: the
    // file watcher can fire again (every ~2s) while a freshly spawned thread
    // is still cloning chunks_meta below — before the old code set the flag.
    // Two concurrent embeds both File::create the same vectors.bin.tmp and
    // truncate each other's output → corrupted vector file.
    if EMBED_IN_PROGRESS
        .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
        .is_err()
    {
        eprintln!("embedding already in progress — skipping duplicate trigger");
        return Ok(());
    }
    // RAII guard: resets the flag on any exit path (error or success).
    let _guard = EmbedGuard;

    // Clone chunk metadata ( lightweight: ~16 bytes per chunk) under a brief
    // read lock, then release.  This avoids holding the read lock for the
    // entire embedding duration, which would block the file watcher's write
    // lock and — because Windows SRW locks are fair — deadlock all searches.
    let (chunks_count, chunks_meta): (usize, Vec<crate::store::ChunkRec>) = {
        let ix = idx_arc.read().unwrap_or_else(|e| e.into_inner());
        (ix.chunks.len(), ix.chunks.clone())
    };

    if chunks_count == 0 {
        anyhow::bail!("no chunks in index — run `memex index` first");
    }

    let total = chunks_count as u64;
    EMBED_TOTAL.store(total, Ordering::Relaxed);
    EMBED_DONE.store(0, Ordering::Relaxed);

    // Limit ONNX Runtime threads to reduce CPU pressure
    // (must be set before model initialization)
    std::env::set_var("OMP_NUM_THREADS", "2");

    // Stream vectors to a temp file, then rename → vectors.bin
    let vec_path = vector::vector_path()?;
    let tmp_path = vec_path.with_extension("bin.tmp");
    let mut file = std::fs::File::create(&tmp_path)
        .with_context(|| format!("无法创建临时向量文件: {}", tmp_path.display()))?;

    // ---- 增量复用（v1.2.1）----
    // vectors.bin 按位置与 BigramIndex.chunks 对齐，且位置稳定：
    // add_file 只追加、remove_file 只原地标记死、compact 无调用方。
    // 因此位置 < old_count 的存活 chunk 的旧向量仍然有效 —— 流式拷贝
    // 复用，不跑 ONNX；只有新增尾部（位置 >= old_count）需要嵌入。
    // chunk 已变死的位置改写为空标记（0 长度）。
    let mut new_start: usize = 0;
    let mut reuse_ok = false;
    {
        use std::io::Read;
        if let Ok(mut old) = std::fs::File::open(&vec_path) {
            let mut hdr = [0u8; 8];
            let mut old_count: u64 = 0;
            let mut readable =
                old.read_exact(&mut hdr).is_ok() && {
                    old_count = u64::from_le_bytes(hdr);
                    // old file longer than current chunks = index was rebuilt
                    // from scratch — positional reuse is unsafe, full rebuild.
                    old_count <= chunks_count as u64
                };
            if readable && old_count == 0 {
                readable = false;
            }
            // Sidecar fingerprint: guards against an index rebuilt from
            // scratch (deleted index.bin) where positions shifted but the
            // length check above coincidentally passes. Spot-check a few
            // positions' (file_id, char_len) against the current chunks_meta.
            let spots_ok = || -> bool {
                if old_count == 0 {
                    return false;
                }
                let Some(spots) = read_vector_fingerprint() else {
                    return false;
                };
                if spots.0 != old_count {
                    return false;
                }
                spots.1.iter().all(|&(pos, file_id, char_len)| {
                    pos < old_count as usize
                        && (file_id == 0 || {
                            let cur = &chunks_meta[pos];
                            // cur dead (file removed since) is fine — the
                            // copy loop drops that vector anyway. A different
                            // live file_id at the same position means the
                            // index was rebuilt — positions shifted.
                            cur.file_id == 0
                                || (cur.file_id == file_id && cur.char_len == char_len)
                        })
                })
            };
            if readable && !spots_ok() {
                eprintln!("vectors fingerprint mismatch — full rebuild");
                readable = false;
            }
            if readable {
                let old_count = old_count as usize;
                // bincode format: u64(outer_len) + per vec: u64(inner_len) + data
                file.write_all(&(chunks_count as u64).to_le_bytes())?;
                let mut pos = 0usize;
                let mut reused = 0usize;
                let mut read_err = false;
                let mut buf = vec![0u8; 64 * 1024];
                'copy: while pos < old_count {
                    let mut len_hdr = [0u8; 8];
                    if old.read_exact(&mut len_hdr).is_err() {
                        read_err = true;
                        break 'copy;
                    }
                    let vlen = u64::from_le_bytes(len_hdr) as usize;
                    let dead_now = chunks_meta[pos].file_id == 0;
                    if vlen == 0 {
                        // was empty (dead) — stays an empty marker
                        file.write_all(&0u64.to_le_bytes())?;
                    } else if dead_now {
                        // live → dead: drop the old vector, skip its data
                        let mut skip = vlen * 4;
                        while skip > 0 {
                            let n = std::cmp::min(skip, buf.len());
                            if old.read_exact(&mut buf[..n]).is_err() {
                                read_err = true;
                                break 'copy;
                            }
                            skip -= n;
                        }
                        file.write_all(&0u64.to_le_bytes())?;
                    } else {
                        // still live with a valid old vector — copy through
                        file.write_all(&len_hdr)?;
                        let mut remaining = vlen * 4;
                        while remaining > 0 {
                            let n = std::cmp::min(remaining, buf.len());
                            if old.read_exact(&mut buf[..n]).is_err() {
                                read_err = true;
                                break 'copy;
                            }
                            file.write_all(&buf[..n])?;
                            remaining -= n;
                        }
                        reused += 1;
                    }
                    pos += 1;
                    if pos % 65536 == 0 {
                        EMBED_DONE.store(pos as u64, Ordering::Relaxed);
                    }
                }
                if read_err {
                    // Old file truncated/corrupt mid-way — full rebuild.
                    eprintln!("old vectors.bin unreadable mid-way — full rebuild");
                    file.set_len(0)?;
                    use std::io::Seek;
                    file.seek(std::io::SeekFrom::Start(0))?;
                } else {
                    new_start = old_count;
                    reuse_ok = true;
                    EMBED_DONE.store(new_start as u64, Ordering::Relaxed);
                    eprintln!(
                        "incremental: reused {reused} existing vectors, embedding {} new chunks",
                        chunks_count - new_start
                    );
                }
            }
        }
    }
    if !reuse_ok {
        // First run (or old file unusable): plain full-embed header.
        file.write_all(&(chunks_count as u64).to_le_bytes())?;
    }

    if new_start >= chunks_count {
        // Nothing new to embed — flush and rename. Instant on unchanged index.
        file.flush()?;
        drop(file);
        std::fs::rename(&tmp_path, &vec_path)
            .with_context(|| format!("rename {:?} → {:?}", tmp_path, vec_path))?;
        write_vector_fingerprint(&chunks_meta);
        eprintln!("incremental: all {chunks_count} chunk vectors up to date");
        return Ok(());
    }

    eprintln!("loading bge-small-zh-v1.5 model (from local models/ dir)…");
    let embedder = BgeEmbedder::new()?;
    eprintln!(
        "model loaded. embedding {} new chunks (positions {new_start}..{chunks_count}, batch=16)…",
        chunks_count - new_start
    );

    const BATCH: usize = 16;
    let mut done = 0usize;
    for start in (new_start..chunks_count).step_by(BATCH) {
        let end = std::cmp::min(start + BATCH, chunks_count);

        // Acquire read lock ONLY for reading texts, then release immediately.
        // The lock is held for microseconds (file seek + read), not for the
        // expensive ONNX inference that follows.
        let all_texts = {
            let ix = idx_arc.read().unwrap_or_else(|e| e.into_inner());
            ix.read_texts_batch(start, end)
        };

        let batch: Vec<String> = (0..all_texts.len())
            .map(|i| {
                let text = &all_texts[i];
                if chunks_meta[start + i].file_id == 0 || text.trim().is_empty() {
                    String::from(" ")
                } else {
                    text.clone()
                }
            })
            .collect();

        let embeddings = embedder.embed_batch(&batch)?;

        for (i, emb) in embeddings.into_iter().enumerate() {
            let chunk = &chunks_meta[start + i];
            if chunk.file_id == 0 {
                // Dead chunk: write empty vector (bincode: u64(0))
                file.write_all(&0u64.to_le_bytes())?;
            } else {
                // Write inner length + f32 data (bincode-compatible)
                file.write_all(&(emb.len() as u64).to_le_bytes())?;
                let bytes: &[u8] = unsafe {
                    std::slice::from_raw_parts(emb.as_ptr() as *const u8, emb.len() * 4)
                };
                file.write_all(bytes)?;
            }
        }

        done = end;
        EMBED_DONE.store(done as u64, Ordering::Relaxed);
        eprint!("
  {done}/{} chunks", chunks_count);

        // Yield CPU between batches to avoid hogging
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    eprintln!();

    file.flush()?;
    drop(file);

    // Atomic rename → vectors.bin
    std::fs::rename(&tmp_path, &vec_path)
        .with_context(|| format!("rename {:?} → {:?}", tmp_path, vec_path))?;

    write_vector_fingerprint(&chunks_meta);

    eprintln!("embedded {} chunks → vectors.bin (streaming write)", done);
    Ok(())
}

/// CLI 兼容：从磁盘加载索引后调用 embed_all_with_idx。
pub fn embed_all() -> Result<()> {
    use std::sync::{Arc, RwLock};
    let idx = Arc::new(RwLock::new(crate::store::BigramIndex::load()?));
    embed_all_with_idx(&idx)
}
