//! Persistence: JSON metadata (`meta.json`) + bincode bigram index (`index.bin`)
//! + external chunk text store (`chunks.text`).
//!
//! Index root: `<MEMEX_HOME>/data` (resolves to the exe's directory by default;
//! see `config.rs` for the full resolution chain).
//!
//! Plan B memory optimization: chunk text is stored in a separate file
//! (`chunks.text`), not in the bincode index. This reduces resident memory
//! from ~1.12GB to ~30MB. Text is read on-demand via byte offsets.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::filter::FilterRules;

pub fn index_dir() -> Result<PathBuf> {
    let dir = crate::config::data_dir();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn meta_path() -> Result<PathBuf> {
    Ok(index_dir()?.join("meta.json"))
}

fn index_path() -> Result<PathBuf> {
    Ok(index_dir()?.join("index.bin"))
}

/// Path to the external chunk text file.
fn chunks_text_path() -> Result<PathBuf> {
    Ok(index_dir()?.join("chunks.text"))
}

/// Path to the external postings data file (raw posting bytes).
fn postings_path() -> Result<PathBuf> {
    Ok(index_dir()?.join("postings.bin"))
}

/// Path to the postings index file (bincode: Vec<(String, u64, u32)>).
fn postings_idx_path() -> Result<PathBuf> {
    Ok(index_dir()?.join("postings.idx"))
}

/// Path to the postings delta file (bincode: HashMap<String, Vec<Posting>>).
/// Stores incremental postings added during serve mode (watcher updates).
fn postings_delta_path() -> Result<PathBuf> {
    Ok(index_dir()?.join("postings.delta"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderRec {
    pub id: i64,
    pub path: String,
    pub added_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRec {
    pub id: i64,
    pub folder_id: i64,
    pub rel_path: String,
    pub ext: String,
    pub size: u64,
    pub mtime: u64,
    #[serde(default)]
    pub content_indexed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagRec {
    pub file_id: i64,
    pub dim: String,
    pub value: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceRec {
    pub file_id: i64,
    pub score: f64,
    pub at: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

pub const CONFIDENCE_THRESHOLD: f64 = 0.85;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Meta {
    #[serde(default)]
    pub folders: Vec<FolderRec>,
    #[serde(default)]
    pub files: Vec<FileRec>,
    #[serde(default = "default_one")]
    pub next_folder_id: i64,
    #[serde(default = "default_one")]
    pub next_file_id: i64,
    #[serde(default = "FilterRules::default")]
    pub filter_rules: FilterRules,
    #[serde(default)]
    pub tags: Vec<TagRec>,
    #[serde(default)]
    pub confidence: Vec<ConfidenceRec>,
    #[serde(default)]
    pub dup_counts: HashMap<i64, u32>,
}

fn default_one() -> i64 {
    1
}

impl Meta {
    pub fn load() -> Result<Self> {
        let p = meta_path()?;
        if p.exists() {
            let s = std::fs::read_to_string(&p)?;
            Ok(serde_json::from_str(&s)?)
        } else {
            Ok(Meta {
                folders: Vec::new(),
                files: Vec::new(),
                next_folder_id: 1,
                next_file_id: 1,
                filter_rules: FilterRules::default(),
                tags: Vec::new(),
                confidence: Vec::new(),
                dup_counts: HashMap::new(),
            })
        }
    }

    pub fn save(&self) -> Result<()> {
        let p = meta_path()?;
        let s = serde_json::to_string_pretty(self)?;
        let tmp = p.with_extension("tmp");
        std::fs::write(&tmp, s)?;
        std::fs::rename(&tmp, &p)?;
        Ok(())
    }
}

// ---------- folder ops ----------

pub fn add_folder(meta: &mut Meta, path: &str) -> Result<i64> {
    let abs = std::fs::canonicalize(path).with_context(|| format!("invalid path: {path}"))?;
    let raw = abs.to_string_lossy().to_string();
    let pstr = raw
        .strip_prefix(r"\\?\")
        .map(String::from)
        .unwrap_or(raw);
    if let Some(f) = meta.folders.iter().find(|f| f.path == pstr) {
        return Ok(f.id);
    }
    let id = meta.next_folder_id;
    meta.next_folder_id += 1;
    meta.folders.push(FolderRec {
        id,
        path: pstr,
        added_at: now(),
    });
    Ok(id)
}

pub fn remove_folder(meta: &mut Meta, id: i64) {
    meta.folders.retain(|f| f.id != id);
    meta.files.retain(|f| f.folder_id != id);
    meta.tags.retain(|t| {
        meta.files.iter().any(|f| f.id == t.file_id) || false
    });
    meta.confidence.retain(|c| {
        meta.files.iter().any(|f| f.id == c.file_id)
    });
}

pub fn find_file<'a>(meta: &'a Meta, file: &str) -> Option<&'a FileRec> {
    if let Ok(id) = file.parse::<i64>() {
        return meta.files.iter().find(|f| f.id == id);
    }
    let needle = file.replace('\\', "/");
    meta.files
        .iter()
        .find(|f| f.rel_path.replace('\\', "/").to_lowercase().contains(&needle.to_lowercase()))
}

pub fn tags_of(meta: &Meta, file_id: i64) -> Vec<&TagRec> {
    meta.tags.iter().filter(|t| t.file_id == file_id).collect()
}

pub fn confidence_of(meta: &Meta, file_id: i64) -> Option<&ConfidenceRec> {
    meta.confidence.iter().find(|c| c.file_id == file_id)
}

fn now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}

// ---------- bigram inverted index (Plan B: text separated) ----------

/// Chunk metadata without the text itself. Text lives in `chunks.text`.
/// `char_len` is precomputed for BM25 length normalization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkRec {
    pub file_id: i64,
    pub seq: i64,
    #[serde(default)]
    pub char_len: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Posting {
    pub chunk_id: u32,
    pub count: u32,
}

/// Plan B+: BigramIndex holds metadata + text offsets in memory.
/// Postings are stored on disk (`postings.bin`) and accessed via mmap.
/// During indexing, postings are held in a HashMap (write mode).
/// During serving, postings are mmap'd (read mode, ~50MB resident).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct BigramIndex {
    pub chunks: Vec<ChunkRec>,
    /// In-memory postings (indexing mode only). Empty in serve mode.
    #[serde(skip)]
    pub postings: HashMap<String, Vec<Posting>>,
    /// Byte offsets into `chunks.text`. Length = chunks.len() + 1.
    /// chunk i's text bytes = text_offsets[i] .. text_offsets[i+1].
    /// Dead chunks: text_offsets[i] == text_offsets[i+1] (zero-length).
    #[serde(default)]
    pub text_offsets: Vec<u64>,
    #[serde(skip)]
    pub file_chunks: HashMap<i64, Vec<u32>>,
    /// Serve mode: bigram → (offset, count) in postings.bin.
    #[serde(skip)]
    pub postings_index: HashMap<String, (u64, u32)>,
    /// Serve mode: mmap of postings.bin.
    #[serde(skip)]
    pub postings_mmap: Option<memmap2::Mmap>,
    /// Delta postings for incremental updates in mmap mode (watcher).
    /// Small HashMap — only new postings since last full reindex.
    #[serde(skip)]
    pub postings_delta: HashMap<String, Vec<Posting>>,
}

const DEAD_CHUNK: i64 = 0;

impl BigramIndex {
    /// Magic version byte: v3 = Plan B+ (postings externalized via mmap).
    /// v2 = Plan B (postings in bincode). v1 = legacy (text in ChunkRec).
    const VERSION_V3: u8 = 0xFC;

    pub fn load() -> Result<Self> {
        let p = index_path()?;
        if !p.exists() {
            return Ok(BigramIndex::default());
        }
        let bytes = std::fs::read(&p)?;
        if bytes.is_empty() {
            return Ok(BigramIndex::default());
        }

        if bytes[0] == Self::VERSION_V3 {
            // Plan B+ format: index.bin = chunks + text_offsets only.
            // Postings are in postings.bin (mmap'd) + postings.idx.
            let mut idx: BigramIndex = bincode::deserialize(&bytes[1..])
                .context("index.bin v3 deserialization failed")?;
            idx.rebuild_file_chunks_map();
            // Fix text_offsets if needed
            if idx.text_offsets.len() != idx.chunks.len() + 1 {
                let ct_path = chunks_text_path()?;
                let file_size = std::fs::metadata(&ct_path).map(|m| m.len()).unwrap_or(0);
                idx.text_offsets.clear();
                idx.text_offsets.resize(idx.chunks.len() + 1, 0);
                idx.text_offsets[idx.chunks.len()] = file_size;
            }
            // Load postings index + mmap
            idx.load_postings_disk()?;
            // Load delta (incremental postings from watcher updates)
            idx.load_delta();
            Ok(idx)
        } else if bytes[0] == 0xFD {
            // Plan B format (v2): postings serialized in bincode.
            // Since postings is now #[serde(skip)], use a separate struct.
            #[derive(Deserialize)]
            struct V2Index {
                chunks: Vec<ChunkRec>,
                postings: HashMap<String, Vec<Posting>>,
                #[serde(default)]
                text_offsets: Vec<u64>,
                #[serde(default)]
                file_chunks: HashMap<i64, Vec<u32>>,
            }
            let v2: V2Index = bincode::deserialize(&bytes[1..])
                .context("index.bin v2 deserialization failed")?;
            let mut idx = BigramIndex {
                chunks: v2.chunks,
                postings: v2.postings,
                text_offsets: v2.text_offsets,
                file_chunks: v2.file_chunks,
                postings_index: HashMap::new(),
                postings_mmap: None,
                postings_delta: HashMap::new(),
            };
            idx.rebuild_file_chunks_map();
            if idx.text_offsets.len() != idx.chunks.len() + 1 {
                let ct_path = chunks_text_path()?;
                let file_size = std::fs::metadata(&ct_path).map(|m| m.len()).unwrap_or(0);
                idx.text_offsets.clear();
                idx.text_offsets.resize(idx.chunks.len() + 1, 0);
                idx.text_offsets[idx.chunks.len()] = file_size;
            }
            // Save in v3 format (writes postings to disk)
            idx.save()?;
            // Now load postings from disk (mmap mode)
            idx.postings.clear();
            idx.load_postings_disk()?;
            Ok(idx)
        } else {
            // Legacy format (v1: ChunkRec had text: String, no version prefix)
            migrate_old_format(&bytes)
        }
    }

    pub fn save(&self) -> Result<()> {
        let p = index_path()?;
        // Serialize only chunks + text_offsets (postings are #[serde(skip)])
        let mut bytes = vec![Self::VERSION_V3];
        bytes.extend_from_slice(&bincode::serialize(self)?);
        let tmp = p.with_extension("tmp");
        std::fs::write(&tmp, &bytes)?;
        std::fs::rename(&tmp, &p)?;

        // Postings are NOT written here — use save_postings() for that.
        // This makes interim saves during indexing fast (O(chunks) not O(postings)).
        // Save delta if non-empty (incremental watcher updates in mmap mode).
        self.save_delta();
        Ok(())
    }

    /// Save everything including postings to disk. Call at end of indexing.
    pub fn save_postings(&self) -> Result<()> {
        self.save()?;
        if !self.postings.is_empty() {
            self.save_postings_disk()?;
        }
        Ok(())
    }

    /// After incremental updates in serve mode, save postings and switch
    /// back to mmap mode to free ~950MB of HashMap memory.
    /// Call this after watcher/incremental updates instead of save_postings().
    pub fn save_postings_and_switch_to_mmap(&mut self) -> Result<()> {
        self.save()?;
        if !self.postings.is_empty() {
            self.save_postings_disk()?;
            // Clear HashMap to free memory, then reload mmap
            self.postings.clear();
            self.postings.shrink_to_fit();
            self.load_postings_disk()?;
            eprintln!("postings switched back to mmap mode");
        }
        Ok(())
    }

    /// Save delta postings to postings.delta file.
    fn save_delta(&self) {
        let path = match postings_delta_path() {
            Ok(p) => p,
            Err(_) => return,
        };
        if self.postings_delta.is_empty() {
            // Remove stale delta file if delta is empty
            let _ = std::fs::remove_file(&path);
            return;
        }
        match bincode::serialize(&self.postings_delta) {
            Ok(bytes) => {
                if let Err(e) = std::fs::write(&path, &bytes) {
                    eprintln!("save_delta: write failed: {e}");
                }
            }
            Err(e) => eprintln!("save_delta: serialize failed: {e}"),
        }
    }

    /// Load delta postings from postings.delta file.
    fn load_delta(&mut self) {
        let path = match postings_delta_path() {
            Ok(p) => p,
            Err(_) => return,
        };
        if !path.exists() {
            return;
        }
        match std::fs::read(&path) {
            Ok(bytes) => {
                match bincode::deserialize::<HashMap<String, Vec<Posting>>>(&bytes) {
                    Ok(delta) => {
                        self.postings_delta = delta;
                        if !self.postings_delta.is_empty() {
                            eprintln!(
                                "delta loaded: {} bigrams with new postings",
                                self.postings_delta.len()
                            );
                        }
                    }
                    Err(e) => eprintln!("load_delta: deserialize failed: {e}"),
                }
            }
            Err(e) => eprintln!("load_delta: read failed: {e}"),
        }
    }

    /// Write postings to postings.bin (raw data) + postings.idx (index).
    fn save_postings_disk(&self) -> Result<()> {
        let pb_path = postings_path()?;
        let idx_path = postings_idx_path()?;

        // Sort bigrams for deterministic order
        let mut entries: Vec<(&String, &Vec<Posting>)> =
            self.postings.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));

        use std::io::{Write, Seek};
        let mut file = std::fs::File::create(&pb_path)?;
        let mut index: Vec<(String, u64, u32)> = Vec::with_capacity(entries.len());

        for (bigram, postings) in &entries {
            let offset = file.stream_position()?;
            // Write raw posting data: each Posting is 8 bytes (u32 + u32 LE)
            for p in *postings {
                file.write_all(&p.chunk_id.to_le_bytes())?;
                file.write_all(&p.count.to_le_bytes())?;
            }
            index.push((bigram.to_string(), offset, postings.len() as u32));
        }
        file.flush()?;
        drop(file);

        // Write index as bincode
        let idx_bytes = bincode::serialize(&index)?;
        let tmp = idx_path.with_extension("tmp");
        std::fs::write(&tmp, &idx_bytes)?;
        std::fs::rename(&tmp, &idx_path)?;

        eprintln!(
            "postings saved: {} bigrams, {} bytes data",
            entries.len(),
            std::fs::metadata(&pb_path)?.len()
        );
        Ok(())
    }

    /// Load postings from disk into mmap + index (serve mode).
    pub fn load_postings_disk(&mut self) -> Result<()> {
        let pb_path = postings_path()?;
        let idx_path = postings_idx_path()?;

        if !pb_path.exists() || !idx_path.exists() {
            eprintln!("postings files not found, will use in-memory mode");
            return Ok(());
        }

        // Load index
        let idx_bytes = std::fs::read(&idx_path)?;
        let index: Vec<(String, u64, u32)> = bincode::deserialize(&idx_bytes)
            .context("postings.idx deserialization failed")?;

        self.postings_index = HashMap::with_capacity(index.len());
        for (bigram, offset, count) in index {
            self.postings_index.insert(bigram, (offset, count));
        }

        // mmap postings data
        let file = std::fs::File::open(&pb_path)?;
        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        self.postings_mmap = Some(mmap);

        eprintln!(
            "postings loaded (mmap): {} bigrams",
            self.postings_index.len()
        );
        Ok(())
    }

    /// Get postings for a bigram. Works in both indexing (HashMap) and
    /// serve (mmap) modes.
    pub fn get_postings(&self, bigram: &str) -> Vec<Posting> {
        // Try HashMap first (indexing mode)
        if let Some(p) = self.postings.get(bigram) {
            return p.clone();
        }
        // Serve mode: read from mmap, then merge delta
        let mut result = Vec::new();
        if let Some(mmap) = &self.postings_mmap {
            if let Some(&(offset, count)) = self.postings_index.get(bigram) {
                let start = offset as usize;
                let end = start + (count as usize) * 8;
                if end <= mmap.len() {
                    result.reserve(count as usize);
                    for i in 0..count as usize {
                        let off = start + i * 8;
                        let chunk_id = u32::from_le_bytes([
                            mmap[off], mmap[off + 1], mmap[off + 2], mmap[off + 3],
                        ]);
                        let cnt = u32::from_le_bytes([
                            mmap[off + 4], mmap[off + 5], mmap[off + 6], mmap[off + 7],
                        ]);
                        result.push(Posting { chunk_id, count: cnt });
                    }
                }
            }
        }
        // Merge delta (new postings from incremental watcher updates)
        if let Some(delta) = self.postings_delta.get(bigram) {
            result.extend_from_slice(delta);
        }
        result
    }

    /// Number of unique bigrams (works in both modes).
    pub fn bigram_count(&self) -> usize {
        if !self.postings.is_empty() {
            self.postings.len()
        } else {
            // mmap mode: count unique bigrams in both index and delta
            let mut count = self.postings_index.len();
            for bg in self.postings_delta.keys() {
                if !self.postings_index.contains_key(bg) {
                    count += 1;
                }
            }
            count
        }
    }

    /// Switch from serve mode (mmap) to indexing mode (HashMap).
    /// Called before any write operation (add_file, remove_file, compact).
    /// Merges delta into main HashMap if any.
    fn ensure_indexing_mode(&mut self) {
        if self.postings_mmap.is_none() {
            // Already in indexing mode — just merge delta
            if !self.postings_delta.is_empty() {
                for (bg, delta_posts) in self.postings_delta.drain() {
                    self.postings.entry(bg).or_default().extend(delta_posts);
                }
                self.save_delta(); // Clear delta file
            }
            return;
        }
        let mmap = self.postings_mmap.take().unwrap();
        let index = std::mem::take(&mut self.postings_index);
        for (bigram, (offset, count)) in index {
            let start = offset as usize;
            let end = start + (count as usize) * 8;
            let mut postings = Vec::with_capacity(count as usize);
            if end <= mmap.len() {
                for i in 0..count as usize {
                    let off = start + i * 8;
                    let chunk_id = u32::from_le_bytes([
                        mmap[off], mmap[off + 1], mmap[off + 2], mmap[off + 3],
                    ]);
                    let cnt = u32::from_le_bytes([
                        mmap[off + 4], mmap[off + 5], mmap[off + 6], mmap[off + 7],
                    ]);
                    postings.push(Posting { chunk_id, count: cnt });
                }
            }
            self.postings.insert(bigram, postings);
        }
        self.postings_mmap = None;
        // Merge delta into main HashMap
        if !self.postings_delta.is_empty() {
            for (bg, delta_posts) in self.postings_delta.drain() {
                self.postings.entry(bg).or_default().extend(delta_posts);
            }
            self.save_delta(); // Clear delta file
        }
        self.postings_index = HashMap::new();
    }

    pub fn rebuild_file_chunks_map(&mut self) {
        let mut map: HashMap<i64, Vec<u32>> = HashMap::new();
        for (i, c) in self.chunks.iter().enumerate() {
            if c.file_id != DEAD_CHUNK {
                map.entry(c.file_id).or_default().push(i as u32);
            }
        }
        self.file_chunks = map;
    }

    /// Read a single chunk's text from `chunks.text` by chunk_id.
    pub fn get_chunk_text(&self, chunk_id: u32) -> String {
        let cid = chunk_id as usize;
        if cid >= self.chunks.len() || cid + 1 >= self.text_offsets.len() {
            return String::new();
        }
        let start = self.text_offsets[cid];
        let end = self.text_offsets[cid + 1];
        if start >= end {
            return String::new();
        }
        let path = match chunks_text_path() {
            Ok(p) => p,
            Err(_) => return String::new(),
        };
        let mut file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(_) => return String::new(),
        };
        use std::io::{Seek, Read, SeekFrom};
        if file.seek(SeekFrom::Start(start)).is_err() {
            return String::new();
        }
        let len = (end - start) as usize;
        let mut buf = vec![0u8; len];
        if file.read_exact(&mut buf).is_err() {
            return String::new();
        }
        String::from_utf8_lossy(&buf).into_owned()
    }

    /// Read all chunk texts sequentially (for embed_all).
    pub fn read_all_texts(&self) -> Vec<String> {
        let path = match chunks_text_path() {
            Ok(p) => p,
            Err(_) => return vec![String::new(); self.chunks.len()],
        };
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(_) => return vec![String::new(); self.chunks.len()],
        };
        let mut result = Vec::with_capacity(self.chunks.len());
        for i in 0..self.chunks.len() {
            if i + 1 >= self.text_offsets.len() {
                result.push(String::new());
                continue;
            }
            let start = self.text_offsets[i] as usize;
            let end = self.text_offsets[i + 1] as usize;
            if start >= end || end > bytes.len() {
                result.push(String::new());
            } else {
                result.push(String::from_utf8_lossy(&bytes[start..end]).into_owned());
            }
        }
        result
    }

    /// Append a file's chunks to the index.
    /// In mmap mode, postings go to delta HashMap (not main mmap).
    pub fn add_file(&mut self, file_id: i64, texts: &[String]) {
        let mmap_mode = self.postings_mmap.is_some();
        if !mmap_mode {
            self.ensure_indexing_mode();
        }
        let path = match chunks_text_path() {
            Ok(p) => p,
            Err(_) => {
                eprintln!("add_file: cannot resolve chunks.text path");
                return;
            }
        };

        let current_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

        // Ensure text_offsets has sentinel: length = chunks.len() + 1
        if self.text_offsets.is_empty() {
            self.text_offsets.push(current_size);
        }
        while self.text_offsets.len() < self.chunks.len() {
            self.text_offsets.insert(self.text_offsets.len() - 1, current_size);
        }
        if self.text_offsets.len() == self.chunks.len() {
            self.text_offsets.push(current_size);
        }

        use std::io::Write;
        let mut file = match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            Ok(f) => f,
            Err(e) => {
                eprintln!("add_file: open chunks.text failed: {e}");
                return;
            }
        };

        let mut ids = Vec::with_capacity(texts.len());
        for (seq, text) in texts.iter().enumerate() {
            let chunk_id = self.chunks.len() as u32;
            let char_len = text.chars().count() as u32;

            let start_offset = self.text_offsets[self.chunks.len()];
            let byte_len = text.len() as u64;
            let new_end = start_offset + byte_len;

            if let Err(e) = file.write_all(text.as_bytes()) {
                eprintln!("add_file: write chunks.text failed: {e}");
                return;
            }

            // Insert start_offset before sentinel
            let sentinel_idx = self.chunks.len();
            self.text_offsets.insert(sentinel_idx, start_offset);

            self.chunks.push(ChunkRec {
                file_id,
                seq: seq as i64,
                char_len,
            });

            // Update sentinel at new chunks.len()
            let new_sentinel_idx = self.chunks.len();
            if new_sentinel_idx < self.text_offsets.len() {
                self.text_offsets[new_sentinel_idx] = new_end;
            } else {
                self.text_offsets.push(new_end);
            }

            if mmap_mode {
                index_bigrams(&mut self.postings_delta, text, chunk_id);
            } else {
                index_bigrams(&mut self.postings, text, chunk_id);
            }
            ids.push(chunk_id);
        }

        let _ = file.flush();
        self.file_chunks.insert(file_id, ids);
    }

    /// Remove a file's chunks (postings cleaned, chunks marked dead).
    /// In mmap mode, skips ensure_indexing_mode() — just marks chunks dead.
    /// Dead chunks are naturally filtered by search (file_id=0 not in meta.files).
    pub fn remove_file(&mut self, file_id: i64) {
        let mmap_mode = self.postings_mmap.is_some();
        if !mmap_mode {
            self.ensure_indexing_mode();
        }
        let Some(ids) = self.file_chunks.remove(&file_id) else {
            return;
        };
        for cid in &ids {
            if !mmap_mode {
                // Only clean postings in HashMap mode (can't modify mmap)
                for postings in self.postings.values_mut() {
                    postings.retain(|p| p.chunk_id != *cid);
                }
            }
            if let Some(c) = self.chunks.get_mut(*cid as usize) {
                c.file_id = DEAD_CHUNK;
                c.char_len = 0;
            }
            let ci = *cid as usize;
            if ci + 1 < self.text_offsets.len() {
                self.text_offsets[ci + 1] = self.text_offsets[ci];
            }
        }
        if !mmap_mode {
            self.postings.retain(|_, v| !v.is_empty());
        }
    }

    /// Remove old chunks + append new ones.
    pub fn replace_file(&mut self, file_id: i64, texts: &[String]) {
        self.remove_file(file_id);
        self.add_file(file_id, texts);
    }

    /// Compact: remove dead chunks, rebuild chunks.text, remap chunk_ids.
    pub fn compact(&mut self) {
        self.ensure_indexing_mode();
        if !self.chunks.iter().any(|c| c.file_id == DEAD_CHUNK) {
            return;
        }

        let path = match chunks_text_path() {
            Ok(p) => p,
            Err(_) => return,
        };
        let old_bytes = std::fs::read(&path).unwrap_or_default();

        let tmp = path.with_extension("tmp");
        use std::io::{Write, Seek};
        let mut new_file = match std::fs::File::create(&tmp) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("compact: create temp chunks.text failed: {e}");
                return;
            }
        };

        let mut remap: HashMap<u32, u32> = HashMap::new();
        let mut new_chunks: Vec<ChunkRec> = Vec::new();
        let mut new_offsets: Vec<u64> = Vec::new();
        new_offsets.push(0);

        for (old, c) in self.chunks.iter().enumerate() {
            if c.file_id == DEAD_CHUNK {
                continue;
            }
            remap.insert(old as u32, new_chunks.len() as u32);

            let start = if old < self.text_offsets.len() {
                self.text_offsets[old] as usize
            } else {
                0
            };
            let end = if old + 1 < self.text_offsets.len() {
                self.text_offsets[old + 1] as usize
            } else {
                start
            };
            let text_bytes = if start < end && end <= old_bytes.len() {
                &old_bytes[start..end]
            } else {
                &[][..]
            };

            let offset = new_file.stream_position().unwrap_or(0);
            let _ = new_file.write_all(text_bytes);
            new_offsets.push(offset + text_bytes.len() as u64);

            new_chunks.push(c.clone());
        }
        let _ = new_file.flush();
        drop(new_file);

        let _ = std::fs::rename(&tmp, &path);

        for postings in self.postings.values_mut() {
            for p in postings.iter_mut() {
                if let Some(&np) = remap.get(&p.chunk_id) {
                    p.chunk_id = np;
                }
            }
        }
        self.postings.retain(|_, v| !v.is_empty());

        self.chunks = new_chunks;
        self.text_offsets = new_offsets;

        let mut map: HashMap<i64, Vec<u32>> = HashMap::new();
        for (i, c) in self.chunks.iter().enumerate() {
            map.entry(c.file_id).or_default().push(i as u32);
        }
        self.file_chunks = map;
    }
}

