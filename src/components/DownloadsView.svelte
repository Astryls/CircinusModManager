<script lang="ts">
  import { store } from "$lib/store.svelte";
  const missing = $derived(store.snap?.missing ?? []);
</script>

<main class="center">
  <section class="card">
    <h3>Downloads</h3>
    <p class="lead">SteamCMD downloads with a smart throttle arrive in milestone 2: a persistent queue, adaptive batch sizes (25 and shrinking on failure), per-item timeouts, exponential backoff when Steam starts refusing, and retries with <span class="mono">validate</span>. Steam collections and Rentry lists will feed this queue.</p>
  </section>
  {#if missing.length}
    <section class="card">
      <h3>Listed in ModsConfig.xml but not installed <span class="aside">{missing.length}</span></h3>
      <div class="ids">{#each missing as id}<span class="mono">{id}</span>{/each}</div>
      <p class="lead">These will become the first entries of the download queue once the Steam Workshop database resolves them to workshop ids.</p>
    </section>
  {/if}
</main>

<style>
  .center { display: flex; flex-direction: column; gap: 12px; min-height: 0; min-width: 0; overflow: auto; }
  .lead { margin: 0; color: var(--text-2); font-size: 13.5px; line-height: 1.5; max-width: 70ch; }
  .ids { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 10px; }
  .ids .mono { background: var(--surface-2); padding: 2px 7px; border-radius: 6px; color: var(--text-2); }
</style>
