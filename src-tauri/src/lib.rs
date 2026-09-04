pub mod cli;
mod commands;
pub mod content_search;
pub mod doc_cache;
pub mod doc_extractor;
pub mod engine;
pub mod ipc;
pub mod knowledge_index;
mod mft_enum;
mod path_tree;
pub mod persistence;
mod scanner;
mod system;
mod tray;
mod usn_monitor;

use std::sync::{Arc, Mutex};
use parking_lot::RwLock;
use tauri::Manager;
use tracing_appender::rolling;
use tracing_subscriber::{fmt, EnvFilter};

use crate::doc_cache::DocCache;
use crate::engine::{SearchEngine, SharedEngine};
use crate::persistence::Database;
use crate::usn_monitor::UsnMonitorManager;

pub struct AppState {
    pub scan_result: Mutex<Option<ScanResult>>,
    pub engine: SharedEngine,
    pub monitor: Mutex<Option<Arc<UsnMonitorManager>>>,
    pub knowledge_folders: Mutex<Vec<String>>,
    pub db: Arc<Database>,
    pub doc_cache: Arc<DocCache>,
    pub initial_query: Mutex<Option<String>>,
}


#[derive(serde::Serialize, Clone, Debug)]
pub struct ScanResult {
    pub count: usize,
    pub time_ms: u64,
}

