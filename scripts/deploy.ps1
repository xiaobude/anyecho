# scripts/deploy.ps1
$targetDir = "$env:LOCALAPPDATA\Microsoft\WindowsApps"
$releaseDir = Join-Path $PSScriptRoot "..\src-tauri\target\release"

Write-Host "`n🚀 正在发布二进制文件到: $targetDir" -ForegroundColor Cyan

if (-not (Test-Path $targetDir)) {
    New-Item -ItemType Directory -Force -Path $targetDir | Out-Null
}

$binaries = @("anyecho.exe", "ae.exe")

foreach ($bin in $binaries) {
    $src = Join-Path $releaseDir $bin
    $dest = Join-Path $targetDir $bin

    if (Test-Path $src) {
        try {
            Copy-Item $src $dest -Force
            $item = Get-Item $dest
            $sizeMB = [math]::Round($item.Length / 1MB, 2)
            Write-Host "  ✓ $($bin.PadRight(12)) ($sizeMB MB) -> 发布成功" -ForegroundColor Green
        } catch {
            Write-Host "  ✕ $bin 发布失败: $_" -ForegroundColor Red
        }
    } else {
        Write-Warning "  ⚠️ 未找到 $src"
    }
}

Write-Host "`n🎉 全部发布成功！您现在可以在任意终端/PowerShell中直接运行 'ae' 或 'anyecho'！`n" -ForegroundColor Cyan
