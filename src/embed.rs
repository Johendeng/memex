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

/// 对所有已索引 chunk 做嵌入，流式写入 vectors.bin。
/// 调用前提：index.bin 中已有 chunk（先跑 `memex index`）。
/// 使用传入的共享索引引用（避免重复加载 976MB index.bin）。
///
/// 优化要点（v1.2）：
/// - 流式读取 chunks.text（按 batch seek+read，不全量加载）
/// - 增量写入临时文件，最后 rename（内存峰值 ≈ 1 batch）
/// - ONNX 线程限制为 2，batch=16，每 batch sleep 10ms 让出 CPU
/// - 不在搜索栏暴露按钮，由 indexer 自动后台触发
pub fn embed_all_with_idx(idx: &crate::store::BigramIndex) -> Result<()> {
    use crate::vector;
    use std::io::Write;

    if idx.chunks.is_empty() {
        anyhow::bail!("no chunks in index — run `memex index` first");
    }

    let total = idx.chunks.len() as u64;
    EMBED_TOTAL.store(total, Ordering::Relaxed);
    EMBED_DONE.store(0, Ordering::Relaxed);
    EMBED_IN_PROGRESS.store(true, Ordering::Relaxed);

    // Ensure EMBED_IN_PROGRESS is reset even on error
    let _guard = EmbedGuard;

    // Limit ONNX Runtime threads to reduce CPU pressure
    // (must be set before model initialization)
    std::env::set_var("OMP_NUM_THREADS", "2");

    eprintln!("loading bge-small-zh-v1.5 model (from local models/ dir)…");
    let embedder = BgeEmbedder::new()?;
    eprintln!("model loaded. embedding {} chunks (streaming, batch=16)…", total);

    // Stream vectors to a temp file, then rename → vectors.bin
    let vec_path = vector::vector_path()?;
    let tmp_path = vec_path.with_extension("bin.tmp");
    let mut file = std::fs::File::create(&tmp_path)
        .with_context(|| format!("无法创建临时向量文件: {}", tmp_path.display()))?;

    // bincode format for Vec<Vec<f32>>: u64(outer_len) + for each: u64(inner_len) + data
    // Write outer length header
    file.write_all(&(idx.chunks.len() as u64).to_le_bytes())?;

    const BATCH: usize = 16;
    let mut done = 0usize;
    for start in (0..idx.chunks.len()).step_by(BATCH) {
        let end = std::cmp::min(start + BATCH, idx.chunks.len());

        // Stream-read only this batch's texts from chunks.text file
        let all_texts = idx.read_texts_batch(start, end);

        let batch: Vec<String> = (0..all_texts.len())
            .map(|i| {
                let text = &all_texts[i];
                if idx.chunks[start + i].file_id == 0 || text.trim().is_empty() {
                    String::from(" ")
                } else {
                    text.clone()
                }
            })
            .collect();

        let embeddings = embedder.embed_batch(&batch)?;

        for (i, emb) in embeddings.into_iter().enumerate() {
            let chunk = &idx.chunks[start + i];
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
  {done}/{} chunks", idx.chunks.len());

        // Yield CPU between batches to avoid hogging
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    eprintln!();

    file.flush()?;
    drop(file);

    // Atomic rename → vectors.bin
    std::fs::rename(&tmp_path, &vec_path)
        .with_context(|| format!("rename {:?} → {:?}", tmp_path, vec_path))?;

    eprintln!("embedded {} chunks → vectors.bin (streaming write)", done);
    Ok(())
}

/// CLI 兼容：从磁盘加载索引后调用 embed_all_with_idx。
pub fn embed_all() -> Result<()> {
    let idx = crate::store::BigramIndex::load()?;
    embed_all_with_idx(&idx)
}
