use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

/// 提取结果
#[derive(Debug, Clone)]
pub struct ExtractedDoc {
    pub text: String,
    pub doc_type: &'static str,
}

/// 检查是否为支持全文提取的文档或代码扩展名
pub fn is_supported_document_ext(ext: &str) -> bool {
    let lower = ext.to_lowercase();
    matches!(
        lower.as_str(),
        // Office / 复合文档
        "docx" | "xlsx" | "pptx" | "pdf" | "epub" | "odt" | "ods" | "odp" |
        // 纯文本 & 代码
        "txt" | "md" | "rs" | "py" | "js" | "ts" | "json" | "csv" | "xml" | "yaml" | "yml" |
        "toml" | "ini" | "cfg" | "log" | "sh" | "bat" | "ps1" | "html" | "css" | "svelte" |
        "c" | "cpp" | "h" | "hpp" | "go" | "java" | "vue" | "jsx" | "tsx" | "sql" | "r" |
        "rb" | "php" | "swift" | "kt" | "lua" | "dockerfile" | "makefile" | "cmake"
    )
}

/// 判断是否为二进制文档格式 (需要解压/解码)
pub fn is_binary_document_ext(ext: &str) -> bool {
    let lower = ext.to_lowercase();
    matches!(
        lower.as_str(),
        "docx" | "xlsx" | "pptx" | "pdf" | "epub" | "odt" | "ods" | "odp"
    )
}

/// 从文件路径中提取文档内容
pub fn extract_document_text(path: &Path) -> Option<ExtractedDoc> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    let file = File::open(path).ok()?;
    let metadata = file.metadata().ok()?;

    // 限制单文件最大 50MB (防止超大文件耗尽内存)
    if metadata.len() == 0 || metadata.len() > 50 * 1024 * 1024 {
        return None;
    }

    match ext.as_str() {
        "docx" => extract_docx(file),
        "xlsx" => extract_xlsx(file),
        "pptx" => extract_pptx(file),
        "pdf" => extract_pdf(path),
        "epub" => extract_epub(file),
        "odt" | "ods" | "odp" => extract_opendocument(file),
        // 纯文本与代码格式
        _ if is_supported_document_ext(&ext) => extract_plain_text(file),
        _ => None,
    }
}

/// 1. Word 文档 (.docx) 极速提取
fn extract_docx(file: File) -> Option<ExtractedDoc> {
    let reader = BufReader::new(file);
    let mut zip = zip::ZipArchive::new(reader).ok()?;
    let mut full_text = String::new();

    // 读取主正文 word/document.xml
    if let Ok(mut doc_entry) = zip.by_name("word/document.xml") {
        let mut xml_content = String::new();
        if doc_entry.read_to_string(&mut xml_content).is_ok() {
            extract_xml_text_nodes(&xml_content, "w:t", &mut full_text);
        }
    }

    // 补充读取页眉页脚 (word/header*.xml, word/footer*.xml)
    for i in 0..zip.len() {
        if let Ok(mut entry) = zip.by_index(i) {
            let name = entry.name().to_string();
            if (name.starts_with("word/header") || name.starts_with("word/footer"))
                && name.ends_with(".xml")
            {
                let mut xml_content = String::new();
                if entry.read_to_string(&mut xml_content).is_ok() {
                    extract_xml_text_nodes(&xml_content, "w:t", &mut full_text);
                }
            }
        }
    }

    if full_text.trim().is_empty() {
        None
    } else {
        Some(ExtractedDoc {
            text: full_text,
            doc_type: "docx",
        })
    }
}

/// 2. Excel 电子表格 (.xlsx) 极速提取
fn extract_xlsx(file: File) -> Option<ExtractedDoc> {
    let reader = BufReader::new(file);
    let mut zip = zip::ZipArchive::new(reader).ok()?;
    let mut full_text = String::new();

    // 提取共享字符串池 xl/sharedStrings.xml
    if let Ok(mut ss_entry) = zip.by_name("xl/sharedStrings.xml") {
        let mut xml_content = String::new();
        if ss_entry.read_to_string(&mut xml_content).is_ok() {
            extract_xml_text_nodes(&xml_content, "t", &mut full_text);
        }
    }

    // 提取各工作表单元格内联文字 xl/worksheets/sheet*.xml
    for i in 0..zip.len() {
        if let Ok(mut entry) = zip.by_index(i) {
            let name = entry.name().to_string();
            if name.starts_with("xl/worksheets/sheet") && name.ends_with(".xml") {
                let mut xml_content = String::new();
                if entry.read_to_string(&mut xml_content).is_ok() {
                    extract_xml_text_nodes(&xml_content, "v", &mut full_text);
                }
            }
        }
    }

    if full_text.trim().is_empty() {
        None
    } else {
        Some(ExtractedDoc {
            text: full_text,
            doc_type: "xlsx",
        })
    }
}

/// 3. PowerPoint 幻灯片 (.pptx) 极速提取
fn extract_pptx(file: File) -> Option<ExtractedDoc> {
    let reader = BufReader::new(file);
    let mut zip = zip::ZipArchive::new(reader).ok()?;
    let mut full_text = String::new();

    for i in 0..zip.len() {
        if let Ok(mut entry) = zip.by_index(i) {
            let name = entry.name().to_string();
            if name.starts_with("ppt/slides/slide") && name.ends_with(".xml") {
                let mut xml_content = String::new();
                if entry.read_to_string(&mut xml_content).is_ok() {
                    extract_xml_text_nodes(&xml_content, "a:t", &mut full_text);
                    full_text.push('\n');
                }
            }
        }
    }

    if full_text.trim().is_empty() {
        None
    } else {
        Some(ExtractedDoc {
            text: full_text,
            doc_type: "pptx",
        })
    }
}

