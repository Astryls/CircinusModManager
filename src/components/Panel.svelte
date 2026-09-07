<script lang="ts">
  // Error boundary: a panel that throws shows the error instead of taking the app down.
  import type { Snippet } from "svelte";
  import { api } from "$lib/api";
  let { name, children }: { name: string; children: Snippet } = $props();
</script>

<svelte:boundary
  onerror={(e) => {
    // The console is not somewhere a player can look, so this goes to the log file as well.
    const err = e as { message?: string; stack?: string };
    console.error(`[circinus] ${name} failed to render:`, e);
    api.logFromTheWindow(`${name} failed to render: ${err?.message ?? String(e)}`, err?.stack).catch(() => {});
  }}
>
  {@render children()}
  {#snippet failed(error, reset)}
    <div class="card fail">
      <div class="label">{name} could not be shown</div>
      <p class="mono">{String(error).slice(0, 600)}</p>
      <button class="btn sm" onclick={reset}>Try again</button>
    </div>
  {/snippet}
</svelte:boundary>

<style>
  .fail { color: var(--text-2); }
  .fail p { white-space: pre-wrap; user-select: text; margin: 8px 0 10px; color: #ffb4ae; }
</style>
