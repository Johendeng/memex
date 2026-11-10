//! MCP (Model Context Protocol) stdio server.
//!
//! 手写 JSON-RPC 2.0 over stdio，不引入 rmcp 以控体积（tech-design §2）。
//! 四个工具：memex_search / memex_read / memex_tag / memex_report_confidence。
//!
//! 协议要点：
//! - 传输：每行一条 JSON-RPC 消息（newline-delimited）
//! - 通知（无 id）不回应；请求（有 id）必须回 result 或 error
//! - tools/call 返回 {"content":[{"type":"text","text":"<json>"}]}

use std::io::{BufRead, Write};

use anyhow::Result;
use serde_json::{json, Value};

use crate::memories;
use crate::search;
use crate::vector;
use crate::store::{
    self, BigramIndex, ConfidenceRec, Meta, TagRec, CONFIDENCE_THRESHOLD,
};

/// MCP server entry (stdio transport): read stdin line by line, write responses to stdout.
pub fn serve() -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    for line in stdin.lock().lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue, // 非法 JSON，静默丢弃
        };
        if let Some(resp) = handle_request(&req, None) {
            serde_json::to_writer(&mut stdout, &resp)?;
            stdout.write_all(b"\n")?;
            stdout.flush()?;
        }
    }

    Ok(())
}

/// Process one JSON-RPC 2.0 MCP request → optional response.
/// `Some(resp)` = reply to client; `None` = notification (no reply).
/// Shared by stdio `serve()` and HTTP `POST /mcp` transport.
/// `idx`: optional shared index (HTTP server passes it to avoid reloading
/// the 976MB index.bin per request); stdio passes `None` (loads per call).
pub fn handle_request(req: &Value, idx: Option<&BigramIndex>) -> Option<Value> {
    let id = req.get("id").cloned();
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");

    let result = match method {
        "initialize" => Some(handle_initialize()),
        "notifications/initialized" => None,
        "tools/list" => Some(handle_tools_list()),
        "tools/call" => {
            let params = req.get("params").cloned().unwrap_or(Value::Null);
            Some(handle_tools_call(params, idx))
        }
        _ => id.clone().map(|_| {
            json!({
                "jsonrpc": "2.0",
                "id": Value::Null,
                "error": {"code": -32601, "message": format!("method not found: {method}")}
            })
        }),
    };

    if let Some(mut resp) = result {
        if let Some(ref id) = id {
            resp["id"] = id.clone();
            Some(resp)
        } else {
            None
        }
    } else {
        None
    }
}

// ------------------------------------------------------------ MCP protocol

fn handle_initialize() -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": Value::Null,
        "result": {
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "memex",
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    })
}

