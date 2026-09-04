<script lang="ts">
  // Error boundary: a panel that throws shows the error instead of taking the app down.
  import type { Snippet } from "svelte";
  let { name, children }: { name: string; children: Snippet } = $props();
</script>

<svelte:boundary onerror={(e) => console.error(`[circinus] ${name} failed to render:`, e)}>
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
