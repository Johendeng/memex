# Memex

> **Local knowledge base + AI memory. Your files, your index, your Agent.**

[![version](https://img.shields.io/badge/version-1.2.0-blue)]() [![license](https://img.shields.io/badge/license-Apache--2.0-green)]() [![platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-orange)]() [![rust](https://img.shields.io/badge/Rust-2021-orange)]() [![vue](https://img.shields.io/badge/Vue-3-42b883)]()

[中文](./README.md) | [English](./README.en.md)

---

## What is Memex

Your computer is scattered with hundreds of documents — technical notes, project plans, PDF papers, code snippets. You know they're in some folder, but finding the right one takes forever.

**Memex builds these files into a searchable local knowledge base.** No cloud, no online service — all data stays on your machine, the process runs on `127.0.0.1`, no internet, no uploads, no account required.

It also works as an **AI memory** for your Agent (Claude Code / Cursor / Codex / DSH, etc.), letting AI search your files, remember your preferences, and read/write notes. Indexing belongs to the tool; understanding belongs to the Agent.

---

## Why Memex

| Pain Point | How Memex Solves It |
|------------|---------------------|
| Too many files, can't find anything | **Full-text inverted index + CJK bigram**, search by fragments, millisecond response |
| Keyword search not smart enough | **BM25 + semantic vector hybrid retrieval (RRF fusion)**, matches by meaning |
| PDF / image text not searchable | **pdfium for complex PDF + OCR for images** (optional) |
| Duplicate files wasting space | **SHA-256 dedup scan**, one-click cleanup |
| AI assistant doesn't know your files | **MCP protocol integration**, Agent searches your local knowledge base directly |
| AI assistant has no long-term memory | **KV memory + Markdown notes**, Agent can remember and recall |
| Don't want data in the cloud | **Fully local**, data never leaves your disk |

---

## Key Features

### 🔍 Hybrid Search
- **Keyword search**: bigram inverted index, no Chinese tokenizer needed, < 5ms response
- **Semantic search**: bge-small-zh-v1.5 vectors + BM25 weighting, RRF(k=60) fusion ranking
- **Similar documents**: cosine similarity-based, find files with "similar content"

### 📄 Multi-Format Parsing
txt / md / html / csv / json / docx / xlsx / pptx / pdf / image OCR — 12 formats, pure Rust, no external dependencies.

### 🏷️ Multi-Dimensional Classification
- Dimension + value tag system (e.g., `tech_stack=Rust`, `domain=traffic`)
- Confidence state machine: Agent reports confidence, ≥ 0.85 gets cache acceleration
- Duplicate detection: size → SHA-256 tiered scan, keeps shallowest path

### 🧠 AI Memory
- **KV memory**: `namespace + key` structured storage, Agent long-term memory
- **Markdown notes**: `data/notes/*.md`, indexed into search pool
- **13 MCP tools**: search, read, remember, notes, tagging, stats

### 🖥️ Desktop Experience
- WebView2 window / system tray / browser access — three modes
- Real-time file watching: incremental indexing auto-triggered, new files searchable in seconds
- Visual UI: global search, multi-dimensional classification, stats dashboard, memory management, settings
- Auto-update: Gitee primary + GitHub fallback

### 🔒 Fully Local
- No internet, no uploads, no account
- Data directory sits next to the exe, transparent and controllable
- Binds to `127.0.0.1` only, no external exposure

---

## Installation

### Windows
1. Download `Memex-Setup-x.x.x.exe`
2. Double-click to install, default path `C:\Memex\`
3. Launch from desktop shortcut or system tray

### macOS
1. Download `Memex-macOS-universal.tar.gz` (Intel + Apple Silicon universal)
2. Extract and drag `Memex.app` to `/Applications/`
3. First launch: right-click → Open (bypass Gatekeeper)

### Linux
1. Download `Memex-x86_64.AppImage`
2. `chmod +x Memex-x86_64.AppImage`
3. Double-click to run, or add to application menu

> All platform installers are available at [GitHub Releases](https://github.com/Johendeng/memex/releases).

---

## Quick Start

### 1. Add File Sources
Open Memex → "File Sources" in sidebar → click "Browse…" next to the input → select a folder → click "Add Source"

Supported file types:
| Category | Extensions |
|----------|------------|
| Documents | `.md` `.txt` `.html` `.csv` `.json` |
| Office | `.docx` `.xlsx` `.pptx` |
| PDF | `.pdf` (pdfium enhanced) |
| Code | `.rs` `.py` `.js` `.ts` `.go` `.java` `.c` `.cpp` `.sh` |
| Images | `.png` `.jpg` `.bmp` `.tiff` `.webp` (OCR) |

### 2. Search
- **Keyword search**: type a fragment, press Enter
- **Hybrid search**: enable "Semantic Search" toggle, finds files by meaning
- Results include highlighted snippets, file paths, tags

### 3. Connect to AI Agent

Add to your Agent's MCP config:

```json
{
  "mcpServers": {
    "memex": {
      "command": "C:/Memex/memex.exe",
      "args": ["mcp", "serve"]
    }
  }
}
```

Or HTTP mode (multi-Agent sharing, recommended for high-frequency use):

```json
{
  "mcpServers": {
    "memex-http": {
      "url": "http://127.0.0.1:18321/mcp",
      "transport": "http"
    }
  }
}
```

Once connected, your Agent can:
- `memex_search` — search your local files
- `memex_read` — read file content
- `memex_find_similar` — find similar documents
- `memex_remember / recall / forget` — store and retrieve memories
- `memex_write_note / read_note` — read and write notes
- `memex_tag` — tag files
- `memex_stats` — view knowledge base overview

---

## Use Cases

### 📚 Technical Document Library
Drop all your technical notes, PDF papers, and project docs into one folder. After indexing, search "traffic control" to find a plan from three years ago, or "RBAC permission design" to match a note buried in the text body.

### 🤖 Agent Memory Hub
Your Agent remembers preferences via MCP ("This user prefers concise responses" / "Project uses Rust 2021 edition"), and recalls them in the next conversation. Notes are stored as local Markdown files — you can view and edit them anytime.

### 🗂️ Duplicate File Cleanup
Scan years of accumulated folders, SHA-256 exact matching, list all duplicate copies, one-click cleanup.

---

## Command Line

```bash
memex info                              # Index overview
memex folders add <path>                # Add file source
memex index                             # Full/incremental index
memex search "keyword" [-l 20] [--hybrid] # Search (--hybrid for semantic)
memex embed                             # Generate vector index
memex serve [--open] [--no-window]      # Start service (window/tray/browser)
memex mcp serve                         # MCP stdio service
```

---

## Tech Stack

| Layer | Technology |
|-------|------------|
| Backend | Rust 2021, pure Rust index engine (bigram inverted + mmap postings) |
| Frontend | Vue 3 + Vite, embedded into exe at compile time (rust-embed) |
| Desktop | wry + tao + tray-icon |
| Semantic | fastembed + ONNX Runtime (bge-small-zh-v1.5) |
| PDF | pdfium-render (runtime binding) |
| OCR | rusto-rs / PP-OCRv6 |
| Protocol | MCP (JSON-RPC over stdio / HTTP) |

---

## For Developers

Build instructions, architecture, module responsibilities, and extension guides are in [CONTRIBUTING.md](./CONTRIBUTING.md).

```bash
# Frontend
cd frontend && npm install && npm run build

# Backend (basic)
cargo build --release

# Full (PDF + OCR)
cargo build --release --features "pdfium ocr"
```

---

## License

[Apache-2.0](./LICENSE) · Copyright 2026 JohenDeng

---

## Links

- **GitHub**: [Johendeng/memex](https://github.com/Johendeng/memex)
- **Gitee**: [JohenTeng/memex](https://gitee.com/JohenTeng/memex)
- **Releases**: [GitHub Releases](https://github.com/Johendeng/memex/releases)
