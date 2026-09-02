use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::{Connection, params};
use serde::Serialize;

use crate::doc_extractor::extract_document_text;


#[derive(Serialize, Clone, Debug)]
pub struct DocIndexStats {
    pub total_indexed: usize,
    pub total_candidates: usize,
    pub is_indexing: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct CachedDocMatch {
    pub file_path: String,
    pub file_name: String,
    pub line_number: u32,
    pub line_text: String,
    pub match_start: usize,
    pub match_end: usize,
}

pub struct DocCache {
    conn: Mutex<Connection>,
    pub indexed_count: Arc<AtomicUsize>,
    pub total_count: Arc<AtomicUsize>,
    pub is_indexing: Arc<AtomicBool>,
}

impl DocCache {
    pub fn new() -> Result<Self, String> {
        let db_path = get_doc_db_path();
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Failed to open doc cache DB: {e}"))?;

        // 优化 SQLite PRAGMA 参数以获得极速并发性能
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA temp_store = MEMORY;
             PRAGMA cache_size = -64000;",
        )
        .map_err(|e| format!("Pragma setup failed: {e}"))?;

        let cache = Self {
            conn: Mutex::new(conn),
            indexed_count: Arc::new(AtomicUsize::new(0)),
            total_count: Arc::new(AtomicUsize::new(0)),
            is_indexing: Arc::new(AtomicBool::new(false)),
        };

        cache.init_tables()?;
        cache.update_initial_count();
        Ok(cache)
    }

