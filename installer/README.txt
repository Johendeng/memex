Memex v1.1.0 — 本地知识库
================================

【安装目录】
  C:\Memex\
    ├── memex.exe        主程序
    ├── models\            ONNX 向量模型 (bge-small-zh-v1.5, ~91MB)
    ├── data\              索引与缓存（运行时自动创建）
    │   ├── meta.json      文件元数据
    │   ├── index.bin      bigram 索引
    │   ├── chunks.text    chunk 文本
    │   ├── postings.bin   倒排表
    │   ├── postings.idx   倒排表索引
    │   ├── vectors.bin    向量数据
    │   └── memex.lock   单实例锁
    └── unins000.exe       卸载程序

【注册表】
  Memex 运行时不写任何注册表。
  安装/卸载器仅在 HKLM\...\Uninstall\Memex 写一条标准卸载条目，
  以便 Windows「设置 → 应用」列表显示。卸载后自动清除。

【数据目录自定义】
  默认: C:\Memex\data\ (exe 所在目录下的 data\)
  自定义: 设置环境变量 MEMEX_HOME=<路径>
  例如: MEMEX_HOME=D:\MyData → 数据存到 D:\MyData\data\

【命令行】
  memex.exe serve          启动服务 + WebView2 窗口
  memex.exe serve --no-window   无窗口模式（仅 HTTP 服务）
  memex.exe serve --open   启动并自动打开浏览器
  memex.exe info           打印索引统计
  memex.exe index <path>   扫描并索引指定目录
  memex.exe mcp serve      MCP stdio 模式

【端口】
  HTTP: 127.0.0.1:18321
  MCP:  127.0.0.1:18321/mcp

【卸载】
  方法1: 运行 C:\Memex\unins000.exe
  方法2: Windows「设置 → 应用 → Memex → 卸载」
  卸载会删除程序文件和 data\ 目录（索引数据不可恢复）。