fn handle_tools_list() -> Value {
    let tools = json!([
        {
            "name": "memex_search",
            "description": "在已索引的本地文件中检索（bigram 关键词 + bge-small-zh 语义向量 + RRF 融合）。返回命中文件的路径、片段、标签和置信度。hybrid=true 时启用语义检索（需先运行 memex embed）。\n\n【何时主动检索】\n• 用户问的问题可能已有文档记录时\n• 用户提到项目/人名/技术名词，想看是否有相关资料\n• 需要查找本地文件中的具体内容或数据\n• 做决策前，查找是否有相关文档支撑",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "搜索词，支持中英文混合"},
                    "limit": {"type": "integer", "description": "最大返回数（默认20）", "default": 20},
                    "hybrid": {"type": "boolean", "description": "启用混合检索（BM25+向量+RRF），默认 false", "default": false}
                },
                "required": ["query"]
            }
        },
        {
            "name": "memex_read",
            "description": "读取一个已索引文件的解析文本。如果该文件已被 Agent 标记为已验证（confidence=verified），直接返回缓存文本，不碰源文件（省 token）；否则返回源文件路径，提示 Agent 自行读取。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file": {"type": "string", "description": "文件 ID（数字）或路径片段"}
                },
                "required": ["file"]
            }
        },
        {
            "name": "memex_tag",
            "description": "为文件打标签（维度+值），用于多维分类。同一维度可多值并存。来源标记为 agent。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file": {"type": "string", "description": "文件 ID（数字）或路径片段"},
                    "dim": {"type": "string", "description": "维度名，如 领域/设备类型/主题"},
                    "value": {"type": "string", "description": "标签值"},
                    "note": {"type": "string", "description": "可选备注"}
                },
                "required": ["file", "dim", "value"]
            }
        },
        {
            "name": "memex_report_confidence",
            "description": "报告 Agent 对某文件解析结果的置信度分数。score≥0.85 则标记为 verified（后续 memex_read 直接返回缓存文本）；否则标记为 below。源文件变更后 verified 自动降级为 reverify。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file": {"type": "string", "description": "文件 ID（数字）或路径片段"},
                    "score": {"type": "number", "description": "0.0–1.0 的置信度分数"},
                    "note": {"type": "string", "description": "可选备注"}
                },
                "required": ["file", "score"]
            }
        },
        {
            "name": "memex_write_note",
            "description": "📝 写入 Markdown 笔记到知识库（data/notes/<title>.md）。笔记持久保存，可被 Agent 后续搜索和读取。这是本地记忆体的核心写入能力。\n\n【与 memex_remember 的分工】\n• remember = 一句话事实/偏好/决策（轻量，Agent 自主高频保存）\n• write_note = 需要展开的详细内容（重量，以用户显式要求为主）\n\n【何时使用】\n• 用户说\"写笔记\"/\"记录方案\"/\"保存这个到笔记\"\n• 方案设计、技术调研、问题分析结论、会议纪要（内容超过几句话）\n• Agent 解决了复杂问题后，可将方案结论整理成笔记（门槛高于 remember）\n\n对话结束前，如本轮写过笔记，简要列出供用户审阅。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "笔记标题（用作文件名）"},
                    "content": {"type": "string", "description": "Markdown 正文内容"},
                    "tags": {"type": "string", "description": "可选标签，逗号分隔（如：方案,交通,AIDC）"},
                    "overwrite": {"type": "boolean", "description": "覆盖已有同名笔记，默认 false", "default": false}
                },
                "required": ["title", "content"]
            }
        },
        {
            "name": "memex_list_notes",
            "description": "📋 列出所有 Agent 笔记。返回标题、大小、修改时间和内容预览。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": {"type": "integer", "description": "最大返回数（默认50）", "default": 50}
                }
            }
        },
        {
            "name": "memex_read_note",
            "description": "📖 读取一条笔记的完整内容。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "笔记标题"}
                },
                "required": ["title"]
            }
        },
        {
            "name": "memex_delete_note",
            "description": "🗑️ 删除一条笔记。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "笔记标题"}
                },
                "required": ["title"]
            }
        },
        {
            "name": "memex_remember",
            "description": "🧠 存储一条键值记忆（Agent 的长期记忆）。Agent 应主动判断何时保存，无需用户指示。同一 key+namespace 会更新而非重复插入。\n\n【何时主动保存（高门槛）】\n• 用户表达了偏好、约束或工作习惯\n• 做了决策及理由（选 A 不选 B，为什么）\n• 解决了非平凡问题（记录方案结论，不是排查过程）\n• 用户透露了环境、项目、团队的关键事实\n• 下次对话需要知道才能有效的上下文\n\n【何时不要保存】\n• 闲聊、临时调试信息、一次性操作\n• 知识库中已有的内容\n• 用户能轻易重新推导的信息\n\n用户显式说\"记住\"/\"保存这个\"时无条件执行。\n对话结束前，简要列出本轮保存的记忆，供用户清理。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "key": {"type": "string", "description": "记忆键名（如 user_preference, project_decision_42）"},
                    "value": {"type": "string", "description": "记忆值"},
                    "namespace": {"type": "string", "description": "命名空间（默认 'default'），用于隔离不同上下文", "default": "default"},
                    "note": {"type": "string", "description": "可选备注"}
                },
                "required": ["key", "value"]
            }
        },
        {
            "name": "memex_recall",
            "description": "🔍 检索 Agent 存储的记忆。可按 key 精确查找，或按关键词模糊搜索 key 和 value。不传 key/query 时列出全部记忆。\n\n【何时主动检索】\n• 任务开始时：查用户偏好、项目背景、历史决策\n• 做决策前：查是否有相关约束或先前的决策\n• 用户提到之前的对话内容或要求时",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "key": {"type": "string", "description": "精确匹配的 key（优先使用）"},
                    "query": {"type": "string", "description": "模糊搜索词（匹配 key 或 value 的子串）"},
                    "namespace": {"type": "string", "description": "命名空间（默认搜索全部）"}
                }
            }
        },
        {
            "name": "memex_forget",
            "description": "遗忘一条记忆（删除指定 key+namespace 的记忆）。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "key": {"type": "string", "description": "记忆键名"},
                    "namespace": {"type": "string", "description": "命名空间（默认 'default'）", "default": "default"}
                },
                "required": ["key"]
            }
        },
        {
            "name": "memex_find_similar",
            "description": "🔗 查找与指定文件语义相似的文档（基于向量余弦相似度）。需先生成向量索引。",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file": {"type": "string", "description": "文件 ID（数字）或路径片段"},
                    "limit": {"type": "integer", "description": "最大返回数（默认10）", "default": 10}
                },
                "required": ["file"]
            }
        },
        {
            "name": "memex_stats",
            "description": "📊 获取知识库统计概览：文件数、chunk 数、索引大小、标签分布、向量状态、笔记和记忆数量。",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        }
    ]);

    json!({
        "jsonrpc": "2.0",
        "id": Value::Null,
        "result": {"tools": tools}
    })
}

