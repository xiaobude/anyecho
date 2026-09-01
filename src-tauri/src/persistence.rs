use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::{Connection, params};
use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct SearchHistoryEntry {
    pub id: i64,
    pub query: String,
    pub result_count: i64,
    pub searched_at: i64,
}

#[derive(Serialize, Clone, Debug)]
pub struct Favorite {
    pub id: i64,
    pub file_path: String,
    pub file_name: String,
    pub added_at: i64,
}

#[derive(Serialize, Clone, Debug)]
pub struct ExclusionRule {
    pub id: i64,
    pub pattern: String,
    pub is_regex: bool,
    pub created_at: i64,
}

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    pub fn new() -> Result<Self, String> {
        let db_path = get_db_path();
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let conn = Connection::open(&db_path)
            .map_err(|e| format!("Failed to open database: {e}"))?;

        let db = Self { conn: Mutex::new(conn) };
        db.run_migrations()?;
        Ok(db)
    }

    fn run_migrations(&self) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS search_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                query TEXT NOT NULL,
                result_count INTEGER NOT NULL DEFAULT 0,
                searched_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_history_time ON search_history(searched_at DESC);

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS favorites (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_path TEXT NOT NULL UNIQUE,
                file_name TEXT NOT NULL,
                added_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS exclusion_rules (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                pattern TEXT NOT NULL,
                is_regex INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL
            );",
        )
        .map_err(|e| format!("Migration failed: {e}"))?;
        Ok(())
    }

    pub fn save_search(&self, query: &str, result_count: usize) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        conn.execute(
            "INSERT INTO search_history (query, result_count, searched_at) VALUES (?1, ?2, ?3)",
            params![query, result_count as i64, now],
        )
        .map_err(|e| format!("Save search failed: {e}"))?;
        Ok(())
    }

    pub fn get_recent_searches(&self, limit: usize) -> Result<Vec<SearchHistoryEntry>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, query, result_count, searched_at FROM search_history ORDER BY searched_at DESC, id DESC LIMIT ?1")
            .map_err(|e| format!("Prepare failed: {e}"))?;

        let rows = stmt
            .query_map(params![limit as i64], |row| {
                Ok(SearchHistoryEntry {
                    id: row.get(0)?,
                    query: row.get(1)?,
                    result_count: row.get(2)?,
                    searched_at: row.get(3)?,
                })
            })
            .map_err(|e| format!("Query failed: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Collect failed: {e}"))
    }

    pub fn clear_search_history(&self) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM search_history", [])
            .map_err(|e| format!("Clear failed: {e}"))?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .ok()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, value],
        )
        .map_err(|e| format!("Set setting failed: {e}"))?;
        Ok(())
    }

    pub fn add_favorite(&self, file_path: &str, file_name: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        conn.execute(
            "INSERT OR REPLACE INTO favorites (file_path, file_name, added_at) VALUES (?1, ?2, ?3)",
            params![file_path, file_name, now],
        )
        .map_err(|e| format!("Add favorite failed: {e}"))?;
        Ok(())
    }

    pub fn remove_favorite(&self, file_path: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM favorites WHERE file_path = ?1", params![file_path])
            .map_err(|e| format!("Remove favorite failed: {e}"))?;
        Ok(())
    }

    pub fn get_favorites(&self) -> Result<Vec<Favorite>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, file_path, file_name, added_at FROM favorites ORDER BY added_at DESC")
            .map_err(|e| format!("Prepare failed: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(Favorite {
                    id: row.get(0)?,
                    file_path: row.get(1)?,
                    file_name: row.get(2)?,
                    added_at: row.get(3)?,
                })
            })
            .map_err(|e| format!("Query failed: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Collect failed: {e}"))
    }

    pub fn add_exclusion(&self, pattern: &str, is_regex: bool) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        conn.execute(
            "INSERT INTO exclusion_rules (pattern, is_regex, created_at) VALUES (?1, ?2, ?3)",
            params![pattern, is_regex as i32, now],
        )
        .map_err(|e| format!("Add exclusion failed: {e}"))?;
        Ok(())
    }

    pub fn remove_exclusion(&self, id: i64) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM exclusion_rules WHERE id = ?1", params![id])
            .map_err(|e| format!("Remove exclusion failed: {e}"))?;
        Ok(())
    }

    pub fn get_exclusions(&self) -> Result<Vec<ExclusionRule>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, pattern, is_regex, created_at FROM exclusion_rules ORDER BY created_at DESC")
            .map_err(|e| format!("Prepare failed: {e}"))?;
        let rows = stmt
            .query_map([], |row| {
                Ok(ExclusionRule {
                    id: row.get(0)?,
                    pattern: row.get(1)?,
                    is_regex: row.get::<_, i32>(2)? != 0,
                    created_at: row.get(3)?,
                })
            })
            .map_err(|e| format!("Query failed: {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Collect failed: {e}"))
    }
}

pub fn get_db_path() -> PathBuf {
    let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(local_app_data)
        .join("anyecho")
        .join("anyecho.db")
}

pub fn get_snapshot_path() -> PathBuf {
    let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(local_app_data)
        .join("anyecho")
        .join("index_cache.bin")
}


#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_db() -> Database {
        let conn = Connection::open_in_memory().unwrap();
        let db = Database {
            conn: Mutex::new(conn),
        };
        db.run_migrations().unwrap();
        db
    }

    #[test]
    fn test_search_history() {
        let db = create_test_db();
        db.save_search("rust", 10).unwrap();
        db.save_search("anyecho", 5).unwrap();

        let history = db.get_recent_searches(10).unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].query, "anyecho");

        db.clear_search_history().unwrap();
        let empty = db.get_recent_searches(10).unwrap();
        assert!(empty.is_empty());
    }

    #[test]
    fn test_favorites_and_settings() {
        let db = create_test_db();

        db.set_setting("theme", "dark").unwrap();
        assert_eq!(db.get_setting("theme"), Some("dark".to_string()));

        db.add_favorite("C:\\test.txt", "test.txt").unwrap();
        let favs = db.get_favorites().unwrap();
        assert_eq!(favs.len(), 1);
        assert_eq!(favs[0].file_name, "test.txt");

        db.remove_favorite("C:\\test.txt").unwrap();
        let favs_after = db.get_favorites().unwrap();
        assert!(favs_after.is_empty());
    }

    #[test]
    fn test_exclusions() {
        let db = create_test_db();
        db.add_exclusion("C:\\Windows", false).unwrap();
        let rules = db.get_exclusions().unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].pattern, "C:\\Windows");

        db.remove_exclusion(rules[0].id).unwrap();
        let rules_after = db.get_exclusions().unwrap();
        assert!(rules_after.is_empty());
    }
}

