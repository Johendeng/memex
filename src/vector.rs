//! 向量存储 + 余弦相似度暴力检索 + BM25 + RRF 融合。
//!
//! 向量存储为独立 `vectors.bin`（bincode 序列化），与 `index.bin` 分开
//! 以保持 M1/M2 的索引格式不变。
//! 向量按 chunk_id 索引（0-based，与 BigramIndex.chunks 对齐）。

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::store::{BigramIndex, Meta};

// ----------------------------------------------------------- vector storage

pub fn vector_path() -> Result<PathBuf> {
    Ok(crate::store::index_dir()?.join("vectors.bin"))
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct VectorIndex {
    /// chunk_id -> embedding (bge-small-zh = 512-dim)
    /// Index is chunk_id; length must match BigramIndex.chunks.len().
    pub vectors: Vec<Vec<f32>>,
}

impl VectorIndex {
    pub fn load() -> Result<Self> {
        let p = vector_path()?;
        if p.exists() {
            let bytes = std::fs::read(&p)?;
            Ok(bincode::deserialize(&bytes)
                .context("vectors.bin 反序列化失败（版本不兼容？删除文件后重新 embed）")?)
        } else {
            Ok(VectorIndex::default())
        }
    }

    pub fn save(&self) -> Result<()> {
        let p = vector_path()?;
        let bytes = bincode::serialize(self)?;
        std::fs::write(&p, bytes)?;
        Ok(())
    }

    /// 暴力余弦相似度 top-K。
    pub fn top_k(&self, query: &[f32], k: usize) -> Vec<(u32, f32)> {
        let mut results: Vec<(u32, f32)> = self
            .vectors
            .iter()
            .enumerate()
            .filter_map(|(i, v)| {
                if v.is_empty() {
                    return None;
                }
                Some((i as u32, cosine(query, v)))
            })
            .collect();
        results.sort_by(|a, b| b.1.total_cmp(&a.1));
        results.truncate(k);
        results
    }
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }
    dot / (mag_a * mag_b)
}

// ----------------------------------------------------------- BM25 scoring

/// k1 = 1.2, b = 0.75（tech-design §BM25 默认参数）
const K1: f64 = 1.2;
const B: f64 = 0.75;

/// 对查询大词项（bigram）列表做 BM25 打分，结果写入 `scores`（chunk_id → 累计分）。
pub fn bm25_score(
    scores: &mut std::collections::HashMap<u32, f64>,
    idx: &BigramIndex,
    query_bigrams: &[String],
) {
    let n = idx.chunks.len() as f64;
    if n == 0.0 || query_bigrams.is_empty() {
        return;
    }

    // Precompute average chunk character length.
    let total_chars: f64 = idx.chunks.iter().map(|c| c.char_len as f64).sum();
    let avg_len = total_chars / n;

    for bg in query_bigrams {
        let plist = idx.get_postings(bg);
        let df = plist.len() as f64;
        if df == 0.0 {
            continue;
        }
        // BM25 IDF (clamped to >= 0.0): log((N - df + 0.5) / (df + 0.5) + 1)
        let idf = ((n - df + 0.5) / (df + 0.5)).ln() + 1.0;
        if idf <= 0.0 {
            continue;
        }

        for p in &plist {
            let tf = p.count as f64;
            let chunk_len = idx.chunks[p.chunk_id as usize].char_len as f64;
            let norm = 1.0 - B + B * chunk_len / avg_len;
            let tf_norm = tf / (tf + K1 * norm);
            let s = tf_norm * idf;
            *scores.entry(p.chunk_id).or_insert(0.0) += s;
        }
    }
}

// ----------------------------------------------------------- RRF fusion

