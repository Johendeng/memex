//! Bigram inverted-index search (CJK-friendly substring matching).

use std::collections::HashMap;

use anyhow::Result;

use crate::store::{BigramIndex, Meta};

#[derive(Debug, serde::Serialize)]
pub struct SearchHit {
    pub file_id: i64,
    pub folder_path: String,
    pub rel_path: String,
    pub ext: String,
    pub chunk_seq: i64,
    /// Snippet with the first match wrapped in `<b>…</b>`.
    pub snippet: String,
    pub rank: f64,
}

/// Query bigrams of `q` using the same tokenization as the indexer.
pub fn query_bigrams(q: &str) -> Vec<String> {
    let chars: Vec<char> = q.to_lowercase().chars().collect();
    let mut out = Vec::new();
    for w in chars.windows(2) {
        if w[0].is_alphanumeric() || w[1].is_alphanumeric() {
            out.push(w.iter().collect());
        }
    }
    out
}

pub fn search(meta: &Meta, idx: &BigramIndex, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }

    let bigrams = query_bigrams(q);

    // Score per chunk.
    let mut scores: HashMap<u32, f64> = HashMap::new();
    if bigrams.is_empty() {
        // Single char / all punctuation: fall back to a linear contains() scan.
        let ql = q.to_lowercase();
        for (cid, _) in idx.chunks.iter().enumerate() {
            if idx.get_chunk_text(cid as u32).to_lowercase().contains(&ql) {
                scores.insert(cid as u32, 1.0);
            }
        }
    } else {
        // Each distinct query bigram contributes 1.0 per chunk (not per
        // occurrence). This prevents xlsx files where "情报板" appears 500
        // times from outscoring a PDF whose title contains "显科" once.
        // Result: chunks matching MORE distinct query bigrams rank higher,
        // which is the desired behavior for CJK concept searches.
        for bg in &bigrams {
            let plist = idx.get_postings(bg);
            for p in &plist {
                *scores.entry(p.chunk_id).or_insert(0.0) += 1.0;
            }
        }
        // Title boost (independent pass) — see apply_title_boost().
        apply_title_boost(&mut scores, meta, idx, &bigrams, q);
    }

    // Post-filter: for ASCII-only queries, require the full query to appear
    // as a case-insensitive substring in the chunk text. This eliminates
    // bigram false positives (e.g. "modbus" → bigrams mo/od/db/bu/us matching
    // "motor","modified","database","buffer","status" scattered in an unrelated
    // document).
    //
    // For CJK queries, skip the exact-substring filter: users searching
    // "显科协议" mean "显科 + 协议" (two concepts), not the literal string
    // "显科协议". Bigram scoring already ranks chunks matching more query
    // bigrams higher, so results stay relevant without the over-strict filter.
    let ql = q.to_lowercase();
    let has_cjk = q.chars().any(|c| !c.is_ascii());

    let mut ranked: Vec<(u32, f64)> = scores.into_iter().collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut hits = Vec::new();
    let mut seen_files: std::collections::HashSet<i64> = std::collections::HashSet::new();
    for (chunk_id, score) in ranked.into_iter().filter(|(cid, _)| {
        if has_cjk { true } else { idx.get_chunk_text(*cid).to_lowercase().contains(&ql) }
    }) {
        let chunk = &idx.chunks[chunk_id as usize];
        if !seen_files.insert(chunk.file_id) {
            continue; // keep one chunk per file
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
        if hits.len() >= limit { break; }
    }
    Ok(hits)
}

/// Window ~80 chars around the first (case-insensitive) match, match wrapped in `<b>`.
pub fn make_snippet(text: &str, query: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let low: Vec<char> = text.to_lowercase().chars().collect();
    let ql: Vec<char> = query.to_lowercase().chars().collect();

    if ql.is_empty() || chars.len() < ql.len() {
        let end = chars.len().min(120);
        return chars[..end].iter().collect();
    }

    let pos = (0..=low.len().saturating_sub(ql.len()))
        .find(|&i| low[i..i + ql.len()] == ql[..]);

    match pos {
        Some(i) => {
            let start = i.saturating_sub(40);
            let end = (i + ql.len() + 40).min(chars.len());
            let pre: String = chars[start..i].iter().collect();
            let mt: String = chars[i..i + ql.len()].iter().collect();
            let post: String = chars[i + ql.len()..end].iter().collect();
            format!("{pre}<b>{mt}</b>{post}")
        }
        None => {
            let end = chars.len().min(120);
            chars[..end].iter().collect()
        }
    }
}

