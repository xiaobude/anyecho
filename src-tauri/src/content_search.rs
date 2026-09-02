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

    // 1. 若为二进制 Office / PDF / EPUB 文档，使用专用提取引擎
    if is_binary_document_ext(&file.ext) {
        let extracted = extract_document_text(path)?;
        let mut matches = Vec::new();
        for (line_idx, line) in extracted.text.lines().enumerate() {
            let line_lower = line.to_lowercase();
            if let Some(pos) = line_lower.find(keyword_lower) {
                matches.push(ContentMatch {
                    file_path: file.full_path.clone(),
                    file_name: file.name.clone(),
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
                file_path: file.full_path.clone(),
                file_name: file.name.clone(),
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
    fn test_content_search_and_preview() {
        let temp_dir = std::env::temp_dir();
        let test_file = temp_dir.join("anyecho_test_search.txt");
        let content = "First line\nSecond line with keyword target\nThird line\nFourth line with target again\nFifth line";
        fs::write(&test_file, content).unwrap();

        let indexed = IndexedFile {
            name: "anyecho_test_search.txt".to_string(),
            full_path: test_file.to_str().unwrap().to_string(),
            name_lower: "anyecho_test_search.txt".to_string(),
            full_path_lower: test_file.to_str().unwrap().to_lowercase(),
            pinyin_first: None,
            pinyin_full: None,
            ext: "txt".to_string(),
            size: content.len() as u64,
            mtime: 123456789,
            is_directory: false,
            file_attributes: 0,
            frn: 1,
            parent_frn: 0,
            volume: 'C',
        };

        let response = search_content(&[indexed], "target", None);
        assert_eq!(response.matches.len(), 2);
        assert_eq!(response.matches[0].line_number, 2);
        assert_eq!(response.matches[1].line_number, 4);

        let preview = get_content_preview(test_file.to_str().unwrap(), "target", 1).unwrap();
        assert_eq!(preview.lines.len(), 5);

        let _ = fs::remove_file(test_file);
    }
}