// ------------------------------------------------------------ tools/call dispatch

fn handle_tools_call(params: Value, idx: Option<&BigramIndex>) -> Value {
    let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    let result = match name {
        "memex_search" => tool_search(&args, idx),
        "memex_read" => tool_read(&args, idx),
        "memex_tag" => tool_tag(&args),
        "memex_report_confidence" => tool_confidence(&args),
        "memex_write_note" => tool_write_note(&args),
        "memex_list_notes" => tool_list_notes(&args),
        "memex_read_note" => tool_read_note(&args),
        "memex_delete_note" => tool_delete_note(&args),
        "memex_remember" => tool_remember(&args),
        "memex_recall" => tool_recall(&args),
        "memex_forget" => tool_forget(&args),
        "memex_find_similar" => tool_find_similar(&args, idx),
        "memex_stats" => tool_stats(&args, idx),
        _ => Err(format!("unknown tool: {name}")),
    };

    match result {
        Ok(json_text) => json!({
            "jsonrpc": "2.0",
            "id": Value::Null,
            "result": {"content": [{"type": "text", "text": json_text}]}
        }),
        Err(msg) => json!({
            "jsonrpc": "2.0",
            "id": Value::Null,
            "result": {
                "content": [{"type": "text", "text": format!("{{\"error\": \"{msg}\"}}")}],
                "isError": true
            }
        }),
    }
}

// ------------------------------------------------------------ tool implementations

