<script lang="ts">
  import { store, type ShowOnly, type Tab } from "$lib/store.svelte";
  import { I } from "$lib/icons";
  import { SOURCE_LABEL, type Source } from "$lib/types";

  const tabs: { id: Tab; label: string; count: () => number }[] = [
    { id: "active", label: "Active", count: () => store.active.length },
    { id: "inactive", label: "Inactive", count: () => store.inactive.length },
    { id: "all", label: "All", count: () => store.mods.length }
  ];
  const previewCount = $derived(store.preview?.moves.length ?? 0);
  const counts = $derived(store.showOnlyCounts);
  const choices = $derived.by((): { id: ShowOnly; label: string; n?: number; icon?: string }[] => [
    { id: null, label: "All mods" },
    { id: "attention", label: "Needs attention", n: counts.attention, icon: I.warn },
    { id: "error", label: "With errors", n: counts.error, icon: I.error },
    { id: "warning", label: "With warnings", n: counts.warning, icon: I.warn },
    { id: "conflict", label: "With conflicts", n: counts.conflict, icon: I.error },
    { id: "note", label: "With HALO notes", n: counts.note, icon: I.note },
    { id: "changed", label: "Changed since last launch", n: counts.changed, icon: I.change },
    ...(store.preview ? [{ id: "moved" as ShowOnly, label: "HALO would move", n: counts.moved, icon: I.halo }] : [])
  ]);
  const sources: Source[] = ["workshop", "local", "steamcmd", "git", "ludeon"];
  const version = $derived(store.snap?.gameVersion.majorMinor ?? "this version");
  let open = $state(false);
  let menu = $state<HTMLElement | null>(null);
  /** Filters other than "all": how many are narrowing the list right now. */
  const narrowing = $derived((store.showOnly ? 1 : 0) + (store.onlyCurrentVersion ? 1 : 0) + (store.sources.length < sources.length ? 1 : 0));
  const currentLabel = $derived(choices.find((c) => c.id === store.showOnly)?.label ?? "All mods");
  function toggleSource(s: Source) {
    store.sources = store.sources.includes(s) ? store.sources.filter((x) => x !== s) : [...store.sources, s];
  }
  function clearAll() {
    store.showOnly = null;
    store.onlyCurrentVersion = false;
    store.sources = [...sources];
  }
  function onWindowClick(e: MouseEvent) {
    if (open && menu && !menu.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window onclick={onWindowClick} onkeydown={(e) => e.key === "Escape" && (open = false)} />

<div class="toolbar">
  <div class="seg" role="tablist">
    {#each tabs as t}<button role="tab" class:on={store.tab === t.id} aria-selected={store.tab === t.id} onclick={() => (store.tab = t.id)}>{t.label} <span class="num">{t.count()}</span></button>{/each}
  </div>
  <div class="filter" bind:this={menu}>
    <button class="btn" class:on={narrowing > 0} onclick={() => (open = !open)} aria-haspopup="menu" aria-expanded={open} title="Narrow the list to mods with errors, warnings, conflicts, HALO notes or changes">
      {@html I.search}Show: {currentLabel}{#if narrowing > 1}<span class="cnt">+{narrowing - 1}</span>{/if}
    </button>
    {#if open}
      <div class="menu card" role="menu">
        <div class="label">Show only</div>
        {#each choices as c}
          <button class="opt" class:on={store.showOnly === c.id} role="menuitemradio" aria-checked={store.showOnly === c.id} onclick={() => { store.showOnly = c.id; open = false; }}>
            <span class="ico">{#if c.icon}{@html c.icon}{/if}</span><span class="t">{c.label}</span>{#if c.n != null}<span class="n num">{c.n}</span>{/if}
          </button>
        {/each}
        <div class="label">From</div>
        <div class="chips">
          {#each sources as s}<button class="chip" class:on={store.sources.includes(s)} onclick={() => toggleSource(s)}>{SOURCE_LABEL[s]}</button>{/each}
        </div>
        <div class="label">Version</div>
        <label class="switch sm"><input type="checkbox" bind:checked={store.onlyCurrentVersion} />Only mods made for {version}</label>
        {#if narrowing}<button class="btn sm clear" onclick={() => { clearAll(); open = false; }}>Show everything</button>{/if}
      </div>
    {/if}
  </div>
  <span class="sp"></span>
  <button class="btn" onclick={() => (store.showImport = true)} title="Import a mod list (Ctrl I)">{@html I.download}Import</button>
  <button class="btn" onclick={() => store.rescan(false)} title="Read the mod folders again">{@html I.refresh}Refresh</button>
  {#if store.preview}
    <button class="btn" onclick={() => (store.preview = null)}>Discard</button>
    <button class="btn primary" onclick={() => store.haloApply()}>{@html I.check}Apply {previewCount} move{previewCount === 1 ? "" : "s"}</button>
  {:else}
    <button class="btn primary" onclick={() => store.haloPreview()} title="Preview the load order HALO would use, then apply it or discard it">{@html I.halo}Sort with HALO</button>
  {/if}
</div>

<style>
  .toolbar { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
  .sp { flex: 1; }
  .toolbar .btn { height: 32px; }
  .filter { position: relative; }
  .filter > .btn.on { background: var(--amber-soft); color: var(--amber); }
  .filter .cnt { font-size: 11px; font-weight: 700; opacity: 0.8; }
  .menu { position: absolute; top: 38px; left: 0; z-index: 20; width: 300px; padding: 10px; box-shadow: var(--shadow-float); display: flex; flex-direction: column; gap: 2px; }
  .menu .label { margin: 8px 0 4px 6px; }
  .menu .label:first-child { margin-top: 0; }
  .opt { display: flex; align-items: center; gap: 8px; height: 30px; padding: 0 8px; border-radius: 8px; font-size: 13px; color: var(--text-2); text-align: left; width: 100%; }
  .opt:hover { background: var(--surface-2); color: var(--text); }
  .opt.on { background: var(--surface-3); color: var(--text); }
  .opt .ico { width: 16px; height: 16px; display: grid; place-items: center; flex: none; }
  .opt .ico :global(svg) { width: 15px; height: 15px; }
  .opt .t { flex: 1; }
  .opt .n { color: var(--text-3); font-size: 12px; }
  .menu .chips { padding: 0 4px; }
  .menu .chip { height: 24px; font-size: 11.5px; padding: 0 8px; }
  .menu .switch.sm { font-size: 12.5px; padding: 2px 6px; }
  .menu .clear { margin: 8px 4px 0; }
</style>
