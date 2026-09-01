<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import type { SearchHistoryEntry, Favorite, ExclusionRule } from '../types';
  import type { Language, Translations } from '../i18n';

  let {
    t,
    currentLang = 'zh',
    onLanguageChange,
    onClose,
  }: {
    t: Translations;
    currentLang: Language;
    onLanguageChange: (lang: Language) => void;
    onClose: () => void;
  } = $props();

  type Tab = 'general' | 'history' | 'favorites' | 'exclusions' | 'knowledge';
  let activeTab = $state<Tab>('general');

  let recentSearches = $state<SearchHistoryEntry[]>([]);
  let favorites = $state<Favorite[]>([]);
  let exclusions = $state<ExclusionRule[]>([]);
  let knowledgeFolders = $state<string[]>([]);
  let newExclusion = $state('');
  let newKnowledgeFolder = $state('');

  function switchTab(tab: Tab) {
    activeTab = tab;
    loadTabData(tab);
  }

  async function loadTabData(tab: Tab) {
    try {
      switch (tab) {
        case 'history':
          recentSearches = await invoke<SearchHistoryEntry[]>('get_recent_searches', { limit: 50 });
          break;
        case 'favorites':
          favorites = await invoke<Favorite[]>('get_favorites');
          break;
        case 'exclusions':
          exclusions = await invoke<ExclusionRule[]>('get_exclusions');
          break;
        case 'knowledge':
          knowledgeFolders = await invoke<string[]>('get_knowledge_folders');
          break;
      }
    } catch (e) {
      console.error('Failed to load tab data:', e);
    }
  }

  async function clearHistory() {
    try {
      await invoke('clear_search_history');
      recentSearches = [];
    } catch (e) {
      console.error('Failed to clear history:', e);
    }
  }

  async function removeFavorite(path: string) {
    try {
      await invoke('remove_favorite', { path });
      favorites = favorites.filter(f => f.file_path !== path);
    } catch (e) {
      console.error('Failed to remove favorite:', e);
    }
  }

  async function addExclusion() {
    if (!newExclusion.trim()) return;
    try {
      await invoke('add_exclusion', { pattern: newExclusion.trim() });
      newExclusion = '';
      exclusions = await invoke<ExclusionRule[]>('get_exclusions');
    } catch (e) {
      console.error('Failed to add exclusion:', e);
    }
  }

  async function removeExclusion(id: number) {
    try {
      await invoke('remove_exclusion', { id });
      exclusions = exclusions.filter(e => e.id !== id);
    } catch (e) {
      console.error('Failed to remove exclusion:', e);
    }
  }

  async function addKnowledgeFolder() {
    if (!newKnowledgeFolder.trim()) return;
    try {
      await invoke('add_knowledge_folder', { path: newKnowledgeFolder.trim() });
      newKnowledgeFolder = '';
      knowledgeFolders = await invoke<string[]>('get_knowledge_folders');
    } catch (e) {
      console.error('Failed to add knowledge folder:', e);
    }
  }

  async function removeKnowledgeFolder(path: string) {
    try {
      await invoke('remove_knowledge_folder', { path });
      knowledgeFolders = knowledgeFolders.filter(f => f !== path);
    } catch (e) {
      console.error('Failed to remove knowledge folder:', e);
    }
  }

  function formatTime(ts: number): string {
    const d = new Date(ts * 1000);
    return `${d.getMonth() + 1}/${d.getDate()} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`;
  }

  onMount(() => {
    loadTabData('general');
  });
</script>