/// memex_search: 关键词 + 可选混合检索，附 tag/confidence 信息。
fn tool_search(args: &Value, idx: Option<&BigramIndex>) -> Result<String, String> {
    let query = args.get("query")
        .and_then(|q| q.as_str())
        .ok_or("missing 'query'")?;
    let limit = args.get("limit")
        .and_then(|l| l.as_u64())
        .map(|n| n as usize)
        .unwrap_or(20);
    let hybrid = args.get("hybrid")
        .and_then(|h| h.as_bool())
        .unwrap_or(false);

    let meta = Meta::load().map_err(|e| format!("load meta: {e}"))?;

    // Use shared index if provided, otherwise load from disk
    let owned_idx;
    let ix: &BigramIndex = match idx {
        Some(shared) => shared,
        None => {
            owned_idx = BigramIndex::load().map_err(|e| format!("load index: {e}"))?;
            &owned_idx
        }
    };

    let hits = if hybrid {
        let vec_idx = vector::VectorIndex::load().map_err(|e| format!("load vectors: {e}"))?;
        if vec_idx.vectors.is_empty() {
            eprintln!("[memex] no vectors — run `memex embed` first; falling back to keyword");
            search::search(&meta, ix, query, limit)
                .map_err(|e| format!("search: {e}"))?
        } else {
            vector::hybrid_search(&meta, ix, &vec_idx, query, limit)
                .map_err(|e| format!("hybrid search: {e}"))?
        }
    } else {
        search::search(&meta, ix, query, limit)
            .map_err(|e| format!("search: {e}"))?
    };

    let results: Vec<Value> = hits.iter().map(|h| {
        let tags = store::tags_of(&meta, h.file_id);
        let tags_json: Vec<Value> = tags.iter().map(|t| json!({
            "dim": t.dim, "value": t.value, "source": t.source
        })).collect();

        let conf = store::confidence_of(&meta, h.file_id);
        let conf_json = conf.map(|c| json!({
            "status": c.status, "score": c.score
        })).unwrap_or(json!({"status": "unverified"}));

        let snippet_clean = h.snippet.replace("<b>", "").replace("</b>", "");

        json!({
            "file_id": h.file_id,
            "path": format!("{}/{}", h.folder_path, h.rel_path),
            "filename": h.rel_path.split('/').next_back().unwrap_or(&h.rel_path),
            "ext": h.ext,
            "score": format!("{:.1}", h.rank),
            "snippet": snippet_clean,
            "chunk_seq": h.chunk_seq,
            "tags": tags_json,
            "confidence": conf_json
        })
    }).collect();

    Ok(serde_json::to_string_pretty(&json!({"results": results})).unwrap())
}

/// memex_read: verified → 返回缓存文本；否则 → 返回源文件路径。
fn tool_read(args: &Value, idx: Option<&BigramIndex>) -> Result<String, String> {
    let file = args.get("file")
        .and_then(|f| f.as_str())
        .ok_or("missing 'file'")?;

    let meta = Meta::load().map_err(|e| format!("load meta: {e}"))?;
    let file_rec = store::find_file(&meta, file)
        .ok_or_else(|| format!("file not found: {file}"))?;

    let conf = store::confidence_of(&meta, file_rec.id);
    let is_verified = conf.map(|c| c.status == "verified").unwrap_or(false);

    if is_verified {
        // verified: 从索引返回解析文本（M4 简化版——未来 M5 用 parse_text_ref 缓存）
        // Use shared index if provided, otherwise load from disk
        let owned_idx;
        let ix: &BigramIndex = match idx {
            Some(shared) => shared,
            None => {
                owned_idx = BigramIndex::load().map_err(|e| format!("load index: {e}"))?;
                &owned_idx
            }
        };
        // Plan B: read chunk texts from chunks.text via offset table
        let chunk_ids: Vec<u32> = ix.chunks.iter()
            .enumerate()
            .filter(|(_, c)| c.file_id == file_rec.id)
            .map(|(cid, _)| cid as u32)
            .collect();
        let text: String = chunk_ids.iter()
            .map(|cid| ix.get_chunk_text(*cid))
            .collect();
        Ok(serde_json::to_string_pretty(&json!({
            "strategy": "cached",
            "file_id": file_rec.id,
            "text": text
        })).unwrap())
    } else {
        // 未验证：返回源文件路径，Agent 自读后回报 score
        let folder = meta.folders.iter().find(|f| f.id == file_rec.folder_id);
        let full_path = folder.map(|f| format!("{}/{}", f.path, file_rec.rel_path))
            .unwrap_or_else(|| file_rec.rel_path.clone());
        Ok(serde_json::to_string_pretty(&json!({
            "strategy": "source",
            "file_id": file_rec.id,
            "source_hint": full_path,
            "message": "请读取源文件后用 memex_report_confidence 回报置信度"
        })).unwrap())
    }
}