/// Reciprocal Rank Fusion (k = 60).
///
/// `keyword_ranks` — bigram/BM25 top-K results, each entry: (chunk_id, bm25_score).
/// `vector_ranks` — cosine top-K results, each entry: (chunk_id, cosine_sim).
///
/// RRF is rank-based (score-agnostic): the i-th result in each list gets weight
/// `1/(60 + i)`, summed across lists. Ties are broken by BM25 score descending.
pub fn rrf_fusion(
    keyword_ranks: &[(u32, f64)],
    vector_ranks: &[(u32, f32)],
    top_k: usize,
) -> Vec<(u32, f64)> {
    const K: f64 = 60.0;
    let mut fused: std::collections::HashMap<u32, f64> = std::collections::HashMap::new();

    for (rank, (cid, _)) in keyword_ranks.iter().enumerate() {
        *fused.entry(*cid).or_insert(0.0) += 1.0 / (K + (rank + 1) as f64);
    }
    for (rank, (cid, _)) in vector_ranks.iter().enumerate() {
        *fused.entry(*cid).or_insert(0.0) += 1.0 / (K + (rank + 1) as f64);
    }

    let mut results: Vec<(u32, f64)> = fused.into_iter().collect();
    results.sort_by(|a, b| {
        let ord = b.1.total_cmp(&a.1);
        // Tiebreak: prefer keyword-ranked items
        match ord {
            std::cmp::Ordering::Equal => {
                let a_kw = keyword_ranks.iter().position(|(c, _)| c == &a.0);
                let b_kw = keyword_ranks.iter().position(|(c, _)| c == &b.0);
                b_kw.cmp(&a_kw) // earlier rank = better
            }
            o => o,
        }
    });
    results.truncate(top_k);
    results
}

// ----------------------------------------------------------- hybrid search (public)

use crate::search::{SearchHit, highlight_snippet};

/// 混合检索：BM25 关键词 + 向量语义 + RRF 融合。
///
/// 若 `vec_idx` 为空（未 embed），退化为纯 BM25。
pub fn hybrid_search(
    meta: &Meta,
    idx: &BigramIndex,
    vec_idx: &VectorIndex,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchHit>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }

    // 1. BM25 keyword path
    let q_bigrams = crate::search::query_bigrams(q);
    let mut bm25_scores: std::collections::HashMap<u32, f64> =
        std::collections::HashMap::new();
    bm25_score(&mut bm25_scores, idx, &q_bigrams);
    // Title boost: files whose stem matches query terms get a strong
    // bonus on chunk#0, ensuring title matches rank above body-only hits.
    crate::search::apply_title_boost(&mut bm25_scores, meta, idx, &q_bigrams, q);
    // CJK queries skip the exact-substring filter (same as keyword search):
    // "流量统计" should match a title like "流量分析" via bigram overlap.
    let has_cjk = q.chars().any(|c| !c.is_ascii());
    let ql_lower = q.to_lowercase();
    let mut bm25_ranks: Vec<(u32, f64)> = bm25_scores
        .into_iter()
        .filter(|(cid, _)| {
            if has_cjk {
                true
            } else {
                idx.get_chunk_text(*cid)
                    .to_lowercase()
                    .contains(&ql_lower)
            }
        })
        .collect();
    sort_desc(&mut bm25_ranks);

    // 2. Vector semantic path (skip if no vectors)
    let vector_ranks: Vec<(u32, f32)> = if vec_idx.vectors.is_empty() {
        Vec::new()
    } else {
        // Lazy-load embedder to get query vector (model cached after first run)
        let query_vec = crate::embed::BgeEmbedder::new()
            .and_then(|e| e.embed_single(q));
        match query_vec {
            Ok(qv) => vec_idx.top_k(&qv, limit * 3),
            Err(_) => Vec::new(),
        }
    };

    // 3. RRF fusion (generous top_k — we truncate after title boost)
    let mut fused = rrf_fusion(&bm25_ranks, &vector_ranks, limit * 20);

    // 3b. Post-RRF title boost: RRF is rank-based (1/(60+rank) ≈ 0.016),
    // so a chunk in both lists gets ~0.032 while a title match in only the
    // BM25 list gets ~0.016 — body-only matches win. Adding +0.1 per matched
    // title bigram (+0.15 if full query in stem) ensures title matches
    // always rank above body-only hits, even in hybrid mode.
    {
        let file_stems: std::collections::HashMap<i64, String> = meta
            .files
            .iter()
            .map(|f| {
                (
                    f.id,
                    std::path::Path::new(&f.rel_path)
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_lowercase())
                        .unwrap_or_default(),
                )
            })
            .collect();
        let ql_lower = q.to_lowercase();
        for (chunk_id, score) in &mut fused {
            let chunk = &idx.chunks[*chunk_id as usize];
            if chunk.seq != 0 {
                continue; // only boost title chunks
            }
            if let Some(stem) = file_stems.get(&chunk.file_id) {
                if stem.is_empty() {
                    continue;
                }
                let matched = q_bigrams
                    .iter()
                    .filter(|bg| stem.contains(bg.as_str()))
                    .count();
                if matched > 0 {
                    *score += 0.1 * matched as f64;
                    if stem.contains(&ql_lower) {
                        *score += 0.15;
                    }
                }
            }
        }
        fused.sort_by(|a, b| b.1.total_cmp(&a.1));
    }

    // 3c. Post-RRF dup boost: files marked "高频·N份" (in meta.dup_counts)
    // get +0.1 × N on ALL their chunks — same scale as post-RRF title boost.
    // 3 copies → +0.3, 5 copies → +0.5 (RRF scores are ~0.016-0.032, so
    // this is a strong but not dominating boost).
    if !meta.dup_counts.is_empty() {
        let dup_file_chunks: std::collections::HashMap<i64, Vec<u32>> =
            idx.chunks
                .iter()
                .enumerate()
                .filter(|(_, c)| c.file_id != 0)
                .fold(
                    std::collections::HashMap::new(),
                    |mut m, (cid, c)| {
                        m.entry(c.file_id).or_default().push(cid as u32);
                        m
                    },
                );
        let mut chunk_boost: std::collections::HashMap<u32, f64> =
            std::collections::HashMap::new();
        for (fid, dc) in &meta.dup_counts {
            let bonus = 0.1 * *dc as f64;
            if let Some(chunks) = dup_file_chunks.get(fid) {
                for &cid in chunks {
                    chunk_boost.insert(cid, bonus);
                }
            }
        }
        for (cid, score) in &mut fused {
            if let Some(&b) = chunk_boost.get(cid) {
                *score += b;
            }
        }
        fused.sort_by(|a, b| b.1.total_cmp(&a.1));
    }

    // 4. Build hits (dedup by file, truncate to limit)
    let mut hits = Vec::new();
    let mut seen_files: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for (chunk_id, rrf_score) in fused {
        let chunk = &idx.chunks[chunk_id as usize];
        if !seen_files.insert(chunk.file_id) {
            continue; // one chunk per file
        }
        let Some(file) = meta.files.iter().find(|f| f.id == chunk.file_id) else {
            continue;
        };
        let Some(folder) = meta.folders.iter().find(|f| f.id == file.folder_id) else {
            continue;
        };
        let chunk_text = idx.get_chunk_text(chunk_id);
        hits.push(SearchHit {
            file_id: file.id,
            folder_path: folder.path.clone(),
            rel_path: file.rel_path.clone(),
            ext: file.ext.clone(),
            chunk_seq: chunk.seq,
            snippet: highlight_snippet(&chunk_text, q),
            rank: rrf_score,
        });
        if hits.len() >= limit {
            break;
        }
    }
    Ok(hits)
}

