# Memex

> **文件不用整理，照样 3 秒找到任何一个。**

[![version](https://img.shields.io/badge/version-1.2.1-blue)]() [![license](https://img.shields.io/badge/license-Apache--2.0-green)]() [![platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-orange)]() [![rust](https://img.shields.io/badge/Rust-2021-orange)]() [![vue](https://img.shields.io/badge/Vue-3-42b883)]()

🏠 **[官网首页](https://johendeng.github.io/memex-landing/)** | ⬇️ **[下载安装](#安装)** | 🚀 **[快速上手](#快速上手)** | 🌐 [English](./README.en.md)

---

## 你是不是也这样？

- 明明记得文档「就存在哪个文件夹」，翻了半天还是没找到
- 搜「权限设计」什么都搜不到——因为文件里写的是「RBAC 方案」
- 方案 PDF、会议记录、代码片段、截图……散落十几个目录，全靠脑子记
- 试过用文件夹分类、用 Notion 整理，坚持不到一周就放弃了

如果你中了任何一条，Memex 就是为你做的。

**一句话：装上、选好文件夹，剩下的不用管。** Memex 自动索引你的所有文档，之后任何文件 3 秒搜到——不用整理、不用分类、不用上传。

| 你的痛点 | Memex 的解法 |
|---------|-------------|
| 只记得内容片段，记不住文件名 | **全文倒排索引 + 中文 bigram**，只言片语也能搜到，毫秒级响应 |
| 换个说法就搜不到 | **BM25 + 语义向量混合检索**，搜「权限设计」也能找到「RBAC 方案」 |
| PDF / Word / PPT / 图片里的字搜不到 | **12 种格式全解析**，PDF 用 pdfium，图片走 OCR |
| 不想让 AI 助手瞎猜我的文件 | **MCP 接入 Claude / Cursor 等 Agent**，AI 直接搜你的知识库 |
| AI 聊完就忘，每次都要重新交代 | **AI 记忆体**：Agent 可以记住你的偏好，跨会话召回 |
| 不想把数据传到云端 | **纯本地**：不联网、不上传、不需要账号，数据不离开你的硬盘 |

---

## 主要功能

### 🔍 混合检索
- **关键词搜索**：bigram 倒排索引，中文无需分词，< 5ms 响应
- **语义搜索**：bge-small-zh-v1.5 向量 + BM25 加权，RRF(k=60) 融合排序
- **相似文档**：基于向量余弦相似度，找到「内容相近」的文件

### 📄 多格式解析
txt / md / html / csv / json / docx / xlsx / pptx / pdf / 图片 OCR — 共 12 种格式，纯 Rust 解析，无外部依赖。

### 🏷️ 多维分类与治理
- 维度+值标签体系（如 `技术栈=Rust`、`领域=交通`）
- 可信度状态机：Agent 标记可信度，≥ 0.85 走缓存加速
- 重复文件检测：size → SHA-256 分级扫描，保留最浅路径

### 🧠 AI 记忆体
- **KV 记忆**：`namespace + key` 结构化存储，Agent 长期记忆
- **Markdown 笔记**：`data/notes/*.md`，可被索引进检索池
- **15 个 MCP 工具**：搜索、阅读、记忆、笔记、打标、标签编辑/删除、统计

### 🖥️ 桌面体验
- WebView2 窗口 / 系统托盘常驻 / 浏览器访问三种模式
- 实时文件监听：增量索引自动触发，新增文件秒级可搜
- 可视化界面：全局搜索、多维分类、统计面板、记忆管理、设置
- 自动更新：Gitee 主源 + GitHub 备源

### 🔒 纯本地
- 不联网，不上传，不需要账号
- 数据目录就在 exe 旁边，透明可控
- 仅绑定 `127.0.0.1`，不对外暴露

---

## 安装

### Windows
1. 下载 `Memex-Setup-x.x.x.exe`
2. 双击安装，默认路径 `C:\Memex\`
3. 桌面快捷方式启动，或从系统托盘唤醒

### macOS
1. 下载 `Memex-macOS-universal.tar.gz`（Intel + Apple Silicon 通用）
2. 解压后将 `Memex.app` 拖入 `/Applications/`
3. 首次打开：右键 → 打开（绕过 Gatekeeper）

### Linux
1. 下载 `Memex-x86_64.AppImage`
2. `chmod +x Memex-x86_64.AppImage`
3. 双击运行，或加入应用菜单

> 三平台安装包：[GitHub Releases](https://github.com/Johendeng/memex/releases) | [Gitee Releases](https://gitee.com/JohenTeng/memex/releases)（国内下载更快）

---

## 快速上手

### 1. 添加文件源
打开 Memex → 左侧「文件源」→ 输入框旁点「浏览…」选择文件夹 → 点「添加源」

支持的文件类型：
| 类别 | 扩展名 |
|------|--------|
| 文档 | `.md` `.txt` `.html` `.csv` `.json` |
| Office | `.docx` `.xlsx` `.pptx` |
| PDF | `.pdf`（pdfium 增强） |
| 代码 | `.rs` `.py` `.js` `.ts` `.go` `.java` `.c` `.cpp` `.sh` |
| 图片 | `.png` `.jpg` `.bmp` `.tiff` `.webp`（OCR 识别） |

### 2. 搜索
- **关键词搜索**：输入只言片语，回车即搜
- **混合搜索**：开启「语义搜索」开关，意思相近的文件也能找到
- 搜索结果带高亮片段、文件路径、标签

### 3. 接入 AI Agent

在 Agent 的 MCP 配置中加入：

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

或 HTTP 模式（多 Agent 共享，推荐高频场景）：

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

接入后，Agent 可以：
- `memex_search` — 搜索你的本地文件
- `memex_read` — 读取文件内容
- `memex_find_similar` — 找相似文档
- `memex_remember / recall / forget` — 存取记忆
- `memex_write_note / read_note` — 读写笔记
- `memex_tag` — 给文件打标签
- `memex_stats` — 查看知识库概况

---

## 场景示例

### 📚 技术文档库
把所有技术笔记、PDF 资料、项目文档丢进一个文件夹。Memex 建好索引后，搜「流量管控」能找到三年前的方案文档，搜「RBAC 权限设计」能匹配到只在正文出现过的笔记。

### 🤖 Agent 记忆中枢
Agent 通过 MCP 记住你的偏好（「这个用户偏好简洁回复」「项目用 Rust 2021 edition」），下次对话直接召回。笔记存在本地 Markdown 文件里，你随时可以查看和编辑。

### 🗂️ 重复文件清理
扫描多年积累的文件夹，SHA-256 精确比对，列出所有重复副本，一键清理冗余。

---

## 命令行

```bash
memex info                              # 索引概况
memex folders add <path>                # 添加文件源
memex index                             # 全量/增量索引
memex search "关键词" [-l 20] [--hybrid] # 搜索（--hybrid 开启语义）
memex embed                             # 生成向量索引
memex serve [--open] [--no-window]      # 启动服务（窗口/托盘/浏览器）
memex mcp serve                         # MCP stdio 服务
```

---

## 技术栈

| 层 | 技术 |
|----|------|
| 后端 | Rust 2021，纯 Rust 索引引擎（bigram 倒排 + mmap postings） |
| 前端 | Vue 3 + Vite，编译期嵌入 exe（rust-embed） |
| 桌面 | wry + tao + tray-icon |
| 语义 | fastembed + ONNX Runtime（bge-small-zh-v1.5） |
| PDF | pdfium-render（运行时绑定） |
| OCR | rusto-rs / PP-OCRv6 |
| 协议 | MCP（JSON-RPC over stdio / HTTP） |

---

## 开发者

构建指南、架构说明、模块职责、扩展指南等开发者文档见 [CONTRIBUTING.md](./CONTRIBUTING.md)。

```bash
# 前端
cd frontend && npm install && npm run build

# 后端（基础版）
cargo build --release

# 完整版（PDF + OCR）
cargo build --release --features "pdfium ocr"
```

---

## 许可

[Apache-2.0](./LICENSE) · Copyright 2026 JohenDeng

---

## 链接

- **GitHub**：[Johendeng/memex](https://github.com/Johendeng/memex)
- **Gitee**：[JohenTeng/memex](https://gitee.com/JohenTeng/memex)
- **Releases**：[GitHub Releases](https://github.com/Johendeng/memex/releases)