/// memex_tag: 添加标签，检测同维度冲突（多值并存但提示）。
fn tool_tag(args: &Value) -> Result<String, String> {
    let file = args.get("file")
        .and_then(|f| f.as_str())
        .ok_or("missing 'file'")?;
    let dim = args.get("dim")
        .and_then(|d| d.as_str())
        .ok_or("missing 'dim'")?;
    let value = args.get("value")
        .and_then(|v| v.as_str())
        .ok_or("missing 'value'")?;
    let note = args.get("note")
        .and_then(|n| n.as_str());

    let mut meta = store::Meta::load().map_err(|e| format!("load meta: {e}"))?;
    let file_rec = store::find_file(&meta, file)
        .ok_or_else(|| format!("file not found: {file}"))?;
    let file_id = file_rec.id;

    // 冲突检测：同 dim 已有不同 value
    let existing: Vec<String> = meta.tags.iter()
        .filter(|t| t.file_id == file_id && t.dim == dim && t.value != value)
        .map(|t| t.value.clone())
        .collect();
    let conflict = if existing.is_empty() {
        None
    } else {
        Some(format!("同维度已有不同值: {}", existing.join(", ")))
    };

    let at = now();
    meta.tags.push(TagRec {
        file_id,
        dim: dim.to_string(),
        value: value.to_string(),
        source: "agent".to_string(),
        by_agent: None,
        note: note.map(String::from),
        at: at.clone(),
    });
    meta.save().map_err(|e| format!("save meta: {e}"))?;

    Ok(serde_json::to_string_pretty(&json!({
        "ok": true,
        "file_id": file_id,
        "dim": dim,
        "value": value,
        "at": at,
        "conflict": conflict
    })).unwrap())
}

/// memex_report_confidence: 置信度状态机。
/// unverified → verified/below → reverify（源变更）→ verified
fn tool_confidence(args: &Value) -> Result<String, String> {
    let file = args.get("file")
        .and_then(|f| f.as_str())
        .ok_or("missing 'file'")?;
    let score = args.get("score")
        .and_then(|s| s.as_f64())
        .ok_or("missing 'score'")?;
    let note = args.get("note")
        .and_then(|n| n.as_str());

    let mut meta = store::Meta::load().map_err(|e| format!("load meta: {e}"))?;
    let file_rec = store::find_file(&meta, file)
        .ok_or_else(|| format!("file not found: {file}"))?;
    let file_id = file_rec.id;

    let new_status = if score >= CONFIDENCE_THRESHOLD {
        "verified"
    } else {
        "below"
    };

    let at = now();

    // upsert confidence record
    if let Some(rec) = meta.confidence.iter_mut().find(|c| c.file_id == file_id) {
        rec.score = score;
        rec.status = new_status.to_string();
        rec.at = at.clone();
        rec.note = note.map(String::from);
    } else {
        meta.confidence.push(ConfidenceRec {
            file_id,
            score,
            at: at.clone(),
            status: new_status.to_string(),
            note: note.map(String::from),
        });
    }
    meta.save().map_err(|e| format!("save meta: {e}"))?;

    Ok(serde_json::to_string_pretty(&json!({
        "file_id": file_id,
        "status": new_status,
        "score": score,
        "threshold": CONFIDENCE_THRESHOLD,
        "at": at
    })).unwrap())
}

// ------------------------------------------------------------ memory tools

/// memex_write_note: 写 Markdown 笔记到 data/notes/
fn tool_write_note(args: &Value) -> Result<String, String> {
    let title = args.get("title")
        .and_then(|t| t.as_str())
        .ok_or("missing 'title'")?;
    let content = args.get("content")
        .and_then(|c| c.as_str())
        .ok_or("missing 'content'")?;
    let tags = args.get("tags")
        .and_then(|t| t.as_str());
    let overwrite = args.get("overwrite")
        .and_then(|o| o.as_bool())
        .unwrap_or(false);

    let path = memories::write_note(title, content, tags, overwrite)
        .map_err(|e| format!("write_note: {e}"))?;

    Ok(serde_json::to_string_pretty(&json!({
        "ok": true,
        "title": title,
        "path": path.to_string_lossy(),
        "message": "笔记已保存。如需可搜索，请将 data/notes 目录添加为索引文件夹。"
    })).unwrap())
}

