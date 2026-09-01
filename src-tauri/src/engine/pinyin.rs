use pinyin::ToPinyin;

/// 提取中文字符串的拼音信息：(首字母缩写, 全拼)
/// 例如: "凡响" -> (Some("fx"), Some("fanxiang"))
/// 如果字符串不包含任何中文汉字，返回 (None, None) 避免额外内存开销
pub fn extract_pinyin(text: &str) -> (Option<String>, Option<String>) {
    // 检查是否包含 CJK 汉字字符
    let has_cjk = text.chars().any(|c| ('\u{4e00}'..='\u{9fa5}').contains(&c));
    if !has_cjk {
        return (None, None);
    }

    let mut first = String::with_capacity(text.len());
    let mut full = String::with_capacity(text.len() * 4);

    for c in text.chars() {
        if let Some(p) = c.to_pinyin() {
            let plain = p.plain();
            if let Some(first_ch) = plain.chars().next() {
                first.push(first_ch);
            }
            full.push_str(plain);
        } else {
            let lower = c.to_ascii_lowercase();
            first.push(lower);
            full.push(lower);
        }
    }

    (
        Some(first.to_lowercase()),
        Some(full.to_lowercase()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pinyin_extraction() {
        let (first, full) = extract_pinyin("凡响.txt");
        assert_eq!(first, Some("fx.txt".to_string()));
        assert_eq!(full, Some("fanxiang.txt".to_string()));

        let (none_first, none_full) = extract_pinyin("anyecho.rs");
        assert_eq!(none_first, None);
        assert_eq!(none_full, None);
    }
}
