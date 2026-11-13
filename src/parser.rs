use std::path::Path;
use std::sync::OnceLock;

use anyhow::{Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;
use regex::Regex;
use zip::ZipArchive;

/// Extract plain text from a file based on its extension.
///
/// M1: txt/md/html/csv/json (direct UTF-8 read + HTML strip).
/// M2: docx/xlsx (zip+XML), pdf (pdf-extract / lopdf).
pub fn extract_text(path: &Path) -> Result<String> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "html" | "htm" => {
            let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
            Ok(strip_html(&String::from_utf8_lossy(&bytes)))
        }
        "docx" => extract_docx(path),
        "xlsx" => extract_xlsx(path),
        "pptx" => extract_pptx(path),
        "pdf" => extract_pdf(path),
        #[cfg(feature = "ocr")]
        "png" | "jpg" | "jpeg" | "bmp" | "tiff" | "tif" | "webp" => {
            crate::ocr::extract_text(path)
        }
        _ => {
            let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
            Ok(strip_bom(&String::from_utf8_lossy(&bytes)).into_owned())
        }
    }
}

// ----------------------------------------------------------- BOM

fn strip_bom(s: &str) -> std::borrow::Cow<'_, str> {
    if s.starts_with('\u{feff}') {
        std::borrow::Cow::Borrowed(&s[3..])
    } else {
        std::borrow::Cow::Borrowed(s)
    }
}

// ----------------------------------------------------------- HTML

static SCRIPT_RE: OnceLock<Regex> = OnceLock::new();
static STYLE_RE: OnceLock<Regex> = OnceLock::new();
static TAG_RE: OnceLock<Regex> = OnceLock::new();

fn strip_html(input: &str) -> String {
    let script = SCRIPT_RE.get_or_init(|| Regex::new(r"(?is)<script\b[^>]*>.*?</script\s*>").unwrap());
    let style = STYLE_RE.get_or_init(|| Regex::new(r"(?is)<style\b[^>]*>.*?</style\s*>").unwrap());
    let tag = TAG_RE.get_or_init(|| Regex::new(r"(?s)<[^>]+>").unwrap());

    let s = script.replace_all(input, " ");
    let s = style.replace_all(&s, " ");
    let s = tag.replace_all(&s, " ");
    let s = s
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

// ----------------------------------------------------------- DOCX

/// 从 .docx 提取文本：解 zip 读 word/document.xml，提取 <w:t> 文本，
/// <w:p> 段落边界插入换行。
fn extract_docx(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut zip = ZipArchive::new(file).context("docx 不是有效 zip")?;
    let entry = zip
        .by_name("word/document.xml")
        .context("docx 缺 word/document.xml")?;
    let xml = std::io::read_to_string(entry)?;
    Ok(extract_docx_xml(&xml))
}

fn extract_docx_xml(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut in_t = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name());
                if local == "t" {
                    in_t = true;
                } else if local == "p" || local == "br" {
                    out.push('\n');
                }
            }
            Ok(Event::End(e)) => {
                if local_name(e.name()) == "t" {
                    in_t = false;
                }
            }
            Ok(Event::Text(e)) => {
                if in_t {
                    out.push_str(&e.unescape().unwrap_or_default());
                }
            }
            Ok(Event::Empty(e)) => {
                if local_name(e.name()) == "tab" {
                    out.push('\t');
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

// ----------------------------------------------------------- XLSX

/// 从 .xlsx 提取文本：读 sharedStrings.xml 获取共享字符串表，
/// 遍历各 sheet 的 cell，t="s" 的值是 sharedStrings 索引。
fn extract_xlsx(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut zip = ZipArchive::new(file).context("xlsx 不是有效 zip")?;

    let shared: Vec<String> = match zip.by_name("xl/sharedStrings.xml") {
        Ok(entry) => parse_shared_strings(&std::io::read_to_string(entry)?),
        Err(_) => Vec::new(),
    };

    let names: Vec<String> = (0..zip.len())
        .filter_map(|i| {
            let entry = zip.by_index(i).ok()?;
            let name = entry.name().to_string();
            if name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml") {
                Some(name)
            } else {
                None
            }
        })
        .collect();

    let mut out = String::new();
    for name in &names {
        let xml = std::io::read_to_string(zip.by_name(name)?)?;
        let rows = parse_sheet(&xml, &shared);
        for row in rows {
            out.push_str(&row);
            out.push('\n');
        }
    }
    Ok(out)
}

fn parse_shared_strings(xml: &str) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut strings = Vec::new();
    let mut in_t = false;
    let mut current = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name());
                // 在 <si> 边界清零，让同一 <si> 内多个 <r><t> 富文本 run 累加
                if local == "si" { current.clear(); }
                if local == "t" { in_t = true; }
            }
            Ok(Event::End(e)) => {
                let local = local_name(e.name());
                if local == "t" { in_t = false; }
                if local == "si" {
                    strings.push(std::mem::take(&mut current));
                }
            }
            Ok(Event::Text(e)) => {
                if in_t {
                    current.push_str(&e.unescape().unwrap_or_default());
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    strings
}

/// 解析一个 sheet 的 XML，返回每行的拼接文本（单元格用 \t 分隔）。
fn parse_sheet(xml: &str, shared: &[String]) -> Vec<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut rows = Vec::new();
    let mut current_row: Vec<String> = Vec::new();
    let mut cell_is_shared = false;
    let mut cell_text = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                if local_name(e.name()) == "c" {
                    cell_is_shared = e.attributes().any(|a| {
                        a.map(|a| a.key.as_ref() == b"t" && a.value.as_ref() == b"s")
                            .unwrap_or(false)
                    });
                    cell_text.clear();
                }
            }
            Ok(Event::End(e)) => {
                let local = local_name(e.name());
                if local == "c" {
                    let val = if cell_is_shared {
                        let idx: usize = cell_text.trim().parse().unwrap_or(usize::MAX);
                        shared.get(idx).cloned().unwrap_or_default()
                    } else {
                        cell_text.clone()
                    };
                    if !val.is_empty() {
                        current_row.push(val);
                    }
                }
                if local == "row" {
                    if !current_row.is_empty() {
                        rows.push(current_row.join("\t"));
                    }
                    current_row.clear();
                }
            }
            Ok(Event::Text(e)) => {
                cell_text.push_str(&e.unescape().unwrap_or_default());
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    rows
}

