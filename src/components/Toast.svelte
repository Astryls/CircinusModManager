<script lang="ts">
  import { store } from "$lib/store.svelte";
</script>

{#if store.toast}
  <div class="toast {store.toast.kind}" role="status">{store.toast.msg}</div>
{/if}
{#if store.progress}
  <div class="toast prog" role="status">Reading mods… {store.progress.done} of {store.progress.total}</div>
{/if}

<style>
  .toast { position: fixed; left: 50%; bottom: 26px; transform: translateX(-50%); max-width: min(720px, calc(100vw - 40px)); padding: 10px 16px; border-radius: 12px; background: var(--surface-2); color: var(--text); font-weight: 600; font-size: 13px; box-shadow: var(--shadow-float); z-index: 60; white-space: pre-wrap; }
  .toast.err { background: #3a1c1a; color: #ffb4ae; }
  .toast.warn { background: #3a2c14; color: #ffd28a; }
  .toast.prog { bottom: 70px; color: var(--text-2); }
</style>
