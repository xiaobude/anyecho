use std::fs;
use std::path::Path;


use memmap2::Mmap;
use rayon::prelude::*;
use serde::Serialize;

use crate::doc_extractor::{extract_document_text, is_binary_document_ext, is_supported_document_ext};
use crate::engine::matcher::IndexedFile;
use crate::engine::filter::ParsedQuery;

const MAX_CONTENT_SIZE: u64 = 50 * 1024 * 1024; // 50MB


#[derive(Serialize, Clone, Debug)]
pub struct ContentMatch {
    pub file_path: String,
    pub file_name: String,
    pub line_number: u32,
    pub line_text: String,
    pub match_start: usize,
    pub match_end: usize,
    pub size: u64,
    pub mtime: i64,
    pub ext: String,
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
        .filter_map(|file| {
            search_file_first_match(file, &keyword_lower)
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

pub fn search_content_with_query_and_cache(
    files: &[IndexedFile],
    parsed: &ParsedQuery,
    keyword: &str,
    cached_matches: Option<&[crate::doc_cache::CachedDocMatch]>,
) -> ContentSearchResponse {
    let start = std::time::Instant::now();
    let keyword_lower = keyword.to_lowercase();

    // 构建已缓存命中映射表 (路径 -> CachedDocMatch)
    let mut cached_map: std::collections::HashMap<String, &crate::doc_cache::CachedDocMatch> = std::collections::HashMap::new();
    if let Some(cached) = cached_matches {
        for c in cached {
            cached_map.insert(c.file_path.to_lowercase(), c);
        }
    }

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

    // 分离出：已在 SQLite 缓存中命中的文件 vs 需现场磁盘/流式搜索的文件
    let mut all_matches: Vec<ContentMatch> = Vec::new();
    let mut files_to_scan: Vec<&IndexedFile> = Vec::new();

    for file in &candidates {
        let path_lower = file.full_path.to_lowercase();
        if let Some(cached) = cached_map.get(&path_lower) {
            // ⚡ 由 SQLite FTS5 缓存毫秒级直接响应！无需重复磁盘 I/O
            all_matches.push(ContentMatch {
                file_path: cached.file_path.clone(),
                file_name: cached.file_name.clone(),
                line_number: cached.line_number,
                line_text: cached.line_text.clone(),
                match_start: cached.match_start,
                match_end: cached.match_end,
                size: file.size,
                mtime: file.mtime,
                ext: file.ext.clone(),
            });
        } else {

            files_to_scan.push(file);
        }
    }

    // 对未缓存的文件使用多核并行 + Early-Exit 流式短路扫描 (全局达到上限后立即终止其余 I/O 线程)
    let found_count = std::sync::atomic::AtomicUsize::new(all_matches.len());
    let max_target = 200;

    let disk_matches: Vec<ContentMatch> = files_to_scan
        .par_iter()
        .filter_map(|file| {
            if found_count.load(std::sync::atomic::Ordering::Relaxed) >= max_target {
                return None;
            }
            if let Some(m) = search_file_first_match(file, &keyword_lower) {
                found_count.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                Some(m)
            } else {
                None
            }
        })
        .collect();

    all_matches.extend(disk_matches);


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
    parsed: &ParsedQuery,
    keyword: &str,
) -> ContentSearchResponse {
    search_content_with_query_and_cache(files, parsed, keyword, None)
}

/// 核心短路优化：
/// 1. ⚡ 零 I/O 短路：先比对文件名（支持 URL 解码后的中文名），命中立即返回（0ms 零磁盘读取）
/// 2. 单文件找到首个关键词后立即返回，不再向下扫描剩余数万行，极大节省 CPU 和 I/O
pub fn search_file_first_match(file: &IndexedFile, keyword_lower: &str) -> Option<ContentMatch> {
    let decoded_name = decode_percent_encoded(&file.name);
    let decoded_path = decode_percent_encoded(&file.full_path);

    // ⚡ 终极加速：如果关键词本身就在文件名中（例如：通知-强化责任-安全检查.docx）
    // 100% 必然命中！0 毫秒立即返回，完全不触发任何磁盘读写或解压！
    let decoded_name_lower = decoded_name.to_lowercase();
    if let Some(pos) = decoded_name_lower.find(keyword_lower) {
        return Some(ContentMatch {
            file_path: decoded_path,
            file_name: decoded_name.clone(),
            line_number: 0,
            line_text: format!("📄 [文件名命中] {}", decoded_name),
            match_start: pos,
            match_end: pos + keyword_lower.len(),
            size: file.size,
            mtime: file.mtime,
            ext: file.ext.clone(),
        });
    }

    let path = Path::new(&file.full_path);
    if !path.exists() {
        return None;
    }

    // 1. PDF 专属按页逐流流式短路搜索 (命中即停，无需解出全书所有页面)
    if file.ext.eq_ignore_ascii_case("pdf") {
        return search_pdf_first_match(path, keyword_lower, &decoded_path, &decoded_name, file.size, file.mtime, &file.ext);
    }


    // 2. 其余二进制 Office / EPUB / ODT 文档
    if is_binary_document_ext(&file.ext) {
        let extracted = extract_document_text(path)?;
        for (line_idx, line) in extracted.text.lines().enumerate() {
            let line_lower = line.to_lowercase();
            if let Some(pos) = line_lower.find(keyword_lower) {
                return Some(ContentMatch {
                    file_path: decoded_path,
                    file_name: decoded_name,
                    line_number: (line_idx + 1) as u32,
                    line_text: line.trim().to_string(),
                    match_start: pos,
                    match_end: pos + keyword_lower.len(),
                    size: file.size,
                    mtime: file.mtime,
                    ext: file.ext.clone(),
                });
            }
        }
        return None;
    }

    // 3. 纯文本 / 代码文件，使用极速 mmap 内存映射并在首个命中处短路返回
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

    for (line_idx, line) in content.lines().enumerate() {
        let line_lower = line.to_lowercase();
        if let Some(pos) = line_lower.find(keyword_lower) {
            return Some(ContentMatch {
                file_path: decoded_path,
                file_name: decoded_name,
                line_number: (line_idx + 1) as u32,
                line_text: line.trim().to_string(),
                match_start: pos,
                match_end: pos + keyword_lower.len(),
                size: file.size,
                mtime: file.mtime,
                ext: file.ext.clone(),
            });
        }
    }

    None
}

/// PDF 逐页流式提取并在首个匹配页立即短路退出
fn search_pdf_first_match(
    path: &Path,
    keyword_lower: &str,
    decoded_path: &str,
    decoded_name: &str,
    size: u64,
    mtime: i64,
    ext: &str,
) -> Option<ContentMatch> {
    let doc = lopdf::Document::load(path).ok()?;
    if doc.is_encrypted() {
        return None;
    }

    let pages = doc.get_pages();
    let mut total_line_num = 1u32;

    for (page_num, _) in pages {
        if let Ok(text) = doc.extract_text(&[page_num]) {
            for line in text.lines() {
                let line_lower = line.to_lowercase();
                if let Some(pos) = line_lower.find(keyword_lower) {
                    return Some(ContentMatch {
                        file_path: decoded_path.to_string(),
                        file_name: decoded_name.to_string(),
                        line_number: total_line_num,
                        line_text: line.trim().to_string(),
                        match_start: pos,
                        match_end: pos + keyword_lower.len(),
                        size,
                        mtime,
                        ext: ext.to_string(),
                    });
                }
                total_line_num += 1;
            }
        }
    }

    None
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