fn sort_desc(v: &mut Vec<(u32, f64)>) {
    v.sort_by(|a, b| b.1.total_cmp(&a.1));
}

/// 纯 BM25 检索（无向量路）。
pub fn bm25_only_search(
    meta: &Meta,
    idx: &BigramIndex,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchHit>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let q_bigrams = crate::search::query_bigrams(q);
    let mut bm25_scores: std::collections::HashMap<u32, f64> =
        std::collections::HashMap::new();
    bm25_score(&mut bm25_scores, idx, &q_bigrams);
    // Title boost (same as keyword + hybrid search).
    crate::search::apply_title_boost(&mut bm25_scores, meta, idx, &q_bigrams, q);
    let has_cjk = q.chars().any(|c| !c.is_ascii());
    let ql_lower = q.to_lowercase();
    let ranked: Vec<(u32, f64)> = bm25_scores
        .into_iter()
        .filter(|(cid, _)| {
            if has_cjk {
                true
            } else {
                idx.get_chunk_text(*cid)
                    .to_lowercase()
                    .contains(&ql_lower)
            }
        })
        .collect();
    let mut ranked = ranked;
    sort_desc(&mut ranked);

    let mut hits = Vec::new();
    let mut seen_files: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for (chunk_id, score) in ranked.into_iter() {
        let chunk = &idx.chunks[chunk_id as usize];
        if !seen_files.insert(chunk.file_id) {
            continue; // one chunk per file
        }
        let Some(file) = meta.files.iter().find(|f| f.id == chunk.file_id) else {
            continue;
        };
        let Some(folder) = meta.folders.iter().find(|f| f.id == file.folder_id) else {
            continue;
        };
        let chunk_text = idx.get_chunk_text(chunk_id);
        hits.push(SearchHit {
            file_id: file.id,
            folder_path: folder.path.clone(),
            rel_path: file.rel_path.clone(),
            ext: file.ext.clone(),
            chunk_seq: chunk.seq,
            snippet: highlight_snippet(&chunk_text, q),
            rank: score,
        });
        if hits.len() >= limit {
            break;
        }
    }
    Ok(hits)
}
