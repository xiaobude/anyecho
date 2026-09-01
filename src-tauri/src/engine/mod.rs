pub mod filter;
pub mod matcher;
pub mod pinyin;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use rayon::prelude::*;
use parking_lot::RwLock;
use serde::Serialize;

use crate::path_tree::ResolvedFile;
use self::filter::ParsedQuery;
use self::matcher::{matches_query, IndexedFile};
use self::pinyin::extract_pinyin;

#[derive(Serialize, Clone, Debug)]
pub struct SearchItemDto {
    pub name: String,
    pub full_path: String,
    pub size: u64,
    pub mtime: i64,
    pub is_directory: bool,
    pub ext: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct SearchResponse {
    pub items: Vec<SearchItemDto>,
    pub total_matches: usize,
    pub total_files: usize,
    pub search_time_us: u64,
}

pub struct SearchEngine {
    files: Vec<IndexedFile>,
    frn_to_index: HashMap<u64, usize>,
    children_map: HashMap<u64, Vec<usize>>,
    exclusions: Vec<String>,
}

impl SearchEngine {
    pub fn new() -> Self {
        Self {
            files: Vec::new(),
            frn_to_index: HashMap::new(),
            children_map: HashMap::new(),
            exclusions: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn set_exclusions(&mut self, patterns: Vec<String>) {

        self.exclusions = patterns.iter().map(|p| p.to_lowercase()).collect();
    }

    fn is_excluded(&self, path_lower: &str) -> bool {
        self.exclusions.iter().any(|ex| path_lower.starts_with(ex))
    }

    pub fn load_resolved_files(&mut self, resolved: Vec<ResolvedFile>) {
        tracing::info!("Indexing {} files into SearchEngine...", resolved.len());
        let start = Instant::now();

        self.files = resolved
            .into_par_iter()
            .map(|f| {
                let name_lower = f.name.to_lowercase();
                let full_path_lower = f.full_path.to_lowercase();
                let (pinyin_first, pinyin_full) = extract_pinyin(&f.name);

                let ext = if f.is_directory {
                    String::new()
                } else {
                    f.name
                        .rsplit_once('.')
                        .map(|(_, e)| e.to_lowercase())
                        .unwrap_or_default()
                };

                IndexedFile {
                    name: f.name,
                    full_path: f.full_path,
                    name_lower,
                    full_path_lower,
                    pinyin_first,
                    pinyin_full,
                    ext,
                    size: f.size,
                    mtime: f.mtime,
                    is_directory: f.is_directory,
                    file_attributes: f.file_attributes,
                    frn: f.frn,
                    parent_frn: f.parent_frn,
                    volume: f.volume,
                }
            })
            .collect();

        self.rebuild_maps();

        tracing::info!(
            "SearchEngine indexed {} files in {}ms",
            self.files.len(),
            start.elapsed().as_millis()
        );
    }

    fn rebuild_maps(&mut self) {
        self.frn_to_index.clear();
        self.children_map.clear();
        for (idx, file) in self.files.iter().enumerate() {
            self.frn_to_index.insert(file.frn, idx);
            self.children_map
                .entry(file.parent_frn)
                .or_default()
                .push(idx);
        }
    }

    pub fn search(&self, query_str: &str, offset: usize, limit: usize) -> SearchResponse {
        let start = Instant::now();
        let trimmed = query_str.trim();

        if trimmed.is_empty() {
            let filtered: Vec<&IndexedFile> = if self.exclusions.is_empty() {
                self.files.iter().collect()
            } else {
                self.files.iter().filter(|f| !self.is_excluded(&f.full_path_lower)).collect()
            };

            let total_matches = filtered.len();
            let slice_end = (offset + limit).min(total_matches);
            let items = if offset < total_matches {
                filtered[offset..slice_end]
                    .par_iter()
                    .map(|f| to_dto(f))
                    .collect()
            } else {
                Vec::new()
            };

            return SearchResponse {
                items,
                total_matches,
                total_files: self.files.len(),
                search_time_us: start.elapsed().as_micros() as u64,
            };
        }

        let parsed_query = ParsedQuery::parse(trimmed);

        let matching_indices: Vec<usize> = self
            .files
            .par_iter()
            .enumerate()
            .filter_map(|(idx, file)| {
                if !self.exclusions.is_empty() && self.is_excluded(&file.full_path_lower) {
                    return None;
                }
                if matches_query(file, &parsed_query) {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect();

        let total_matches = matching_indices.len();
        let slice_end = (offset + limit).min(total_matches);

        let items = if offset < total_matches {
            matching_indices[offset..slice_end]
                .par_iter()
                .map(|&idx| to_dto(&self.files[idx]))
                .collect()
        } else {
            Vec::new()
        };


        SearchResponse {
            items,
            total_matches,
            total_files: self.files.len(),
            search_time_us: start.elapsed().as_micros() as u64,
        }
    }

    pub fn total_count(&self) -> usize {
        self.files.len()
    }

    pub fn add_file(&mut self, file: IndexedFile) {
        let idx = self.files.len();
        self.frn_to_index.insert(file.frn, idx);
        self.children_map
            .entry(file.parent_frn)
            .or_default()
            .push(idx);
        self.files.push(file);
    }

    pub fn remove_file(&mut self, frn: u64) -> bool {
        let Some(&idx) = self.frn_to_index.get(&frn) else {
            return false;
        };

        let file = &self.files[idx];
        let parent_frn = file.parent_frn;

        if let Some(children) = self.children_map.get_mut(&parent_frn) {
            children.retain(|&i| i != idx);
        }

        self.frn_to_index.remove(&frn);
        self.children_map.remove(&frn);

        let last_idx = self.files.len() - 1;
        if idx != last_idx {
            let swapped = self.files.swap_remove(idx);
            if let Some(mapped_idx) = self.frn_to_index.get_mut(&swapped.frn) {
                *mapped_idx = idx;
            }
            if let Some(siblings) = self.children_map.get_mut(&swapped.parent_frn) {
                for s in siblings.iter_mut() {
                    if *s == last_idx {
                        *s = idx;
                        break;
                    }
                }
            }
        } else {
            self.files.pop();
        }

        true
    }

    pub fn update_file(&mut self, frn: u64, new_name: Option<String>, new_size: Option<u64>, new_mtime: Option<i64>, new_attrs: Option<u32>) -> bool {
        let Some(&idx) = self.frn_to_index.get(&frn) else {
            return false;
        };

        let file = &mut self.files[idx];

        if let Some(name) = new_name {
            file.name_lower = name.to_lowercase();
            let (pf, pfull) = extract_pinyin(&name);
            file.pinyin_first = pf;
            file.pinyin_full = pfull;
            file.ext = if file.is_directory {
                String::new()
            } else {
                name.rsplit_once('.')
                    .map(|(_, e)| e.to_lowercase())
                    .unwrap_or_default()
            };
            file.name = name;
        }
        if let Some(size) = new_size {
            file.size = size;
        }
        if let Some(mtime) = new_mtime {
            file.mtime = mtime;
        }
        if let Some(attrs) = new_attrs {
            file.file_attributes = attrs;
            file.is_directory = attrs & 0x10 != 0;
        }

        true
    }

    pub fn apply_rename(&mut self, frn: u64, new_name: String, new_parent_frn: Option<u64>) -> bool {
        let Some(&idx) = self.frn_to_index.get(&frn) else {
            return false;
        };

        let old_parent = self.files[idx].parent_frn;

        if let Some(new_parent) = new_parent_frn {
            if let Some(children) = self.children_map.get_mut(&old_parent) {
                children.retain(|&i| i != idx);
            }
            self.children_map.entry(new_parent).or_default().push(idx);
            self.files[idx].parent_frn = new_parent;
        }

        self.files[idx].name = new_name.clone();
        self.files[idx].name_lower = new_name.to_lowercase();
        let (pf, pfull) = extract_pinyin(&new_name);
        self.files[idx].pinyin_first = pf;
        self.files[idx].pinyin_full = pfull;

        self.rebuild_path_for_subtree(frn);

        true
    }

    fn rebuild_path_for_subtree(&mut self, root_frn: u64) {
        let Some(&root_idx) = self.frn_to_index.get(&root_frn) else {
            return;
        };

        let parent_frn = self.files[root_idx].parent_frn;
        let parent_path = if let Some(&parent_idx) = self.frn_to_index.get(&parent_frn) {
            self.files[parent_idx].full_path.clone()
        } else {
            let vol = self.files[root_idx].volume;
            format!("{}:", vol)
        };

        self.files[root_idx].full_path = format!("{}\\{}", parent_path, self.files[root_idx].name);
        self.files[root_idx].full_path_lower = self.files[root_idx].full_path.to_lowercase();

        let root_full_path = self.files[root_idx].full_path.clone();

        let mut queue: Vec<(u64, String)> = vec![(root_frn, root_full_path)];

        while let Some((dir_frn, dir_path)) = queue.pop() {
            let child_indices: Vec<usize> = self.children_map
                .get(&dir_frn)
                .cloned()
                .unwrap_or_default();

            for child_idx in child_indices {
                let child_name = self.files[child_idx].name.clone();
                let child_full = format!("{}\\{}", dir_path, child_name);
                self.files[child_idx].full_path = child_full.clone();
                self.files[child_idx].full_path_lower = child_full.to_lowercase();

                if self.files[child_idx].is_directory {
                    queue.push((self.files[child_idx].frn, child_full));
                }
            }
        }
    }

    pub fn lookup_frn(&self, frn: u64) -> Option<usize> {
        self.frn_to_index.get(&frn).copied()
    }

    pub fn files_ref(&self) -> &[IndexedFile] {
        &self.files
    }

    pub fn get_full_path(&self, idx: usize) -> Option<&str> {
        self.files.get(idx).map(|f| f.full_path.as_str())
    }

    pub fn save_snapshot(&self, file_path: &std::path::Path) -> Result<(), String> {
        let snapshot_items: Vec<SnapshotFileItem> = self
            .files
            .iter()
            .map(|f| SnapshotFileItem {
                name: f.name.clone(),
                full_path: f.full_path.clone(),
                size: f.size,
                mtime: f.mtime,
                is_directory: f.is_directory,
                file_attributes: f.file_attributes,
                frn: f.frn,
                parent_frn: f.parent_frn,
                volume: f.volume,
            })
            .collect();

        let snapshot = IndexSnapshot {
            version: 1,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            files: snapshot_items,
        };

        if let Some(parent) = file_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let encoded = bincode::serialize(&snapshot)
            .map_err(|e| format!("Snapshot serialization failed: {e}"))?;

        std::fs::write(file_path, encoded)
            .map_err(|e| format!("Snapshot write failed: {e}"))?;

        tracing::info!(
            "Saved index snapshot with {} files to {}",
            snapshot.files.len(),
            file_path.display()
        );
        Ok(())
    }

    pub fn load_snapshot(&mut self, file_path: &std::path::Path) -> Result<usize, String> {
        let bytes = std::fs::read(file_path)
            .map_err(|e| format!("Failed to read snapshot file: {e}"))?;

        let snapshot: IndexSnapshot = bincode::deserialize(&bytes)
            .map_err(|e| format!("Failed to deserialize snapshot: {e}"))?;

        let count = snapshot.files.len();
        let start = Instant::now();

        self.files = snapshot
            .files
            .into_par_iter()
            .map(|f| {
                let name_lower = f.name.to_lowercase();
                let full_path_lower = f.full_path.to_lowercase();
                let (pinyin_first, pinyin_full) = extract_pinyin(&f.name);
                let ext = if f.is_directory {
                    String::new()
                } else {
                    f.name
                        .rsplit_once('.')
                        .map(|(_, e)| e.to_lowercase())
                        .unwrap_or_default()
                };

                IndexedFile {
                    name: f.name,
                    full_path: f.full_path,
                    name_lower,
                    full_path_lower,
                    pinyin_first,
                    pinyin_full,
                    ext,
                    size: f.size,
                    mtime: f.mtime,
                    is_directory: f.is_directory,
                    file_attributes: f.file_attributes,
                    frn: f.frn,
                    parent_frn: f.parent_frn,
                    volume: f.volume,
                }
            })
            .collect();

        self.rebuild_maps();

        tracing::info!(
            "Restored index snapshot ({} files) in {}ms",
            count,
            start.elapsed().as_millis()
        );

        Ok(count)
    }
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct SnapshotFileItem {
    pub name: String,
    pub full_path: String,
    pub size: u64,
    pub mtime: i64,
    pub is_directory: bool,
    pub file_attributes: u32,
    pub frn: u64,
    pub parent_frn: u64,
    pub volume: char,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct IndexSnapshot {
    pub version: u32,
    pub timestamp: i64,
    pub files: Vec<SnapshotFileItem>,
}

fn to_dto(file: &IndexedFile) -> SearchItemDto {
    let (size, mtime) = if (file.size == 0 || file.mtime == 0) && !file.is_directory {
        if let Ok(meta) = std::fs::metadata(&file.full_path) {
            let sz = meta.len();
            let mt = meta.modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(file.mtime);
            (sz, mt)
        } else {
            (file.size, file.mtime)
        }
    } else {
        (file.size, file.mtime)
    };

    SearchItemDto {
        name: file.name.clone(),
        full_path: file.full_path.clone(),
        size,
        mtime,
        is_directory: file.is_directory,
        ext: file.ext.clone(),
    }
}


pub type SharedEngine = Arc<RwLock<SearchEngine>>;


#[cfg(test)]
mod tests {
    use super::*;

    fn sample_files() -> Vec<ResolvedFile> {
        vec![
            ResolvedFile {
                full_path: "C:\\Windows\\explorer.exe".to_string(),
                name: "explorer.exe".to_string(),
                frn: 1,
                parent_frn: 0,
                is_directory: false,
                file_attributes: 0,
                size: 5000000,
                mtime: 1700000000,
                volume: 'C',
            },
            ResolvedFile {
                full_path: "C:\\Projects\\anyecho\\凡响项目方案.docx".to_string(),
                name: "凡响项目方案.docx".to_string(),
                frn: 2,
                parent_frn: 10,
                is_directory: false,
                file_attributes: 0,
                size: 1048576,
                mtime: 1710000000,
                volume: 'C',
            },
            ResolvedFile {
                full_path: "C:\\Program Files\\Tencent\\WeChat\\微信.exe".to_string(),
                name: "微信.exe".to_string(),
                frn: 3,
                parent_frn: 11,
                is_directory: false,
                file_attributes: 0,
                size: 20480000,
                mtime: 1715000000,
                volume: 'C',
            },
            ResolvedFile {
                full_path: "C:\\Projects\\anyecho".to_string(),
                name: "anyecho".to_string(),
                frn: 10,
                parent_frn: 0,
                is_directory: true,
                file_attributes: 16,
                size: 0,
                mtime: 1710000000,
                volume: 'C',
            },
        ]
    }

    #[test]
    fn test_search_engine_basic_and_pinyin() {
        let mut engine = SearchEngine::new();
        engine.load_resolved_files(sample_files());
        assert_eq!(engine.total_count(), 4);

        // 1. 普通文件名搜索
        let res = engine.search("explorer", 0, 10);
        assert_eq!(res.total_matches, 1);
        assert_eq!(res.items[0].name, "explorer.exe");

        // 2. 拼音首字母搜索 "fx" -> 凡响项目方案.docx
        let res_pinyin = engine.search("fx", 0, 10);
        assert_eq!(res_pinyin.total_matches, 1);
        assert_eq!(res_pinyin.items[0].name, "凡响项目方案.docx");

        // 3. 拼音首字母搜索 "wx" -> 微信.exe
        let res_wx = engine.search("wx", 0, 10);
        assert_eq!(res_wx.total_matches, 1);
        assert_eq!(res_wx.items[0].name, "微信.exe");

        // 4. 扩展名过滤 "ext:docx"
        let res_ext = engine.search("ext:docx", 0, 10);
        assert_eq!(res_ext.total_matches, 1);
        assert_eq!(res_ext.items[0].name, "凡响项目方案.docx");

        // 5. 类型过滤 "type:folder"
        let res_folder = engine.search("type:folder", 0, 10);
        assert_eq!(res_folder.total_matches, 1);
        assert_eq!(res_folder.items[0].name, "anyecho");
    }

    #[test]
    fn test_snapshot_save_and_load() {
        let mut engine = SearchEngine::new();
        engine.load_resolved_files(sample_files());

        let temp_dir = std::env::temp_dir().join("anyecho_snap_test");
        let _ = std::fs::create_dir_all(&temp_dir);
        let snap_file = temp_dir.join("test_snap.bin");

        engine.save_snapshot(&snap_file).unwrap();
        assert!(snap_file.exists());

        let mut restored_engine = SearchEngine::new();
        let loaded_count = restored_engine.load_snapshot(&snap_file).unwrap();
        assert_eq!(loaded_count, 4);

        let res = restored_engine.search("fx", 0, 10);
        assert_eq!(res.total_matches, 1);
        assert_eq!(res.items[0].name, "凡响项目方案.docx");

        let _ = std::fs::remove_file(&snap_file);
        let _ = std::fs::remove_dir(&temp_dir);
    }
}


