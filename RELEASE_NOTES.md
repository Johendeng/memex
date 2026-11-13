# v1.2.0 — 稳定性与性能优化 / Stability & Performance

> 修复「查找相似」卡死、搜索被索引阻塞、向量生成 CPU 占用过高等关键问题。

---

## 中文

### 🐛 Bug 修复

#### 「查找相似」点击后搜索卡死（关键）
- **三阶段锁拆分**：Phase 1 短暂锁获取文件分片列表 → Phase 2 完全无锁计算向量余弦相似度 → Phase 3 短暂锁解析元数据，彻底消除对搜索请求的阻塞
- **空向量 panic 修复**：当 `vectors[0]` 为空向量（dim=0）时 centroid 计算越界 panic 导致线程死亡，改用 `iter().find(|v| !v.is_empty())` 安全获取维度
- **前端 loading 状态**：点击后添加 `similarLoading` 指示器，避免空白页面

#### 搜索被索引阻塞
- 磁盘 I/O 移出写锁（`append_texts_to_disk` 无锁写入）
- 新增 `recover_read` 机制：try_read 失败后重试 60×50ms
- 统一锁顺序 idx→meta，消除死锁风险
- 验证：索引过程中搜索 0-2ms

#### 软件卡死（pdfium.dll 缺失）
- 缺失 pdfium.dll 时 panic 导致锁中毒，所有后续操作卡死
- 已通过 OnceLock 延迟初始化 + 友好错误提示修复

### ⚡ 性能优化

#### 语义向量生成 CPU 占用 80%+
- 进程级 `SetPriorityClass(BELOW_NORMAL_PRIORITY_CLASS)` via kernel32 FFI
- HTTP 线程恢复 `SetThreadPriority(THREAD_PRIORITY_NORMAL)`
- ONNX Runtime 32 线程全部继承 BelowNormal 优先级，不影响用户其他操作

### ✨ 新功能

#### OCR 结果缓存
- 索引重建时复用已有 OCR 结果，避免重复识别
- FNV hash 缓存键，除非 OCR 结果被删除否则不重复 OCR

#### 文件源移除同步清理
- 移除文件源时提醒将同步移除关联索引文件
- 用户确认后一并清理，避免残留无效索引

#### 后台索引进度可视化
- 页面底部 taskBar 实时展示后台索引/向量化进度
- 用户对后台操作有感知，不再"悄悄干"

#### 移除搜索栏向量生成按钮
- 索引和向量化步骤不再影响用户正常操作
- 全部转为后台非阻塞执行

### 📦 安装包

| 平台 | 文件 | 说明 |
|------|------|------|
| Windows | `Memex-Setup-1.2.0.exe` | Inno Setup 安装程序，x86_64 |
| macOS | `Memex-macOS-universal.tar.gz` | Universal Binary (Intel + Apple Silicon) |
| Linux | `Memex-x86_64.AppImage` | 免安装，一个文件通吃所有发行版 |

### 📝 许可

Apache-2.0 · Copyright 2026 JohenDeng

---

## English

### 🐛 Bug Fixes

#### "Find Similar" freezes search (critical)
- **Three-phase lock split**: Phase 1 briefly locks to get chunk list → Phase 2 fully lock-free cosine similarity computation → Phase 3 briefly locks for metadata, eliminating all blocking of search requests
- **Empty vector panic fix**: `vectors[0]` could be empty (dim=0) causing centroid index-out-of-bounds panic and thread death; now uses `iter().find(|v| !v.is_empty())` for safe dimension lookup
- **Frontend loading state**: added `similarLoading` indicator to avoid blank page after click

#### Search blocked by indexing
- Disk I/O moved out of write lock (`append_texts_to_disk` lock-free writes)
- Added `recover_read` mechanism: retries 60×50ms on try_read failure
- Unified lock ordering idx→meta, eliminating deadlock risk
- Verified: search during indexing completes in 0-2ms

#### App freeze (missing pdfium.dll)
- Missing pdfium.dll caused panic → lock poisoning → all subsequent operations frozen
- Fixed via OnceLock lazy initialization + friendly error message

### ⚡ Performance

#### Embedding generation CPU usage 80%+
- Process-level `SetPriorityClass(BELOW_NORMAL_PRIORITY_CLASS)` via kernel32 FFI
- HTTP threads restored to `SetThreadPriority(THREAD_PRIORITY_NORMAL)`
- ONNX Runtime 32 threads all inherit BelowNormal priority, no impact on user's other tasks

