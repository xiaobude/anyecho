use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::{doc, Index, IndexReader, IndexWriter, ReloadPolicy};

use crate::content_search::is_text_extension;

const MAX_INDEX_FILE_SIZE: u64 = 10 * 1024 * 1024;

#[derive(Serialize, Clone, Debug)]
pub struct KnowledgeMatch {
    pub file_path: String,
    pub file_name: String,
    pub snippet: String,
    pub score: f32,
}

#[derive(Serialize, Clone, Debug)]
pub struct KnowledgeSearchResponse {
    pub matches: Vec<KnowledgeMatch>,
    pub total_matches: usize,
    pub search_time_us: u64,
}

pub struct KnowledgeIndexer {
    index: Index,
    reader: IndexReader,
    path_field: Field,
    content_field: Field,
    name_field: Field,
}

impl KnowledgeIndexer {
    pub fn new(index_path: &Path) -> Result<Self, String> {
        fs::create_dir_all(index_path)
            .map_err(|e| format!("Failed to create index dir: {e}"))?;

        let mut schema_builder = Schema::builder();
        let path_field = schema_builder.add_text_field("path", STORED);
        let name_field = schema_builder.add_text_field("name", TEXT | STORED);
        let content_field = schema_builder.add_text_field("content", TEXT);
        let schema = schema_builder.build();

        let index = Index::create_in_dir(index_path, schema.clone())
            .or_else(|_| Index::open_in_dir(index_path))
            .map_err(|e| format!("Failed to create/open index: {e}"))?;

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| format!("Failed to create reader: {e}"))?;

        Ok(Self {
            index,
            reader,
            path_field,
            content_field,
            name_field,
        })
    }

    pub fn index_folder(&self, folder_path: &str) -> Result<usize, String> {
        let path = Path::new(folder_path);
        if !path.exists() || !path.is_dir() {
            return Err(format!("Path does not exist or is not a directory: {folder_path}"));
        }

        let mut writer: IndexWriter = self.index.writer(50_000_000)
            .map_err(|e| format!("Failed to create writer: {e}"))?;

        let mut count = 0;
        self.index_directory_recursive(&mut writer, path, &mut count)?;

        writer.commit().map_err(|e| format!("Commit failed: {e}"))?;
        self.reader.reload().map_err(|e| format!("Reload failed: {e}"))?;

        tracing::info!("Indexed {} files from {}", count, folder_path);
        Ok(count)
    }

    fn index_directory_recursive(
        &self,
        writer: &mut IndexWriter,
        dir: &Path,
        count: &mut usize,
    ) -> Result<(), String> {
        let entries = fs::read_dir(dir)
            .map_err(|e| format!("Failed to read dir {}: {e}", dir.display()))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Dir entry error: {e}"))?;
            let path = entry.path();

            if path.is_dir() {
                let dir_name = path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                if dir_name.starts_with('.') || dir_name.starts_with('$') || dir_name == "node_modules" || dir_name == "target" {
                    continue;
                }
                self.index_directory_recursive(writer, &path, count)?;
            } else if path.is_file() {
                let metadata = match fs::metadata(&path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };

                if metadata.len() > MAX_INDEX_FILE_SIZE || metadata.len() == 0 {
                    continue;
                }

                let ext = path.extension()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_default();

                if !is_text_extension(&ext) {
                    continue;
                }

                let content = match fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                if content.trim().is_empty() {
                    continue;
                }

                let file_path = path.to_string_lossy().to_string();
                let file_name = path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                writer.add_document(doc!(
                    self.path_field => file_path.clone(),
                    self.name_field => file_name,
                    self.content_field => content,
                )).map_err(|e| format!("Add doc failed: {e}"))?;

                *count += 1;

                if *count % 1000 == 0 {
                    tracing::info!("Indexed {} files so far...", count);
                }
            }
        }

        Ok(())
    }

    pub fn search(&self, query_str: &str, limit: usize) -> Result<KnowledgeSearchResponse, String> {
        let start = std::time::Instant::now();

        let searcher = self.reader.searcher();
        let query_parser = QueryParser::for_index(&self.index, vec![self.content_field, self.name_field]);
        let query = query_parser.parse_query(query_str)
            .map_err(|e| format!("Query parse error: {e}"))?;

        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))
            .map_err(|e| format!("Search error: {e}"))?;

        let mut matches = Vec::new();

        for (score, doc_address) in top_docs {
            let retrieved_doc = searcher.doc::<TantivyDocument>(doc_address)
                .map_err(|e| format!("Doc retrieve error: {e}"))?;

            let file_path = retrieved_doc.get_first(self.path_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let file_name = retrieved_doc.get_first(self.name_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let content = retrieved_doc.get_first(self.content_field)
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let snippet = extract_snippet(content, query_str, 150);

            matches.push(KnowledgeMatch {
                file_path,
                file_name,
                snippet,
                score,
            });
        }

        let total_matches = matches.len();

        Ok(KnowledgeSearchResponse {
            matches,
            total_matches,
            search_time_us: start.elapsed().as_micros() as u64,
        })
    }

    pub fn clear(&self) -> Result<(), String> {
        let mut writer: IndexWriter = self.index.writer(50_000_000)
            .map_err(|e| format!("Failed to create writer: {e}"))?;
        writer.delete_all_documents().map_err(|e| format!("Clear failed: {e}"))?;
        writer.commit().map_err(|e| format!("Commit failed: {e}"))?;
        self.reader.reload().map_err(|e| format!("Reload failed: {e}"))?;
        Ok(())
    }
}

fn extract_snippet(content: &str, keyword: &str, max_len: usize) -> String {
    let keyword_lower = keyword.to_lowercase();
    let content_lower = content.to_lowercase();

    if let Some(pos) = content_lower.find(&keyword_lower) {
        let start = pos.saturating_sub(50);
        let end = (pos + keyword.len() + 100).min(content.len());
        let snippet = &content[start..end];
        if start > 0 {
            format!("...{}...", snippet)
        } else {
            format!("{}...", snippet)
        }
    } else {
        content.chars().take(max_len).collect()
    }
}

pub fn get_knowledge_index_path() -> PathBuf {
    let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(local_app_data).join("anyecho").join("knowledge_index")
}

pub fn get_knowledge_folders_path() -> PathBuf {
    let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(local_app_data).join("anyecho").join("knowledge_folders.json")
}

pub fn load_knowledge_folders() -> Vec<String> {
    let path = get_knowledge_folders_path();
    if !path.exists() {
        return Vec::new();
    }
    fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_knowledge_folders(folders: &[String]) -> Result<(), String> {
    let path = get_knowledge_folders_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create dir: {e}"))?;
    }
    let json = serde_json::to_string_pretty(folders)
        .map_err(|e| format!("Serialize error: {e}"))?;
    fs::write(&path, json).map_err(|e| format!("Write error: {e}"))?;
    Ok(())
}
