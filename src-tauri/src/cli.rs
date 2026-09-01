use std::env;
use std::time::Instant;
use crate::engine::SearchEngine;
use crate::persistence::{get_snapshot_path, Database};
use crate::scanner::scan_all_volumes_with_files;

pub fn handle_cli_args() -> bool {
    let args: Vec<String> = env::args().collect();
    if args.len() <= 1 {
        return false;
    }

    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }

    run_cli(&args[1..]);
    true
}

pub fn run_cli(args: &[String]) {
    if args.is_empty() {
        print_help();
        return;
    }

    let first = args[0].as_str();

    match first {
        "-h" | "--help" | "help" => {
            print_help();
        }
        "-v" | "--version" | "version" => {
            println!("凡响 AnyEcho v{}", env!("CARGO_PKG_VERSION"));
        }
        "scan" | "--scan" => {
            run_cli_scan();
        }
        _ => {
            let mut limit = 50;
            let mut json_mode = false;
            let mut path_only = false;
            let mut query_parts = Vec::new();

            let mut i = 0;
            while i < args.len() {
                let arg = &args[i];
                if arg == "--json" || arg == "-j" {
                    json_mode = true;
                } else if arg == "--path" || arg == "-p" {
                    path_only = true;
                } else if arg == "--limit" || arg == "-n" {
                    if i + 1 < args.len() {
                        if let Ok(n) = args[i + 1].parse::<usize>() {
                            limit = n;
                            i += 1;
                        }
                    }
                } else if arg == "search" && i == 0 {
                    // skip leading search subcommand keyword
                } else {
                    query_parts.push(arg.clone());
                }
                i += 1;
            }

            let query = query_parts.join(" ");
            run_cli_search(&query, limit, json_mode, path_only);
        }
    }
}

fn print_help() {
    println!(r#"
⚡ 凡响 AnyEcho - 超高速 Windows 桌面级文件搜索利器 (CLI 命令行模式)

使用方式 (Usage):
    ae <query> [options]
    anyecho <query> [options]
    anyecho search <query> [options]

常用查询示例 (Examples):
    ae qwen                     # 模糊检索包含 qwen 的所有文件
    ae fx                       # 中文拼音首字母缩写检索 (匹配 '凡响')
    ae type:ai                  # 检索所有 AI 模型权重 (gguf, safetensors, pt, nvfp4...)
    ae ext:pdf size:>10MB       # 检索大于 10MB 的 PDF 文档
    ae "D:\AI\*.md"             # 检索指定路径下的 Markdown 笔记

选项 (Options):
    -n, --limit <NUM>           限制返回结果条数 (默认: 50)
    -p, --path                  仅输出完整文件绝对路径 (便于管道传递)
    -j, --json                  以 JSON 格式输出结果
    scan, --scan                立即触发重新扫描所有驱动器并更新快照
    -h, --help                  显示帮助信息
    -v, --version               显示版本号
"#);
}

fn run_cli_scan() {
    println!("🔍 正在全盘扫描 NTFS USN Journal 建立索引...");
    let start = Instant::now();

    match scan_all_volumes_with_files() {
        Ok((scan_res, resolved_files)) => {
            let mut engine = SearchEngine::new();
            if let Ok(db) = Database::new() {
                if let Ok(exclusions) = db.get_exclusions() {
                    let patterns: Vec<String> = exclusions.into_iter().map(|e| e.pattern).collect();
                    engine.set_exclusions(patterns);
                }
            }

            engine.load_resolved_files(resolved_files);

            let snapshot_path = get_snapshot_path();
            if let Err(e) = engine.save_snapshot(&snapshot_path) {
                eprintln!("⚠️ 快照保存失败: {}", e);
            } else {
                println!("💾 索引快照已持久化: {}", snapshot_path.display());
            }

            println!("✅ 扫描完成！共索引 {} 个文件 (底层扫描耗时: {} ms, 总耗时: {} ms)", 
                engine.len(), scan_res.time_ms, start.elapsed().as_millis());
        }
        Err(e) => {
            eprintln!("❌ 扫描失败: {}", e);
        }
    }
}

fn run_cli_search(query: &str, limit: usize, json_mode: bool, path_only: bool) {
    let start = Instant::now();

    let mut engine = SearchEngine::new();
    let snapshot_path = get_snapshot_path();

    let loaded = if snapshot_path.exists() {
        engine.load_snapshot(&snapshot_path).is_ok()
    } else {
        false
    };

    if !loaded {
        if let Ok((_scan_res, resolved_files)) = scan_all_volumes_with_files() {
            if let Ok(db) = Database::new() {
                if let Ok(exclusions) = db.get_exclusions() {
                    let patterns: Vec<String> = exclusions.into_iter().map(|e| e.pattern).collect();
                    engine.set_exclusions(patterns);
                }
            }
            engine.load_resolved_files(resolved_files);
            let _ = engine.save_snapshot(&snapshot_path);
        }
    }

    let response = engine.search(query, 0, limit);
    let search_us = start.elapsed().as_micros();

    if json_mode {
        println!("{}", serde_json::to_string_pretty(&response).unwrap_or_default());
        return;
    }

    if path_only {
        for item in &response.items {
            println!("{}", item.full_path);
        }
        return;
    }

    // Formatted Table Output
    println!();
    println!("⚡ 凡响 AnyEcho | 检索: \"{}\" | 命中: {} 个 (显示前 {} 条, 耗时: {:.2} ms)", 
        query, response.total_matches, response.items.len(), search_us as f64 / 1000.0);
    println!("{}", "-".repeat(95));
    println!("{:<4} {:<32} {:<10} {:<10} {}", "#", "名称 (Name)", "类型", "大小", "路径 (Path)");
    println!("{}", "-".repeat(95));

    for (idx, item) in response.items.iter().enumerate() {
        let name_truncated = if item.name.chars().count() > 30 {
            format!("{}...", item.name.chars().take(27).collect::<String>())
        } else {
            item.name.clone()
        };

        let type_str = if item.is_directory {
            "DIR".to_string()
        } else if !item.ext.is_empty() {
            item.ext.to_uppercase()
        } else {
            "-".to_string()
        };

        let size_str = format_size(item.size, item.is_directory);

        println!("{:<4} {:<32} {:<10} {:<10} {}", 
            idx + 1, name_truncated, type_str, size_str, item.full_path);
    }

    println!("{}", "-".repeat(95));
    println!();
}

fn format_size(bytes: u64, is_dir: bool) -> String {
    if is_dir {
        return "<DIR>".to_string();
    }
    if bytes == 0 {
        return "0 B".to_string();
    }
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;
    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }
    if unit_idx == 0 {
        format!("{} {}", size as u64, UNITS[unit_idx])
    } else {
        format!("{:.1} {}", size, UNITS[unit_idx])
    }
}
