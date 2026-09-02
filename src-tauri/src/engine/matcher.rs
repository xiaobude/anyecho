use crate::engine::filter::{DateFilter, ParsedQuery, SizeFilter, TypeFilter};

pub struct IndexedFile {
    pub name: String,
    pub full_path: String,
    pub name_lower: String,
    pub full_path_lower: String,
    pub pinyin_first: Option<String>,
    pub pinyin_full: Option<String>,
    pub ext: String,
    pub size: u64,
    pub mtime: i64,
    pub is_directory: bool,
    pub file_attributes: u32,
    pub frn: u64,
    pub parent_frn: u64,
    pub volume: char,
}

pub fn matches_query(file: &IndexedFile, query: &ParsedQuery) -> bool {
    // 1. 类型过滤
    if let Some(ref type_filter) = query.type_filter {
        match type_filter {
            TypeFilter::FileOnly if file.is_directory => return false,
            TypeFilter::FolderOnly if !file.is_directory => return false,
            _ => {}
        }
    }

    // 2. 扩展名过滤
    if !query.ext_filters.is_empty() {
        if file.is_directory || !query.ext_filters.iter().any(|ext| ext == &file.ext) {
            return false;
        }
    }

    // 3. 大小过滤
    for filter in &query.size_filters {
        match filter {
            SizeFilter::GreaterThan(min) => {
                if file.size < *min {
                    return false;
                }
            }
            SizeFilter::LessThan(max) => {
                if file.size > *max {
                    return false;
                }
            }
            SizeFilter::Between(min, max) => {
                if file.size < *min || file.size > *max {
                    return false;
                }
            }
        }
    }

    // 4. 日期过滤
    for filter in &query.date_filters {
        match filter {
            DateFilter::After(ts) => {
                if file.mtime < *ts {
                    return false;
                }
            }
            DateFilter::Before(ts) => {
                if file.mtime > *ts {
                    return false;
                }
            }
            DateFilter::Between(min, max) => {
                if file.mtime < *min || file.mtime > *max {
                    return false;
                }
            }
        }
    }

    // 5. 路径包含与排除过滤
    for inc in &query.path_includes {
        if !file.full_path_lower.contains(inc) {
            return false;
        }
    }
    for exc in &query.path_excludes {
        if file.full_path_lower.contains(exc) {
            return false;
        }
    }

    // 6. 正则表达式过滤
    for re in &query.regex_patterns {
        if !re.is_match(&file.name) && !re.is_match(&file.full_path) {
            return false;
        }
    }

    // 7. 文本词条与拼音匹配 (所有词条必须同时满足 AND 关系)
    for term in &query.text_terms {
        if !match_single_term(file, term) {
            return false;
        }
    }

    true
}

fn match_single_term(file: &IndexedFile, term: &str) -> bool {
    // 1. 文件名直接子串匹配
    if file.name_lower.contains(term) {
        return true;
    }

    // 2. 如果包含路径分隔符 (\ 或 /)
    if term.contains('\\') || term.contains('/') {
        let norm_term = term.replace('/', "\\");
        if norm_term.contains('*') || norm_term.contains('?') {
            let full_norm = if norm_term.starts_with('*') {
                norm_term
            } else {
                format!("*{}*", norm_term)
            };
            if wildcard_match(&file.full_path_lower, &full_norm) {
                return true;
            }
        } else if file.full_path_lower.contains(&norm_term) {
            return true;
        }
    }

    // 3. 通配符模式匹配 (* 或 ?) 对文件名
    if (term.contains('*') || term.contains('?')) && wildcard_match(&file.name_lower, term) {
        return true;
    }

    // 4. 中文拼音首字母缩写匹配 (如 fx 匹配 凡响)
    if let Some(ref pinyin_first) = file.pinyin_first {
        if pinyin_first.contains(term) {
            return true;
        }
    }

    // 5. 中文全拼匹配 (如 fanxiang 匹配 凡响)
    if let Some(ref pinyin_full) = file.pinyin_full {
        if pinyin_full.contains(term) {
            return true;
        }
    }

    false

}

/// 快速通配符模式匹配 (*, ?)
fn wildcard_match(text: &str, pattern: &str) -> bool {
    let t_bytes = text.as_bytes();
    let p_bytes = pattern.as_bytes();
    let mut t_idx = 0;
    let mut p_idx = 0;
    let mut star_idx = None;
    let mut match_idx = 0;

    while t_idx < t_bytes.len() {
        if p_idx < p_bytes.len() && (p_bytes[p_idx] == b'?' || p_bytes[p_idx] == t_bytes[t_idx]) {
            t_idx += 1;
            p_idx += 1;
        } else if p_idx < p_bytes.len() && p_bytes[p_idx] == b'*' {
            star_idx = Some(p_idx);
            match_idx = t_idx;
            p_idx += 1;
        } else if let Some(star) = star_idx {
            p_idx = star + 1;
            match_idx += 1;
            t_idx = match_idx;
        } else {
            return false;
        }
    }

    while p_idx < p_bytes.len() && p_bytes[p_idx] == b'*' {
        p_idx += 1;
    }

    p_idx == p_bytes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wildcard() {
        assert!(wildcard_match("anyecho_test.rs", "*.rs"));
        assert!(wildcard_match("anyecho_test.rs", "any*test.?s"));
        assert!(!wildcard_match("anyecho_test.txt", "*.rs"));
    }
}
