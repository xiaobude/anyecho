<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import type { SearchItem, SearchResponse, ScanResult, FilterCategory, FilterOption, ContentMatch, ContentPreview, SearchHistoryEntry, ColumnWidths, DocIndexStats } from './lib/types';
  import { dictionaries, type Language } from './lib/i18n';
  import VirtualList from './lib/components/VirtualList.svelte';
  import ContextMenu from './lib/components/ContextMenu.svelte';
  import ContentPreviewPanel from './lib/components/ContentPreview.svelte';
  import SettingsPanel from './lib/components/SettingsPanel.svelte';
  import Toast from './lib/components/Toast.svelte';

  import { formatBytes } from './lib/utils/format';

  let currentLang = $state<Language>('zh');
  const t = $derived(dictionaries[currentLang]);

  let query = $state('');
  let activeFilter = $state<FilterCategory>('all');
  let scanStatus = $state<'idle' | 'scanning' | 'ready'>('idle');
  let fileCount = $state(0);
  let scanTime = $state(0);
  let docStats = $state<DocIndexStats | null>(null);


  let searchResults = $state<SearchItem[]>([]);
  let totalMatches = $state(0);
  let totalBytes = $state(0);
  let searchTimeUs = $state(0);
  let selectedIndex = $state(0);


  function normalizeQueryPunctuation(q: string): string {
    if (!q) return '';
    return q
      .replace(/：/g, ':')
      .replace(/[“”]/g, '"')
      .replace(/[‘’]/g, "'");
  }

  function isQueryContentSearch(q: string): boolean {
    if (!q) return false;
    const normalized = normalizeQueryPunctuation(q).toLowerCase();
    return /(?:^|\s)(?:content:|c:|content"|c")/.test(normalized) || normalized.startsWith('content:') || normalized.startsWith('c:') || normalized.startsWith('content"') || normalized.startsWith('c"');
  }

  function extractContentKeyword(q: string): string {
    if (!q) return '';
    const normalized = normalizeQueryPunctuation(q);
    const m = normalized.match(/(?:content:|c:|content"|c")\s*"?([^"\s]+)"?/i);
    if (m && m[1]) return m[1].replace(/["']/g, '').trim();
    return '';
  }


  // Content search states
  let isContentSearch = $derived(isQueryContentSearch(query));
  let contentMatches = $state<ContentMatch[]>([]);
  let contentSelectedIndex = $state(0);
  let contentSearchInProgress = $state(false);
  let contentTotalMatches = $state(0);
  let contentFilesSearched = $state(0);
  let contentSearchTimeUs = $state(0);


  // Knowledge search states
  let isKnowledgeSearch = $state(false);

  // Spotlight mode state
  let isSpotlight = $state(false);
  const appWindow = getCurrentWindow();

  // Preview panel state
  let preview = $state<ContentPreview | null>(null);
  let previewFile = $state<SearchItem | null>(null);
  let previewContent = $state<ContentPreview | null>(null);
  let isPreviewLoading = $state(false);

  // Settings modal state
  let showSettings = $state(false);

  // Search history state
  let searchHistory = $state<SearchHistoryEntry[]>([]);
  let showSuggestions = $state(false);
  let suggestionSelectedIndex = $state(-1);
  let userNavigatedList = $state(false);


  // Column widths (resizable)
  let colWidths = $state<ColumnWidths>({
    index: 44,
    name: 260,
    type: 100,
    path: 0, // flex-1
    size: 95,
    date: 135,
  });
  let resizingCol = $state<keyof ColumnWidths | null>(null);
  let resizeStartX = $state(0);
  let resizeStartW = $state(0);

  function startResize(col: keyof ColumnWidths, e: MouseEvent) {
    resizingCol = col;
    resizeStartX = e.clientX;
    resizeStartW = colWidths[col];
    document.addEventListener('mousemove', onResize);
    document.addEventListener('mouseup', stopResize);
    e.preventDefault();
  }

  function onResize(e: MouseEvent) {
    if (!resizingCol) return;
    const diff = e.clientX - resizeStartX;
    const minW = resizingCol === 'index' ? 30 : resizingCol === 'type' ? 60 : 70;
    const newW = Math.max(minW, resizeStartW + diff);
    colWidths = { ...colWidths, [resizingCol]: newW };
  }

  function stopResize() {
    resizingCol = null;
    document.removeEventListener('mousemove', onResize);
    document.removeEventListener('mouseup', stopResize);
  }

  let contextMenu = $state<{
    visible: boolean;
    x: number;
    y: number;
    item: SearchItem | null;
  }>({
    visible: false,
    x: 0,
    y: 0,
    item: null,
  });

  let toastMessage = $state('');
  let toastTimer: number | null = null;
  let debounceTimer: number | null = null;
  let searchInputRef = $state<HTMLInputElement | null>(null);

  const filterOptions = $derived<FilterOption[]>([
    { id: 'all', label: t.filterAll, icon: '⚡', queryPrefix: '' },
    { id: 'ai', label: t.filterAi, icon: '🤖', queryPrefix: 'type:ai ' },
    { id: 'folder', label: t.filterFolder, icon: '📁', queryPrefix: 'type:folder ' },
    { id: 'doc', label: t.filterDoc, icon: '📄', queryPrefix: 'type:doc ' },
    { id: 'image', label: t.filterImage, icon: '🖼️', queryPrefix: 'type:image ' },
    { id: 'video', label: t.filterVideo, icon: '🎬', queryPrefix: 'type:video ' },
    { id: 'audio', label: t.filterAudio, icon: '🎵', queryPrefix: 'type:audio ' },
    { id: 'app', label: t.filterApp, icon: '⚡', queryPrefix: 'type:app ' },
    { id: 'archive', label: t.filterArchive, icon: '📦', queryPrefix: 'type:archive ' },
  ]);


  function showToast(msg: string) {
    toastMessage = msg;
    if (toastTimer) clearTimeout(toastTimer);
    toastTimer = window.setTimeout(() => {
      toastMessage = '';
    }, 2000);
  }

  async function handleLanguageChange(lang: Language) {
    currentLang = lang;
    try {
      await invoke('set_setting', { key: 'language', value: lang });
    } catch (e) {
      console.error('Failed to persist language setting:', e);
    }
  }

  async function hideWindow() {
    if (isSpotlight) {
      await appWindow.hide();
    }
  }

  async function startScan() {
    try {
      scanStatus = 'scanning';
      const result = await invoke<ScanResult>('start_scan');
      fileCount = result.count;
      scanTime = result.time_ms;
      scanStatus = 'ready';
      showToast(`${t.toastScanComplete} ${fileCount.toLocaleString()} ${t.statusFiles}`);
      executeSearch();
    } catch (e) {
      console.error('Scan failed:', e);
      scanStatus = 'idle';
      showToast(`${t.toastScanFailed} ${e}`);
    }
  }

  async function executeSearch() {
    const rawQuery = query.trim();
    if (!rawQuery && activeFilter === 'all') {
      contentMatches = [];
      try {
        const res = await invoke<SearchResponse>('search', {
          query: '',
          offset: 0,
          limit: 100,
        });
        searchResults = res.items;
        totalMatches = res.total_matches;
        totalBytes = res.total_bytes || 0;
        searchTimeUs = res.search_time_us;
        selectedIndex = 0;
      } catch (e) {
        console.error('Initial empty search failed:', e);
      }
      return;
    }

    let fullQuery = rawQuery;
    const currentOpt = filterOptions.find((o) => o.id === activeFilter);
    if (currentOpt && currentOpt.queryPrefix && !rawQuery.includes(currentOpt.queryPrefix.trim())) {
      fullQuery = `${currentOpt.queryPrefix}${rawQuery}`;
    }

    if (isQueryContentSearch(fullQuery)) {
      contentMatches = [];
      contentTotalMatches = 0;
      contentFilesSearched = 0;
      contentSelectedIndex = 0;
      contentSearchInProgress = true;

      try {
        const res = await invoke<{
          matches: ContentMatch[];
          files_searched: number;
          total_matches: number;
          search_time_us: number;
          is_complete: boolean;
        }>('search_content', { query: fullQuery });

        contentMatches = res.matches;
        contentTotalMatches = res.total_matches;
        contentFilesSearched = res.files_searched;
        contentSearchTimeUs = res.search_time_us;
        contentSearchInProgress = !res.is_complete;
      } catch (e) {
        console.error('Content search failed:', e);
        contentSearchInProgress = false;
      }
      return;
    }

    try {
      const res = await invoke<SearchResponse>('search', {
        query: fullQuery,
        offset: 0,
        limit: 100,
      });
      searchResults = res.items;
      totalMatches = res.total_matches;
      totalBytes = res.total_bytes || 0;
      searchTimeUs = res.search_time_us;
      selectedIndex = 0;
    } catch (e) {
      console.error('Search failed:', e);
    }
  }


  function handleQueryChange() {
    userNavigatedList = false;
    suggestionSelectedIndex = -1;
    if (query.trim()) {
      showSuggestions = false;
    }
    if (debounceTimer) clearTimeout(debounceTimer);
    debounceTimer = window.setTimeout(() => {
      executeSearch();
    }, 120);
  }

  function handleFilterClick(id: FilterCategory) {
    activeFilter = id;
    userNavigatedList = false;
    suggestionSelectedIndex = -1;
    showSuggestions = false;
    executeSearch();
  }


  async function handleOpenFile(item: SearchItem) {
    try {
      await invoke('open_file', { path: item.full_path });
      hideWindow();
    } catch (e) {
      console.error('Failed to open file:', e);
      showToast(String(e));
    }
  }

  async function handleShowInFolder(item: SearchItem) {
    try {
      await invoke('show_in_folder', { path: item.full_path });
      hideWindow();
    } catch (e) {
      console.error('Failed to show in folder:', e);
      showToast(String(e));
    }
  }

  async function handleCopyPath(item: SearchItem) {
    try {
      await navigator.clipboard.writeText(item.full_path);
      showToast(t.toastCopiedPath);
    } catch (e) {
      console.error('Failed to copy path:', e);
    }
  }

  async function handleCopyName(item: SearchItem) {
    try {
      await navigator.clipboard.writeText(item.name);
      showToast(t.toastCopiedName);
    } catch (e) {
      console.error('Failed to copy name:', e);
    }
  }

  function handleContextMenu(e: MouseEvent, item: SearchItem) {
    e.preventDefault();
    contextMenu = {
      visible: true,
      x: e.clientX,
      y: e.clientY,
      item,
    };
  }

  async function handleContentMatchClick(match: ContentMatch, index: number) {
    contentSelectedIndex = index;
    try {
      const p = await invoke<ContentPreview>('get_content_preview', {
        filePath: match.file_path,
        targetLine: match.line_number,
        contextLines: 6,
        keyword: extractContentKeyword(query),
      });
      preview = p;
    } catch (e) {
      console.error('Failed to get preview:', e);
    }
  }

  function handleContentMatchDblClick(match: ContentMatch) {
    handleOpenFile({
      name: match.file_name,
      full_path: match.file_path,
      size: 0,
      mtime: 0,
      is_directory: false,
      ext: '',
    });
  }



  function highlightText(text: string, keyword: string): Array<{ text: string; isMatch: boolean }> {
    if (!keyword) return [{ text, isMatch: false }];
    const parts: Array<{ text: string; isMatch: boolean }> = [];
    const lowerText = text.toLowerCase();
    const lowerKeyword = keyword.toLowerCase();
    let lastIndex = 0;

    let idx = lowerText.indexOf(lowerKeyword, lastIndex);
    while (idx !== -1) {
      if (idx > lastIndex) {
        parts.push({ text: text.slice(lastIndex, idx), isMatch: false });
      }
      parts.push({ text: text.slice(idx, idx + keyword.length), isMatch: true });
      lastIndex = idx + keyword.length;
      idx = lowerText.indexOf(lowerKeyword, lastIndex);
    }

    if (lastIndex < text.length) {
      parts.push({ text: text.slice(lastIndex), isMatch: false });
    }

    return parts.length > 0 ? parts : [{ text, isMatch: false }];
  }

  async function loadSearchHistory() {
    try {
      searchHistory = await invoke<SearchHistoryEntry[]>('get_recent_searches', { limit: 8 });
    } catch (e) {
      console.error('Failed to load history:', e);
    }
  }

  function applySuggestion(sugQuery: string) {
    query = sugQuery;
    showSuggestions = false;
    suggestionSelectedIndex = -1;
    userNavigatedList = false;
    executeSearch();
    searchInputRef?.focus();
  }

  function handleGlobalKeyDown(e: KeyboardEvent) {
    if (showSettings) {
      if (e.key === 'Escape') showSettings = false;
      return;
    }

    if (preview) {
      if (e.key === 'Escape') {
        preview = null;
        e.preventDefault();
        return;
      }
    }

    if (contextMenu.visible) {
      contextMenu.visible = false;
    }

    // 1. 历史搜索下拉列表活跃时的键盘导航
    if (showSuggestions && searchHistory.length > 0) {
      if (e.key === 'Escape') {
        e.preventDefault();
        showSuggestions = false;
        suggestionSelectedIndex = -1;
        return;
      }

      if (e.key === 'ArrowDown') {
        e.preventDefault();
        suggestionSelectedIndex = (suggestionSelectedIndex + 1) % searchHistory.length;
        return;
      }

      if (e.key === 'ArrowUp') {
        e.preventDefault();
        suggestionSelectedIndex = suggestionSelectedIndex <= 0 ? searchHistory.length - 1 : suggestionSelectedIndex - 1;
        return;
      }

      if (e.key === 'Enter') {
        e.preventDefault();
        if (suggestionSelectedIndex >= 0 && searchHistory[suggestionSelectedIndex]) {
          applySuggestion(searchHistory[suggestionSelectedIndex].query);
        } else {
          // 未特意上下选择建议时，按回车仅确认搜索并关闭建议窗口，绝不误打开文件
          showSuggestions = false;
          executeSearch();
        }
        return;
      }
    }

    // 2. 普通搜索结果列表导航
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      userNavigatedList = true;
      if (isContentSearch) {
        if (contentMatches.length > 0) {
          contentSelectedIndex = Math.min(contentMatches.length - 1, contentSelectedIndex + 1);
          handleContentMatchClick(contentMatches[contentSelectedIndex], contentSelectedIndex);
        }
      } else {
        if (searchResults.length > 0) {
          selectedIndex = Math.min(searchResults.length - 1, selectedIndex + 1);
        }
      }
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      userNavigatedList = true;
      if (isContentSearch) {
        if (contentMatches.length > 0) {
          contentSelectedIndex = Math.max(0, contentSelectedIndex - 1);
          handleContentMatchClick(contentMatches[contentSelectedIndex], contentSelectedIndex);
        }
      } else {
        if (searchResults.length > 0) {
          selectedIndex = Math.max(0, selectedIndex - 1);
        }
      }
    } else if (e.key === 'Enter') {
      e.preventDefault();
      // 用户在输入框打完字后直接按回车：确认并记录搜索词，收起下拉，不误开第一个文件
      if (!userNavigatedList && document.activeElement === searchInputRef) {
        showSuggestions = false;
        if (query.trim()) {
          invoke('record_search_history', { query: query.trim(), resultCount: totalMatches }).catch(() => {});
          loadSearchHistory();
        }
        return;
      }

      // 用户主动按过方向键上下浏览或焦点在列表上时，按回车正常打开选中的文件
      if (isContentSearch) {
        const item = contentMatches[contentSelectedIndex];
        if (item) {
          if (e.shiftKey) {
            handleShowInFolder({
              name: item.file_name,
              full_path: item.file_path,
              size: 0,
              mtime: 0,
              is_directory: false,
              ext: '',
            });
          } else {
            handleContentMatchDblClick(item);
          }
        }
      } else {
        const item = searchResults[selectedIndex];
        if (item) {
          if (e.shiftKey) {
            handleShowInFolder(item);
          } else {
            handleOpenFile(item);
          }
        }
      }
    } else if (e.key === 'Escape') {
      if (showSuggestions) {
        showSuggestions = false;
        suggestionSelectedIndex = -1;
      } else if (query) {
        query = '';
        executeSearch();
      } else {
        hideWindow();
      }
    } else if (e.ctrlKey && e.key.toLowerCase() === 'c') {

      if (!isContentSearch && searchResults[selectedIndex]) {
        handleCopyPath(searchResults[selectedIndex]);
      }
    }
  }

  onMount(() => {
    let unlistenBatch: (() => void) | undefined;
    let unlistenDone: (() => void) | undefined;

    (async () => {
      // 1. 加载语言偏好
      try {
        const savedLang = await invoke<string | null>('get_setting', { key: 'language' });
        if (savedLang === 'zh' || savedLang === 'en') {
          currentLang = savedLang;
        }
      } catch (e) {
        console.error('Failed to load language setting:', e);
      }

      // 2. 检查是否有命令行传入的初始搜索词
      try {
        const initQ = await invoke<string | null>('get_initial_query');
        if (initQ && initQ.trim()) {
          query = initQ.trim();
        }
      } catch (e) {
        console.error('Failed to get initial query:', e);
      }

      // 3. 监听多开实例传入的即时搜索词
      const unlistenOpenQuery = await listen<string>('open-query', (event) => {
        if (event.payload && event.payload.trim()) {
          query = event.payload.trim();
          handleQueryChange();
          searchInputRef?.focus();
        }
      });

      // 4. 检查索引状态
      try {
        const status = await invoke<ScanResult | null>('get_scan_status');
        if (status && status.count > 0) {
          fileCount = status.count;
          scanTime = status.time_ms;
          scanStatus = 'ready';
          executeSearch();
        } else {
          startScan();
        }
      } catch (e) {
        console.error('Failed to get scan status:', e);
        startScan();
      }

      // 5. 文档索引状态轮询
      const fetchDocStats = async () => {
        try {
          docStats = await invoke<DocIndexStats>('get_doc_index_stats');
        } catch (e) {
          // ignore
        }
      };
      fetchDocStats();
      const statsInterval = setInterval(fetchDocStats, 3000);

      window.addEventListener('keydown', handleGlobalKeyDown);

      return () => {
        window.removeEventListener('keydown', handleGlobalKeyDown);
        clearInterval(statsInterval);
        unlistenBatch?.();
        unlistenDone?.();
        unlistenOpenQuery();
      };
    })();
  });