/// Enhanced snippet for hybrid/semantic search: highlights ALL query terms
/// (full query, CJK bigrams, ASCII words, individual CJK chars) in the best
/// 120-char window. Falls back gracefully when the full query doesn't appear
/// as a substring (which is the whole point of semantic matching).
pub fn highlight_snippet(text: &str, query: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let low: Vec<char> = text.to_lowercase().chars().collect();
    let q: String = query.trim().to_lowercase();
    let ql: Vec<char> = q.chars().collect();

    if ql.is_empty() || chars.is_empty() {
        let end = chars.len().min(120);
        return chars[..end].iter().collect();
    }

    // 1. Try full query as substring (highest priority — same as make_snippet)
    if ql.len() <= low.len() {
        if let Some(pos) = (0..=low.len().saturating_sub(ql.len()))
            .find(|&i| low[i..i + ql.len()] == ql[..])
        {
            let start = pos.saturating_sub(40);
            let end = (pos + ql.len() + 40).min(chars.len());
            let pre: String = chars[start..pos].iter().collect();
            let mt: String = chars[pos..pos + ql.len()].iter().collect();
            let post: String = chars[pos + ql.len()..end].iter().collect();
            return format!("{pre}<b>{mt}</b>{post}");
        }
    }

    // 2. No full match: collect query terms and find the densest window
    let terms = collect_query_terms(&q);
    if terms.is_empty() {
        let end = chars.len().min(120);
        return chars[..end].iter().collect();
    }

    let window_size = 120usize;
    let (start, end) = if chars.len() <= window_size {
        (0, chars.len())
    } else {
        let mut best_start = 0;
        let mut best_score = 0usize;
        for s in 0..=(chars.len() - window_size) {
            let window_str: String = low[s..s + window_size].iter().collect();
            let score = terms
                .iter()
                .filter(|t| window_str.contains(t.as_str()))
                .map(|t| t.chars().count())
                .sum::<usize>();
            if score > best_score {
                best_score = score;
                best_start = s;
            }
        }
        (best_start, (best_start + window_size).min(chars.len()))
    };

    // 3. Build snippet highlighting all term matches (greedy longest-first)
    build_highlighted(&chars[start..end], &low[start..end], &terms)
}

/// Collect query terms for highlighting, ordered by length descending
/// (so greedy matching prefers longer terms over individual chars).
fn collect_query_terms(q: &str) -> Vec<String> {
    let chars: Vec<char> = q.chars().collect();
    let mut terms: Vec<String> = Vec::new();

    // CJK bigrams
    for w in chars.windows(2) {
        let is_cjk = |c: char| !c.is_ascii() && !c.is_whitespace() && !c.is_ascii_punctuation();
        if is_cjk(w[0]) && is_cjk(w[1]) {
            let bg: String = w.iter().collect();
            if !terms.contains(&bg) {
                terms.push(bg);
            }
        }
    }

    // ASCII words (≥2 chars)
    let mut word = String::new();
    for &c in &chars {
        if c.is_ascii_alphanumeric() {
            word.push(c);
        } else {
            if word.len() >= 2 && !terms.contains(&word) {
                terms.push(word.clone());
            }
            word.clear();
        }
    }
    if word.len() >= 2 && !terms.contains(&word) {
        terms.push(word);
    }

    // Individual CJK chars (shortest, matched last by greedy)
    for &c in &chars {
        if !c.is_ascii() && !c.is_whitespace() && !c.is_ascii_punctuation() {
            let s = c.to_string();
            if !terms.contains(&s) {
                terms.push(s);
            }
        }
    }

    // Sort by length descending for greedy matching
    terms.sort_by(|a, b| b.len().cmp(&a.len()));
    terms
}

