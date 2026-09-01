<script lang="ts">
  import type { SearchItem } from '../types';
  import type { Translations } from '../i18n';

  let {
    x = 0,
    y = 0,
    item,
    t,
    onClose,
    onOpen,
    onShowInFolder,
    onCopyPath,
    onCopyName,
  }: {
    x: number;
    y: number;
    item: SearchItem;
    t: Translations;
    onClose: () => void;
    onOpen: (item: SearchItem) => void;
    onShowInFolder: (item: SearchItem) => void;
    onCopyPath: (item: SearchItem) => void;
    onCopyName: (item: SearchItem) => void;
  } = $props();

  let menuRef = $state<HTMLDivElement | null>(null);

  function handleClickOutside(e: MouseEvent) {
    if (menuRef && !menuRef.contains(e.target as Node)) {
      onClose();
    }
  }

  $effect(() => {
    window.addEventListener('mousedown', handleClickOutside);
    return () => {
      window.removeEventListener('mousedown', handleClickOutside);
    };
  });
</script>

<div
  bind:this={menuRef}
  style="top: {y}px; left: {x}px;"
  class="fixed z-50 min-w-[190px] bg-gray-900 border border-gray-700/80 rounded-lg shadow-2xl py-1 text-xs text-gray-200 select-none backdrop-blur-md"
>
  <div class="px-3 py-1.5 border-b border-gray-800 text-[11px] text-gray-400 font-medium truncate max-w-[260px]">
    {item.name}
  </div>

  <button
    class="w-full flex items-center px-3 py-1.5 hover:bg-blue-600 hover:text-white transition-colors text-left"
    onclick={() => { onOpen(item); onClose(); }}
  >
    <span class="mr-2">🚀</span> {t.menuOpenFile}
  </button>

  <button
    class="w-full flex items-center px-3 py-1.5 hover:bg-blue-600 hover:text-white transition-colors text-left"
    onclick={() => { onShowInFolder(item); onClose(); }}
  >
    <span class="mr-2">📂</span> {t.menuShowInFolder}
  </button>

  <div class="my-1 border-t border-gray-800"></div>

  <button
    class="w-full flex items-center px-3 py-1.5 hover:bg-blue-600 hover:text-white transition-colors text-left"
    onclick={() => { onCopyPath(item); onClose(); }}
  >
    <span class="mr-2">📋</span> {t.menuCopyPath}
  </button>

  <button
    class="w-full flex items-center px-3 py-1.5 hover:bg-blue-600 hover:text-white transition-colors text-left"
    onclick={() => { onCopyName(item); onClose(); }}
  >
    <span class="mr-2">📝</span> {t.menuCopyName}
  </button>
</div>
