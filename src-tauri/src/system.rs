use std::process::Command;

/// 使用系统默认关联程序打开文件或目录
pub fn open_path(path: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/c", "start", "", path])
            .spawn()
            .map_err(|e| format!("Failed to open path: {e}"))?;
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("Failed to open path: {e}"))?;
        Ok(())
    }
}

/// 在 Windows 资源管理器中高亮定位文件
pub fn show_in_folder(path: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let arg = format!("/select,{}", path.replace('/', "\\"));
        Command::new("explorer")
            .arg(arg)
            .spawn()
            .map_err(|e| format!("Failed to reveal in explorer: {e}"))?;
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(parent) = std::path::Path::new(path).parent() {
            open_path(&parent.to_string_lossy())
        } else {
            open_path(path)
        }
    }
}

/// 自动将 ae 命令行工具部署到用户全局 PATH (通过 %LOCALAPPDATA%\Microsoft\WindowsApps 零重启开箱即用)
pub fn ensure_cli_in_path() {
    #[cfg(target_os = "windows")]
    {
        // 1. 获取当前 exe 所在的安装目录
        let current_exe = match std::env::current_exe() {
            Ok(p) => p,
            Err(_) => return,
        };
        let current_dir = match current_exe.parent() {
            Some(p) => p,
            None => return,
        };

        // 查找同目录或资源目录下的 ae.exe
        let candidate_paths = [
            current_dir.join("ae.exe"),
            current_dir.join("resources").join("ae.exe"),
            current_dir.join("resources").join("target").join("release").join("ae.exe"),
        ];

        let ae_src = candidate_paths.iter().find(|p| p.exists());

        // 2. 自动同步部署到 WindowsApps（Windows 10/11 原生已在 PATH 环境变量中）
        if let Some(src) = ae_src {
            if let Ok(local_appdata) = std::env::var("LOCALAPPDATA") {
                let windows_apps = std::path::Path::new(&local_appdata)
                    .join("Microsoft")
                    .join("WindowsApps");

                if windows_apps.exists() {
                    let dest = windows_apps.join("ae.exe");
                    let should_copy = match (std::fs::metadata(src), std::fs::metadata(&dest)) {
                        (Ok(sm), Ok(dm)) => sm.len() != dm.len(),
                        _ => true,
                    };
                    if should_copy {
                        let _ = std::fs::copy(src, dest);
                    }
                }
            }
        }
    }
}

