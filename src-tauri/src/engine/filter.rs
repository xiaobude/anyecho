use std::time::{SystemTime, UNIX_EPOCH};
use regex::Regex;

#[derive(Debug, Clone)]
pub enum SizeFilter {
    GreaterThan(u64),
    LessThan(u64),
    Between(u64, u64),
}

#[derive(Debug, Clone)]
pub enum DateFilter {
    After(i64),
    Before(i64),
    Between(i64, i64),
}

#[derive(Debug, Clone)]
pub enum TypeFilter {
    FileOnly,
    FolderOnly,
}

#[derive(Debug, Clone)]
pub struct ParsedQuery {
    pub text_terms: Vec<String>,
    pub pinyin_terms: Vec<String>,
    pub regex_patterns: Vec<Regex>,
    pub ext_filters: Vec<String>,
    pub size_filters: Vec<SizeFilter>,
    pub date_filters: Vec<DateFilter>,
    pub type_filter: Option<TypeFilter>,
    pub path_includes: Vec<String>,
    pub path_excludes: Vec<String>,
    pub content_terms: Vec<String>,
}

impl ParsedQuery {
    pub fn parse(query_str: &str) -> Self {
        let mut text_terms = Vec::new();
        let mut regex_patterns = Vec::new();
        let mut ext_filters = Vec::new();
        let mut size_filters = Vec::new();
        let mut date_filters = Vec::new();
        let mut type_filter = None;
        let mut path_includes = Vec::new();
        let mut path_excludes = Vec::new();
        let mut content_terms = Vec::new();

        let tokens = query_str.split_whitespace();

        for token in tokens {
            let lower_token = token.to_lowercase();

            if let Some(content) = lower_token.strip_prefix("content:") {
                if !content.is_empty() {
                    content_terms.push(content.to_string());
                }
            } else if let Some(ext) = lower_token.strip_prefix("ext:") {
                for e in ext.split('|') {
                    let cleaned = e.trim_start_matches('.').trim();
                    if !cleaned.is_empty() {
                        ext_filters.push(cleaned.to_string());
                    }
                }
            } else if let Some(size_str) = lower_token.strip_prefix("size:") {
                if let Some(filter) = parse_size_filter(size_str) {
                    size_filters.push(filter);
                }
            } else if let Some(dm_str) = lower_token.strip_prefix("dm:") {
                if let Some(filter) = parse_date_filter(dm_str) {
                    date_filters.push(filter);
                }
            } else if let Some(mod_str) = lower_token.strip_prefix("modified:") {
                if let Some(filter) = parse_date_filter(mod_str) {
                    date_filters.push(filter);
                }
            } else if lower_token == "type:folder" || lower_token == "type:dir" || lower_token == "folder:" {
                type_filter = Some(TypeFilter::FolderOnly);
            } else if lower_token == "type:file" || lower_token == "file:" {
                type_filter = Some(TypeFilter::FileOnly);
            } else if lower_token == "type:ai" || lower_token == "type:model" || lower_token == "ai:" || lower_token == "model:" {
                for e in &[
                    "gguf", "safetensors", "pt", "pth", "onnx", "bin", "ckpt", "tflite",
                    "engine", "trt", "nvfp4", "fp8", "awq", "gptq", "ggml", "mlmodel",
                    "weights", "h5", "pb", "torchscript", "modelfile"
                ] {
                    ext_filters.push(e.to_string());
                }
            } else if let Some(p) = lower_token.strip_prefix("-path:") {

                path_excludes.push(p.to_string());
            } else if let Some(p) = lower_token.strip_prefix("!path:") {
                path_excludes.push(p.to_string());
            } else if let Some(p) = lower_token.strip_prefix("path:") {
                path_includes.push(p.to_string());
            } else if let Some(re_str) = token.strip_prefix("regex:") {
                if let Ok(re) = Regex::new(re_str) {
                    regex_patterns.push(re);
                }
            } else if token.starts_with('/') && token.ends_with('/') && token.len() > 2 {
                let pattern = &token[1..token.len() - 1];
                if let Ok(re) = Regex::new(pattern) {
                    regex_patterns.push(re);
                }
            } else {
                text_terms.push(lower_token.clone());
            }
        }

        Self {
            pinyin_terms: text_terms.clone(),
            text_terms,
            regex_patterns,
            ext_filters,
            size_filters,
            date_filters,
            type_filter,
            path_includes,
            path_excludes,
            content_terms,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.text_terms.is_empty()
            && self.regex_patterns.is_empty()
            && self.ext_filters.is_empty()
            && self.size_filters.is_empty()
            && self.date_filters.is_empty()
            && self.type_filter.is_none()
            && self.path_includes.is_empty()
            && self.path_excludes.is_empty()
            && self.content_terms.is_empty()
    }

    pub fn has_content_search(&self) -> bool {
        !self.content_terms.is_empty()
    }
}

fn parse_size_filter(s: &str) -> Option<SizeFilter> {
    if let Some(val) = s.strip_prefix('>') {
        let bytes = parse_size_bytes(val)?;
        Some(SizeFilter::GreaterThan(bytes))
    } else if let Some(val) = s.strip_prefix('<') {
        let bytes = parse_size_bytes(val)?;
        Some(SizeFilter::LessThan(bytes))
    } else if let Some((min_str, max_str)) = s.split_once('-') {
        let min = parse_size_bytes(min_str)?;
        let max = parse_size_bytes(max_str)?;
        Some(SizeFilter::Between(min, max))
    } else {
        let bytes = parse_size_bytes(s)?;
        Some(SizeFilter::GreaterThan(bytes))
    }
}

fn parse_size_bytes(s: &str) -> Option<u64> {
    let s = s.trim();
    let (num_part, unit) = s
        .chars()
        .position(|c| !c.is_ascii_digit() && c != '.')
        .map_or((s, ""), |pos| s.split_at(pos));

    let num: f64 = num_part.parse().ok()?;
    let multiplier = match unit.trim().to_lowercase().as_str() {
        "b" | "" => 1.0,
        "k" | "kb" => 1024.0,
        "m" | "mb" => 1024.0 * 1024.0,
        "g" | "gb" => 1024.0 * 1024.0 * 1024.0,
        "t" | "tb" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };

    Some((num * multiplier) as u64)
}

fn parse_date_filter(s: &str) -> Option<DateFilter> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;

