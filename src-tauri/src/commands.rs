use std::sync::Arc;
use tauri::Emitter;
use crate::content_search::{self, ContentSearchResponse, ContentPreview};
use crate::engine::filter::ParsedQuery;
use crate::engine::SearchResponse;
use crate::mft_enum;
use crate::scanner;
use crate::system;
use crate::usn_monitor::UsnMonitorManager;
use crate::{AppState, ScanResult};


#[tauri::command]
pub async fn start_scan(state: tauri::State<'_, AppState>) -> Result<ScanResult, String> {
    let engine_clone = state.engine.clone();

    let result = tokio::task::spawn_blocking(move || {
        let (scan_res, resolved_files) = scanner::scan_all_volumes_with_files()?;
        {
            let mut engine = engine_clone.write();
            engine.load_resolved_files(resolved_files);
            let snapshot_path = crate::persistence::get_snapshot_path();
            let _ = engine.save_snapshot(&snapshot_path);
        }
        Ok::<ScanResult, String>(scan_res)
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))??;


    *state.scan_result.lock().unwrap() = Some(result.clone());

    auto_start_monitoring(&state);

    Ok(result)
}

fn auto_start_monitoring(state: &tauri::State<'_, AppState>) {
    let mut monitor_guard = state.monitor.lock().unwrap();
    if monitor_guard.is_some() {
        return;
    }

    let volumes = match mft_enum::detect_ntfs_volumes() {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Failed to detect volumes for monitoring: {}", e);
            return;
        }
    };

    if volumes.is_empty() {
        tracing::warn!("No NTFS volumes found for monitoring");
        return;
    }

    let manager = Arc::new(UsnMonitorManager::new(state.engine.clone()));
    let vol_count = volumes.len();
    manager.start_monitoring(volumes);
    tracing::info!("USN monitoring started for {} volume(s)", vol_count);
    *monitor_guard = Some(manager);
}

#[tauri::command]
pub async fn start_monitoring(state: tauri::State<'_, AppState>) -> Result<String, String> {
    auto_start_monitoring(&state);
    Ok("Monitoring started".to_string())
}

#[tauri::command]
pub fn get_monitor_status(state: tauri::State<'_, AppState>) -> bool {
    state
        .monitor
        .lock()
        .unwrap()
        .as_ref()
        .map(|m| m.is_running())
        .unwrap_or(false)
}

#[tauri::command]
pub fn get_scan_status(state: tauri::State<'_, AppState>) -> Option<ScanResult> {
    state.scan_result.lock().unwrap().clone()
}

#[tauri::command]
pub fn search(
    state: tauri::State<'_, AppState>,
    query: String,
    offset: Option<usize>,
    limit: Option<usize>,
) -> Result<SearchResponse, String> {
    let offset = offset.unwrap_or(0);
    let limit = limit.unwrap_or(100).min(500);

    let engine = state.engine.read();
    let response = engine.search(&query, offset, limit);

    if !query.trim().is_empty() {
        let _ = state.db.save_search(&query, response.total_matches);
    }

    Ok(response)
}

#[tauri::command]
pub fn open_file(path: String) -> Result<(), String> {
    system::open_path(&path)
}

#[tauri::command]
pub fn show_in_folder(path: String) -> Result<(), String> {
    system::show_in_folder(&path)
}

#[tauri::command]
pub async fn search_content(
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    query: String,
) -> Result<ContentSearchResponse, String> {
    let engine = state.engine.clone();
    let app = app_handle.clone();

    let result = tokio::task::spawn_blocking(move || {
        let parsed = ParsedQuery::parse(&query);
        let keyword = parsed.content_terms.first()
            .cloned()
            .unwrap_or_default();

        if keyword.is_empty() {
            return Ok::<ContentSearchResponse, String>(ContentSearchResponse {
                matches: Vec::new(),
                files_searched: 0,
                total_matches: 0,
                search_time_us: 0,
                is_complete: true,
            });
        }

        let eng = engine.read();
        let files = eng.files_ref();

        let response = content_search::search_content_with_query(files, &parsed, &keyword);


        let batch_size = 50;
        let total = response.matches.len();
        let mut sent = 0;

        while sent < total {
            let end = (sent + batch_size).min(total);
            let batch = &response.matches[sent..end];
            let _ = app.emit("content-search-batch", batch);
            sent = end;
        }

        let _ = app.emit("content-search-done", response.total_matches);

        Ok(response)
    })
    .await
    .map_err(|e| format!("Task join error: {e}"))??;

    Ok(result)
}