pub fn run() {
    let initial_query = cli::get_raw_cli_query();


    let log_dir = {

        let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
        let dir = std::path::PathBuf::from(local_app_data)
            .join("anyecho")
            .join("logs");
        let _ = std::fs::create_dir_all(&dir);
        dir
    };

    let file_appender = rolling::daily(&log_dir, "anyecho");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    fmt()
        .with_env_filter(
            EnvFilter::from_default_env().add_directive("anyecho=info".parse().unwrap()),
        )
        .with_writer(non_blocking)
        .with_ansi(false)
        .init();

    tracing::info!("AnyEcho starting up");

    let engine = Arc::new(RwLock::new(SearchEngine::new()));
    let knowledge_folders = knowledge_index::load_knowledge_folders();

    let db = match Database::new() {
        Ok(db) => Arc::new(db),
        Err(e) => {
            tracing::error!("Failed to initialize database: {}", e);
            panic!("Database initialization failed: {e}");
        }
    };

    if let Ok(exclusions) = db.get_exclusions() {
        let patterns: Vec<String> = exclusions.iter().map(|e| e.pattern.clone()).collect();
        if !patterns.is_empty() {
            engine.write().set_exclusions(patterns);
            tracing::info!("Loaded {} exclusion rules from database", exclusions.len());
        }
    }

    let doc_cache = match DocCache::new() {
        Ok(dc) => Arc::new(dc),
        Err(e) => {
            tracing::error!("Failed to initialize doc cache: {}", e);
            panic!("Doc cache initialization failed: {e}");
        }
    };

    let mut initial_scan_result = None;
    let mut initial_monitor = None;
    let snapshot_path = persistence::get_snapshot_path();
    if snapshot_path.exists() {
        let mut loaded_count = 0;
        {
            let mut eng = engine.write();
            match eng.load_snapshot(&snapshot_path) {
                Ok(count) if count > 0 => {
                    tracing::info!("Auto-loaded index cache ({} files) on startup", count);
                    initial_scan_result = Some(ScanResult {
                        count,
                        time_ms: 80,
                    });
                    loaded_count = count;
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!("Failed to auto-load index cache: {}", e);
                }
            }
        }

        if loaded_count > 0 {
            // 1. 启动低优先级后台静默文档文本索引构建
            let engine_clone = engine.clone();
            doc_cache.clone().start_background_indexer(move || {
                let eng = engine_clone.read();
                eng.files()
                    .iter()
                    .filter(|f| !f.is_directory && crate::doc_extractor::is_supported_document_ext(&f.ext))
                    .map(|f| (f.full_path.clone(), f.mtime, f.size))
                    .collect()
            });

            // 2. ⚡ 致命断点修复：日常启动成功恢复快照后，立即自动启动 NTFS USN 变动监听与 5秒防抖落盘！
            if let Ok(volumes) = mft_enum::detect_ntfs_volumes() {
                if !volumes.is_empty() {
                    let manager = Arc::new(UsnMonitorManager::new(engine.clone()));
                    let vol_count = volumes.len();
                    manager.start_monitoring(volumes);
                    initial_monitor = Some(manager);
                    tracing::info!("USN monitoring automatically started for {} volume(s) from startup snapshot", vol_count);
                }
            }
        }
    }


    // 启动 IPC 命名管道服务器，供 CLI (ae) 实时查询内存索引
    ipc::start_ipc_server(engine.clone());
    tracing::info!("IPC named pipe server started");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            tracing::info!("Single instance awakened with args: {:?}", argv);
            use tauri::Emitter;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
                let _ = window.unminimize();
            }
            let query = cli::parse_args_to_query(&argv);
            if !query.is_empty() {
                let _ = app.emit("open-query", query);
            }
        }))
        .setup(|app| {
            system::ensure_cli_in_path();

            let handle = app.handle().clone();
            tray::setup_tray(&handle).unwrap_or_else(|e| {
                tracing::error!("Failed to setup tray: {}", e);
            });


            let shortcut_handle = handle.clone();
            use tauri_plugin_global_shortcut::GlobalShortcutExt;
            let _ = app.global_shortcut().on_shortcut("Alt+Space", move |_id, _shortcut, event| {
                use tauri_plugin_global_shortcut::ShortcutState;
                if event.state == ShortcutState::Pressed {
                    if let Some(window) = shortcut_handle.get_webview_window("main") {
                        if window.is_visible().unwrap_or(false) {
                            let _ = window.hide();
                        } else {
                            let _ = window.show();
                            let _ = window.set_focus();
                            let _ = window.set_always_on_top(true);
                        }
                    }
                }
            });

            tracing::info!("Global shortcut Alt+Space registered");
            Ok(())
        })
        .on_window_event(|window, event| {
            match event {
                tauri::WindowEvent::Focused(false) => {
                    if window.is_always_on_top().unwrap_or(false) {
                        let _ = window.hide();
                        let _ = window.set_always_on_top(false);
                    }
                }
                tauri::WindowEvent::CloseRequested { .. } => {
                    let state = window.state::<AppState>();
                    let eng = state.engine.read();
                    let snapshot_path = persistence::get_snapshot_path();
                    if let Err(e) = eng.save_snapshot(&snapshot_path) {
                        tracing::warn!("Failed to save snapshot on exit: {}", e);
                    } else {
                        tracing::info!("Snapshot saved on GUI exit ({} files)", eng.files_ref().len());
                    }
                }
                _ => {}
            }
        })
        .manage(AppState {
            scan_result: Mutex::new(initial_scan_result),
            engine,
            monitor: Mutex::new(initial_monitor),
            knowledge_folders: Mutex::new(knowledge_folders),
            db,
            doc_cache,
            initial_query: Mutex::new(initial_query),
        })
        .invoke_handler(tauri::generate_handler![
            commands::start_scan,
            commands::get_scan_status,
            commands::search,
            commands::open_file,
            commands::show_in_folder,
            commands::start_monitoring,
            commands::get_monitor_status,
            commands::search_content,
            commands::get_content_preview,
            commands::get_doc_index_stats,
            commands::start_doc_indexing,
            commands::get_initial_query,
            commands::add_knowledge_folder,
            commands::remove_knowledge_folder,
            commands::get_knowledge_folders,
            commands::search_knowledge,
            commands::get_recent_searches,
            commands::clear_search_history,
            commands::add_favorite,
            commands::remove_favorite,
            commands::get_favorites,
            commands::get_setting,
            commands::set_setting,
            commands::add_exclusion,
            commands::remove_exclusion,
            commands::get_exclusions,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

}

