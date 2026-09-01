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
