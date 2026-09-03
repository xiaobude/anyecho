use std::env;
use std::fs;
use std::path::Path;
use std::time::Instant;
use crate::engine::SearchEngine;
use crate::engine::filter::ParsedQuery;
use crate::content_search::search_content_with_query_and_cache;
use crate::doc_cache::DocCache;
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
        run_cli_ls(Path::new("."));
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
            // If the argument is an existing directory (e.g. `ae .`, `ae ..`, `ae src`, `ae D:\AI`), run super ls!
            if args.len() == 1 && (first == "." || first == ".." || Path::new(first).is_dir()) {
                run_cli_ls(Path::new(first));
                return;
            }

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

            #[cfg(windows)]
            let query = get_raw_cli_query().unwrap_or_else(|| query_parts.join(" "));
            #[cfg(not(windows))]
            let query = query_parts.join(" ");

            if query.trim().is_empty() {
                run_cli_ls(Path::new("."));
            } else {
                run_cli_search(&query, limit, json_mode, path_only);
            }
        }
    }
}

pub fn parse_args_to_query(args: &[String]) -> String {
    if args.is_empty() {
        return String::new();
    }
    let slice = if args.len() > 1 && (args[0].ends_with(".exe") || args[0].ends_with("anyecho")) {
        &args[1..]
    } else {
        args
    };
    let mut parts = Vec::new();
    for arg in slice {
        if arg.starts_with("--") || arg.starts_with("-") {
            continue;
        }
        parts.push(arg.clone());
    }
    parts.join(" ")
}

#[cfg(windows)]
pub fn get_raw_cli_query() -> Option<String> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    extern "system" {
        fn GetCommandLineW() -> *const u16;
    }
    unsafe {
        let ptr = GetCommandLineW();
        if ptr.is_null() {
            return None;
        }
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(ptr, len);
        let raw = OsString::from_wide(slice).into_string().ok()?;
        let trimmed = raw.trim();

        // 剥离执行体自身路径（可能带双引号也可能不带）
        let after_exe = if trimmed.starts_with('"') {
            let rest = &trimmed[1..];
            let end = rest.find('"')?;
            rest[end + 1..].trim()
        } else {
            let space_idx = trimmed.find(' ')?;
            trimmed[space_idx + 1..].trim()
        };

        if after_exe.is_empty() {
            return None;
        }

        // 过滤 CLI 选项参数如 --limit 50, --json, --path 等，保留完整未被 shell 剥除引号的查询体
        let tokens = crate::engine::filter::tokenize_query(after_exe);
        let mut cleaned_tokens = Vec::new();
        let mut i = 0;
        while i < tokens.len() {
            let t = &tokens[i];
            if t == "--json" || t == "-j" || t == "--path" || t == "-p" {
                i += 1;
                continue;
            }
            if (t == "--limit" || t == "-n") && i + 1 < tokens.len() {
                i += 2;
                continue;
            }
            if t == "search" && i == 0 {
                i += 1;
                continue;
            }
            cleaned_tokens.push(t.clone());
            i += 1;
        }

        if cleaned_tokens.is_empty() {
            None
        } else {
            Some(cleaned_tokens.join(" "))
        }
    }
}

#[cfg(not(windows))]
pub fn get_raw_cli_query() -> Option<String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        None
    } else {
        Some(parse_args_to_query(&args))
    }
}