// ----------------------------------------------------------- PPTX

/// 从 .pptx 提取文本：解 zip 读 ppt/slides/slide*.xml，
/// 提取 <a:t> 文本（DrawingML），每个 slide 之间插入换行。
/// 也读取 ppt/notesSlides/notesSlide*.xml 提取演讲者备注。
fn extract_pptx(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut zip = ZipArchive::new(file).context("pptx 不是有效 zip")?;

    // 收集所有 slide 文件名（按序号排序）
    let mut slide_names: Vec<(u32, String)> = Vec::new();
    let mut notes_names: Vec<(u32, String)> = Vec::new();
    for i in 0..zip.len() {
        let entry = zip.by_index(i)?;
        let name = entry.name().to_string();
        // slide1.xml → (1, "ppt/slides/slide1.xml")
        if let Some(num) = parse_slide_num(&name, "ppt/slides/slide") {
            slide_names.push((num, name));
        } else if let Some(num) = parse_slide_num(&name, "ppt/notesSlides/notesSlide") {
            notes_names.push((num, name));
        }
    }
    slide_names.sort_by_key(|(n, _)| *n);
    notes_names.sort_by_key(|(n, _)| *n);

    let mut out = String::new();
    for (i, (_, name)) in slide_names.iter().enumerate() {
        if i > 0 {
            out.push_str("\n\n---\n\n");
        }
        let xml = std::io::read_to_string(zip.by_name(name)?)?;
        out.push_str(&extract_drawingml_text(&xml));
    }

    // 追加演讲者备注
    if !notes_names.is_empty() {
        out.push_str("\n\n=== 演讲者备注 ===\n\n");
        for (_, name) in &notes_names {
            let xml = std::io::read_to_string(zip.by_name(name)?)?;
            let notes = extract_drawingml_text(&xml);
            if !notes.is_empty() {
                out.push_str(&notes);
                out.push('\n');
            }
        }
    }

    Ok(out)
}

/// 从文件名提取 slide 序号。
/// "ppt/slides/slide3.xml" → Some(3), "ppt/notesSlides/notesSlide2.xml" → Some(2)
fn parse_slide_num(name: &str, prefix: &str) -> Option<u32> {
    if !name.starts_with(prefix) || !name.ends_with(".xml") {
        return None;
    }
    let mid = &name[prefix.len()..name.len() - 4];
    mid.parse().ok()
}

