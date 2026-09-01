<script lang="ts">
  let { message = '' }: { message: string } = $props();
</script>

{#if message}
  <div class="fixed bottom-10 left-1/2 -translate-x-1/2 z-50 px-4 py-2 bg-gray-900/90 text-white text-xs rounded-full border border-gray-700 shadow-xl backdrop-blur-md flex items-center gap-2 animate-bounce">
    <span>✨</span>
    <span>{message}</span>
  </div>
{/if}
