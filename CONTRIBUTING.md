# Contributing to Memex

> 架构、构建、模块职责、扩展指南等开发者文档。面向使用者的内容见 [README.md](./README.md)。

---


> **本地知识库索引 + AI 记忆体。索引归工具，理解归 Agent。**

[![version](https://img.shields.io/badge/version-1.1.0-blue)]() [![license](https://img.shields.io/badge/license-Apache--2.0-green)](./LICENSE) [![rust](https://img.shields.io/badge/Rust-2021-orange)]() [![vue](https://img.shields.io/badge/Vue-3-42b883)]()

Memex 不接大模型。它只做一件事：把你散落的本地文件建成**全文倒排 + 内容向量**索引，再通过 **MCP** 暴露给本地 Agent（Claude Code / Cursor / Codex / DSH …）。语义理解交给 Agent，定位与检索归工具。

---

## 0. 一分钟了解

| | |
|---|---|
| **是什么** | 单进程的本地文档索引/检索服务 + 轻量记忆体（KV 记忆 + Markdown 笔记），对外只讲 MCP |
| **不是什么** | 不是聊天客户端、不是 RAG 服务、不联网、不做 Embedding 以外的推理、没有数据库和账号体系 |
| **给谁用** | ① 想让 Agent 检索自己硬盘的开发者 ② 想把"记忆/笔记"落成本地文件的 Agent 宿主 |
| **怎么接** | `memex.exe mcp serve`（stdio）或 `POST http://127.0.0.1:18321/mcp`（HTTP），13 个工具 |
| **怎么开发** | Rust 2021 + Vue 3 / Vite，前端编译期嵌入 exe，`rust-embed` 零磁盘依赖 |

> 本文档面向**接入方**和**贡献者**。只想装来用的话，看 [§5.6 安装包](#56-安装包) 即可。

---

## 1. 当前能力

### 1.1 能力矩阵

| 能力 | 说明 | 对外入口 | 代码 |
|---|---|---|---|
| 文件夹源 | 多根目录，路径级增删 | `folders *` / MCP 隐式 | [src/store.rs](src/store.rs) |
| 全局过滤 | 扩展名白/黑名单、大小上限、glob 排除、隐藏/系统文件 | `filter show\|reset` | [src/filter.rs](src/filter.rs) |
| 文档解析 | txt / md / html / csv / json / rst / org / docx / xlsx / pptx / pdf，图片 OCR（可选） | — | [src/parser.rs](src/parser.rs) |
| 分块建索引 | 500 字 + 80 字重叠；bigram 倒排；`postings` 走 mmap | `index` / `rebuild-postings` | [src/indexer.rs](src/indexer.rs) |
| 增量索引 | mtime+size 指纹跳过；`notify` 监听自动增量；两阶段（标题秒级 → 内容后台） | `cmd_index_run` | [src/indexer.rs](src/indexer.rs) |
| 关键词检索 | bigram 计分 + 标题加权 + ASCII 全串后过滤，< 5ms 量级 | `search` | [src/search.rs](src/search.rs) |
| 语义检索 | bge-small-zh-v1.5（512 维）+ BM25 + 余弦 → RRF(k=60) | `search --hybrid` / `embed` | [src/vector.rs](src/vector.rs)、[src/embed.rs](src/embed.rs) |
| 相似文档 | 文件级 chunk 质心 → 余弦 top-K | `memex_find_similar` | [src/vector.rs](src/vector.rs) |
| 多维标签 | 维度 + 值，同维度多值并存并提示冲突 | `memex_tag` / UI 分类页 | [src/mcp.rs:435-484](src/mcp.rs#L435-L484) |
| 可信度状态机 | `unverified → verified/below → reverify`（源变更自动降级），阈值 0.85 | `memex_report_confidence` | [src/mcp.rs:488-535](src/mcp.rs#L488-L535) |
| 重复文件治理 | size → SHA-256 分组，保留最浅路径，删除其余并打标 | `cmd_dedup_scan` / `cmd_dedup_clean` | [src/dedup.rs](src/dedup.rs) |
| KV 记忆 | `namespace + key` upsert，纯 JSON 文件 | `memex_remember/recall/forget` | [src/memories.rs](src/memories.rs) |
| Markdown 笔记 | `data/notes/<title>.md`，可被监听器索引进检索 | `memex_write_note` 等 4 个工具 | [src/memories.rs](src/memories.rs) |
| 数据导出 | 元数据 JSON、文件清单 CSV、标签回灌 | `cmd_export*` / `cmd_import_tags` | [src/httpd.rs:1105-1169](src/httpd.rs#L1105-L1169) |
| 桌面壳 | WebView2 窗口 / 托盘常驻 / 浏览器模式 | `serve [--open\|--no-window]` | [src/window.rs](src/window.rs) |
| 自动更新 | Gitee 优先、GitHub 兜底，下载后拉起安装包 | `cmd_check_update` 等 | [src/update.rs](src/update.rs) |

### 1.2 明确没做的（别指望）

- ❌ 不做 `resources` / `prompts` / `logging` / `ping` 等 MCP 能力，只实现 `tools`
- ❌ 不做 MCP 进度通知与取消（长任务靠轮询 `cmd_*_progress`）
- ❌ 不引入 Tantivy / SQLite（历史上规划过，见 §2.2，最终选了纯 Rust bigram）
- ❌ 不做多用户、鉴权、远端访问（仅绑 `127.0.0.1`）
- ❌ 不做增量向量重算（`embed` 目前是整批重算）

---

## 2. 整体规划

### 2.1 设计原则（改代码前请先认同这四条）

1. **索引归工具，理解归 Agent** —— 不在进程内调用任何 LLM，不猜用户意图。
2. **纯 Rust 优先，不引 C 工具链** —— 宁可手写 HTTP、手写 JSON-RPC，也不为省事引入重依赖（[Cargo.toml:16-17](Cargo.toml#L16-L17)）。
3. **内存红线** —— chunk 正文外置到 `chunks.text`、postings 走 mmap，常驻从 ~1.1GB 压到几十 MB（[src/store.rs:7-9](src/store.rs#L7-L9)、[src/store.rs:228-231](src/store.rs#L228-L231)）。
4. **协议最小化** —— MCP 只实现 4 个方法；宁可少，不要半吊子兼容。

### 2.2 里程碑（从代码注释复原）

> ⚠️ 原始设计文档 `docs/tech-design.md` 被 [.gitignore:31](.gitignore#L31) 忽略，**未入库**。下表由源码注释（`M1`–`M5`、`Plan B`、`tech-design §x`）复原，可能与最初计划有出入。

| 里程碑 | 目标 | 实际落地 | 状态 |
|---|---|---|---|
| **M1** 采集与索引 | 文件夹源、全局过滤、SQLite + FTS5 trigram | 改道**纯 Rust bigram 倒排**（避开 C 工具链），CJK 只言片语场景更稳 | ✅ 已替代 |
| **M2** 文档解析 | docx / xlsx / pdf | zip+XML 手解 docx/xlsx/pptx，pdf-extract；后加 pdfium（feature） | ✅ |
| **M3** 语义检索 | Tantivy + 向量混合 | Tantivy 未引入；落地 bge-small-zh + BM25 + RRF，向量暴力 top-K | ✅（实现路径变更） |
| **M4** 知识治理 | 标签 + 置信度 | 维度/值标签、四态状态机、verified 走缓存文本 | ✅ |
| **M5** 记忆体 | Agent 长期记忆 | KV `memories.json` + `notes/*.md`，配套 7 个工具 | ✅ |
| **Plan B / B+** 内存优化 | 把正文与 postings 挪出 `index.bin` | v3 格式：`chunks.text` + `postings.bin` + mmap + delta 增量 | ✅ |
| **1.0.x** 产品化 | 可安装、可自更新 | 托盘/窗口、Inno Setup、Gitee/GitHub 自动更新、去重、OCR | ✅ 当前 |

### 2.3 下一步候选（未排期，欢迎认领）

| 方向 | 动机 | 建议入手点 |
|---|---|---|
| MCP 长任务进度 | 目前只能自定义轮询，标准客户端看不见 | `tools/call` 期间发 `notifications/progress` |
| `/mcp` 补齐 streamable-HTTP | 见 §7.4 的 3 条兼容性问题 | [src/httpd.rs:141-157](src/httpd.rs#L141-L157) |
| 索引常驻（消除 stdio 每次重载） | stdio 模式每次调用重载 ~1GB 索引 | [src/mcp.rs:330-336](src/mcp.rs#L330-L336) |
| 增量向量 | 新增文件要全量 `embed` | [src/embed.rs](src/embed.rs) |
| 跨平台打包 | 代码已 `cfg(windows)` 分支，缺 macOS/Linux 产物 | [src/window.rs](src/window.rs) |
| 恢复 `docs/tech-design.md` | 当前设计意图只能从注释反推 | 新建 `docs/` 并在 .gitignore 放行 |

---

## 3. 架构

### 3.1 进程与线程模型

单进程。`serve` 模式下的线程分工：

```
main thread ── wry/tao 事件循环（窗口模式） 或 阻塞等待（tray/浏览器模式）
  ├─ http thread       TcpListener accept → 每连接 spawn 一个线程（Connection: close）
  ├─ watcher thread    notify → 增量索引，置 META_DIRTY 标记
  ├─ index bg thread   Phase 2 内容解析（标题 Phase 1 同步完成）
  └─ embed thread      cmd_embed 触发，进度写原子变量
```

共享状态就两个：`Arc<Mutex<Meta>>`（JSON 元数据）与 `Arc<RwLock<BigramIndex>>`（索引）。`META_DIRTY` 为真时，下一次 API 调用先重载 `meta.json`（[src/httpd.rs:243-250](src/httpd.rs#L243-L250)）。

### 3.2 数据流

```
索引：  文件夹 ─walkdir+filter→ 解析文本 ─切块→ bigram 倒排 ──→ data/
                                        └─(可选)→ ONNX 512d 向量 ──→ vectors.bin

检索：  query ─┬→ bigram 倒排查分 → 标题加权 → 后过滤 ─┐
               └→ BM25 分 ─┬─ RRF(k=60) ──→ top-K 命中 ─→ 拼 snippet/tag/confidence
                           │
        query ─→ embedding ─→ 余弦 top-K ─┘

记忆：  Agent ─MCP tools_call→ dispatch ─┬→ memories.json（KV upsert）
                                       └→ notes/<title>.md（Markdown）
```

### 3.3 模块职责

| 模块 | 职责 | 备注 |
|---|---|---|
| [src/main.rs](src/main.rs) | clap CLI、模式分派、单实例锁 | `mod` 声明与 `lib.rs` 重复，见 §9 |
| [src/lib.rs](src/lib.rs) | 库入口（模块导出） | 二进制实际不依赖 lib target |
| [src/config.rs](src/config.rs) | `MEMEX_HOME` → exe 目录 → `%LOCALAPPDATA%\Memex` 解析链 | |
| [src/store.rs](src/store.rs) | `Meta`、`BigramIndex`（v1/v2/v3 兼容加载）、保存 | 索引导出/迁移都在这 |
| [src/filter.rs](src/filter.rs) | `FilterRules` + globset 编译 + `accepts()` | 默认白名单见源码 |
| [src/parser.rs](src/parser.rs) | 按扩展名分发解析，纯 Rust | docx/xlsx/pptx = zip+XML |
| [src/indexer.rs](src/indexer.rs) | 两阶段索引、增量、watcher、进度原子量 | 核心 770 行 |
| [src/search.rs](src/search.rs) | bigram 计分、标题 boost、ASCII 后过滤 | |
| [src/vector.rs](src/vector.rs) | 向量存取、余弦、BM25、RRF 融合 | |
| [src/embed.rs](src/embed.rs) | fastembed + ort（`load-dynamic`）封装 | 模型手动放置 |
| [src/mcp.rs](src/mcp.rs) | MCP JSON-RPC、13 个工具、stdio 循环 | stdio / HTTP 共用 |
| [src/httpd.rs](src/httpd.rs) | 手写 HTTP/1.1、`/api` 命令总线、`/mcp`、静态资源 | 无 Web 框架 |
| [src/memories.rs](src/memories.rs) | KV 记忆 + Markdown 笔记 | |
| [src/dedup.rs](src/dedup.rs) | SHA-256 去重扫描与清理 | |
| [src/ocr.rs](src/ocr.rs) | rusTO PaddleOCR PP-OCRv6（`ocr` feature） | RTen 限 2 线程 |
| [src/window.rs](src/window.rs) | wry + tao + tray-icon 三种 UI 模式 | |
| [src/update.rs](src/update.rs) | 版本检查、下载、拉起安装包 | |
| [src/single_instance.rs](src/single_instance.rs) | `{data}/memex.lock` + PID 探活 | 同 data 目录只允许一个 `serve` |
| [src/applog.rs](src/applog.rs) | 追加日志到 `{data}/memex.log` | GUI 无控制台时排障用 |

### 3.4 目录结构

```
memex/
├─ Cargo.toml                  # 单 crate，19 个模块，features: pdfium / ocr
├─ build.rs                    # 嵌图标 + 拷 WebView2Loader.dll 到 target/<profile>/
├─ src/                        # 见 §3.3
├─ frontend/
│  ├─ package.json             # Vue 3 + Vite + vue-router + marked
│  ├─ vite.config.js           # dev:5173 / build→dist
│  └─ src/
│     ├─ main.js               # 5 条路由（hash 模式）
│     ├─ api.js                # 唯一桥：POST /api {cmd,args}
│     ├─ components/           # HelpTour、MarkdownPreview
│     └─ views/                 # Search / Classification / Stats / Memories / Settings / Verified / FolderSources
├─ frontend/dist/              # 前端构建产物（rust-embed 编译期嵌入，gitignore）
├─ assets/                     # 图标源（png/ico）+ gen_icon.py
├─ scripts/download-assets.sh  # 下载 pdfium.dll + PP-OCRv6 模型
├─ installer/                  # Inno Setup 脚本（memex.iss）+ launcher.bat + README.txt
├─ landing/                    # 官网落地页（静态 HTML + 截图）
├─ models/                     # 模型（gitignore）：bge-small-zh-v1.5/、ocr/
├─ data/                       # 运行时数据（gitignore），见 §4
├─ start.bat                   # 双击启动（serve --open）
└─ README.md
```

---

## 4. 数据布局

根目录 `MEMEX_HOME`（默认 = exe 所在目录，安装包落在 `C:\Memex\`）：

| 文件 | 内容 | 格式 | 备注 |
|---|---|---|---|
| `data/meta.json` | 文件夹、文件、标签、置信度、过滤规则、去重计数 | JSON | 人类可读，可直接看/改 |
| `data/index.bin` | chunk 元数据 + 正文偏移表 | bincode，首字节 `0xFC`(v3) / `0xFD`(v2) / 其它(v1) | **不含正文** |
| `data/chunks.text` | 所有 chunk 正文，按偏移顺序拼接 | 纯文本 | dead chunk = 零长度 |
| `data/postings.bin` | bigram 倒排原文 | 二进制 | serve 时 mmap |
| `data/postings.idx` | `bigram → (offset, count)` | bincode | |
| `data/postings.delta` | watcher 增量倒排 | bincode | 下次全量索引时合并 |
| `data/vectors.bin` | chunk 向量，与 `chunks` 下标对齐 | bincode `Vec<Vec<f32>>` | 512 维，空 = 未 embed |
| `data/memories.json` | KV 记忆 | JSON | |
| `data/notes/*.md` | Agent 笔记 | Markdown | 文件名 = 标题 |
| `data/memex.lock` | 单实例 PID | 4 字节 BE | 进程退出即删 |
| `data/memex.log` | 运行日志 | 文本 | 无控制台时唯一线索 |
| `models/bge-small-zh-v1.5/` | `onnx/model.onnx` + tokenizer 等 4 个 json | — | 见 §5.4 |
| `models/ocr/` | `det.rten` / `rec.rten` / `dict.txt` | — | 仅 `ocr` feature |

**关键不变量**（改索引代码务必维持）：

1. `text_offsets.len() == chunks.len() + 1`；
2. `vectors.len() == chunks.len()`，未嵌入的 chunk 用空 `Vec` 占位；
3. 删除/替换 chunk 时把 `text_offsets[i] == text_offsets[i+1]` 置零长度，**不要**重排下标（向量按下标对齐）。

---

## 5. 构建

### 5.1 前置

| 依赖 | 版本 | 用途 |
|---|---|---|
| Rust | stable（MSVC toolchain，edition 2021） | 后端 |
| Node.js | ≥ 18 | 前端构建 |
| WebView2 Runtime | Win10/11 一般自带 | 窗口模式 |
| Inno Setup 6 | 可选 | 打安装包 |
| bash | 可选（Git Bash / WSL） | `scripts/download-assets.sh` |

### 5.2 三步构建（顺序不能反）

```bash
# 1) 前端 —— 必须先于 cargo build，rust-embed 在编译期嵌入 frontend/dist/
cd frontend && npm install && npm run build && cd ..

# 2) 后端（基础版：无 OCR、无 pdfium）
cargo build --release

# 3) 可选：带 PDF 增强 + 图片 OCR
cargo build --release --features "pdfium ocr"
```

产物：`target/release/memex.exe`（+ 由 `build.rs` 自动拷入的 `WebView2Loader.dll`）。

> 当前**打包链路只覆盖 Windows x64**（Inno Setup + WebView2）。Linux/macOS 的 `cfg` 分支代码是齐的，可以编译跑 `serve`/`mcp`，但没有安装包与发布产物。

> 想跑完整能力，2) 请用带 features 的那条；否则图片扩展名分支与 OCR 相关代码不会被编译进来（[src/ocr.rs:23](src/ocr.rs#L23) 整个模块是 `#![cfg(feature = "ocr")]`）。

### 5.3 Feature 开关

| Feature | 默认 | 带来什么 | 额外资产 |
|---|---|---|---|
| `pdfium` | off | 复杂 PDF 文本提取（`pdfium-render` 运行时绑定） | `pdfium.dll` 与 exe 同目录 |
| `ocr` | off | 图片文字识别（`rusto-rs` / RTen / PP-OCRv6） | `models/ocr/{det,rec}.rten + dict.txt` |

### 5.4 外部资产（都不在仓库里）

**① 嵌入模型**（约 91MB，语义检索必需）

```
models/bge-small-zh-v1.5/
├─ onnx/model.onnx
├─ tokenizer.json
├─ config.json
├─ special_tokens_map.json
└─ tokenizer_config.json
```

按 [src/embed.rs:12-20](src/embed.rs#L12-L20) 的 hf-mirror 脚本下载即可；路径由 `config::bge_model_dir()` 固定，**不读环境变量**。

**② `onnxruntime.dll`**（语义检索必需，与 `ort` 版本匹配）

必与 `memex.exe` 同目录（`fastembed` 用 `ort-load-dynamic` 运行时加载）。当前仓库**没有**自动下载脚本，需从 ONNX Runtime 官方 release 取对应版本手工放入 `target/release/`。缺它时：检索仍可用（关键词模式），`embed`/hybrid 会失败。

**③ pdfium + OCR 模型**

```bash
bash scripts/download-assets.sh   # 下载 pdfium.dll → target/release/，模型 → models/ocr/
```

### 5.5 开发模式（前端热更）

```bash
# 终端 A：后端（tray-only，不弹窗口，改动后重启）
cargo run -- serve --no-window

# 终端 B：前端
cd frontend && npm run dev
# 打开 http://localhost:5173
```

`frontend/src/api.js` 在 5173 端口下自动把请求指向 `http://127.0.0.1:18321`（[frontend/src/api.js:11](frontend/src/api.js#L11)），所以前后端可以分开跑。注意 **Rust 改动要重启**；`rust-embed` 的 `debug-embed` 只影响静态资源的读取来源，不做 HMR。

### 5.6 安装包

```bash
# 1) 按 §5.2 构建，2) 备好 target/release/ 下的 DLL 与 models/
ISCC.exe installer/memex.iss
# → installer/Output/Memex-Setup-1.1.0.exe
```

安装到 `C:\Memex\`，可选桌面快捷方式 / 开机自启（`serve --no-window`）。运行时**不写注册表**，只占一个 HKLM 卸载项（[installer/memex.iss:12-13](installer/memex.iss#L12-L13)）。

### 5.7 命令行速查

```bash
memex info                                  # 索引概况
memex folders add <path> | list | remove <id>
memex filter show | reset
memex index                                 # 全量/增量重建
memex rebuild-postings                      # 仅用 chunks.text 重建 postings
memex embed                                 # 生成/更新向量（整批）
memex search "关键词" [-l 20] [--hybrid]
memex read <file_id>                        # 打印解析后的全文
memex mcp serve                             # MCP stdio 服务（注意子命令）
memex serve [--port 18321] [--open] [--no-window]
```

无子命令直接跑 `memex` 会弹模式选择窗（桌面快捷方式路径，[src/main.rs:122-128](src/main.rs#L122-L128)）。

### 5.8 冒烟验证

仓库**没有自动化测试**（无 `tests/`、无 `#[test]`），改完请按下面四条手工过一遍：

```bash
# 1) 索引链路
memex folders add /path/to/docs && memex index
memex search "关键词" -l 3

# 2) 语义检索（需 §5.4 的模型 + onnxruntime.dll）
memex embed && memex search "语义查询" --hybrid

# 3) 服务 + API（另开终端）
memex serve --no-window
curl -s -X POST http://127.0.0.1:18321/api -H "Content-Type: application/json" -d '{"cmd":"cmd_info"}'

# 4) MCP 握手
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | memex mcp serve
```

---

## 6. 接口与协议

Memex 有两个接口层：**对外 MCP**（给 Agent / 宿主）和**对内 `/api`**（给自带前端）。二者刻意不统一。

### 6.1 MCP（对外）

传输两种，共用同一 `handle_request()`（[src/mcp.rs:53-84](src/mcp.rs#L53-L84)）：

| 传输 | 启动方式 | 端点 | 帧格式 |
|---|---|---|---|
| stdio | `memex.exe mcp serve` | stdin/stdout | **NDJSON，一行一条 JSON-RPC**（无 `Content-Length`） |
| HTTP | `memex.exe serve` 后 | `POST http://127.0.0.1:<port>/mcp` | 一问一答 `application/json`；通知回 `202` |

**方法集（只有 4 个）**

```
initialize / notifications/initialized / tools/list / tools/call
protocolVersion = "2024-11-05"，capabilities 仅声明 tools
其它方法 → error -32601
```

**工具清单（13 个）**

| 分组 | 工具 | 一句话 |
|---|---|---|
| 检索 | `memex_search` | 关键词 + 可选 hybrid（BM25+向量+RRF），返回路径/片段/标签/可信度 |
| 检索 | `memex_read` | verified 文件直接回缓存正文，否则回源路径让 Agent 自读 |
| 检索 | `memex_find_similar` | 基于向量余弦的相似文档 |
| 记忆 | `memex_remember` / `memex_recall` / `memex_forget` | KV 记忆（namespace 隔离，同 key 覆盖） |
| 笔记 | `memex_write_note` / `list_notes` / `read_note` / `delete_note` | `data/notes/*.md` |
| 治理 | `memex_tag` | 维度+值打标，同维度冲突会回 `conflict` 提示 |
| 治理 | `memex_report_confidence` | 0–1 分，≥0.85 记 verified |
| 治理 | `memex_stats` | 文件/chunk/bigram/向量/标签/笔记/记忆统计 |

工具 `description` 里写明了**何时该调、何时不该调**（[src/mcp.rs:105-262](src/mcp.rs#L105-L262)），这是协议的一部分，接入时建议原样保留给模型看。

**返回形状**（注意是两级编码）

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "result": {
    "content": [{ "type": "text", "text": "{\"results\":[ ... ]}" }],
    "isError": false
  }
}
```

失败时走 `isError: true` + 文本内 `{"error": "..."}`，**不用** JSON-RPC 的 `error` 字段（[src/mcp.rs:294-309](src/mcp.rs#L294-L309)）。

**一次 stdio 会话**

```jsonl
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}
{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"memex","version":"1.1.0"}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/list"}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"memex_search","arguments":{"query":"流量","hybrid":true,"limit":5}}}
```

### 6.2 `/api` 命令总线（对内）

> 这是前端私有通道，**不承诺稳定**，第三方接入请走 MCP。

```http
POST /api
Content-Type: application/json

{"cmd": "cmd_search", "args": {"query": "流量", "limit": 20, "hybrid": true}}
```

成功 `200 + 结果 JSON`；未知名返回 `{"error":"unknown cmd: xxx"}`（仍为 200），内部错误 `500`。共 **41 个命令**，覆盖 info / folders / filter / index / search / embed / tags / confidence / dedup / notes / memories / export / update / 文件预览。

### 6.3 前端桥

[frontend/src/api.js](frontend/src/api.js) 只有一个 `call(cmd, args)`：优先 `window.__TAURI__.invoke`（历史预留），否则 `fetch('/api')`。新增 UI 功能时，**先加 `cmd_*`，再在 `api.js` 加方法，最后在 view 里调**。

---

## 7. 接入指南

### 7.1 stdio 接入（先验证这个）

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

> ⚠️ `args` 必须是 `["mcp", "serve"]`。写成 `["mcp"]` 会因 clap 缺少必填子命令直接退出（[src/main.rs:69-72](src/main.rs#L69-L72)）。

### 7.2 HTTP 接入（多 Agent 共享、Memex 常驻时用）

```json
{
  "mcpServers": {
    "memex-http": { "url": "http://127.0.0.1:18321/mcp", "transport": "http" }
  }
}
```

两种模式的真实差别：

| | stdio | HTTP |
|---|---|---|
| 索引加载 | **每次 `tools/call` 重新 `BigramIndex::load()`**（[src/mcp.rs:330-336](src/mcp.rs#L330-L336)） | 进程内共享，零加载 |
| 前置条件 | 无 | `memex serve` 必须常驻 |
| 适合 | 单 Agent、偶发调用 | 高频、多 Agent |

> 索引体量上去以后，每次调用都要把整个索引从磁盘读回内存（源码注释提到过 976MB 的 `index.bin`，[src/mcp.rs:52](src/mcp.rs#L52)），stdio 单次调用会到秒级。**高频场景请用 HTTP，或先做 §2.3 的"索引常驻"。**

### 7.3 接入 DSH（DeepSeek Harness）

DSH 走插件体系，宿主自带 `@deepseek-ai/dsh-mcp-client`，**一个只有配置的 bundle 就能挂 MCP server**，工具会变成 `mcp__<serverName>__<tool>`：

```yaml
# cordis.patch.yml
- insert:
    - id: memex-mcp
      name: '@deepseek-ai/dsh-mcp-client'
      config:
        serverName: memex
        transport: streamable-http
        url: http://127.0.0.1:18321/mcp
        failOnStartupError: true
```

`package.json` 只需 `dsh.bundle.patch` 指向该文件，再通过 `plugin_manager` 的 `install_bundle` 装进当前 profile。**此为参考配置，接入前请先读 §7.4。**

长期看，优于 MCP 的做法是写**原生 DSH 插件**（TS，直接消费 `/api`）：可拿到结构化返回、纳入宿主的权限/审批体系、支持进度上报，并省掉双层 JSON 编码的 token 开销。

### 7.4 兼容性注意（接第三方 MCP 客户端前必读）

1. **`GET /mcp` 会落到 SPA fallback**，返回 `index.html` 而不是 SSE 流（[src/httpd.rs:160-192](src/httpd.rs#L160-L192)）。严格按 streamable-HTTP 规范实现、会开 SSE 或发 `DELETE` 关会话的客户端可能报错。
2. **不返回 `Mcp-Session-Id`**，不支持会话续传。
3. **通知一律 `202`**，无 `Content-Length` 以外的元数据。
4. **无鉴权、CORS `*`**，安全性完全依赖绑定 `127.0.0.1`；不要把端口暴露到局域网。
5. **单实例锁**：同一 `data/` 只能有一个 `serve`（[src/single_instance.rs](src/single_instance.rs)），第二个进程会打印占用者的 PID 后退出。
6. **返回是 JSON 字符串套 JSON**，客户端/模型需要二次 `JSON.parse`；写适配层时记得处理。

---

## 8. 扩展指南

### 8.1 加一个 MCP 工具

三处，缺一不可：

1. [src/mcp.rs:105](src/mcp.rs#L105) `handle_tools_list()` —— 加 schema 与 `description`（把"何时调用"写清）；
2. [src/mcp.rs:273-292](src/mcp.rs#L273-L292) `handle_tools_call()` —— 加 `match` 分支；
3. 实现 `fn tool_xxx(args: &Value) -> Result<String, String>`，返回**已序列化的 JSON 字符串**。

若前端也要用 → 再到 [src/httpd.rs](src/httpd.rs) 加 `cmd_*` 分支 + [frontend/src/api.js](frontend/src/api.js) 加方法。

### 8.2 加一种文档格式

[src/parser.rs:14-37](src/parser.rs#L14-L37) 的 `match ext` 加分支 → [src/filter.rs:22-32](src/filter.rs#L22-L32) 的 `include_exts` 默认值加扩展名 → 重跑 `memex index`。纯文本类可直接复用 `_` 分支。

### 8.3 加一个页面

`frontend/src/views/Xxx.vue` → [frontend/src/main.js:6](frontend/src/main.js#L6) 加路由 → [frontend/src/App.vue:44](frontend/src/App.vue#L44) 的 `nav` 加条目 → （如需数据）按 §8.4 加命令 → `npm run build` 让 `rust-embed` 重新嵌入。

### 8.4 加一条 `/api` 命令

在 [src/httpd.rs](src/httpd.rs) 的 `dispatch()` 里加 `"cmd_xxx" => { ... }` 分支。**注意锁顺序**：先 `meta.lock()` 再 `idx.read()/write()`，耗时工作（哈希、embedding）要在锁外做——`cmd_dedup_scan` 就是"锁内快照 → 锁外计算 → 锁内落地"的范例（[src/httpd.rs:602-639](src/httpd.rs#L602-L639)）。

---

## 9. 已知限制与坑

| # | 问题 | 影响 | 处理 |
|---|---|---|---|
| 1 | `lib.rs` 与 `main.rs` 各声明一套 `mod` | 模块被编译两次；改代码时两处签名要对齐 | 二进制其实不依赖 lib target，长期应收敛为单入口 |
| 2 | stdio 每次调用重载索引 | 大库下单次调用秒级 | 用 HTTP 模式，或改共享索引 |
| 3 | `/mcp` 无 SSE / 无 Session / `GET` 走错分支 | 严格客户端可能握手失败 | 见 §7.4 |
| 4 | 手写 HTTP：每连接一线程、`Connection: close`、无 keep-alive | 高并发下线程数上涨 | 本地单用户场景可接受 |
| 5 | 版本号不一致：`Cargo.toml` / `memex.iss` 已是 1.1.0，`start.bat` 仍打印 1.0.0 | 发布易出错 | 以 `Cargo.toml` 为准，改版本时同步 `memex.iss` 与 `start.bat` |
| 6 | `mcp.rs` 文件头注释仍写"四个工具"，实际 13 个 | 误导读者 | 已在本 README 更正 |
| 7 | `docs/` 被 gitignore，`tech-design.md` 不在仓库 | 设计意图只能从注释反推 | 见 §2.3 |
| 8 | `models/` 与 `*.dll` 全部 gitignore | 新 clone 无法直接跑语义检索 | 按 §5.4 准备 |
| 9 | 单实例锁按 `data/` 判定 | 换端口也不能起第二个 | 需要多实例时用不同 `MEMEX_HOME` |
| 10 | `embed` 为整批重算，无增量 | 新增大库耗时长 | 见 §2.3 |
| 11 | 无任何自动化测试 | 回归全靠手工冒烟 | 见 §5.8，欢迎先补 `parser` / `search` 的单测 |

---

## 10. 许可

Apache-2.0，见 [LICENSE](./LICENSE)。

---

**附：术语表**

| 词 | 含义 |
|---|---|
| bigram | 相邻两字符作索引项，CJK 场景无需分词，代价是索引体积 |
| RRF | Reciprocal Rank Fusion，`Σ 1/(k+rank)`，此处 k=60 |
| verified | Agent 回报置信度 ≥ 0.85 的文件，`memex_read` 直接返回缓存正文 |
| dead chunk | 文件被删除/替换后留下的空 chunk 占位，保证向量下标对齐 |
| Plan B / B+ | 把正文与 postings 从 `index.bin` 外置的两轮内存优化 |
