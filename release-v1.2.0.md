# v1.2.0 — 稳定性与性能优化 / Stability & Performance

> 修复「查找相似」卡死、搜索被索引阻塞、向量生成 CPU 占用过高等关键问题。
>
> Fixes critical issues: "Find Similar" freezing search, indexing blocking search queries, and high CPU usage during embedding generation.

---

## 中文

### 🐛 Bug 修复

- **「查找相似」点击后搜索卡死（关键）**
  - 后端三阶段锁拆分：获取分片列表 → 无锁计算余弦相似度 → 解析元数据，彻底消除对搜索请求的阻塞
  - 修复空向量（dim=0）导致 centroid 越界 panic，线程死亡后返回 "Empty reply from server"
  - 前端添加 loading 指示器，避免点击后空白页面

- **搜索被索引阻塞**
  - 磁盘 I/O 移出写锁（`append_texts_to_disk` 无锁写入）
  - 新增 `recover_read` 重试机制（60×50ms）
  - 统一锁顺序 idx→meta，消除死锁风险
  - 验证：索引过程中搜索 0-2ms

- **软件卡死（pdfium.dll 缺失）**
  - 缺失 DLL 时 panic 导致锁中毒，所有后续操作卡死
  - 改用 OnceLock 延迟初始化 + 友好错误提示

### ⚡ 性能优化

- **语义向量生成 CPU 占用 80%+ → 不影响用户操作**
  - 进程级 `SetPriorityClass(BELOW_NORMAL)` via kernel32 FFI
  - HTTP 线程恢复 `THREAD_PRIORITY_NORMAL`
  - ONNX Runtime 全部线程继承 BelowNormal 优先级

### ✨ 新功能

- **OCR 结果缓存** — 索引重建时复用已有 OCR 结果，避免重复识别
- **文件源移除同步清理** — 移除文件源时提醒并清理关联索引文件
- **后台索引进度可视化** — 页面底部 taskBar 实时展示索引/向量化进度
- **移除搜索栏向量生成按钮** — 索引和向量化全部转为后台非阻塞执行

### 📦 下载

| 平台 | 文件 | 说明 |
|------|------|------|
| Windows | `Memex-Setup-1.2.0.exe` | Inno Setup 安装程序，x86_64 |
| macOS | `Memex-macOS-universal.tar.gz` | Universal Binary (Intel + Apple Silicon) |
| Linux | `Memex-x86_64.AppImage` | 免安装，一个文件通吃所有发行版 |

---

## English

### 🐛 Bug Fixes

- **"Find Similar" freezes search (critical)**
  - Backend three-phase lock split: get chunk list → lock-free cosine similarity → parse metadata, eliminating all blocking of search requests
  - Fixed empty vector (dim=0) causing centroid out-of-bounds panic, thread death returning "Empty reply from server"
  - Frontend loading indicator to avoid blank page after click

- **Search blocked by indexing**
  - Disk I/O moved out of write lock (`append_texts_to_disk` lock-free writes)
  - Added `recover_read` retry mechanism (60×50ms)
  - Unified lock ordering idx→meta, eliminating deadlock risk
  - Verified: search during indexing completes in 0-2ms

- **App freeze (missing pdfium.dll)**
  - Missing DLL caused panic → lock poisoning → all operations frozen
  - Switched to OnceLock lazy initialization + friendly error message

### ⚡ Performance

- **Embedding generation CPU usage 80%+ → no impact on user tasks**
  - Process-level `SetPriorityClass(BELOW_NORMAL)` via kernel32 FFI
  - HTTP threads restored to `THREAD_PRIORITY_NORMAL`
  - ONNX Runtime threads all inherit BelowNormal priority

### ✨ New Features

- **OCR result caching** — Reuses existing OCR results on index rebuild, avoiding redundant recognition
- **File source removal cleanup** — Prompts and cleans up associated index files when removing a file source
- **Background indexing progress** — Bottom taskBar shows real-time indexing/embedding progress
- **Removed vector generation button from search bar** — Indexing and vectorization fully background non-blocking

### 📦 Downloads

| Platform | File | Notes |
|----------|------|-------|
| Windows | `Memex-Setup-1.2.0.exe` | Inno Setup installer, x86_64 |
| macOS | `Memex-macOS-universal.tar.gz` | Universal Binary (Intel + Apple Silicon) |
| Linux | `Memex-x86_64.AppImage` | No installation needed, single file for all distros |

---

**License**: Apache-2.0 · Copyright 2026 JohenDeng