fn print_help() {
    println!(r#"
⚡ 凡响 AnyEcho - 超级终端命令行利器 (CLI / Super-ls Mode)

使用方式 (Usage):
    ae                          # 【超级 ls 模式】不带参数列出当前目录所有文件与目录
    ae <dir_path>               # 【超级 ls 模式】列出指定目录的文件 (如: ae src, ae ..)
    ae <query> [options]        # 【全盘极速搜索】毫秒级全盘模糊/拼音/类型搜索
    anyecho <query> [options]   # 同样支持通过 anyecho 主程序调用

常用查询示例 (Examples):
    ae                          # 查看当前目录列表 (带图标、类型、大小、时间)
    ae qwen                     # 模糊检索包含 qwen 的所有文件
    ae fx                       # 中文拼音首字母检索 (匹配 '凡响')
    ae type:ai                  # 检索所有 AI 模型与权重 (gguf, safetensors, pt, nvfp4...)
    ae ext:pdf size:>10MB       # 检索大于 10MB 的 PDF 文档
    ae "D:\AI\*.md"             # 检索指定路径下的 Markdown 笔记
    ae c:"父亲和儿子"           # 【全文检索】在所有文本中极速查找包含 "父亲和儿子" 的行
    ae type:doc c:"父亲和儿子"   # 【组合过滤】仅在文档类文件中检索指定文本内容
    ae *.txt content:hello      # 【通配符过滤】仅在 txt 文件中检索内容 hello


选项 (Options):
    -n, --limit <NUM>           限制全局搜索结果条数 (默认: 50)
    -p, --path                  仅输出完整文件绝对路径 (便于管道传递)
    -j, --json                  以 JSON 格式输出结果
    scan, --scan                立即重新扫描全盘 NTFS 日志并更新快照
    -h, --help                  显示此帮助信息
    -v, --version               显示版本号
"#);
}

struct LsItem {
    name: String,
    is_dir: bool,
    type_str: String,
    icon: &'static str,
    size_formatted: String,
    mtime_formatted: String,
}

pub fn run_cli_ls(target_dir: &Path) {
    let canonical = fs::canonicalize(target_dir).unwrap_or_else(|_| target_dir.to_path_buf());
    let display_path = canonical.to_string_lossy().replace(r"\\?\", "");

    let read_res = fs::read_dir(target_dir);
    let entries = match read_res {
        Ok(e) => e,
        Err(err) => {
            eprintln!("❌ 无法读取目录 {}: {}", display_path, err);
            return;
        }
    };

    let mut items: Vec<LsItem> = Vec::new();
    let mut total_files = 0usize;
    let mut total_dirs = 0usize;
    let mut total_bytes = 0u64;

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let metadata = entry.metadata().ok();
        let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
        let size = if is_dir { 0 } else { metadata.as_ref().map(|m| m.len()).unwrap_or(0) };
        let mtime = metadata.and_then(|m| m.modified().ok());

        let ext = if is_dir {
            "DIR".to_string()
        } else {
            name.rsplit_once('.')
                .map(|(_, e)| e.to_uppercase())
                .unwrap_or_else(|| "-".to_string())
        };

        let icon = get_file_icon(&ext, is_dir);
        let size_formatted = format_size(size, is_dir);
        let mtime_formatted = mtime.map(format_system_time).unwrap_or_else(|| "-".to_string());

        if is_dir {
            total_dirs += 1;
        } else {
            total_files += 1;
            total_bytes += size;
        }

        items.push(LsItem {
            name,
            is_dir,
            type_str: ext,
            icon,
            size_formatted,
            mtime_formatted,
        });
    }


    // 目录优先，其次按名称不区分大小写排序
    items.sort_by(|a, b| {
        match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        }
    });

    println!();
    println!("📂 目录: {}", display_path);
    println!("{}", "=".repeat(88));
    println!("{:<4} {:<8} {:<10} {:<20} {}", "#", "类型", "大小", "修改时间", "名称");
    println!("{}", "-".repeat(88));

    for (idx, item) in items.iter().enumerate() {
        let display_name = format!("{} {}", item.icon, item.name);
        println!("{:<4} {:<8} {:<10} {:<20} {}", 
            idx + 1, 
            item.type_str, 
            item.size_formatted, 
            item.mtime_formatted, 
            display_name
        );
    }

    println!("{}", "-".repeat(88));
    println!("📊 共计: 📁 {} 个目录, 📄 {} 个文件 (总大小: {})", 
        total_dirs, total_files, format_size(total_bytes, false));
    println!();
}

fn get_file_icon(ext: &str, is_dir: bool) -> &'static str {
    if is_dir {
        return "📁";
    }
    match ext {
        "GGUF" | "SAFETENSORS" | "PT" | "PTH" | "ONNX" | "NVFP4" | "FP8" | "AWQ" | "GPTQ" | "GGML" | "BIN" | "CKPT" => "🤖",
        "DOC" | "DOCX" | "PDF" | "XLS" | "XLSX" | "PPT" | "PPTX" | "CSV" => "📄",
        "TXT" | "MD" | "LOG" => "📝",
        "RS" | "TS" | "JS" | "PY" | "C" | "CPP" | "GO" | "JAVA" | "HTML" | "CSS" | "SVELTE" | "VUE" => "💻",
        "JSON" | "YAML" | "TOML" | "XML" | "INI" | "CONF" | "ENV" => "⚙️",
        "PNG" | "JPG" | "JPEG" | "GIF" | "WEBP" | "SVG" | "ICO" | "BMP" => "🖼️",
        "MP4" | "MKV" | "AVI" | "MOV" | "WMV" => "🎬",
        "MP3" | "FLAC" | "WAV" | "AAC" | "OGG" | "M4A" => "🎵",
        "ZIP" | "RAR" | "7Z" | "TAR" | "GZ" | "BZ2" => "📦",
        "EXE" | "MSI" | "BAT" | "CMD" | "PS1" => "⚡",
        _ => "📄",
    }
}