/// memex_list_notes: 列出所有笔记
fn tool_list_notes(args: &Value) -> Result<String, String> {
    let limit = args.get("limit")
        .and_then(|l| l.as_u64())
        .map(|n| n as usize)
        .unwrap_or(50);

    let notes = memories::list_notes()
        .map_err(|e| format!("list_notes: {e}"))?;

    let notes: Vec<Value> = notes.into_iter()
        .take(limit)
        .map(|n| json!({
            "name": n.name,
            "filename": n.filename,
            "size": n.size,
            "mtime": n.mtime,
            "preview": n.preview,
        }))
        .collect();

    Ok(serde_json::to_string_pretty(&json!({
        "notes": notes,
        "count": notes.len()
    })).unwrap())
}

/// memex_read_note: 读取笔记全文
fn tool_read_note(args: &Value) -> Result<String, String> {
    let title = args.get("title")
        .and_then(|t| t.as_str())
        .ok_or("missing 'title'")?;

    let content = memories::read_note(title)
        .map_err(|e| format!("read_note: {e}"))?;

    Ok(serde_json::to_string_pretty(&json!({
        "title": title,
        "content": content,
        "size": content.len()
    })).unwrap())
}

/// memex_delete_note: 删除笔记
fn tool_delete_note(args: &Value) -> Result<String, String> {
    let title = args.get("title")
        .and_then(|t| t.as_str())
        .ok_or("missing 'title'")?;

    let deleted = memories::delete_note(title)
        .map_err(|e| format!("delete_note: {e}"))?;

    Ok(serde_json::to_string_pretty(&json!({
        "ok": deleted,
        "title": title,
        "message": if deleted { "笔记已删除" } else { "笔记不存在" }
    })).unwrap())
}

/// memex_remember: 存储 KV 记忆
fn tool_remember(args: &Value) -> Result<String, String> {
    let key = args.get("key")
        .and_then(|k| k.as_str())
        .ok_or("missing 'key'")?;
    let value = args.get("value")
        .and_then(|v| v.as_str())
        .ok_or("missing 'value'")?;
    let namespace = args.get("namespace")
        .and_then(|n| n.as_str())
        .unwrap_or("default");
    let note = args.get("note")
        .and_then(|n| n.as_str());

    let mut store = memories::MemoryStore::load()
        .map_err(|e| format!("load memories: {e}"))?;
    let was_update = store.upsert(key, value, namespace, note);
    store.save()
        .map_err(|e| format!("save memories: {e}"))?;

    Ok(serde_json::to_string_pretty(&json!({
        "ok": true,
        "key": key,
        "namespace": namespace,
        "action": if was_update { "updated" } else { "created" },
        "at": now(),
        "total_memories": store.memories.len()
    })).unwrap())
}

/// memex_recall: 检索记忆
fn tool_recall(args: &Value) -> Result<String, String> {
    let key = args.get("key")
        .and_then(|k| k.as_str());
    let query = args.get("query")
        .and_then(|q| q.as_str());
    let namespace = args.get("namespace")
        .and_then(|n| n.as_str());

    let store = memories::MemoryStore::load()
        .map_err(|e| format!("load memories: {e}"))?;

    let results: Vec<Value> = if let Some(k) = key {
        // Exact match by key
        let ns = namespace.unwrap_or("default");
        store.recall(k, ns).iter().map(|m| json!({
            "key": m.key, "value": m.value, "namespace": m.namespace,
            "at": m.at, "note": m.note
        })).collect()
    } else if let Some(q) = query {
        // Fuzzy search
        store.search(q, namespace).iter().map(|m| json!({
            "key": m.key, "value": m.value, "namespace": m.namespace,
            "at": m.at, "note": m.note
        })).collect()
    } else {
        // List all (optionally filtered by namespace)
        store.list(namespace).iter().map(|m| json!({
            "key": m.key, "value": m.value, "namespace": m.namespace,
            "at": m.at, "note": m.note
        })).collect()
    };

    Ok(serde_json::to_string_pretty(&json!({
        "results": results,
        "count": results.len()
    })).unwrap())
}