/// 4. PDF 文档 (.pdf) 原生提取 (跳过加密与纯图片件)
fn extract_pdf(path: &Path) -> Option<ExtractedDoc> {
    // 使用 lopdf 原生安全加载
    let doc = match lopdf::Document::load(path) {
        Ok(d) => d,
        Err(_) => return None,
    };

    // 🔒 遇到加密 PDF 立即安全跳过 (符合精益求精原则，不报错不阻塞)
    if doc.is_encrypted() {
        return None;
    }

    let pages = doc.get_pages();
    if pages.is_empty() {
        return None;
    }

    let mut full_text = String::with_capacity(pages.len() * 512);

    // 逐页抽取文本流
    for (page_num, _) in pages {
        if let Ok(text) = doc.extract_text(&[page_num]) {
            if !text.trim().is_empty() {
                full_text.push_str(&text);
                full_text.push('\n');
            }
        }
    }

    // 🖼️ 若提取出的文本为空或几乎无文字 (纯图片扫描件)，安全返回 None
    if full_text.trim().is_empty() {
        None
    } else {
        Some(ExtractedDoc {
            text: full_text,
            doc_type: "pdf",
        })
    }
}

/// 5. 电子书 (.epub) 极速提取
fn extract_epub(file: File) -> Option<ExtractedDoc> {
    let reader = BufReader::new(file);
    let mut zip = zip::ZipArchive::new(reader).ok()?;
    let mut full_text = String::new();

    for i in 0..zip.len() {
        if let Ok(mut entry) = zip.by_index(i) {
            let name = entry.name().to_lowercase();
            if name.ends_with(".xhtml") || name.ends_with(".html") || name.ends_with(".htm") {
                let mut html = String::new();
                if entry.read_to_string(&mut html).is_ok() {
                    strip_html_tags(&html, &mut full_text);
                    full_text.push('\n');
                }
            }
        }
    }

    if full_text.trim().is_empty() {
        None
    } else {
        Some(ExtractedDoc {
            text: full_text,
            doc_type: "epub",
        })
    }
}

/// 6. OpenDocument (.odt, .ods, .odp) 提取
fn extract_opendocument(file: File) -> Option<ExtractedDoc> {
    let reader = BufReader::new(file);
    let mut zip = zip::ZipArchive::new(reader).ok()?;
    let mut full_text = String::new();

    if let Ok(mut content_entry) = zip.by_name("content.xml") {
        let mut xml_content = String::new();
        if content_entry.read_to_string(&mut xml_content).is_ok() {
            strip_html_tags(&xml_content, &mut full_text);
        }
    }

    if full_text.trim().is_empty() {
        None
    } else {
        Some(ExtractedDoc {
            text: full_text,
            doc_type: "odt",
        })
    }
}

/// 7. 纯文本与代码文件提取
fn extract_plain_text(mut file: File) -> Option<ExtractedDoc> {
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;

    // 优先尝试 UTF-8
    let text = if let Ok(s) = String::from_utf8(bytes.clone()) {
        s
    } else {
        // Fallback: 容错损失转换 (GBK / ASCII)
        String::from_utf8_lossy(&bytes).to_string()
    };

    if text.trim().is_empty() {
        None
    } else {
        Some(ExtractedDoc {
            text,
            doc_type: "text",
        })
    }
}

/// 高性能 XML 标签文本提取器 (避免引入重型 DOM 解析树，微秒级流式扫描)
fn extract_xml_text_nodes(xml: &str, target_tag: &str, out: &mut String) {
    let open_tag_prefix = format!("<{}", target_tag);
    let close_tag = format!("</{}>", target_tag);

    let mut cursor = 0;
    while let Some(start_pos) = xml[cursor..].find(&open_tag_prefix) {
        let abs_start = cursor + start_pos;
        // 找到当前开始标签的闭合 '>'
        if let Some(tag_end_rel) = xml[abs_start..].find('>') {
            let content_start = abs_start + tag_end_rel + 1;
            // 找到结束标签 </w:t>
            if let Some(close_rel) = xml[content_start..].find(&close_tag) {
                let content_end = content_start + close_rel;
                let raw_val = &xml[content_start..content_end];
                if !raw_val.is_empty() {
                    unescape_xml_entities(raw_val, out);
                    out.push(' ');
                }
                cursor = content_end + close_tag.len();
                continue;
            }
        }
        cursor = abs_start + open_tag_prefix.len();
    }
}

/// 快速剥离 HTML / XML 标签
fn strip_html_tags(html: &str, out: &mut String) {
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
}

/// 快速转义 XML 基础实体 (&amp;, &lt;, &gt;, &quot;, &apos;)
fn unescape_xml_entities(raw: &str, out: &mut String) {
    if !raw.contains('&') {
        out.push_str(raw);
        return;
    }
    let unescaped = raw
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'");
    out.push_str(&unescaped);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_xml_text_nodes() {
        let sample = r#"<w:p><w:r><w:t>凡响 AnyEcho</w:t></w:r><w:r><w:t xml:space="preserve"> 超级搜索</w:t></w:r></w:p>"#;
        let mut out = String::new();
        extract_xml_text_nodes(sample, "w:t", &mut out);
        assert!(out.contains("凡响 AnyEcho"));
        assert!(out.contains("超级搜索"));
    }

    #[test]
    fn test_supported_extensions() {
        assert!(is_supported_document_ext("docx"));
        assert!(is_supported_document_ext("pdf"));
        assert!(is_supported_document_ext("xlsx"));
        assert!(is_supported_document_ext("pptx"));
        assert!(is_supported_document_ext("txt"));
        assert!(!is_supported_document_ext("exe"));
        assert!(!is_supported_document_ext("zip"));
    }
}
