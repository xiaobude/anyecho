<script lang="ts">
  import type { ContentPreview } from '../types';
  import type { Translations } from '../i18n';

  let {
    preview,
    t,
    onClose,
  }: {
    preview: ContentPreview;
    t: Translations;
    onClose: () => void;
  } = $props();

  function highlightLine(text: string, keyword: string): Array<{ text: string; isMatch: boolean }> {
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
</script>

<div class="fixed top-0 right-0 h-full w-[500px] bg-gray-900 border-l border-gray-700/80 shadow-2xl z-40 flex flex-col">
  <!-- Header -->
  <div class="flex items-center justify-between px-4 py-3 border-b border-gray-800 bg-gray-900/95 backdrop-blur-sm">
    <div class="flex-1 min-w-0">
      <div class="text-xs text-gray-400 truncate font-mono" title={preview.file_path}>
        {preview.file_path}
      </div>
      <div class="text-[10px] text-gray-500 mt-0.5">
        {t.colPreview}: <span class="text-amber-400 font-mono font-bold">{preview.keyword}</span>
      </div>
    </div>
    <button
      onclick={onClose}
      class="ml-2 p-1.5 text-gray-400 hover:text-white hover:bg-gray-800 rounded-lg transition-colors shrink-0"
      title={t.settingsClose}
    >
      <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
      </svg>
    </button>
  </div>

  <!-- Content -->
  <div class="flex-1 overflow-y-auto font-mono text-xs">
    {#each preview.lines as line}
      <div
        class="flex border-b border-gray-800/40 {line.is_match ? 'bg-amber-500/10' : ''}"
      >
        <!-- Line number -->
        <div class="w-12 shrink-0 text-right pr-2 py-1 text-gray-500 select-none border-r border-gray-800/40 {line.is_match ? 'text-amber-400 font-bold' : ''}">
          {line.line_number}
        </div>

        <!-- Line content -->
        <div class="flex-1 py-1 px-3 whitespace-pre overflow-x-auto">
          {#each highlightLine(line.text, preview.keyword) as part}
            {#if part.isMatch}
              <mark class="bg-amber-400/30 text-amber-200 rounded px-0.5">{part.text}</mark>
            {:else}
              <span class="text-gray-300">{part.text}</span>
            {/if}
          {/each}
        </div>
      </div>
    {/each}
  </div>
</div>