### ✨ New Features

#### OCR result caching
- Reuses existing OCR results on index rebuild, avoiding redundant recognition
- FNV hash cache keys; no repeat OCR unless results are deleted

#### File source removal cleanup
- Removing a file source now prompts that associated index files will be removed
- Cleanup executed after user confirmation, preventing stale index remnants

#### Background indexing progress visualization
- Bottom taskBar shows real-time indexing/embedding progress
- Users have visibility into background operations

#### Removed vector generation button from search bar
- Indexing and vectorization no longer interfere with normal operation
- All converted to background non-blocking execution

### 📦 Installers

| Platform | File | Notes |
|----------|------|-------|
| Windows | `Memex-Setup-1.2.0.exe` | Inno Setup installer, x86_64 |
| macOS | `Memex-macOS-universal.tar.gz` | Universal Binary (Intel + Apple Silicon) |
| Linux | `Memex-x86_64.AppImage` | No installation needed, single file for all distros |

### 📝 License

Apache-2.0 · Copyright 2026 JohenDeng

---

# v1.1.0 — 首个公开发布 / First Public Release

> Memex 本地知识库 + AI 记忆体，三平台首发。
>
> Memex — local knowledge base + AI memory. First release on all three platforms.

---

## 中文

### ✨ 核心功能

#### 混合检索引擎
- **BM25 倒排索引 + 中文 bigram**：无需分词，只言片语秒搜，毫秒级响应
- **语义向量检索**：bge-small-zh-v1.5 模型 + BM25 加权，RRF(k=60) 融合排序，意思相近也能匹配
- **相似文档推荐**：基于向量余弦相似度查找内容相近的文件
- **mmap postings + delta 增量索引**：内存占用从 2.7GB 降至 139MB，支持 18500+ 文件 / 713K chunks

#### 多格式解析（12 种）
- 文档：txt / md / html / csv / json
- Office：docx / xlsx / pptx
- PDF：pdfium-render 增强解析（复杂排版、表格、图片 PDF）
- 代码：rs / py / js / ts / go / java / c / cpp / sh
- 图片 OCR：rusto-rs + PP-OCRv6 Small（png / jpg / bmp / tiff / webp）

#### 多维分类与治理
- 维度+值标签体系（如 `技术栈=Rust`、`领域=交通`）
- 可信度状态机：Agent 标记可信度，≥ 0.85 走缓存加速
- **SHA-256 重复文件检测**：size → hash 分级扫描，保留最浅路径，一键清理冗余

#### AI 记忆体（MCP 协议）
- **13 个 MCP 工具**：搜索、阅读、相似文档、记忆(KV)、笔记(Markdown)、打标、统计
- **KV 记忆**：namespace + key 结构化存储，Agent 长期记忆
- **Markdown 笔记**：`data/notes/*.md`，可被索引进检索池
- **stdio + HTTP 双模式**：HTTP 模式支持多 Agent 共享同一知识库
- **记忆可视化界面**：浏览器内管理 Agent 记忆和笔记

#### 桌面体验
- **三平台原生应用**：Windows (Inno Setup) / macOS (Universal Binary) / Linux (AppImage)
- WebView2 窗口 / 系统托盘常驻 / 浏览器访问，三种使用模式
- **实时文件监听**：增量索引自动触发，新增文件秒级可搜
- 可视化界面：全局搜索、多维分类、统计面板、记忆管理、设置
- **文件夹浏览按钮**：原生文件选择器(rfd)，一键添加文件源
- MD 文件预览/编辑面板
- 数据导出/迁移：支持索引数据备份和迁移
- **自动更新**：Gitee 主源 + GitHub 备源双线路
- **OCR 进度展示**：页面底部实时显示 OCR 构建进度
- **OCR 并发限制**：限制 2 线程，避免过度占用 CPU/内存

#### 纯本地 & 隐私
- 不联网，不上传，不需要账号
- 数据目录在 exe 旁，透明可控（`MEMEX_HOME` 可覆盖）
- 仅绑定 `127.0.0.1`，不对外暴露

### 📦 安装包

| 平台 | 文件 | 说明 |
|------|------|------|
| Windows | `Memex-Setup-1.1.0.exe` | Inno Setup 安装程序，x86_64 |
| macOS | `Memex-macOS-universal.tar.gz` | Universal Binary (Intel + Apple Silicon) |
| Linux | `Memex-x86_64.AppImage` | 免安装，一个文件通吃所有发行版 |