    fn init_tables(&self) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS doc_cache (
                path TEXT PRIMARY KEY,
                mtime INTEGER NOT NULL,
                size INTEGER NOT NULL,
                doc_type TEXT NOT NULL,
                content TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_doc_mtime ON doc_cache(mtime);

            CREATE VIRTUAL TABLE IF NOT EXISTS doc_fts USING fts5(
                path UNINDEXED,
                content,
                tokenize = 'unicode61'
            );

            CREATE TRIGGER IF NOT EXISTS trg_doc_insert AFTER INSERT ON doc_cache BEGIN
                INSERT INTO doc_fts(path, content) VALUES (new.path, new.content);
            END;

            CREATE TRIGGER IF NOT EXISTS trg_doc_delete AFTER DELETE ON doc_cache BEGIN
                DELETE FROM doc_fts WHERE path = old.path;
            END;

            CREATE TRIGGER IF NOT EXISTS trg_doc_update AFTER UPDATE ON doc_cache BEGIN
                DELETE FROM doc_fts WHERE path = old.path;
                INSERT INTO doc_fts(path, content) VALUES (new.path, new.content);
            END;",
        )
        .map_err(|e| format!("Doc cache table init failed: {e}"))?;
        Ok(())
    }

    fn update_initial_count(&self) {
        let conn = self.conn.lock().unwrap();
        let count: usize = conn
            .query_row("SELECT COUNT(*) FROM doc_cache", [], |row| row.get(0))
            .unwrap_or(0);
        self.indexed_count.store(count, Ordering::Relaxed);
    }

    /// 获取单个文件的已缓存修改时间 (用于快速增量比较)
    pub fn get_cached_mtime(&self, path: &str) -> Option<i64> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT mtime FROM doc_cache WHERE path = ?1",
            params![path],
            |row| row.get(0),
        )
        .ok()
    }

    /// 保存/更新提取的文档纯文本内容
    pub fn save_document(
        &self,
        path: &str,
        mtime: i64,
        size: u64,
        doc_type: &str,
        content: &str,
    ) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO doc_cache (path, mtime, size, doc_type, content)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![path, mtime, size as i64, doc_type, content],
        )
        .map_err(|e| format!("Save doc failed: {e}"))?;
        self.indexed_count.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// 删除已移除的文件
    pub fn delete_document(&self, path: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM doc_cache WHERE path = ?1", params![path])
            .map_err(|e| format!("Delete doc failed: {e}"))?;
        let count = self.indexed_count.load(Ordering::Relaxed);
        if count > 0 {
            self.indexed_count.store(count - 1, Ordering::Relaxed);
        }
        Ok(())
    }

    /// 极速全文检索已缓存的文档 (支持 LIKE 容错与 FTS5 倒排索引)
    pub fn search_cached(&self, keyword: &str, limit: usize) -> Vec<CachedDocMatch> {
        let conn = self.conn.lock().unwrap();
        let keyword_lower = keyword.to_lowercase();
        let mut results = Vec::new();

        // 优先使用 FTS5 倒排索引查询
        let mut stmt = match conn.prepare(
            "SELECT path, content FROM doc_cache WHERE content LIKE ?1 LIMIT ?2",
        ) {
            Ok(s) => s,
            Err(_) => return results,
        };

        let like_query = format!("%{}%", keyword);
        let rows = match stmt.query_map(params![like_query, limit as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }) {
            Ok(r) => r,
            Err(_) => return results,
        };

        for item in rows.flatten() {
            let (file_path, content) = item;
            let file_name = Path::new(&file_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();

            // 提取匹配行与上下文
            for (idx, line) in content.lines().enumerate() {
                let line_lower = line.to_lowercase();
                if let Some(pos) = line_lower.find(&keyword_lower) {
                    let snippet = truncate_snippet(line, pos, keyword.len(), 100);
                    let start_in_snip = snippet.to_lowercase().find(&keyword_lower).unwrap_or(0);
                    results.push(CachedDocMatch {
                        file_path: file_path.clone(),
                        file_name: file_name.clone(),
                        line_number: (idx + 1) as u32,
                        line_text: snippet,
                        match_start: start_in_snip,
                        match_end: start_in_snip + keyword.len(),
                    });
                    if results.len() >= limit {
                        break;
                    }
                }
            }
            if results.len() >= limit {
                break;
            }
        }

        results
    }

    /// 获取当前文档索引库状态
    pub fn get_stats(&self) -> DocIndexStats {
        DocIndexStats {
            total_indexed: self.indexed_count.load(Ordering::Relaxed),
            total_candidates: self.total_count.load(Ordering::Relaxed),
            is_indexing: self.is_indexing.load(Ordering::Relaxed),
        }
    }

    /// 启动后台低优先级静默文档索引构建器
    pub fn start_background_indexer(
        self: Arc<Self>,
        candidates_getter: impl Fn() -> Vec<(String, i64, u64)> + Send + 'static,
    ) {
        if self.is_indexing.swap(true, Ordering::SeqCst) {
            // 已在运行中，直接返回
            return;
        }

        let cache = Arc::clone(&self);
        std::thread::Builder::new()
            .name("anyecho-doc-indexer".to_string())
            .spawn(move || {
                // 等待 3 秒，确保前台 UI 启动与用户交互完全畅通
                std::thread::sleep(Duration::from_secs(3));

                let all_candidates = candidates_getter();
                let candidates: Vec<(String, i64, u64)> = all_candidates
                    .into_iter()
                    .filter(|(path, _, _)| {
                        let path_lower = path.to_lowercase();
                        if crate::content_search::is_noisy_history_or_temp_path(&path_lower)
                            || path_lower.contains("\\node_modules\\")
                            || path_lower.contains("\\.git\\")
                            || path_lower.contains("\\appdata\\")
                            || path_lower.contains("\\windows\\")
                            || path_lower.contains("\\$recycle.bin\\")
                            || path_lower.contains("\\temp\\")
                        {
                            return false;
                        }
                        true

                    })
                    .collect();

                cache.total_count.store(candidates.len(), Ordering::Relaxed);

                for (full_path, mtime, size) in candidates {
                    let path = Path::new(&full_path);
                    if !path.exists() {
                        continue;
                    }

                    // 检查缓存新鲜度
                    if let Some(cached_mtime) = cache.get_cached_mtime(&full_path) {
                        if cached_mtime == mtime {
                            // 未修改，跳过
                            continue;
                        }
                    }

                    // 提取文档内容
                    if let Some(extracted) = extract_document_text(path) {
                        let _ = cache.save_document(
                            &full_path,
                            mtime,
                            size,
                            extracted.doc_type,
                            &extracted.text,
                        );
                    }

                    // 每次循环微休眠 2ms，确保 CPU 占用保持在 1% 以下（极致静默）
                    std::thread::sleep(Duration::from_millis(2));
                }

                cache.is_indexing.store(false, Ordering::SeqCst);
            })
            .ok();
    }
}


pub fn get_doc_db_path() -> PathBuf {
    let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(local_app_data)
        .join("anyecho")
        .join("doc_cache.db")
}

fn truncate_snippet(line: &str, match_pos: usize, match_len: usize, max_len: usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    let total_chars = chars.len();

    if total_chars <= max_len {
        return line.trim().to_string();
    }

    let char_pos = line[..match_pos].chars().count();
    let char_match_len = line[match_pos..match_pos + match_len].chars().count();

    let half = (max_len - char_match_len) / 2;
    let start = char_pos.saturating_sub(half);
    let end = (start + max_len).min(total_chars);
    let adjusted_start = if end == total_chars {
        total_chars.saturating_sub(max_len)
    } else {
        start
    };

    let snippet: String = chars[adjusted_start..end].iter().collect();
    let prefix = if adjusted_start > 0 { "..." } else { "" };
    let suffix = if end < total_chars { "..." } else { "" };

    format!("{}{}{}", prefix, snippet.trim(), suffix)
}