</script>

<div class="flex flex-col h-screen bg-gray-950 text-gray-100 font-sans overflow-hidden select-none {isSpotlight ? 'bg-opacity-95 backdrop-blur-xl' : ''}">
  <!-- Top search bar -->
  <header class="flex flex-col px-4 pt-3 pb-2 bg-gray-900/80 border-b border-gray-800/80 backdrop-blur-md shrink-0 shadow-lg">
    <div class="flex items-center gap-3">
      <div class="flex items-center gap-1.5 shrink-0 select-none">
        <span class="text-xl">⚡</span>
        <span class="font-black text-base tracking-wider bg-gradient-to-r from-blue-400 via-indigo-300 to-purple-400 bg-clip-text text-transparent">{t.appName}</span>
        <span class="text-[10px] text-blue-400 font-bold px-1.5 py-0.5 bg-blue-500/10 rounded border border-blue-500/20">{t.appSub}</span>
      </div>

      <div class="flex-1 relative flex items-center">
        <input
          bind:this={searchInputRef}
          type="text"
          bind:value={query}
          oninput={handleQueryChange}
          onfocus={() => { if (!query) showSuggestions = true; }}
          onblur={() => { setTimeout(() => { showSuggestions = false; }, 200); }}
          placeholder={t.searchPlaceholder}
          class="w-full pl-4 pr-10 py-2 bg-gray-800/90 border border-gray-700/80 rounded-xl text-gray-100 placeholder-gray-500 text-sm focus:outline-none focus:border-blue-500 focus:ring-2 focus:ring-blue-500/30 transition-all shadow-inner"
        />
        {#if query}
          <button
            onclick={() => { query = ''; handleQueryChange(); searchInputRef?.focus(); }}
            class="absolute right-3 text-gray-400 hover:text-white text-xs px-1.5 py-0.5 rounded-full hover:bg-gray-700"
            title={t.settingsClearHistory}
          >
            ✕
          </button>
        {/if}

        <!-- Search suggestions dropdown -->
        {#if showSuggestions && searchHistory.length > 0}
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="absolute top-full left-0 right-0 mt-2 bg-gray-900/95 border border-gray-700/90 rounded-xl shadow-2xl z-50 overflow-hidden backdrop-blur-xl animate-in fade-in zoom-in-95 duration-100"
            onmousedown={(e) => e.stopPropagation()}
          >
            <div class="px-3 py-1.5 text-xs text-gray-400 font-semibold border-b border-gray-800 flex items-center justify-between bg-gray-950/40">
              <span class="flex items-center gap-1.5">
                <span>🕒</span>
                <span>{t.settingsHistory}</span>
              </span>
              <div class="flex items-center gap-3">
                <button
                  type="button"
                  onclick={(e) => {
                    e.stopPropagation();
                    invoke('clear_search_history').then(() => { searchHistory = []; showSuggestions = false; }).catch(() => {});
                  }}
                  class="text-[10px] text-gray-400 hover:text-red-400 hover:underline transition-colors"
                >
                  {t.settingsClearHistory}
                </button>
                <button
                  type="button"
                  onclick={(e) => { e.stopPropagation(); showSuggestions = false; }}
                  class="text-gray-400 hover:text-white text-xs px-1 hover:bg-gray-800 rounded"
                  title="Close / 关闭 (Esc)"
                >
                  ✕
                </button>
              </div>
            </div>
            <div class="max-h-60 overflow-y-auto py-1">
              {#each searchHistory as entry, idx}
                {@const isHighlighted = idx === suggestionSelectedIndex}
                <button
                  type="button"
                  onmouseenter={() => { suggestionSelectedIndex = idx; }}
                  onclick={() => applySuggestion(entry.query)}
                  class="w-full px-3 py-1.5 text-left text-xs transition-colors flex items-center justify-between {isHighlighted ? 'bg-blue-600/30 text-white font-medium' : 'text-gray-200 hover:bg-gray-800/80'}"
                >
                  <span class="truncate font-mono flex items-center gap-2">
                    <span class="text-gray-500 text-[10px]">🔍</span>
                    {entry.query}
                  </span>
                  <span class="text-[11px] text-gray-400 shrink-0 ml-2 tabular-nums">{entry.result_count} {t.statusFiles}</span>
                </button>
              {/each}
            </div>
          </div>
        {/if}

      </div>

      <!-- Language Toggle -->
      <button
        onclick={() => handleLanguageChange(currentLang === 'zh' ? 'en' : 'zh')}
        class="px-2.5 py-1.5 bg-gray-800/80 hover:bg-gray-700 active:bg-gray-600 text-gray-300 rounded-xl text-xs font-semibold transition-colors shrink-0 flex items-center gap-1 border border-gray-700/80"
        title="Switch UI Language / 切换界面语言"
      >
        <span>🌐</span>
        <span>{currentLang === 'zh' ? 'EN' : '中'}</span>
      </button>

      <!-- Settings Button -->
      <button
        onclick={() => { showSettings = true; }}
        class="p-2 text-gray-400 hover:text-white hover:bg-gray-800 rounded-xl transition-colors shrink-0"
        title={t.settings}
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
        </svg>
      </button>

      <!-- Rebuild Index Button -->
      <button
        onclick={startScan}
        disabled={scanStatus === 'scanning'}
        class="px-3.5 py-2 bg-blue-600 hover:bg-blue-500 active:bg-blue-700 disabled:bg-gray-800 disabled:text-gray-500 text-white rounded-xl text-xs font-semibold transition-all shrink-0 flex items-center gap-1.5 shadow-md shadow-blue-900/30"
      >
        {#if scanStatus === 'scanning'}
          <span class="inline-block w-3 h-3 border-2 border-white border-t-transparent rounded-full animate-spin"></span>
          <span>{t.scanning}</span>
        {:else}
          <span>🔄</span>
          <span>{t.rebuildIndex}</span>
        {/if}
      </button>
    </div>

    <!-- Filter chips -->
    <div class="flex items-center gap-1.5 mt-2.5 overflow-x-auto no-scrollbar py-0.5">
      {#each filterOptions as opt}
        <button
          onclick={() => handleFilterClick(opt.id)}
          class="px-2.5 py-1 rounded-lg text-xs font-medium flex items-center gap-1 transition-colors shrink-0 {activeFilter === opt.id ? 'bg-blue-600 text-white shadow-sm' : 'text-gray-400 hover:text-gray-200 hover:bg-gray-800/80'}"
        >
          <span>{opt.icon}</span>
          <span>{opt.label}</span>
        </button>
      {/each}
    </div>
  </header>

  <!-- Content search mode indicator -->
  {#if isContentSearch}
    <div class="flex items-center gap-2 px-4 py-1.5 bg-amber-500/10 border-b border-amber-500/20 text-xs shrink-0">
      <span class="px-1.5 py-0.5 bg-amber-500/20 text-amber-400 rounded font-semibold">{t.modeContent}</span>
      <span class="text-gray-400">
        {t.statusSearchedFiles} <strong class="text-gray-200">{contentFilesSearched}</strong> {t.statusFiles}
        {#if contentSearchInProgress}
          <span class="inline-block w-2.5 h-2.5 border-2 border-amber-400 border-t-transparent rounded-full animate-spin ml-1"></span>
        {/if}
      </span>
    </div>
  {/if}

  <!-- List header -->
  {#if !isContentSearch}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="flex items-center px-3 py-1.5 bg-gray-900/80 border-b border-gray-800/80 text-[11px] font-semibold text-gray-400 shrink-0 select-none">
      <!-- 序号 -->
      <div style="width: {colWidths.index}px" class="relative text-center shrink-0 flex items-center justify-center">
        <span>#</span>
      </div>

      <!-- 名称 -->
      <div style="width: {colWidths.name}px" class="relative pr-3 shrink-0 flex items-center">
        <span class="truncate">{t.colName}</span>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="absolute right-0 top-0 bottom-0 w-2 cursor-col-resize flex justify-center items-center hover:bg-blue-500/30 group z-10"
          onmousedown={(e) => startResize('name', e)}
        >
          <div class="w-[1px] h-3 bg-gray-700 group-hover:bg-blue-400 transition-colors"></div>
        </div>
      </div>

      <!-- 类型 -->
      <div style="width: {colWidths.type}px" class="relative pr-3 shrink-0 flex items-center">
        <span class="truncate">{t.colType}</span>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="absolute right-0 top-0 bottom-0 w-2 cursor-col-resize flex justify-center items-center hover:bg-blue-500/30 group z-10"
          onmousedown={(e) => startResize('type', e)}
        >
          <div class="w-[1px] h-3 bg-gray-700 group-hover:bg-blue-400 transition-colors"></div>
        </div>
      </div>

      <!-- 路径 -->
      <div class="flex-1 pr-3 truncate flex items-center" style="min-width: 120px">
        <span class="truncate">{t.colPath}</span>
      </div>

      <!-- 大小 -->
      <div style="width: {colWidths.size}px" class="relative pr-3 shrink-0 flex items-center justify-end">
        <span class="truncate">{t.colSize}</span>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="absolute right-0 top-0 bottom-0 w-2 cursor-col-resize flex justify-center items-center hover:bg-blue-500/30 group z-10"
          onmousedown={(e) => startResize('size', e)}
        >
          <div class="w-[1px] h-3 bg-gray-700 group-hover:bg-blue-400 transition-colors"></div>
        </div>
      </div>

      <!-- 修改日期 -->
      <div style="width: {colWidths.date}px" class="relative shrink-0 flex items-center justify-end">
        <span class="truncate">{t.colDate}</span>
        <!-- svelte-ignore a11y_no_static_element_interactions -->
        <div
          class="absolute right-0 top-0 bottom-0 w-2 cursor-col-resize flex justify-center items-center hover:bg-blue-500/30 group z-10"
          onmousedown={(e) => startResize('date', e)}
        >
          <div class="w-[1px] h-3 bg-gray-700 group-hover:bg-blue-400 transition-colors"></div>
        </div>
      </div>
    </div>

  {:else}
    <div class="flex items-center px-3 py-1.5 bg-gray-900/60 border-b border-gray-800/60 text-[11px] font-semibold text-gray-400 shrink-0 select-none">
      <div class="w-10 mr-2 text-center">{t.colNum}</div>
      <div class="w-1/3 min-w-[180px] pr-2">{t.colName}</div>
      <div class="flex-1 pr-2">{t.colLine}</div>
      <div class="w-16 text-right">{t.colLine}</div>
    </div>
  {/if}

  <!-- Main content area -->
  <main class="flex-1 flex flex-col overflow-hidden relative">
    {#if scanStatus === 'scanning'}
      <div class="flex flex-col items-center justify-center h-full text-center">
        <div class="w-12 h-12 border-4 border-blue-500 border-t-transparent rounded-full animate-spin mb-4 shadow-lg shadow-blue-500/20"></div>
        <p class="text-sm font-medium text-gray-200 mb-1">{t.stateScanningUsn}</p>
        <p class="text-xs text-gray-500">{t.stateScanningDesc}</p>
      </div>
    {:else if scanStatus === 'idle'}
      <div class="flex flex-col items-center justify-center h-full text-gray-500 text-center">
        <span class="text-4xl mb-3">📁</span>
        <p class="text-sm text-gray-300 font-medium mb-1">{t.stateNoIndex}</p>
        <p class="text-xs text-gray-500 mb-4">{t.stateStartIndexPrompt}</p>
        <button
          onclick={startScan}
          class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-xs font-semibold rounded-lg shadow-lg"
        >
          {t.stateStartIndexBtn}
        </button>
      </div>
    {:else if isContentSearch}
      <!-- Content search results -->
      {#if contentMatches.length === 0 && !contentSearchInProgress}
        <div class="flex flex-col items-center justify-center h-full text-gray-500">
          <div class="text-3xl mb-2">🔍</div>
          <p class="text-sm">{t.stateNoResults}</p>
        </div>
      {:else}
        <div class="flex-1 w-full overflow-y-auto">
          {#each contentMatches as match, idx (idx)}
            {@const isSelected = idx === contentSelectedIndex}
            {@const keyword = extractContentKeyword(query)}
            <div
              role="button"
              tabindex="0"
              class="flex items-center px-3 text-xs border-b border-gray-800/40 cursor-pointer transition-colors duration-75 {isSelected ? 'bg-amber-600/20 text-white border-amber-500/30' : 'text-gray-200 hover:bg-gray-800/60'}"
              onclick={() => handleContentMatchClick(match, idx)}
              ondblclick={() => handleContentMatchDblClick(match)}
              onkeydown={(e) => { if (e.key === 'Enter') handleContentMatchDblClick(match); }}
            >
              <div class="w-10 text-center text-[10px] text-gray-500 mr-2 shrink-0 tabular-nums">
                {idx + 1}
              </div>

              <div class="w-1/3 min-w-[180px] font-medium truncate pr-2 {isSelected ? 'text-amber-200 font-semibold' : ''}" title={match.file_path}>
                {match.file_name}
              </div>

              <div class="flex-1 truncate text-[11px] pr-2 font-mono">
                {#each highlightText(match.line_text.trim(), keyword) as part}
                  {#if part.isMatch}
                    <mark class="bg-amber-400/30 text-amber-200 rounded px-0.5">{part.text}</mark>
                  {:else}
                    <span class="text-gray-400">{part.text}</span>
                  {/if}
                {/each}
              </div>

              <div class="w-16 text-right text-gray-500 shrink-0 tabular-nums text-[10px] font-mono">
                {match.line_number === 0 ? '文件名' : `:${match.line_number}`}
              </div>
            </div>

          {/each}
        </div>
      {/if}
    {:else}
      <VirtualList
        items={searchResults}
        {selectedIndex}
        {t}
        {colWidths}
        onSelect={(idx) => { selectedIndex = idx; }}
        onOpen={handleOpenFile}
        onContextMenu={handleContextMenu}
      />
    {/if}
  </main>

  <!-- Footer status bar -->
  <footer class="flex items-center justify-between px-3 py-1.5 bg-gray-900 border-t border-gray-800 text-[11px] text-gray-400 shrink-0 select-none">
    <div class="flex items-center gap-3">
      {#if isContentSearch}
        <span>
          {t.statusMatched} <strong class="text-gray-200 tabular-nums">{contentTotalMatches.toLocaleString()}</strong>
        </span>
        {#if contentSearchTimeUs > 0}
          <span class="px-1.5 py-0.5 bg-gray-800 text-amber-400 rounded text-[10px] font-mono border border-gray-700/50">
            ⚡ {(contentSearchTimeUs / 1000).toFixed(2)} ms
          </span>
        {/if}
      {:else}
        <span>
          {t.statusMatched} <strong class="text-gray-200 tabular-nums">{totalMatches.toLocaleString()}</strong> / {fileCount.toLocaleString()} {t.statusFiles}
          {#if totalBytes > 0}
            <span class="text-gray-400 font-mono ml-1.5">({formatBytes(totalBytes)})</span>
          {/if}
        </span>

        {#if searchTimeUs > 0}
          <span class="px-1.5 py-0.5 bg-gray-800 text-emerald-400 rounded text-[10px] font-mono border border-gray-700/50">
            ⚡ {(searchTimeUs / 1000).toFixed(2)} ms
          </span>
        {/if}
      {/if}

      {#if docStats && docStats.total_indexed > 0}
        <span class="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full bg-blue-950/60 border border-blue-800/40 text-[10px] text-blue-300 font-mono" title="SQLite 文档全文索引库">
          <span>📖</span>
          {#if docStats.is_indexing}
            <span class="animate-pulse">文档索引中 {docStats.total_indexed.toLocaleString()} / {docStats.total_candidates.toLocaleString()}</span>
          {:else}
            <span>文档库 {docStats.total_indexed.toLocaleString()} 篇</span>
          {/if}
        </span>
      {/if}
    </div>


    <div class="flex-1 text-center truncate px-4 text-gray-500">
      {#if isContentSearch && contentMatches[contentSelectedIndex]}
        <span class="text-gray-300 font-mono" title={contentMatches[contentSelectedIndex].file_path}>
          {contentMatches[contentSelectedIndex].file_path}:{contentMatches[contentSelectedIndex].line_number}
        </span>
      {:else if searchResults[selectedIndex]}
        <span class="text-gray-300 font-mono">{searchResults[selectedIndex].full_path}</span>
      {/if}
    </div>

    <div class="flex items-center gap-2 text-gray-500 shrink-0">
      {#if isContentSearch}
        <span><kbd class="px-1 py-0.5 bg-gray-800 border border-gray-700 rounded text-[10px] text-gray-400 font-mono">↵</kbd> {t.keyOpen}</span>
        <span><kbd class="px-1 py-0.5 bg-gray-800 border border-gray-700 rounded text-[10px] text-gray-400 font-mono">Esc</kbd> {t.settingsClose}</span>
      {:else}
        <span><kbd class="px-1 py-0.5 bg-gray-800 border border-gray-700 rounded text-[10px] text-gray-400 font-mono">↵</kbd> {t.keyOpen}</span>
        <span><kbd class="px-1 py-0.5 bg-gray-800 border border-gray-700 rounded text-[10px] text-gray-400 font-mono">Shift+↵</kbd> {t.keyReveal}</span>
        <span><kbd class="px-1 py-0.5 bg-gray-800 border border-gray-700 rounded text-[10px] text-gray-400 font-mono">Ctrl+C</kbd> {t.keyCopy}</span>
      {/if}
    </div>
  </footer>

  <!-- Context menu -->
  {#if contextMenu.visible && contextMenu.item}
    <ContextMenu
      x={contextMenu.x}
      y={contextMenu.y}
      item={contextMenu.item}
      {t}
      onClose={() => { contextMenu.visible = false; }}
      onOpen={handleOpenFile}
      onShowInFolder={handleShowInFolder}
      onCopyPath={handleCopyPath}
      onCopyName={handleCopyName}
    />
  {/if}

  <!-- Content preview drawer -->
  {#if preview}
    <ContentPreviewPanel
      {preview}
      {t}
      onClose={() => { preview = null; }}
    />
  {/if}

  <!-- Toast notification -->
  <Toast message={toastMessage} />

  <!-- Settings panel -->
  {#if showSettings}
    <SettingsPanel
      {t}
      {currentLang}
      onLanguageChange={handleLanguageChange}
      onClose={() => { showSettings = false; loadSearchHistory(); }}
    />
  {/if}
</div>

<style>
  .no-scrollbar::-webkit-scrollbar {
    display: none;
  }
  .no-scrollbar {
    -ms-overflow-style: none;
    scrollbar-width: none;
  }
</style>
