//! Memex core — M1: folder sources, global filters, SQLite + FTS5 (trigram) indexing.
//!
//! Milestone notes:
//! - M1 ships BM25-less trigram substring search (best for CJK "只言片语").
//! - Tantivy + hybrid vector retrieval arrives in M3 (see docs/tech-design.md).

pub mod config;
pub mod update;
pub mod memories;
#[cfg(feature = "ocr")]
pub mod ocr;
pub mod dedup;
pub mod embed;
pub mod filter;
pub mod httpd;
pub mod indexer;
pub mod mcp;
pub mod parser;
pub mod search;
pub mod store;
pub mod vector;
