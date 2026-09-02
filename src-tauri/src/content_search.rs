use std::fs;
use std::path::Path;

use memmap2::Mmap;
use rayon::prelude::*;
use serde::Serialize;

use crate::doc_extractor::{extract_document_text, is_binary_document_ext, is_supported_document_ext};
use crate::engine::matcher::IndexedFile;

const MAX_CONTENT_SIZE: u64 = 50 * 1024 * 1024; // 50MB

#[derive(Serialize, Clone, Debug)]
pub struct ContentMatch {
    pub file_path: String,
    pub file_name: String,
    pub line_number: u32,
    pub line_text: String,
    pub match_start: usize,
    pub match_end: usize,
}

#[derive(Serialize, Clone, Debug)]
pub struct ContentSearchResponse {
    pub matches: Vec<ContentMatch>,
    pub files_searched: usize,
    pub total_matches: usize,
    pub search_time_us: u64,
    pub is_complete: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct PreviewLine {
    pub line_number: u32,
    pub text: String,
    pub is_match: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct ContentPreview {
    pub file_path: String,
    pub lines: Vec<PreviewLine>,
    pub keyword: String,
}

pub fn is_text_extension(ext: &str) -> bool {
    is_supported_document_ext(ext)
}

/// 过滤命令行历史、AnyEcho自身日志、IDE/Agent临时日志与无意义临时输出（防止自己搜索的命令或日志被当成结果命中）
pub fn is_noisy_history_or_temp_path(path_lower: &str) -> bool {
    // 1. 终端与命令行历史记录文件
    if path_lower.ends_with("consolehost_history.txt")
        || path_lower.ends_with(".bash_history")
        || path_lower.ends_with(".zsh_history")
        || path_lower.ends_with(".node_repl_history")
        || path_lower.ends_with(".python_history")
        || path_lower.contains("\\psreadline\\")
    {
        return true;
    }

    // 2. AnyEcho 自身的日志与数据缓存文件
    if path_lower.contains("\\anyecho\\logs\\")
        || path_lower.ends_with("anyecho.db")
        || path_lower.ends_with("doc_cache.db")
        || path_lower.ends_with("index_cache.bin")
    {
        return true;
    }

    // 3. 开发环境、Agent/大模型系统日志、临时 step 输出与 Temp 缓存
    if path_lower.contains("\\.gemini\\")
        || path_lower.contains("\\antigravity-cli\\")
        || path_lower.contains("\\.system_generated\\")
        || path_lower.contains("\\appdata\\local\\temp\\")
        || path_lower.contains("\\.vscode\\")
        || path_lower.contains("\\.idea\\")
    {
        return true;
    }

    false
}


/// 智能反解微信/浏览器下载文件时保存的 URL 百分号转义文件名 (如 %E6%9C%8D%E5%8A%A1%E5%90%88%E5%90%8C -> 服务合同)
pub fn decode_percent_encoded(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let bytes = s.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16) {
                decoded.push(byte);
                i += 3;
                continue;
            }
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(decoded).unwrap_or_else(|_| s.to_string())
}

pub fn search_content(
    files: &[IndexedFile],
    keyword: &str,
    file_filter: Option<&str>,
) -> ContentSearchResponse {
    let start = std::time::Instant::now();
    let keyword_lower = keyword.to_lowercase();

    let candidates: Vec<&IndexedFile> = files
        .iter()
        .filter(|f| {
            if f.is_directory {
                return false;
            }
            if !is_supported_document_ext(&f.ext) {
                return false;
            }
            if f.size > MAX_CONTENT_SIZE {
                return false;
            }
            if is_noisy_history_or_temp_path(&f.full_path_lower) {
                return false;
            }
            if let Some(filter) = file_filter {
                if !f.name_lower.contains(filter) && !f.full_path_lower.contains(filter) {
                    return false;
                }
            }
            true
        })
        .collect();

    let files_searched = candidates.len();

    let mut all_matches: Vec<ContentMatch> = candidates
        .par_iter()
        .flat_map(|file| {
            search_file_content(file, &keyword_lower).unwrap_or_default()
        })
        .collect();

    all_matches.sort_by(|a, b| {
        a.file_path.cmp(&b.file_path).then(a.line_number.cmp(&b.line_number))
    });

    let total_matches = all_matches.len();

    ContentSearchResponse {
        matches: all_matches,
        files_searched,
        total_matches,
        search_time_us: start.elapsed().as_micros() as u64,
        is_complete: true,
    }
}

pub fn search_content_with_query(
    files: &[IndexedFile],
    parsed: &crate::engine::filter::ParsedQuery,
    keyword: &str,
) -> ContentSearchResponse {
    let start = std::time::Instant::now();
    let keyword_lower = keyword.to_lowercase();

    let candidates: Vec<&IndexedFile> = files
        .iter()
        .filter(|f| {
            if f.is_directory {
                return false;
            }
            if !is_supported_document_ext(&f.ext) {
                return false;
            }
            if f.size > MAX_CONTENT_SIZE {
                return false;
            }
            if is_noisy_history_or_temp_path(&f.full_path_lower) {
                return false;
            }
            if !crate::engine::matcher::matches_query(f, parsed) {
                return false;
            }
            true
        })
        .collect();

    let files_searched = candidates.len();

    let mut all_matches: Vec<ContentMatch> = candidates
        .par_iter()
        .flat_map(|file| {
            search_file_content(file, &keyword_lower).unwrap_or_default()
        })
        .collect();

    all_matches.sort_by(|a, b| {
        a.file_path.cmp(&b.file_path).then(a.line_number.cmp(&b.line_number))
    });

    let total_matches = all_matches.len();

    ContentSearchResponse {
        matches: all_matches,
        files_searched,
        total_matches,
        search_time_us: start.elapsed().as_micros() as u64,
        is_complete: true,
    }
}

pub fn search_file_content(file: &IndexedFile, keyword_lower: &str) -> Option<Vec<ContentMatch>> {
    let path = Path::new(&file.full_path);
    if !path.exists() {
        return None;
    }

    let decoded_path = decode_percent_encoded(&file.full_path);
    let decoded_name = decode_percent_encoded(&file.name);

    // 1. 若为二进制 Office / PDF / EPUB 文档，使用专用提取引擎
    if is_binary_document_ext(&file.ext) {
        let extracted = extract_document_text(path)?;
        let mut matches = Vec::new();
        for (line_idx, line) in extracted.text.lines().enumerate() {
            let line_lower = line.to_lowercase();
            if let Some(pos) = line_lower.find(keyword_lower) {
                matches.push(ContentMatch {
                    file_path: decoded_path.clone(),
                    file_name: decoded_name.clone(),
                    line_number: (line_idx + 1) as u32,
                    line_text: line.trim().to_string(),
                    match_start: pos,
                    match_end: pos + keyword_lower.len(),
                });
            }
        }
        return if matches.is_empty() { None } else { Some(matches) };
    }

    // 2. 若为纯文本 / 代码文件，使用极速 mmap 内存映射
    let file_handle = fs::File::open(path).ok()?;
    let metadata = file_handle.metadata().ok()?;

    if metadata.len() > MAX_CONTENT_SIZE || metadata.len() == 0 {
        return None;
    }

    let mmap = unsafe { Mmap::map(&file_handle) }.ok()?;
    let content = if let Ok(s) = std::str::from_utf8(&mmap) {
        s
    } else {
        return None;
    };

    let mut matches = Vec::new();

    for (line_idx, line) in content.lines().enumerate() {
        let line_lower = line.to_lowercase();
        if let Some(pos) = line_lower.find(keyword_lower) {
            matches.push(ContentMatch {
                file_path: decoded_path.clone(),
                file_name: decoded_name.clone(),
                line_number: (line_idx + 1) as u32,
                line_text: line.trim().to_string(),
                match_start: pos,
                match_end: pos + keyword_lower.len(),
            });
        }
    }

    if matches.is_empty() {
        None
    } else {
        Some(matches)
    }
}

pub fn get_content_preview(file_path: &str, keyword: &str, context_lines: u32) -> Option<ContentPreview> {
    let path = Path::new(file_path);
    if !path.exists() {
        return None;
    }

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let content = if is_binary_document_ext(ext) {
        extract_document_text(path).map(|d| d.text)?
    } else {
        fs::read_to_string(path).ok()?
    };

    let keyword_lower = keyword.to_lowercase();
    let lines: Vec<&str> = content.lines().collect();

    let mut match_line_numbers: Vec<u32> = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if line.to_lowercase().contains(&keyword_lower) {
            match_line_numbers.push((idx + 1) as u32);
        }
    }

