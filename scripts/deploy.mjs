import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';


const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const projectRoot = path.resolve(__dirname, '..');

const localAppData = process.env.LOCALAPPDATA || path.join(process.env.USERPROFILE || 'C:\\Users\\xiaobude', 'AppData', 'Local');
const targetDir = path.join(localAppData, 'Microsoft', 'WindowsApps');
const releaseDir = path.join(projectRoot, 'src-tauri', 'target', 'release');


console.log(`\n🚀 正在发布二进制文件到: ${targetDir}`);

if (!fs.existsSync(targetDir)) {
  fs.mkdirSync(targetDir, { recursive: true });
}

const binaries = ['anyecho.exe', 'ae.exe'];
let successCount = 0;

for (const bin of binaries) {
  const src = path.join(releaseDir, bin);
  const dest = path.join(targetDir, bin);

  if (fs.existsSync(src)) {
    try {
      fs.copyFileSync(src, dest);
      const stat = fs.statSync(dest);
      const sizeMB = (stat.size / (1024 * 1024)).toFixed(2);
      console.log(`  ✓ ${bin.padEnd(12)} (${sizeMB} MB) -> 发布成功`);
      successCount++;
    } catch (err) {
      console.error(`  ✕ ${bin} 发布失败: ${err.message}`);
    }
  } else {
    console.warn(`  ⚠️ 未找到 ${src}`);
  }
}

if (successCount === binaries.length) {
  console.log(`\n🎉 全部发布成功！您现在可以在任意终端/PowerShell中直接运行 'ae' 或 'anyecho'！\n`);
}