<!-- Backdrop -->
<!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
<div role="dialog" aria-modal="true" aria-label={t.settingsTitle} tabindex="-1" class="fixed inset-0 bg-black/60 z-50 flex items-center justify-center backdrop-blur-sm" onclick={(e) => { if (e.target === e.currentTarget) onClose(); }} onkeydown={(e) => { if (e.key === 'Escape') onClose(); }}>
  <!-- Panel -->
  <div class="w-[660px] max-h-[82vh] bg-gray-900 rounded-2xl border border-gray-700/80 shadow-2xl flex flex-col overflow-hidden">
    <!-- Header -->
    <div class="flex items-center justify-between px-5 py-4 border-b border-gray-800 bg-gray-900/90">
      <div class="flex items-center gap-2">
        <span class="text-base">⚙️</span>
        <h2 class="text-base font-bold text-gray-100">{t.settingsTitle}</h2>
      </div>
      <button onclick={onClose} aria-label={t.settingsClose} class="p-1.5 text-gray-400 hover:text-white hover:bg-gray-800 rounded-lg transition-colors">
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
        </svg>
      </button>
    </div>

    <!-- Tabs -->
    <div class="flex border-b border-gray-800 px-5 bg-gray-950/40">
      {#each [
        ['general', currentLang === 'zh' ? '通用设置' : 'General'],
        ['history', t.settingsHistory],
        ['favorites', currentLang === 'zh' ? '收藏夹' : 'Favorites'],
        ['exclusions', t.settingsExclusions],
        ['knowledge', t.knowledgeBase]
      ] as [tab, label]}
        <button
          onclick={() => switchTab(tab as Tab)}
          class="px-3.5 py-2.5 text-xs font-medium border-b-2 transition-colors {activeTab === tab ? 'border-blue-500 text-blue-400' : 'border-transparent text-gray-400 hover:text-gray-200'}"
        >
          {label}
        </button>
      {/each}
    </div>

    <!-- Content -->
    <div class="flex-1 overflow-y-auto p-5">
      {#if activeTab === 'general'}
        <div class="space-y-4">
          <!-- Language Selector -->
          <div class="flex items-center justify-between py-2">
            <div>
              <div class="text-sm font-medium text-gray-200">{t.settingsLanguage}</div>
              <div class="text-xs text-gray-500 mt-0.5">Switch UI language / 切换界面语言</div>
            </div>
            <div class="flex items-center bg-gray-800 border border-gray-700 rounded-lg p-0.5">
              <button
                onclick={() => onLanguageChange('zh')}
                class="px-3 py-1 text-xs rounded-md font-medium transition-colors {currentLang === 'zh' ? 'bg-blue-600 text-white' : 'text-gray-400 hover:text-gray-200'}"
              >
                简体中文
              </button>
              <button
                onclick={() => onLanguageChange('en')}
                class="px-3 py-1 text-xs rounded-md font-medium transition-colors {currentLang === 'en' ? 'bg-blue-600 text-white' : 'text-gray-400 hover:text-gray-200'}"
              >
                English
              </button>
            </div>
          </div>

          <div class="border-t border-gray-800 pt-4">
            <div class="flex items-center justify-between py-2">
              <div>
                <div class="text-sm font-medium text-gray-200">{currentLang === 'zh' ? '全局唤醒热键' : 'Global Shortcut'}</div>
                <div class="text-xs text-gray-500 mt-0.5">{currentLang === 'zh' ? '使用 Alt+Space 快速唤出/隐藏搜索窗口' : 'Press Alt+Space to toggle search window'}</div>
              </div>
              <kbd class="px-2.5 py-1 bg-gray-800 border border-gray-700 rounded text-xs text-gray-300 font-mono shadow-sm">Alt + Space</kbd>
            </div>
          </div>

          <div class="border-t border-gray-800 pt-4">
            <div class="flex items-center justify-between py-2">
              <div>
                <div class="text-sm font-medium text-gray-200">Spotlight Mode</div>
                <div class="text-xs text-gray-500 mt-0.5">{currentLang === 'zh' ? '窗口失焦时自动隐藏，极简浮窗沉浸式搜索' : 'Auto-hide when window loses focus'}</div>
              </div>
              <span class="px-2 py-0.5 bg-emerald-500/10 text-emerald-400 rounded text-xs font-medium border border-emerald-500/20">Enabled</span>
            </div>
          </div>

          <div class="border-t border-gray-800 pt-4">
            <div class="text-sm font-medium text-gray-200 mb-2">{currentLang === 'zh' ? '关于 AnyEcho 凡响' : 'About AnyEcho'}</div>
            <div class="text-xs text-gray-500 space-y-1 font-mono">
              <div>AnyEcho v0.1.0 (NTFS USN Journal Engine)</div>
              <div>Data path: %LOCALAPPDATA%/anyecho/</div>
            </div>
          </div>
        </div>

      {:else if activeTab === 'history'}
        <div class="space-y-2">
          {#if recentSearches.length === 0}
            <div class="text-center text-gray-500 text-sm py-8">{currentLang === 'zh' ? '暂无搜索历史' : 'No search history'}</div>
          {:else}
            <div class="flex justify-end mb-3">
              <button onclick={clearHistory} class="px-3 py-1.5 text-xs text-red-400 hover:text-red-300 hover:bg-red-500/10 rounded-lg transition-colors font-medium">
                {t.settingsClearHistory}
              </button>
            </div>
            {#each recentSearches as entry}
              <div class="flex items-center justify-between py-2 px-3 rounded-lg hover:bg-gray-800/60 transition-colors">
                <div class="flex-1 min-w-0">
                  <div class="text-sm text-gray-200 truncate font-mono">{entry.query}</div>
                  <div class="text-[10px] text-gray-500 mt-0.5">{formatTime(entry.searched_at)} · {entry.result_count} {t.statusFiles}</div>
                </div>
              </div>
            {/each}
          {/if}
        </div>

      {:else if activeTab === 'favorites'}
        <div class="space-y-2">
          {#if favorites.length === 0}
            <div class="text-center text-gray-500 text-sm py-8">{currentLang === 'zh' ? '暂无收藏文件' : 'No favorites added yet'}</div>
          {:else}
            {#each favorites as fav}
              <div class="flex items-center justify-between py-2 px-3 rounded-lg hover:bg-gray-800/60 transition-colors">
                <div class="flex-1 min-w-0">
                  <div class="text-sm text-gray-200 truncate">{fav.file_name}</div>
                  <div class="text-[10px] text-gray-500 mt-0.5 truncate font-mono">{fav.file_path}</div>
                </div>
                <button onclick={() => removeFavorite(fav.file_path)} class="ml-2 p-1 text-gray-500 hover:text-red-400 transition-colors" title={t.menuRemoveFavorite}>
                  <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>
            {/each}
          {/if}
        </div>

      {:else if activeTab === 'exclusions'}
        <div class="space-y-3">
          <div class="flex gap-2">
            <input
              bind:value={newExclusion}
              onkeydown={(e) => { if (e.key === 'Enter') addExclusion(); }}
              placeholder={t.settingsExclusionPlaceholder}
              class="flex-1 px-3 py-2 bg-gray-800 border border-gray-700 rounded-lg text-sm text-gray-200 placeholder-gray-500 focus:outline-none focus:border-blue-500 font-mono"
            />
            <button onclick={addExclusion} class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-xs font-semibold rounded-lg transition-colors">
              {t.settingsAddExclusion}
            </button>
          </div>

          <div class="text-xs text-gray-500">
            {t.settingsExclusionsDesc}
          </div>

          {#if exclusions.length === 0}
            <div class="text-center text-gray-500 text-sm py-8">{currentLang === 'zh' ? '暂无排除规则' : 'No exclusion rules'}</div>
          {:else}
            {#each exclusions as rule}
              <div class="flex items-center justify-between py-2 px-3 rounded-lg bg-gray-800/40">
                <div class="text-sm text-gray-300 font-mono truncate">{rule.pattern}</div>
                <button onclick={() => removeExclusion(rule.id)} aria-label="Remove exclusion rule" class="ml-2 p-1 text-gray-500 hover:text-red-400 transition-colors shrink-0">
                  <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>
            {/each}
          {/if}
        </div>

      {:else if activeTab === 'knowledge'}
        <div class="space-y-3">
          <div class="flex gap-2">
            <input
              bind:value={newKnowledgeFolder}
              onkeydown={(e) => { if (e.key === 'Enter') addKnowledgeFolder(); }}
              placeholder={t.settingsFolderPlaceholder}
              class="flex-1 px-3 py-2 bg-gray-800 border border-gray-700 rounded-lg text-sm text-gray-200 placeholder-gray-500 focus:outline-none focus:border-blue-500 font-mono"
            />
            <button onclick={addKnowledgeFolder} class="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-xs font-semibold rounded-lg transition-colors">
              {t.settingsAddFolder}
            </button>
          </div>

          <div class="text-xs text-gray-500">
            {t.settingsKnowledgeDesc}
          </div>

          {#if knowledgeFolders.length === 0}
            <div class="text-center text-gray-500 text-sm py-8">{currentLang === 'zh' ? '暂无知识库文件夹' : 'No knowledge folders'}</div>
          {:else}
            {#each knowledgeFolders as folder}
              <div class="flex items-center justify-between py-2 px-3 rounded-lg bg-gray-800/40">
                <div class="text-sm text-gray-300 font-mono truncate">{folder}</div>
                <button onclick={() => removeKnowledgeFolder(folder)} aria-label="Remove knowledge folder" class="ml-2 p-1 text-gray-500 hover:text-red-400 transition-colors shrink-0">
                  <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>
            {/each}
          {/if}
        </div>
      {/if}
    </div>
  </div>
</div>