/// memex_forget: 删除记忆
fn tool_forget(args: &Value) -> Result<String, String> {
    let key = args.get("key")
        .and_then(|k| k.as_str())
        .ok_or("missing 'key'")?;
    let namespace = args.get("namespace")
        .and_then(|n| n.as_str())
        .unwrap_or("default");

    let mut store = memories::MemoryStore::load()
        .map_err(|e| format!("load memories: {e}"))?;
    let deleted = store.forget(key, namespace);
    if deleted {
        store.save().map_err(|e| format!("save memories: {e}"))?;
    }

    Ok(serde_json::to_string_pretty(&json!({
        "ok": deleted,
        "key": key,
        "namespace": namespace,
        "remaining": store.memories.len()
    })).unwrap())
}

/// memex_find_similar: 查找相似文档
fn tool_find_similar(args: &Value, idx: Option<&BigramIndex>) -> Result<String, String> {
    let file = args.get("file")
        .and_then(|f| f.as_str())
        .ok_or("missing 'file'")?;
    let limit = args.get("limit")
        .and_then(|l| l.as_u64())
        .map(|n| n as usize)
        .unwrap_or(10);

    let meta = Meta::load().map_err(|e| format!("load meta: {e}"))?;
    let file_rec = store::find_file(&meta, file)
        .ok_or_else(|| format!("file not found: {file}"))?;

    let vec_idx = vector::VectorIndex::load()
        .map_err(|e| format!("load vectors: {e}"))?;
    if vec_idx.vectors.is_empty() {
        return Err("向量索引为空，请先运行 memex embed".to_string());
    }

    // Use shared index if provided, otherwise load
    let owned_idx;
    let ix: &BigramIndex = match idx {
        Some(shared) => shared,
        None => {
            owned_idx = BigramIndex::load().map_err(|e| format!("load index: {e}"))?;
            &owned_idx
        }
    };

    // Collect chunk_ids for this file
    let file_chunks: Vec<u32> = ix.chunks.iter()
        .enumerate()
        .filter(|(_, c)| c.file_id == file_rec.id)
        .map(|(i, _)| i as u32)
        .collect();

    if file_chunks.is_empty() {
        return Err("文件无索引数据".to_string());
    }

    // Average chunk vectors into centroid
    let dim = vec_idx.vectors[0].len();
    let mut centroid = vec![0.0f32; dim];
    let mut count = 0u32;
    for cid in &file_chunks {
        if let Some(v) = vec_idx.vectors.get(*cid as usize) {
            if !v.is_empty() {
                for (i, &x) in v.iter().enumerate() { centroid[i] += x; }
                count += 1;
            }
        }
    }
    if count == 0 {
        return Err("文件无向量数据".to_string());
    }
    for x in &mut centroid { *x /= count as f32; }

    use std::collections::HashSet;
    let file_chunk_set: HashSet<u32> = file_chunks.into_iter().collect();

    let mut scored: Vec<(u32, f32)> = vec_idx.vectors.iter()
        .enumerate()
        .filter_map(|(i, v)| {
            if v.is_empty() || file_chunk_set.contains(&(i as u32)) {
                None
            } else {
                Some((i as u32, vector::cosine(&centroid, v)))
            }
        })
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored.truncate(limit * 5);

    let mut seen_files: HashSet<i64> = HashSet::new();
    let hits: Vec<Value> = scored.iter()
        .filter_map(|(cid, score)| {
            let chunk = ix.chunks.get(*cid as usize)?;
            if seen_files.contains(&chunk.file_id) { return None; }
            seen_files.insert(chunk.file_id);
            let file = meta.files.iter().find(|f| f.id == chunk.file_id)?;
            let folder = meta.folders.iter().find(|fo| fo.id == file.folder_id)?;
            let snippet: String = ix.get_chunk_text(*cid).chars().take(200).collect();
            Some(json!({
                "file_id": chunk.file_id,
                "path": format!("{}/{}", folder.path, file.rel_path),
                "filename": file.rel_path.split('/').next_back().unwrap_or(&file.rel_path),
                "ext": file.ext,
                "score": format!("{:.3}", score),
                "snippet": snippet,
            }))
        })
        .take(limit)
        .collect();

    Ok(serde_json::to_string_pretty(&json!({
        "source_file": format!("{}/{}", 
            meta.folders.iter().find(|f| f.id == file_rec.folder_id).map(|f| f.path.as_str()).unwrap_or(""),
            file_rec.rel_path),
        "similar": hits,
        "count": hits.len()
    })).unwrap())
}