/// 从 DrawingML XML 提取 <a:t> 文本内容。
/// 用于 PPTX slides 和 notes slides。
fn extract_drawingml_text(xml: &str) -> String {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut in_t = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let local = local_name(e.name());
                if local == "t" {
                    in_t = true;
                } else if local == "p" {
                    // <a:p> 段落边界
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                } else if local == "br" {
                    out.push('\n');
                }
            }
            Ok(Event::End(e)) => {
                if local_name(e.name()) == "t" {
                    in_t = false;
                }
            }
            Ok(Event::Text(e)) => {
                if in_t {
                    out.push_str(&e.unescape().unwrap_or_default());
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

// ----------------------------------------------------------- PDF

/// 从 .pdf 提取文本。
///
/// 启用 `pdfium` feature 时优先使用 pdfium-render（无 panic、支持渲染），
/// 失败则回退到 pdf-extract。
///
/// 未启用 `pdfium` feature 时仅用 pdf-extract + catch_unwind。
fn extract_pdf(path: &Path) -> Result<String> {
    #[cfg(feature = "pdfium")]
    {
        if let Ok(text) = extract_pdf_pdfium(path) {
            if !text.trim().is_empty() {
                return Ok(text);
            }
        }
        // pdfium failed or empty text → fall through to pdf-extract
    }

    use std::panic::AssertUnwindSafe;
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        pdf_extract::extract_text(path)
    }));
    match result {
        Ok(Ok(text)) => {
            let cleaned: String = text
                .lines()
                .map(|l| l.trim_end())
                .collect::<Vec<_>>()
                .join("\n");
            Ok(cleaned)
        }
        Ok(Err(e)) => Err(anyhow::anyhow!("pdf 解析失败 {}: {e}", path.display())),
        Err(_) => Err(anyhow::anyhow!(
            "pdf 解析 panic（CID 字体可能不支持）: {}",
            path.display()
        )),
    }
}

/// PDF text extraction using pdfium-render (feature-gated).
#[cfg(feature = "pdfium")]
fn extract_pdf_pdfium(path: &Path) -> Result<String> {
    use pdfium_render::prelude::*;
    use std::sync::OnceLock;

    // Cache the Pdfium bindings so we don't re-search for the DLL on every call.
    static PDFIUM: OnceLock<Option<Pdfium>> = OnceLock::new();
    let pdfium = match PDFIUM.get_or_init(|| {
        // 1. Try bundled pdfium.dll next to exe
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let dll = dir.join("pdfium.dll");
                if dll.exists() {
                    if let Ok(b) = Pdfium::bind_to_library(dll.to_string_lossy().as_ref()) {
                        return Some(Pdfium::new(b));
                    }
                }
            }
        }
        // 2. Try system library
        match Pdfium::bind_to_system_library() {
            Ok(b) => Some(Pdfium::new(b)),
            Err(_) => {
                eprintln!("[pdfium] pdfium library not found, falling back to pdf-extract");
                None
            }
        }
    }) {
        Some(p) => p,
        None => anyhow::bail!("pdfium not available"),
    };

    let document = pdfium.load_pdf_from_file(path, None)
        .map_err(|e| anyhow::anyhow!("pdfium load failed: {e}"))?;

    let mut pages_text = Vec::new();
    for page in document.pages().iter() {
        let text_page = page.text()
            .map_err(|e| anyhow::anyhow!("pdfium text page: {e}"))?;
        let s = text_page.all();
        pages_text.push(s);
    }

    Ok(pages_text.join("\n"))
}

// ----------------------------------------------------------- helpers

fn local_name<'a>(name: quick_xml::name::QName<'a>) -> &'a str {
    let full: &[u8] = name.0;
    match full.iter().position(|&b| b == b':') {
        Some(pos) => std::str::from_utf8(&full[pos + 1..]).unwrap_or(""),
        None => std::str::from_utf8(full).unwrap_or(""),
    }
}

/// Check if a file extension is an image type (always compiled, even without `ocr` feature).
pub fn is_image_ext(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        "png" | "jpg" | "jpeg" | "bmp" | "tiff" | "tif" | "webp"
    )
}