fn format_system_time(st: std::time::SystemTime) -> String {
    use std::time::UNIX_EPOCH;
    if let Ok(duration) = st.duration_since(UNIX_EPOCH) {
        let secs = duration.as_secs() as i64;
        format_timestamp(secs)
    } else {
        "-".to_string()
    }
}

fn format_timestamp(timestamp: i64) -> String {
    if timestamp <= 0 {
        return "-".to_string();
    }
    // Days since Jan 1 1970
    let mut days = timestamp / 86400;
    let day_secs = (timestamp % 86400) as u32;
    let hour = day_secs / 3600;
    let minute = (day_secs % 3600) / 60;
    let second = day_secs % 60;

    let mut year = 1970;
    loop {
        let leap = is_leap_year(year);
        let year_days = if leap { 366 } else { 365 };
        if days < year_days {
            break;
        }
        days -= year_days;
        year += 1;
    }

    let leap = is_leap_year(year);
    let month_days = [
        31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31
    ];
    let mut month = 1;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    let day = days + 1;

    format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", year, month, day, hour, minute, second)
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
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

    let parsed = ParsedQuery::parse(query);

    if !parsed.content_terms.is_empty() {
        let keyword = parsed.content_terms.join(" ");
        let cached_hits = if let Ok(doc_cache) = DocCache::new() {
            doc_cache.search_cached(&keyword, limit * 2)
        } else {
            Vec::new()
        };
        let content_resp = search_content_with_query_and_cache(engine.files(), &parsed, &keyword, Some(&cached_hits));
        let search_ms = content_resp.search_time_us as f64 / 1000.0;

        if json_mode {
            println!("{}", serde_json::to_string_pretty(&content_resp).unwrap_or_default());
            return;
        }

        if path_only {
            let mut seen = std::collections::HashSet::new();
            for m in &content_resp.matches {
                if seen.insert(&m.file_path) {
                    println!("{}", m.file_path);
                }
            }
            return;
        }

        println!();
        println!("⚡ 凡响 AnyEcho | 文档/全文内容检索: \"{}\" | 命中: {} 个文件 (耗时: {:.2} ms)", 
            keyword, content_resp.total_matches, search_ms);
        println!("{}", "-".repeat(95));
        println!("{:<4} {:<32} {:<10} {:<10} {}", "#", "名称 (Name)", "类型", "大小", "路径 (Path)");
        println!("{}", "-".repeat(95));

        for (idx, m) in content_resp.matches.iter().take(limit).enumerate() {
            let display_name = crate::content_search::decode_percent_encoded(&m.file_name);
            let display_path = crate::content_search::decode_percent_encoded(&m.file_path);

            let ext = Path::new(&display_name)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_uppercase())
                .unwrap_or_else(|| "-".to_string());

            let name_truncated = if display_name.chars().count() > 30 {
                format!("{}...", display_name.chars().take(27).collect::<String>())
            } else {
                display_name
            };

            let size_str = if let Ok(meta) = std::fs::metadata(&m.file_path) {
                format_size(meta.len(), false)
            } else {
                "-".to_string()
            };

            println!("{:<4} {:<32} {:<10} {:<10} {}", 
                idx + 1, name_truncated, ext, size_str, display_path);
        }

        println!("{}", "-".repeat(95));
        println!("📊 检索统计: 命中 {} 个包含关键词的文件 (显示前 {} 条)", 
            content_resp.total_matches, content_resp.matches.len().min(limit));
        println!();
        return;
    }


    let response = engine.search(query, 0, limit);
    let search_ms = response.search_time_us as f64 / 1000.0;


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
    let total_size_str = format_size(response.total_bytes, false);
    println!();
    println!("⚡ 凡响 AnyEcho | 检索: \"{}\" | 命中: {} 个 (共 {}, 显示前 {} 条, 耗时: {:.2} ms)", 
        query, response.total_matches, total_size_str, response.items.len(), search_ms);
    println!("{}", "-".repeat(95));
    println!("{:<4} {:<32} {:<10} {:<10} {}", "#", "名称 (Name)", "类型", "大小", "路径 (Path)");
    println!("{}", "-".repeat(95));

    for (idx, item) in response.items.iter().enumerate() {
        let display_name = crate::content_search::decode_percent_encoded(&item.name);
        let display_path = crate::content_search::decode_percent_encoded(&item.full_path);
        let name_truncated = if display_name.chars().count() > 30 {
            format!("{}...", display_name.chars().take(27).collect::<String>())
        } else {
            display_name
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
            idx + 1, name_truncated, type_str, size_str, display_path);
    }


    println!("{}", "-".repeat(95));
    println!("📊 检索统计: 命中 {} 个项目 (总大小: {})", response.total_matches, total_size_str);
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
