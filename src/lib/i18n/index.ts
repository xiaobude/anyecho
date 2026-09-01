export type Language = 'zh' | 'en';

export interface Translations {
  appName: string;
  appSub: string;
  searchPlaceholder: string;
  contentSearchPlaceholder: string;
  knowledgeSearchPlaceholder: string;
  rebuildIndex: string;
  scanning: string;
  knowledgeBase: string;
  settings: string;
  modeFileName: string;
  modeContent: string;
  modeKnowledge: string;

  // Filter Categories
  filterAll: string;
  filterAi: string;
  filterFolder: string;
  filterDoc: string;
  filterImage: string;
  filterVideo: string;
  filterAudio: string;
  filterApp: string;
  filterArchive: string;

  // Table Headers
  colNum: string;
  colName: string;
  colType: string;
  colPath: string;
  colSize: string;
  colDate: string;
  colLine: string;
  colPreview: string;
  colScore: string;

  // Status Bar
  statusMatched: string;
  statusFiles: string;
  statusSearchingContent: string;
  statusSearchedFiles: string;
  keyOpen: string;
  keyReveal: string;
  keyCopy: string;

  // Context Menu
  menuOpenFile: string;
  menuShowInFolder: string;
  menuCopyPath: string;
  menuCopyName: string;
  menuAddFavorite: string;
  menuRemoveFavorite: string;

  // States
  stateNoResults: string;
  stateNoIndex: string;
  stateStartIndexPrompt: string;
  stateStartIndexBtn: string;
  stateScanningUsn: string;
  stateScanningDesc: string;

  // Settings Panel
  settingsTitle: string;
  settingsLanguage: string;
  settingsExclusions: string;
  settingsExclusionsDesc: string;
  settingsAddExclusion: string;
  settingsExclusionPlaceholder: string;
  settingsKnowledge: string;
  settingsKnowledgeDesc: string;
  settingsAddFolder: string;
  settingsFolderPlaceholder: string;
  settingsHistory: string;
  settingsClearHistory: string;
  settingsClose: string;

  // Toast
  toastCopiedPath: string;
  toastCopiedName: string;
  toastAddedFavorite: string;
  toastRemovedFavorite: string;
  toastScanComplete: string;
  toastScanFailed: string;
  toastFolderAdded: string;
}

const zh: Translations = {
  appName: '凡响',
  appSub: 'anyecho',
  searchPlaceholder: '输入文件名/拼音 (如: fx 搜 凡响, ext:pdf, size:>10MB)...',
  contentSearchPlaceholder: '输入内容关键词 (如: content:凡响 ext:rs)...',
  knowledgeSearchPlaceholder: '全文语义搜索指定知识库 (如: 架构设计, 部署手册)...',
  rebuildIndex: '重建索引',
  scanning: '扫描中...',
  knowledgeBase: '知识库',
  settings: '设置',
  modeFileName: '文件名搜索',
  modeContent: '即时内容搜索',
  modeKnowledge: '知识库全文',

  filterAll: '全部',
  filterAi: 'AI模型/权重',
  filterFolder: '文件夹',
  filterDoc: '文档',
  filterImage: '图片',
  filterVideo: '视频',
  filterAudio: '音频',
  filterApp: '程序',
  filterArchive: '压缩包',

  colNum: '#',
  colName: '名称',
  colType: '类型',
  colPath: '路径',
  colSize: '大小',
  colDate: '修改日期',
  colLine: '行号',
  colPreview: '匹配内容预览',
  colScore: '相关度',

  statusMatched: '匹配:',
  statusFiles: '个文件',
  statusSearchingContent: '正在极速 Grep 检索内容...',
  statusSearchedFiles: '已扫文件:',
  keyOpen: '↵ 打开',
  keyReveal: 'Shift+↵ 定位',
  keyCopy: 'Ctrl+C 复制',

  menuOpenFile: '打开文件',
  menuShowInFolder: '在资源管理器中显示',
  menuCopyPath: '复制完整路径',
  menuCopyName: '复制文件名',
  menuAddFavorite: '收藏此文件',
  menuRemoveFavorite: '取消收藏',

  stateNoResults: '未找到匹配的文件或目录',
  stateNoIndex: '尚未建立索引',
  stateStartIndexPrompt: '点击右上角「重建索引」开始全盘极速扫描',
  stateStartIndexBtn: '立即扫描磁盘',
  stateScanningUsn: '正在极速枚举 NTFS USN Journal...',
  stateScanningDesc: '秒级装载数百万文件元数据至 Rust 内存紧凑池',

  settingsTitle: '系统偏好设置',
  settingsLanguage: '界面语言 / Language',
  settingsExclusions: '排除路径规则',
  settingsExclusionsDesc: '匹配以下前缀的路径将自动从搜索结果中排除',
  settingsAddExclusion: '添加规则',
  settingsExclusionPlaceholder: '输入要排除的路径前缀 (如 C:\\Windows)...',
  settingsKnowledge: '深度全文索引知识库',
  settingsKnowledgeDesc: '对以下指定目录构建 Tantivy 倒排索引，支持毫秒级全文检索',
  settingsAddFolder: '添加目录',
  settingsFolderPlaceholder: '输入要深度索引的文件夹绝对路径...',
  settingsHistory: '搜索历史记录',
  settingsClearHistory: '清空历史',
  settingsClose: '完成',

  toastCopiedPath: '已复制完整路径到剪贴板',
  toastCopiedName: '已复制文件名到剪贴板',
  toastAddedFavorite: '已添加到收藏夹',
  toastRemovedFavorite: '已从收藏夹移除',
  toastScanComplete: '扫描完成！已索引',
  toastScanFailed: '扫描失败:',
  toastFolderAdded: '已添加知识库目录',
};