#[tauri::command]
pub fn get_content_preview(
    path: String,
    keyword: String,
) -> Result<Option<ContentPreview>, String> {
    Ok(content_search::get_content_preview(&path, &keyword, 3))
}

#[tauri::command]
pub async fn add_knowledge_folder(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<String, String> {
    let mut folders = state.knowledge_folders.lock().unwrap();
    if folders.contains(&path) {
        return Ok("Folder already added".to_string());
    }

    let indexer = crate::knowledge_index::KnowledgeIndexer::new(
        &crate::knowledge_index::get_knowledge_index_path(),
    )?;

    let count = indexer.index_folder(&path)?;
    folders.push(path.clone());
    crate::knowledge_index::save_knowledge_folders(&folders)?;

    tracing::info!("Added knowledge folder: {} ({} files indexed)", path, count);
    Ok(format!("Indexed {} files from {}", count, path))
}

#[tauri::command]
pub fn remove_knowledge_folder(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    let mut folders = state.knowledge_folders.lock().unwrap();
    folders.retain(|f| f != &path);
    crate::knowledge_index::save_knowledge_folders(&folders)?;
    tracing::info!("Removed knowledge folder: {}", path);
    Ok(())
}

#[tauri::command]
pub fn get_knowledge_folders(
    state: tauri::State<'_, AppState>,
) -> Vec<String> {
    state.knowledge_folders.lock().unwrap().clone()
}

#[tauri::command]
pub async fn search_knowledge(
    _state: tauri::State<'_, AppState>,
    query: String,
) -> Result<crate::knowledge_index::KnowledgeSearchResponse, String> {
    let indexer = crate::knowledge_index::KnowledgeIndexer::new(
        &crate::knowledge_index::get_knowledge_index_path(),
    )?;

    indexer.search(&query, 50)
}

#[tauri::command]
pub fn get_recent_searches(
    state: tauri::State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<crate::persistence::SearchHistoryEntry>, String> {
    state.db.get_recent_searches(limit.unwrap_or(20))
}

#[tauri::command]
pub fn clear_search_history(
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state.db.clear_search_history()
}

#[tauri::command]
pub fn add_favorite(
    state: tauri::State<'_, AppState>,
    path: String,
    name: String,
) -> Result<(), String> {
    state.db.add_favorite(&path, &name)
}

#[tauri::command]
pub fn remove_favorite(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    state.db.remove_favorite(&path)
}

#[tauri::command]
pub fn get_favorites(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<crate::persistence::Favorite>, String> {
    state.db.get_favorites()
}

#[tauri::command]
pub fn get_setting(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Option<String> {
    state.db.get_setting(&key)
}

#[tauri::command]
pub fn set_setting(
    state: tauri::State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    state.db.set_setting(&key, &value)
}

#[tauri::command]
pub fn add_exclusion(
    state: tauri::State<'_, AppState>,
    pattern: String,
) -> Result<(), String> {
    state.db.add_exclusion(&pattern, false)?;
    sync_exclusions_to_engine(&state);
    Ok(())
}

#[tauri::command]
pub fn remove_exclusion(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<(), String> {
    state.db.remove_exclusion(id)?;
    sync_exclusions_to_engine(&state);
    Ok(())
}

#[tauri::command]
pub fn get_exclusions(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<crate::persistence::ExclusionRule>, String> {
    state.db.get_exclusions()
}

fn sync_exclusions_to_engine(state: &tauri::State<'_, AppState>) {
    if let Ok(exclusions) = state.db.get_exclusions() {
        let patterns: Vec<String> = exclusions.iter().map(|e| e.pattern.clone()).collect();
        state.engine.write().set_exclusions(patterns);
    }
}
