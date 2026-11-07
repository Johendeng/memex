# Memex v1.0.0

> 本地文件夹知识库工具 —— **索引归工具，理解归 Agent**

Apache-2.0 License — see [LICENSE](./LICENSE).
>
> Memex 不接大模型。它只做一件事：把你散落的本地文件建成全文倒排 + 内容向量索引，再通过 MCP 暴露给本地的 Agent（Claude Code / Codex / 豆包…）。语义理解交给 Agent，定位与检索归工具。

## ✨ 功能

- **全文搜索** — bigram 倒排索引，毫秒级响应
- **语义检索** — bge-small-zh-v1.5 向量嵌入 + BM25 + RRF 混合融合
- **文档解析** — txt / md / docx / xlsx / pdf（纯 Rust，无外部依赖）
- **MCP 工具** — 4 个标准 MCP 工具，供 AI Agent 调用
- **Web UI** — 内嵌前端，单 exe 启动即用
- **多维标签** — Agent 打标 + 可信度报告
- **统计面板** — 文件 / chunk / bigram / 向量 / 类型分布

## 📦 目录结构

```
memex/
├── memex.exe                        # 主程序（含内嵌前端）
├── onnxruntime.dll                    # ONNX Runtime（语义检索）
├── onnxruntime_providers_shared.dll   # ONNX 共享库
├── models/
│   └── bge-small-zh-v1.5/             # 嵌入模型（512 维）
├── start.bat                          # 一键启动
└── README.md                          # 本文件
```

## 🚀 快速开始

### 1. 启动

双击 `start.bat`，或命令行：

```bash
memex.exe serve --port 18321
```

浏览器打开 `http://127.0.0.1:18321`

### 2. 添加文档文件夹

在「设置」页面添加包含文档的文件夹路径。支持格式：txt / md / docx / xlsx / pdf

### 3. 建立索引

点击「重新索引」按钮，或命令行：

```bash
memex.exe index
```

### 4. 生成向量（可选，启用语义检索）

在「设置」→「本地模型」点击「生成向量」，或命令行：

```bash
memex.exe embed
```

### 5. 搜索

在「全局搜索」页面：
- **关键词搜索**（默认）— bigram 精确匹配
- **语义检索**（开启 toggle）— 混合 BM25 + 向量 + RRF 融合

## 🔧 CLI 命令

```bash
memex.exe index                    # 索引所有文件夹
memex.exe embed                    # 生成/更新向量索引
memex.exe search "关键词"           # 关键词搜索
memex.exe search "语义查询" --hybrid # 混合搜索
memex.exe search "流量" -l 10       # 指定返回数量
memex.exe serve --port 18321       # 启动 Web 服务
memex.exe serve --open             # 启动并打开浏览器
memex.exe mcp                      # 启动 MCP stdio 服务
memex.exe info                     # 显示索引信息
```

## 🔌 MCP 集成

Memex 作为 MCP server 暴露 4 个工具：

| 工具 | 说明 |
|------|------|
| `memex_search` | 搜索知识库（支持 hybrid 语义检索） |
| `memex_read` | 读取指定文件全文 |
| `memex_tag` | 为文件打标签 |
| `memex_report_confidence` | 报告文件可信度 |

在「设置」→「MCP 集成」点击「生成配置 Prompt」获取接入指令。

### 手动配置 MCP

```json
{
  "mcpServers": {
    "memex": {
      "command": "C:/path/to/memex.exe",
      "args": ["mcp"]
    }
  }
}
```

## 🏗️ 技术架构

| 组件 | 技术 |
|------|------|
| 核心引擎 | Rust（单 exe，无 GC） |
| 倒排索引 | bigram + bincode 序列化 |
| 向量检索 | bge-small-zh-v1.5 + ONNX Runtime |
| 混合融合 | BM25 + 余弦相似度 → RRF（k=60） |
| 文档解析 | quick-xml(docx) + calamine(xlsx) + pdf-extract |
| Web 服务 | axum + rust-embed（前端内嵌） |
| MCP | 手写 JSON-RPC 2.0 over stdio |
| 前端 | Vue 3 + Vite（编译嵌入） |

## 📊 性能参考

- 索引 12 文件（~4MB）：~3 秒
- 向量嵌入 266 chunk：~15 秒
- 关键词搜索：< 5ms
- 混合搜索：< 50ms（含向量计算）

## ⚠️ 注意事项

- ONNX Runtime DLL 必须与 exe 在同一目录
- 模型文件路径固定为 `models/bge-small-zh-v1.5/`
- 索引文件（index.bin / vectors.bin / meta.json）在工作目录生成
- 建议将 memex 放在固定路径，避免移动后路径失效

## 📝 License

自用 / 内部使用