/// Migrate old index format (ChunkRec with text) to Plan B format.
fn migrate_old_format(bytes: &[u8]) -> Result<BigramIndex> {
    #[derive(Deserialize)]
    struct LegacyChunk {
        file_id: i64,
        seq: i64,
        text: String,
    }
    #[derive(Deserialize)]
    struct LegacyIndex {
        chunks: Vec<LegacyChunk>,
        postings: HashMap<String, Vec<Posting>>,
    }

    let old: LegacyIndex = bincode::deserialize(bytes)
        .context("index.bin deserialization failed (neither new nor legacy format)")?;

    eprintln!(
        "migrating legacy index: {} chunks, extracting texts to chunks.text...",
        old.chunks.len()
    );

    let ct_path = chunks_text_path()?;
    use std::io::Write;
    let mut file = std::fs::File::create(&ct_path)?;

    let mut chunks = Vec::with_capacity(old.chunks.len());
    let mut text_offsets = Vec::with_capacity(old.chunks.len() + 1);
    let mut offset: u64 = 0;

    for c in &old.chunks {
        let char_len = c.text.chars().count() as u32;
        let text_bytes = c.text.as_bytes();
        let _ = file.write_all(text_bytes);
        text_offsets.push(offset);
        offset += text_bytes.len() as u64;
        chunks.push(ChunkRec {
            file_id: c.file_id,
            seq: c.seq,
            char_len,
        });
    }
    text_offsets.push(offset);
    let _ = file.flush();
    drop(file);

    let mut idx = BigramIndex {
        chunks,
        postings: old.postings,
        text_offsets,
        file_chunks: HashMap::new(),
        postings_index: HashMap::new(),
        postings_mmap: None,
        postings_delta: HashMap::new(),
    };
    idx.rebuild_file_chunks_map();
    idx.save()?;
    eprintln!("migration complete: index.bin shrunk, chunks.text created");
    Ok(idx)
}

/// Index consecutive char bigrams of a chunk (lowercased); pure-punct pairs skipped.
pub fn index_bigrams(postings: &mut HashMap<String, Vec<Posting>>, text: &str, chunk_id: u32) {
    let chars: Vec<char> = text.to_lowercase().chars().collect();
    if chars.len() < 2 {
        return;
    }
    let mut counts: HashMap<String, u32> = HashMap::new();
    for w in chars.windows(2) {
        if !(w[0].is_alphanumeric() || w[1].is_alphanumeric()) {
            continue;
        }
        let bg: String = w.iter().collect();
        *counts.entry(bg).or_insert(0) += 1;
    }
    for (bg, c) in counts {
        postings
            .entry(bg)
            .or_default()
            .push(Posting { chunk_id, count: c });
    }
}