    let day_secs = 86400;

    match s.to_lowercase().as_str() {
        "today" => Some(DateFilter::After(now - day_secs)),
        "yesterday" => Some(DateFilter::Between(now - 2 * day_secs, now - day_secs)),
        "thisweek" | "7d" => Some(DateFilter::After(now - 7 * day_secs)),
        "thismonth" | "30d" => Some(DateFilter::After(now - 30 * day_secs)),
        "thisyear" | "365d" => Some(DateFilter::After(now - 365 * day_secs)),
        _ => {
            if let Some(val) = s.strip_prefix('>') {
                let ts = parse_iso_date(val)?;
                Some(DateFilter::After(ts))
            } else if let Some(val) = s.strip_prefix('<') {
                let ts = parse_iso_date(val)?;
                Some(DateFilter::Before(ts))
            } else {
                let ts = parse_iso_date(s)?;
                Some(DateFilter::After(ts))
            }
        }
    }
}

fn parse_iso_date(s: &str) -> Option<i64> {
    // 简单解析 YYYY-MM-DD
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() == 3 {
        let year: i32 = parts[0].parse().ok()?;
        let month: u32 = parts[1].parse().ok()?;
        let day: u32 = parts[2].parse().ok()?;
        if (1970..=2099).contains(&year) && (1..=12).contains(&month) && (1..=31).contains(&day) {
            // 粗略估算 timestamp
            let days_since_1970 = (year - 1970) as i64 * 365 + ((year - 1968) / 4) as i64 + (month as i64 * 30) + day as i64;
            return Some(days_since_1970 * 86400);
        }
    }
    None
}