const en: Translations = {
  appName: 'AnyEcho',
  appSub: '凡响',
  searchPlaceholder: 'Search files or Pinyin (e.g. fx for 凡响, ext:pdf, size:>10MB)...',
  contentSearchPlaceholder: 'Search file content (e.g. content:anyecho ext:rs)...',
  knowledgeSearchPlaceholder: 'Full-text search in knowledge folders (e.g. architecture, deployment)...',
  rebuildIndex: 'Rebuild Index',
  scanning: 'Scanning...',
  knowledgeBase: 'Knowledge Base',
  settings: 'Settings',
  modeFileName: 'Filename',
  modeContent: 'Content Grep',
  modeKnowledge: 'Knowledge Base',

  filterAll: 'All',
  filterAi: 'AI / Weights',
  filterFolder: 'Folders',
  filterDoc: 'Docs',
  filterImage: 'Images',
  filterVideo: 'Videos',
  filterAudio: 'Audio',
  filterApp: 'Apps',
  filterArchive: 'Archives',

  colNum: '#',
  colName: 'Name',
  colType: 'Type',
  colPath: 'Path',
  colSize: 'Size',
  colDate: 'Date Modified',
  colLine: 'Line',
  colPreview: 'Content Preview',
  colScore: 'Score',

  statusMatched: 'Matched:',
  statusFiles: 'files',
  statusSearchingContent: 'Searching file content with Grep...',
  statusSearchedFiles: 'Files scanned:',
  keyOpen: '↵ Open',
  keyReveal: 'Shift+↵ Reveal',
  keyCopy: 'Ctrl+C Copy',

  menuOpenFile: 'Open File',
  menuShowInFolder: 'Show in Explorer',
  menuCopyPath: 'Copy Full Path',
  menuCopyName: 'Copy File Name',
  menuAddFavorite: 'Add to Favorites',
  menuRemoveFavorite: 'Remove from Favorites',

  stateNoResults: 'No matching files or folders found',
  stateNoIndex: 'No index built yet',
  stateStartIndexPrompt: 'Click "Rebuild Index" to scan all drives',
  stateStartIndexBtn: 'Scan Disk Now',
  stateScanningUsn: 'Enumerating NTFS USN Journal...',
  stateScanningDesc: 'Loading millions of file metadata into Rust memory arena',

  settingsTitle: 'Preferences & Settings',
  settingsLanguage: 'Interface Language',
  settingsExclusions: 'Path Exclusion Rules',
  settingsExclusionsDesc: 'Paths matching these prefixes will be excluded from search results',
  settingsAddExclusion: 'Add Rule',
  settingsExclusionPlaceholder: 'Enter path prefix to exclude (e.g. C:\\Windows)...',
  settingsKnowledge: 'Full-Text Knowledge Folders',
  settingsKnowledgeDesc: 'Build Tantivy inverted indexes for these directories for deep content retrieval',
  settingsAddFolder: 'Add Folder',
  settingsFolderPlaceholder: 'Enter absolute folder path...',
  settingsHistory: 'Search History',
  settingsClearHistory: 'Clear History',
  settingsClose: 'Done',

  toastCopiedPath: 'Copied full path to clipboard',
  toastCopiedName: 'Copied filename to clipboard',
  toastAddedFavorite: 'Added to favorites',
  toastRemovedFavorite: 'Removed from favorites',
  toastScanComplete: 'Scan complete! Indexed',
  toastScanFailed: 'Scan failed:',
  toastFolderAdded: 'Knowledge folder added',
};

export const dictionaries: Record<Language, Translations> = { zh, en };