### 🔧 技术栈

- **后端**：Rust 2021（bigram 倒排索引 + mmap postings + fastembed + ONNX Runtime）
- **前端**：Vue 3 + Vite（rust-embed 编译期嵌入 exe）
- **桌面**：wry + tao + tray-icon
- **PDF**：pdfium-render（运行时绑定）
- **OCR**：rusto-rs / PP-OCRv6 Small 非量化
- **协议**：MCP (JSON-RPC over stdio / HTTP)
- **CI**：GitHub Actions 三平台矩阵构建

### 📝 许可

Apache-2.0 · Copyright 2026 JohenDeng

---

## English

### ✨ Core Features

#### Hybrid Search Engine
- **BM25 inverted index + CJK bigram**: no tokenizer needed, search by fragments, millisecond response
- **Semantic vector search**: bge-small-zh-v1.5 model + BM25 weighting, RRF(k=60) fusion ranking, matches by meaning
- **Similar document recommendation**: cosine similarity-based, finds files with related content
- **mmap postings + delta incremental indexing**: memory footprint reduced from 2.7 GB to 139 MB, supports 18,500+ files / 713K chunks

#### Multi-Format Parsing (12 formats)
- Documents: txt / md / html / csv / json
- Office: docx / xlsx / pptx
- PDF: pdfium-render enhanced parsing (complex layouts, tables, image-based PDFs)
- Code: rs / py / js / ts / go / java / c / cpp / sh
- Image OCR: rusto-rs + PP-OCRv6 Small (png / jpg / bmp / tiff / webp)

#### Multi-Dimensional Classification & Governance
- Dimension + value tag system (e.g., `tech_stack=Rust`, `domain=traffic`)
- Confidence state machine: Agent reports confidence, ≥ 0.85 gets cache acceleration
- **SHA-256 duplicate file detection**: size → hash tiered scan, keeps shallowest path, one-click cleanup

#### AI Memory (MCP Protocol)
- **13 MCP tools**: search, read, similar docs, memory (KV), notes (Markdown), tagging, stats
- **KV memory**: namespace + key structured storage, Agent long-term memory
- **Markdown notes**: `data/notes/*.md`, indexed into the search pool
- **stdio + HTTP dual mode**: HTTP mode allows multiple Agents to share one knowledge base
- **Memory visualization UI**: manage Agent memories and notes in the browser

#### Desktop Experience
- **Three-platform native apps**: Windows (Inno Setup) / macOS (Universal Binary) / Linux (AppImage)
- WebView2 window / system tray / browser access — three usage modes
- **Real-time file watching**: incremental indexing auto-triggered, new files searchable in seconds
- Visual UI: global search, multi-dimensional classification, stats dashboard, memory management, settings
- **Folder browse button**: native file picker (rfd), one-click to add file sources
- Markdown preview/edit panel
- Data export/migration: index data backup and migration support
- **Auto-update**: Gitee primary + GitHub fallback dual-channel
- **OCR progress display**: real-time OCR build progress at the bottom of the page
- **OCR concurrency limit**: capped at 2 threads to avoid excessive CPU/memory usage

#### Fully Local & Privacy
- No internet, no uploads, no account required
- Data directory sits next to the exe, transparent and controllable (`MEMEX_HOME` override supported)
- Binds to `127.0.0.1` only, no external exposure

### 📦 Installers

| Platform | File | Notes |
|----------|------|-------|
| Windows | `Memex-Setup-1.1.0.exe` | Inno Setup installer, x86_64 |
| macOS | `Memex-macOS-universal.tar.gz` | Universal Binary (Intel + Apple Silicon) |
| Linux | `Memex-x86_64.AppImage` | No installation needed, single file for all distros |

### 🔧 Tech Stack

- **Backend**: Rust 2021 (bigram inverted index + mmap postings + fastembed + ONNX Runtime)
- **Frontend**: Vue 3 + Vite (embedded into exe at compile time via rust-embed)
- **Desktop**: wry + tao + tray-icon
- **PDF**: pdfium-render (runtime binding)
- **OCR**: rusto-rs / PP-OCRv6 Small non-quantized
- **Protocol**: MCP (JSON-RPC over stdio / HTTP)
- **CI**: GitHub Actions three-platform matrix build

### 📝 License

Apache-2.0 · Copyright 2026 JohenDeng
