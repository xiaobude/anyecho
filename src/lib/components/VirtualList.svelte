<script lang="ts">
  import type { SearchItem, ColumnWidths } from '../types';
  import type { Translations } from '../i18n';
  import { formatBytes, formatDate, getFileIcon, getFileTypeName } from '../utils/format';

  let {
    items = [],
    selectedIndex = 0,
    itemHeight = 36,
    t,
    colWidths,
    onSelect,
    onOpen,
    onContextMenu,
  }: {
    items: SearchItem[];
    selectedIndex: number;
    itemHeight?: number;
    t: Translations;
    colWidths: ColumnWidths;
    onSelect: (index: number) => void;
    onOpen: (item: SearchItem) => void;
    onContextMenu: (e: MouseEvent, item: SearchItem) => void;
  } = $props();

  let containerRef = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let containerHeight = $state(500);

  const totalHeight = $derived(items.length * itemHeight);
  const overscan = 5;

  const startIndex = $derived(
    Math.max(0, Math.floor(scrollTop / itemHeight) - overscan)
  );
  const visibleCount = $derived(
    Math.ceil(containerHeight / itemHeight) + 2 * overscan
  );
  const endIndex = $derived(
    Math.min(items.length, startIndex + visibleCount)
  );

  const visibleItems = $derived(
    items.slice(startIndex, endIndex).map((item, idx) => ({
      item,
      index: startIndex + idx,
      top: (startIndex + idx) * itemHeight,
    }))
  );

  function handleScroll(e: Event) {
    const target = e.currentTarget as HTMLDivElement;
    scrollTop = target.scrollTop;
  }

  $effect(() => {
    if (containerRef) {
      containerHeight = containerRef.clientHeight;
    }
  });

  $effect(() => {
    if (!containerRef || items.length === 0) return;
    const targetTop = selectedIndex * itemHeight;
    const targetBottom = targetTop + itemHeight;
    const currentScrollTop = containerRef.scrollTop;
    const currentScrollBottom = currentScrollTop + containerHeight;

    if (targetTop < currentScrollTop) {
      containerRef.scrollTop = targetTop;
    } else if (targetBottom > currentScrollBottom) {
      containerRef.scrollTop = targetBottom - containerHeight;
    }
  });
</script>

<div
  bind:this={containerRef}
  onscroll={handleScroll}
  class="flex-1 w-full overflow-y-auto overflow-x-hidden relative select-none"
  style="contain: strict;"
>
  {#if items.length === 0}
    <div class="flex flex-col items-center justify-center h-full text-gray-500">
      <div class="text-3xl mb-2">🔍</div>
      <p class="text-sm">{t.stateNoResults}</p>
    </div>
  {:else}
    <div style="height: {totalHeight}px; width: 100%; position: relative;">
      {#each visibleItems as { item, index, top } (index)}
        {@const iconInfo = getFileIcon(item.ext, item.is_directory)}
        {@const typeName = getFileTypeName(item.ext, item.is_directory, t.appName === '凡响' ? 'zh' : 'en')}
        {@const isSelected = index === selectedIndex}
        <div
          role="button"
          tabindex="0"
          style="position: absolute; top: {top}px; height: {itemHeight}px; left: 0; right: 0;"
          class="flex items-center px-3 text-xs border-b border-gray-800/40 cursor-pointer transition-colors duration-75 {isSelected ? 'bg-blue-600/30 text-white border-blue-500/50' : 'text-gray-200 hover:bg-gray-800/60'}"
          onclick={() => onSelect(index)}
          ondblclick={() => onOpen(item)}
          oncontextmenu={(e) => {
            onSelect(index);
            onContextMenu(e, item);
          }}
          onkeydown={(e) => {
            if (e.key === 'Enter') onOpen(item);
          }}
        >
          <!-- 序号 -->
          <span style="width: {colWidths.index}px" class="text-center text-[10px] text-gray-500 shrink-0 tabular-nums">{index + 1}</span>

          <!-- 文件名 -->
          <div style="width: {colWidths.name}px" class="font-medium truncate pr-3 shrink-0 {isSelected ? 'text-blue-200 font-semibold' : ''}" title={item.name}>
            <span class="inline-block text-sm mr-1.5 {iconInfo.color}">{iconInfo.icon}</span>
            {item.name}
          </div>

          <!-- 类型 / 扩展名 -->
          <div style="width: {colWidths.type}px" class="truncate pr-3 shrink-0 text-[11px] {item.is_directory ? 'text-amber-400/90' : 'text-gray-400 font-mono uppercase'}" title={typeName}>
            {typeName}
          </div>


          <!-- 路径 -->
          <div class="flex-1 text-gray-400 truncate text-[11px] pr-3 font-mono" style="min-width: 120px" title={item.full_path}>
            {item.full_path}
          </div>

          <!-- 大小 -->
          <div style="width: {colWidths.size}px" class="text-right text-gray-300 shrink-0 tabular-nums font-mono pr-3">
            {formatBytes(item.size, item.is_directory)}
          </div>

          <!-- 修改时间 -->
          <div style="width: {colWidths.date}px" class="text-right text-gray-400 shrink-0 tabular-nums font-mono">
            {formatDate(item.mtime)}
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>
