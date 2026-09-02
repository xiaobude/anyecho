use std::fs;
use std::path::Path;

use memmap2::Mmap;
use rayon::prelude::*;
use serde::Serialize;

use crate::engine::matcher::IndexedFile;

const MAX_CONTENT_SIZE: u64 = 10 * 1024 * 1024;

static TEXT_EXTENSIONS: &[&str] = &[
    "txt", "md", "rs", "py", "js", "ts", "json", "csv", "xml", "yaml", "yml",
    "toml", "ini", "cfg", "log", "sh", "bat", "ps1", "html", "css", "svelte",
    "c", "cpp", "h", "hpp", "go", "java", "vue", "jsx", "tsx", "sql", "r",
    "rb", "php", "swift", "kt", "lua", "perl", "pm", "dart", "zig", "nim",
    "hs", "ml", "clj", "scala", "groovy", "dockerfile", "makefile", "cmake",
    "gitignore", "gitattributes", "editorconfig", "env",
];

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
    let lower = ext.to_lowercase();
    TEXT_EXTENSIONS.contains(&lower.as_str())
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
            if !is_text_extension(&f.ext) {
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
            if !is_text_extension(&f.ext) {
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


fn search_file_content(file: &IndexedFile, keyword_lower: &str) -> Option<Vec<ContentMatch>> {
    let path = Path::new(&file.full_path);
    if !path.exists() {
        return None;
    }

    let file_handle = fs::File::open(path).ok()?;
    let metadata = file_handle.metadata().ok()?;

    if metadata.len() > MAX_CONTENT_SIZE {
        return None;
    }

    if metadata.len() == 0 {
        return None;
    }

    let mmap = unsafe { Mmap::map(&file_handle) }.ok()?;
    let content = std::str::from_utf8(&mmap).ok()?;

    let mut matches = Vec::new();

    for (line_idx, line) in content.lines().enumerate() {
        let line_lower = line.to_lowercase();
        if let Some(pos) = line_lower.find(keyword_lower) {
            matches.push(ContentMatch {
                file_path: file.full_path.clone(),
                file_name: file.name.clone(),
                line_number: (line_idx + 1) as u32,
                line_text: line.to_string(),
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

    let content = fs::read_to_string(path).ok()?;
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
        assert!(is_text_extension("rs"));
        assert!(is_text_extension("md"));
        assert!(is_text_extension("txt"));
        assert!(is_text_extension("json"));
        assert!(!is_text_extension("exe"));
        assert!(!is_text_extension("iso"));
    }

    #[test]
    fn test_content_search_and_preview() {
        let temp_dir = std::env::temp_dir().join("anyecho_test_content");
        let _ = fs::create_dir_all(&temp_dir);
        let test_file = temp_dir.join("sample.txt");
        fs::write(
            &test_file,
            "Line 1: AnyEcho search engine\nLine 2: High performance file retrieval\nLine 3: Everything alternative for Windows\n",
        ).unwrap();

        let indexed = IndexedFile {
            name: "sample.txt".to_string(),
            full_path: test_file.to_string_lossy().to_string(),
            name_lower: "sample.txt".to_string(),
            full_path_lower: test_file.to_string_lossy().to_lowercase(),
            pinyin_first: None,
            pinyin_full: None,
            ext: "txt".to_string(),
            size: 100,
            mtime: 0,
            is_directory: false,
            file_attributes: 0,
            frn: 1,
            parent_frn: 0,
            volume: 'C',
        };

        let resp = search_content(&[indexed], "Everything", None);
        assert_eq!(resp.total_matches, 1);
        assert_eq!(resp.matches[0].line_number, 3);

        let preview = get_content_preview(&test_file.to_string_lossy(), "performance", 1);
        assert!(preview.is_some());
        let p = preview.unwrap();
        assert_eq!(p.keyword, "performance");
        assert!(!p.lines.is_empty());

        let _ = fs::remove_file(&test_file);
        let _ = fs::remove_dir(&temp_dir);
    }
}

