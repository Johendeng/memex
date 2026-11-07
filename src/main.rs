mod config;
mod update;
mod memories;
#[cfg(feature = "ocr")]
mod ocr;
mod dedup;
mod embed;
mod filter;
mod httpd;
mod indexer;
mod mcp;
mod parser;
mod search;
mod single_instance;
mod store;
mod vector;

mod window;

use std::collections::HashMap;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};

use store::{add_folder, index_bigrams, remove_folder, BigramIndex, Meta};

#[derive(Parser)]
#[command(name = "memex", version, about = "Memex — local knowledge base indexer (M1)")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show index, source folder and filter summary
    Info,
    /// Manage folder sources
    Folders {
        #[command(subcommand)]
        action: FoldersAction,
    },
    /// Manage the global file filter
    Filter {
        #[command(subcommand)]
        action: FilterAction,
    },
    /// (Re)index all registered folders
    Index,
    /// Rebuild postings.bin from existing chunks.text (no file re-parsing)
    RebuildPostings,
    /// Embed all indexed chunks with bge-small-zh (requires `memex index` first)
    Embed,
    /// Search indexed text
    Search {
        query: String,
        /// Max results (default 20)
        #[arg(short, long, default_value_t = 20)]
        limit: usize,
        /// Use hybrid retrieval: BM25 keyword + vector semantic + RRF fusion
        #[arg(long)]
        hybrid: bool,
    },
    /// Print the parsed text of one indexed file
    Read { file_id: i64 },
    /// Run as MCP stdio server (for Agent integration)
    Mcp {
        #[command(subcommand)]
        action: McpAction,
    },
    /// Serve the HTTP/JSON bridge + embedded frontend
    Serve {
        /// Port to bind (default 18321)
        #[arg(short, long, default_value_t = 18321)]
        port: u16,
        /// Open in system browser instead of native window
        #[arg(long)]
        open: bool,
        /// Run HTTP server only, no window (for MCP / agent use)
        #[arg(long)]
        no_window: bool,
    },
}

#[derive(Subcommand)]
enum FoldersAction {
    Add { path: String },
    List,
    Remove { id: i64 },
}

#[derive(Subcommand)]
enum FilterAction {
    Show,
    /// Restore the built-in default filter
    Reset,
}

#[derive(Subcommand)]
enum McpAction {
    /// Serve over stdio (JSON-RPC); meant to be spawned by an MCP client
    Serve,
}

fn main() -> ExitCode {
    if let Err(e) = run() {
        eprintln!("error: {e:#}");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let mut meta = Meta::load()?;

    match &cli.cmd {
        Cmd::Info => {
            let idx = BigramIndex::load()?;
            println!(
                "Memex index: {} folders, {} files, {} chunks, {} bigrams",
                meta.folders.len(),
                meta.files.len(),
                idx.chunks.len(),
                idx.bigram_count()
            );
            for f in &meta.folders {
                println!("  folder #{}: {}", f.id, f.path);
            }
        }

        Cmd::Folders { action } => match action {
            FoldersAction::Add { path } => {
                let id = add_folder(&mut meta, path)?;
                meta.save()?;
                println!("added folder #{id}");
            }
            FoldersAction::List => {
                if meta.folders.is_empty() {
                    println!("(no folders registered)");
                }
                for f in &meta.folders {
                    println!("{}  {}", f.id, f.path);
                }
            }
            FoldersAction::Remove { id } => {
                remove_folder(&mut meta, *id);
                meta.save()?;
                println!("removed folder #{id}");
            }
        },

        Cmd::Filter { action } => match action {
            FilterAction::Show => {
                println!("{}", serde_json::to_string_pretty(&meta.filter_rules)?);
            }
            FilterAction::Reset => {
                meta.filter_rules = filter::FilterRules::default();
                meta.save()?;
                println!("filter reset to default");
            }
        },

        Cmd::Index => {
            let stats = indexer::index_all(&mut meta)?;
            println!(
                "indexed: {} files, {} chunks (scanned {}, skipped {}, failed {})",
                stats.indexed, stats.chunks, stats.scanned, stats.skipped, stats.failed
            );
        }

        Cmd::RebuildPostings => {
            let mut idx = BigramIndex::load()?;
            println!("rebuilding postings from {} chunks...", idx.chunks.len());
            // Drop mmap/index, start fresh
            idx.postings_mmap = None;
            idx.postings_index = HashMap::new();
            idx.postings.clear();
            idx.postings_delta.clear();
            // Rebuild from chunks.text
            let texts = idx.read_all_texts();
            for (i, text) in texts.iter().enumerate() {
                if i % 100_000 == 0 {
                    eprintln!("  bigram indexing: {}/{}", i, texts.len());
                }
                index_bigrams(&mut idx.postings, text, i as u32);
            }
            let bg = idx.postings.len();
            println!("rebuilt: {} bigrams", bg);
            idx.save_postings()?;
            // Clear HashMap, reload mmap for serve mode
            idx.postings.clear();
            idx.postings.shrink_to_fit();
            idx.load_postings_disk()?;
            println!("done: postings.bin rebuilt, mmap reloaded ({} bigrams)", idx.postings_index.len());
        }

        Cmd::Embed => {
            embed::embed_all()?;
        }

        Cmd::Search {
            query,
            limit,
            hybrid,
        } => {
            let idx = BigramIndex::load()?;
            let hits = if *hybrid {
                let vec_idx = vector::VectorIndex::load()?;
                if vec_idx.vectors.is_empty() {
                    eprintln!("(no vectors — run `memex embed` first; falling back to keyword search)");
                    search::search(&meta, &idx, query, *limit)?
                } else {
                    vector::hybrid_search(&meta, &idx, &vec_idx, query, *limit)?
                }
            } else {
                search::search(&meta, &idx, query, *limit)?
            };
            if hits.is_empty() {
                println!("no results for {query:?}");
            }
            for (i, h) in hits.iter().enumerate() {
                let snippet = h.snippet.replace("<b>", "「").replace("</b>", "」");
                println!(
                    "{}. [{} · {}] {}/{}  chunk#{} rank {:.4}\n   {}",
                    i + 1,
                    h.file_id,
                    h.ext,
                    h.folder_path,
                    h.rel_path,
                    h.chunk_seq,
                    h.rank,
                    snippet
                );
            }
        }

        Cmd::Read { file_id } => {
            let idx = BigramIndex::load()?;
            let chunks: Vec<String> = idx
                .chunks
                .iter()
                .enumerate()
                .filter(|(_, c)| c.file_id == *file_id)
                .map(|(cid, _)| idx.get_chunk_text(cid as u32))
                .collect();
            if chunks.is_empty() {
                println!("no chunks for file #{file_id}");
            } else {
                // de-dup overlap by keeping whole chunks
                for c in &chunks {
                    print!("{c}");
                }
                println!();
            }
        }

        Cmd::Mcp { action } => match action {
            McpAction::Serve => {
                mcp::serve()?;
            }
        },

        Cmd::Serve { port, open, no_window } => {
            // Single-instance guard: refuse to start a second service
            // process. The lock file is released on clean exit (Drop).
            if let Some(existing_pid) = single_instance::acquire_lock()? {
                eprintln!(
                    "already running as PID {existing_pid}; exiting (single-instance mode)."
                );
                return Ok(());
            }

            if *no_window {
                window::run_tray_only(*port)?;
                single_instance::release_lock();
            } else if *open {
                httpd::serve(*port, true)?;
                single_instance::release_lock();
            } else {
                window::run_window(*port)?;
            }
        }
    }
    Ok(())
}