    if match_line_numbers.is_empty() {
        return None;
    }

    let first_match = match_line_numbers[0];
    let context = context_lines as i64;
    let start_line = (first_match as i64 - context).max(1) as usize;
    let last_match = *match_line_numbers.last().unwrap();
    let end_line = (last_match as i64 + context).min(lines.len() as i64) as usize;

    let mut preview_lines = Vec::new();
    for line_num in start_line..=end_line {
        let line_idx = line_num - 1;
        if line_idx < lines.len() {
            let is_match = lines[line_idx].to_lowercase().contains(&keyword_lower);
            preview_lines.push(PreviewLine {
                line_number: line_num as u32,
                text: lines[line_idx].to_string(),
                is_match,
            });
        }
    }

    Some(ContentPreview {
        file_path: file_path.to_string(),
        lines: preview_lines,
        keyword: keyword.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_text_extensions() {
        assert!(is_text_extension("txt"));
        assert!(is_text_extension("md"));
        assert!(is_text_extension("rs"));
        assert!(is_text_extension("docx"));
        assert!(is_text_extension("pdf"));
        assert!(is_text_extension("xlsx"));
        assert!(is_text_extension("pptx"));
        assert!(!is_text_extension("exe"));
        assert!(!is_text_extension("dll"));
    }

    #[test]
    fn test_decode_percent_encoded() {
        let raw = "%E6%98%9F%E6%B5%B7%E5%90%8D%E5%9F%8E%E7%89%A9%E4%B8%9A%E6%9C%8D%E5%8A%A1%E5%90%88%E5%90%8C.pdf";
        let decoded = decode_percent_encoded(raw);
        assert_eq!(decoded, "星海名城物业服务合同.pdf");
    }

    #[test]
    fn test_noisy_path_filter() {
        assert!(is_noisy_history_or_temp_path("c:\\users\\admin\\appdata\\roaming\\microsoft\\windows\\powershell\\psreadline\\consolehost_history.txt"));
        assert!(is_noisy_history_or_temp_path("c:\\ai\\anyecho\\.system_generated\\logs\\1.txt"));
        assert!(!is_noisy_history_or_temp_path("d:\\work\\report.docx"));
    }
}