/// memex_stats: 知识库统计概览
fn tool_stats(_args: &Value, idx: Option<&BigramIndex>) -> Result<String, String> {
    let meta = Meta::load().map_err(|e| format!("load meta: {e}"))?;

    let file_count = meta.files.len();
    let folder_count = meta.folders.len();
    let tag_count = meta.tags.len();
    let confidence_count = meta.confidence.len();
    let verified_count = meta.confidence.iter().filter(|c| c.status == "verified").count();

    // Index stats
    let (chunk_count, bigram_count) = match idx {
        Some(ix) => (ix.chunks.len(), ix.bigram_count()),
        None => {
            match BigramIndex::load() {
                Ok(ix) => (ix.chunks.len(), ix.bigram_count()),
                Err(_) => (0, 0),
            }
        }
    };

    // Vector stats
    let (vec_ready, vec_count) = match vector::VectorIndex::load() {
        Ok(vi) => (!vi.vectors.is_empty(), vi.vectors.len()),
        Err(_) => (false, 0),
    };

    // Notes stats
    let notes_count = memories::list_notes()
        .map(|n| n.len())
        .unwrap_or(0);

    // Memory stats
    let mem_count = memories::MemoryStore::load()
        .map(|s| s.memories.len())
        .unwrap_or(0);

    // Tag distribution
    use std::collections::HashMap;
    let mut tag_dims: HashMap<String, usize> = HashMap::new();
    for t in &meta.tags {
        *tag_dims.entry(t.dim.clone()).or_default() += 1;
    }
    let tag_dist: Vec<Value> = tag_dims.iter()
        .map(|(dim, count)| json!({"dim": dim, "count": count}))
        .collect();

    // Index file sizes
    let data_dir = crate::config::data_dir();
    let index_size = std::fs::metadata(data_dir.join("index.bin"))
        .map(|m| m.len()).unwrap_or(0);
    let chunks_size = std::fs::metadata(data_dir.join("chunks.text"))
        .map(|m| m.len()).unwrap_or(0);
    let postings_size = std::fs::metadata(data_dir.join("postings.bin"))
        .map(|m| m.len()).unwrap_or(0);

    Ok(serde_json::to_string_pretty(&json!({
        "files": file_count,
        "folders": folder_count,
        "chunks": chunk_count,
        "bigrams": bigram_count,
        "tags": tag_count,
        "tag_dims": tag_dist,
        "confidence": {
            "total": confidence_count,
            "verified": verified_count,
        },
        "vectors": {
            "ready": vec_ready,
            "count": vec_count,
        },
        "notes": notes_count,
        "memories": mem_count,
        "index_size_mb": format!("{:.1}", index_size as f64 / 1_048_576.0),
        "chunks_size_mb": format!("{:.1}", chunks_size as f64 / 1_048_576.0),
        "postings_size_mb": format!("{:.1}", postings_size as f64 / 1_048_576.0),
    })).unwrap())
}

fn now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}
