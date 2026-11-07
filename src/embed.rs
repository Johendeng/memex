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

/// 对所有已索引 chunk 做嵌入，保存到 vectors.bin。
/// 调用前提：index.bin 中已有 chunk（先跑 `memex index`）。
/// 使用传入的共享索引引用（避免重复加载 976MB index.bin）。
pub fn embed_all_with_idx(idx: &crate::store::BigramIndex) -> Result<()> {
    use crate::vector;

    if idx.chunks.is_empty() {
        anyhow::bail!("no chunks in index — run `memex index` first");
    }

    let total = idx.chunks.len() as u64;
    EMBED_TOTAL.store(total, Ordering::Relaxed);
    EMBED_DONE.store(0, Ordering::Relaxed);

    eprintln!("loading bge-small-zh-v1.5 model (from local models/ dir)…");
    let embedder = BgeEmbedder::new()?;
    eprintln!("model loaded. embedding {} chunks…", idx.chunks.len());

    let mut vec_idx = vector::VectorIndex::default();
    vec_idx.vectors = vec![Vec::new(); idx.chunks.len()];

    // Skip dead chunks (file_id == 0 = removed but position retained for
    // chunk_id stability). They get an empty vector; never retrieved because
    // their postings have been pruned from the bigram index.
    // Plan B: read all chunk texts from chunks.text file (sequential read)
    let all_texts = idx.read_all_texts();
    const BATCH: usize = 32;
    let mut done = 0usize;
    for start in (0..idx.chunks.len()).step_by(BATCH) {
        let end = std::cmp::min(start + BATCH, idx.chunks.len());
        let batch: Vec<String> = (start..end)
            .map(|i| {
                let text = &all_texts[i];
                if idx.chunks[i].file_id == 0 || text.trim().is_empty() {
                    String::from(" ")
                } else {
                    text.clone()
                }
            })
            .collect();
        let embeddings = embedder.embed_batch(&batch)?;
        for (i, emb) in embeddings.into_iter().enumerate() {
            // Dead chunks: store empty vector to save memory
            if idx.chunks[start + i].file_id == 0 {
                vec_idx.vectors[start + i] = Vec::new();
            } else {
                vec_idx.vectors[start + i] = emb;
            }
        }
        done = end;
        EMBED_DONE.store(done as u64, Ordering::Relaxed);
        eprint!("\r  {done}/{} chunks", idx.chunks.len());
    }
    eprintln!();
    vec_idx.save()?;
    let dim = vec_idx.vectors.iter().find(|v| !v.is_empty()).map(|v| v.len()).unwrap_or(0);
    eprintln!("embedded {} chunks ({}-dim) → vectors.bin", done, dim);
    Ok(())
}

/// CLI 兼容：从磁盘加载索引后调用 embed_all_with_idx。
pub fn embed_all() -> Result<()> {
    let idx = crate::store::BigramIndex::load()?;
    embed_all_with_idx(&idx)
}