/// Greedy longest-match highlighting within a character window.
fn build_highlighted(chars: &[char], low: &[char], terms: &[String]) -> String {
    let n = chars.len();
    let mut result = String::new();
    let mut i = 0;

    while i < n {
        let mut matched_len = 0;
        for term in terms {
            let tl: Vec<char> = term.chars().collect();
            if !tl.is_empty() && i + tl.len() <= n && low[i..i + tl.len()] == tl[..] {
                if tl.len() > matched_len {
                    matched_len = tl.len();
                }
            }
        }

        if matched_len > 0 {
            result.push_str("<b>");
            for j in 0..matched_len {
                result.push(chars[i + j]);
            }
            result.push_str("</b>");
            i += matched_len;
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }

    result
}

/// Title boost: files whose NAME (file stem) contains query terms get a
/// strong, independent bonus on chunk#0 (which contains the prepended title
/// line). This makes title-matching files rank above body-only matches
/// regardless of body-text term frequency.
///
/// Weight: +10.0 per matched title bigram. Full query substring in title →
/// extra +15.0. These dwarf the max body score (~3.0 for a 3-bigram query),
/// so title matches always rank first.
///
/// Also applies a `dup_boost`: files in `meta.dup_counts` (marked "高频·N份"
/// after dedup cleanup) get +2.0 × N on ALL their chunks. 3 copies → +6.0,
/// 5 copies → +10.0 — on par with a single title-bigram match, so
/// frequently-referenced documents float up without dominating.
///
/// Called by `search()` (keyword) and `vector::hybrid_search()` /
/// `vector::bm25_only_search()` (BM25 path) — applied after bigram/BM25
/// scoring, before sorting.
pub fn apply_title_boost(
    scores: &mut HashMap<u32, f64>,
    meta: &Meta,
    idx: &BigramIndex,
    bigrams: &[String],
    query: &str,
) {
    if !bigrams.is_empty() {
        let file_stems: HashMap<i64, String> = meta
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
        let title_chunks: HashMap<i64, u32> = idx
            .chunks
            .iter()
            .enumerate()
            .filter(|(_, c)| c.seq == 0)
            .map(|(cid, c)| (c.file_id, cid as u32))
            .collect();
        let ql_lower = query.to_lowercase();
        for (file_id, stem) in &file_stems {
            if stem.is_empty() {
                continue;
            }
            let matched = bigrams.iter().filter(|bg| stem.contains(bg.as_str())).count();
            if matched == 0 {
                continue;
            }
            if let Some(&chunk0) = title_chunks.get(file_id) {
                let entry = scores.entry(chunk0).or_insert(0.0);
                *entry += 10.0 * matched as f64;
                if stem.contains(&ql_lower) {
                    *entry += 15.0;
                }
            }
        }
    }
    // Dup boost: +2.0 × dup_count for ALL chunks of files marked as 高频
    apply_dup_boost(scores, meta, idx);
}

/// For files in `meta.dup_counts` (files that had N copies, now deduped),
/// add +2.0 × N to every surviving chunk of that file. This makes
/// frequently-copied documents rank higher in all search paths.
pub fn apply_dup_boost(
    scores: &mut HashMap<u32, f64>,
    meta: &Meta,
    idx: &BigramIndex,
) {
    if meta.dup_counts.is_empty() {
        return;
    }
    let file_chunks: HashMap<i64, Vec<u32>> = idx
        .chunks
        .iter()
        .enumerate()
        .filter(|(_, c)| c.file_id != 0)
        .fold(HashMap::new(), |mut m: HashMap<i64, Vec<u32>>, (cid, c)| {
            m.entry(c.file_id).or_default().push(cid as u32);
            m
        });
    for (file_id, dup_count) in &meta.dup_counts {
        let bonus = 2.0 * *dup_count as f64;
        if let Some(chunks) = file_chunks.get(file_id) {
            for &cid in chunks {
                // Only boost chunks already scored by BM25 — don't inject
                // irrelevant files into results purely because they had dupes.
                if let Some(score) = scores.get_mut(&cid) {
                    *score += bonus;
                }
            }
        }
    }
}
