export function formatBytes(bytes: number, isDirectory: boolean = false): string {
  if (isDirectory) return '<DIR>';
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  let size = bytes;
  let unitIndex = 0;
  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex++;
  }
  return `${size.toFixed(unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`;
}

export function formatDate(timestamp: number): string {
  if (!timestamp) return '-';
  // Windows file timestamps may be 100-ns intervals since 1601 or Unix timestamps in seconds
  let date: Date;
  if (timestamp > 10000000000000) {
    // Windows FILETIME format (100-ns intervals since 1601-01-01)
    const msSince1970 = (timestamp - 116444736000000000) / 10000;
    date = new Date(msSince1970);
  } else if (timestamp > 10000000000) {
    // Milliseconds Unix
    date = new Date(timestamp);
  } else {
    // Seconds Unix
    date = new Date(timestamp * 1000);
  }

  if (isNaN(date.getTime())) return '-';

  const y = date.getFullYear();
  const m = String(date.getMonth() + 1).padStart(2, '0');
  const d = String(date.getDate()).padStart(2, '0');
  const hh = String(date.getHours()).padStart(2, '0');
  const mm = String(date.getMinutes()).padStart(2, '0');
  return `${y}-${m}-${d} ${hh}:${mm}`;
}

export function getFileTypeName(ext: string, isDirectory: boolean, lang: 'zh' | 'en' = 'zh'): string {
  if (isDirectory) {
    return lang === 'zh' ? '文件夹' : 'Folder';
  }
  if (!ext) {
    return lang === 'zh' ? '文件' : 'File';
  }
  return ext.toUpperCase();
}


export function getFileIcon(ext: string, isDirectory: boolean): { icon: string; color: string } {
  if (isDirectory) {
    return { icon: '📁', color: 'text-amber-400' };
  }

  const lower = ext.toLowerCase();
  switch (lower) {
    case 'gguf':
    case 'safetensors':
    case 'pt':
    case 'pth':
    case 'onnx':
    case 'ckpt':
    case 'tflite':
    case 'engine':
    case 'trt':
    case 'nvfp4':
    case 'fp8':
    case 'awq':
    case 'gptq':
    case 'ggml':
    case 'mlmodel':
    case 'weights':
    case 'h5':
    case 'pb':
    case 'modelfile':
    case 'torchscript':
      return { icon: '🤖', color: 'text-fuchsia-400 font-bold' };
    case 'pdf':
      return { icon: '📕', color: 'text-red-400' };
    case 'doc':
    case 'docx':
    case 'wps':
      return { icon: '📘', color: 'text-blue-400' };
    case 'xls':
    case 'xlsx':
    case 'csv':
      return { icon: '📗', color: 'text-emerald-400' };
    case 'ppt':
    case 'pptx':
      return { icon: '📙', color: 'text-orange-400' };
    case 'txt':
    case 'md':
    case 'json':
    case 'yaml':
    case 'toml':
    case 'xml':
      return { icon: '📄', color: 'text-gray-300' };
    case 'rs':
    case 'ts':
    case 'js':
    case 'py':
    case 'c':
    case 'cpp':
    case 'go':
    case 'java':
    case 'html':
    case 'css':
    case 'svelte':
      return { icon: '💻', color: 'text-cyan-400' };
    case 'png':
    case 'jpg':
    case 'jpeg':
    case 'gif':
    case 'webp':
    case 'svg':
    case 'ico':
    case 'bmp':
      return { icon: '🖼️', color: 'text-purple-400' };
    case 'mp4':
    case 'mkv':
    case 'avi':
    case 'mov':
    case 'wmv':
    case 'flv':
      return { icon: '🎬', color: 'text-rose-400' };
    case 'mp3':
    case 'flac':
    case 'wav':
    case 'aac':
    case 'ogg':
    case 'm4a':
      return { icon: '🎵', color: 'text-pink-400' };
    case 'zip':
    case 'rar':
    case '7z':
    case 'tar':
    case 'gz':
    case 'bz2':
      return { icon: '📦', color: 'text-yellow-500' };
    case 'exe':
    case 'msi':
    case 'bat':
    case 'cmd':
    case 'ps1':
    case 'lnk':
      return { icon: '⚡', color: 'text-indigo-400' };
    default:
      return { icon: '📄', color: 'text-gray-400' };
  }
}

export function decodeName(name: string): string {
  if (!name || !name.includes('%')) return name;
  try {
    return decodeURIComponent(name);
  } catch {
    return name;
  }
}

