<script lang="ts">
  import { store, type Tab } from "$lib/store.svelte";
  import { I } from "$lib/icons";

  const tabs: { id: Tab; label: string; count: () => number }[] = [
    { id: "active", label: "Active", count: () => store.active.length },
    { id: "inactive", label: "Inactive", count: () => store.inactive.length },
    { id: "all", label: "All", count: () => store.mods.length }
  ];
  const previewCount = $derived(store.preview?.moves.length ?? 0);
</script>

<div class="toolbar">
  <div class="seg" role="tablist">
    {#each tabs as t}<button role="tab" class:on={store.tab === t.id} aria-selected={store.tab === t.id} onclick={() => (store.tab = t.id)}>{t.label} <span class="num">{t.count()}</span></button>{/each}
  </div>
  <span class="sp"></span>
  <button class="btn" onclick={() => (store.showImport = true)} title="Import a mod list (Ctrl I)">{@html I.download}Import</button>
  <button class="btn" onclick={() => store.rescan(false)} title="Re-read mod folders">{@html I.refresh}Refresh</button>
  {#if store.preview}
    <button class="btn" onclick={() => (store.preview = null)}>Discard</button>
    <button class="btn primary" onclick={() => store.haloApply()}>{@html I.check}Apply {previewCount} move{previewCount === 1 ? "" : "s"}</button>
  {:else}
    <button class="btn primary" onclick={() => store.haloPreview()} title="Preview the Harmonized Automated Load Order">{@html I.halo}Sort with HALO</button>
  {/if}
</div>

<style>
  .toolbar { display: flex; align-items: center; gap: 10px; }
  .sp { flex: 1; }
  .toolbar .btn { height: 32px; }
</style>
