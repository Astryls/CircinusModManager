<script lang="ts">
  import { SORTS, store, type ShowOnly, type SortKey, type Tab } from "$lib/store.svelte";
  import { I } from "$lib/icons";
  import { SOURCE_LABEL, type Source } from "$lib/types";

  // New is only there when there is something in it: a tab that always reads zero is a tab
  // nobody presses, and it costs the other three the width it takes.
  const tabs = $derived.by((): { id: Tab; label: string; count: number }[] => [
    { id: "active", label: "Active", count: store.active.length },
    { id: "inactive", label: "Inactive", count: store.inactive.length },
    { id: "all", label: "All", count: store.mods.length },
    ...(store.newUids.length ? [{ id: "new" as Tab, label: "New", count: store.newUids.length }] : [])
  ]);
  const previewCount = $derived(store.preview?.moves.length ?? 0);
  const counts = $derived(store.showOnlyCounts);
  const choices = $derived.by((): { id: ShowOnly; label: string; n?: number; icon?: string }[] => [
    { id: null, label: "All mods" },
    { id: "attention", label: "Needs attention", n: counts.attention, icon: I.warn },
    { id: "error", label: "With errors", n: counts.error, icon: I.error },
    { id: "warning", label: "With warnings", n: counts.warning, icon: I.warn },
    { id: "conflict", label: "With conflicts", n: counts.conflict, icon: I.error },
    { id: "note", label: "With HALO notes", n: counts.note, icon: I.note },
    { id: "collision", label: "Replacing the same textures", n: counts.collision, icon: I.image },
    ...(store.showWeight ? [{ id: "heavy" as ShowOnly, label: "Heavy on frame time", n: counts.heavy, icon: I.gauge }] : []),
    { id: "slow", label: "Slow to load", n: counts.slow, icon: I.halo },
    { id: "changed", label: "Changed since last launch", n: counts.changed, icon: I.change },
    ...(store.preview ? [{ id: "moved" as ShowOnly, label: "HALO would move", n: counts.moved, icon: I.halo }] : [])
  ]);
  const sources: Source[] = ["workshop", "local", "steamcmd", "git", "ludeon"];
  const columns = [
    { key: "time", label: "Time", hint: "Seconds this mod is expected to add to the game's loading time" },
    { key: "load", label: "Load", hint: "Expected share of the list's loading time" },
    { key: "versions", label: "Versions", hint: "Game versions the mod says it supports" },
    { key: "phase", label: "Phase", hint: "Where HALO files the mod — already the sections when the list is arranged by phase" },
    { key: "group", label: "Group", hint: "The group the mod is in" }
  ];
  const version = $derived(store.snap?.gameVersion.majorMinor ?? "this version");
  let open = $state(false);
  let menu = $state<HTMLElement | null>(null);
  let sortOpen = $state(false);
  let sortMenu = $state<HTMLElement | null>(null);
  const sortLabel = $derived(SORTS.find((s) => s.key === store.sortKey)?.label ?? "Load order");
  /** The same three states a column heading has: pick it, turn it round, then back to the load
   *  order. The store owns that cycle, so the menu just hands it the key. */
  function pickSort(k: SortKey) {
    if (k === "order") store.clearSort();
    else store.sortBy(k);
  }
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
    if (sortOpen && sortMenu && !sortMenu.contains(e.target as Node)) sortOpen = false;
  }
</script>

<svelte:window onclick={onWindowClick} onkeydown={(e) => e.key === "Escape" && ((open = false), (sortOpen = false))} />

<div class="toolbar">
  {#if !store.splitMode}
    <!-- Side by side, the panes decide what is shown and these would decide nothing. -->
    <div class="seg" role="tablist">
      {#each tabs as t}<button role="tab" class:on={store.tab === t.id} aria-selected={store.tab === t.id} onclick={() => (store.tab = t.id)}>{t.label} <span class="num">{t.count}</span></button>{/each}
    </div>
  {/if}
  {#if !store.splitMode && store.tab === "new"}
    <button class="btn sm" onclick={() => store.markNewSeen()} title="Stop marking these as new. The dates they arrived stay.">Seen them</button>
  {/if}
  {#if !store.splitMode && store.sorted}
    <!-- A sorted list is not the load order, and the way back has to be visible from the list
         rather than remembered as "click the same heading twice more". -->
    <button class="btn sm sortoff" onclick={() => store.clearSort()} title="Back to the load order">{@html I.close}<span class="lbl">Sorted</span></button>
  {/if}
  {#if store.splitMode === "library" || (!store.splitMode && store.tab !== "inactive")}
    <div class="seg arr" role="radiogroup" aria-label="Arrangement">
      <button role="radio" class:on={!store.byPhase} aria-checked={!store.byPhase} title="The list exactly as ModsConfig.xml has it, top to bottom" onclick={() => store.byPhase && store.setByPhase(false)}><span class="lg">Load order</span><span class="sm">Order</span></button>
      <button role="radio" class:on={store.byPhase} aria-checked={store.byPhase} title="The same mods, gathered under the phase HALO files them in" onclick={() => !store.byPhase && store.setByPhase(true)}><span class="lg">By phase</span><span class="sm">Phase</span></button>
    </div>
  {/if}
  <button class="btn split" class:on={store.splitMode === "library"} aria-pressed={store.splitMode === "library"} onclick={() => (store.split = store.split === "library" ? null : "library")} title="Show inactive and active mods in two panels, and drag between them">
    {@html I.split}<span class="split-lbl">Inactive | Active</span>
  </button>
  <div class="filter" bind:this={menu}>
    <button class="btn" class:on={narrowing > 0} onclick={() => (open = !open)} aria-haspopup="menu" aria-expanded={open} title="Narrow the list to mods with errors, warnings, conflicts, HALO notes or changes">
      {@html I.search}<span class="lbl">Show: {currentLabel}</span>{#if narrowing > 1}<span class="cnt">+{narrowing - 1}</span>{/if}
    </button>
    {#if open}
      <div class="menu card" role="menu">
        <div class="label">Show only</div>
        {#each choices as c}
          <button class="opt" class:on={store.showOnly === c.id} role="menuitemradio" aria-checked={store.showOnly === c.id} onclick={() => { store.showOnly = c.id; open = false; }}>
            <span class="ico">{#if c.icon}{@html c.icon}{/if}</span><span class="t">{c.label}</span>{#if c.n != null}<span class="n num">{c.n}</span>{/if}
          </button>
        {/each}
        <div class="label">Columns</div>
        <div class="chips">
          <button class="chip" class:on={store.showWeight} onclick={() => store.updateSettings({ showWeight: !store.showWeight })} title="Share of frame time, once weights are loaded">Cost</button>
          {#each columns as c}<button class="chip" class:on={store.listColumns.includes(c.key)} onclick={() => store.setListColumn(c.key, !store.listColumns.includes(c.key))} title={c.hint}>{c.label}</button>{/each}
        </div>
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
  <div class="filter sortm" bind:this={sortMenu}>
    <button class="btn" class:on={store.sorted} onclick={() => (sortOpen = !sortOpen)} aria-haspopup="menu" aria-expanded={sortOpen} title="Order the list by something other than the load order">
      {@html I.list}<span class="lbl">Sort: {sortLabel}</span>{#if store.sorted}<span class="cnt">{store.sortDir === 1 ? "↑" : "↓"}</span>{/if}
    </button>
    {#if sortOpen}
      <div class="menu card" role="menu">
        <div class="label">Sort by</div>
        {#each SORTS as s}
          <button class="opt" class:on={store.sortKey === s.key} role="menuitemradio" aria-checked={store.sortKey === s.key} title={s.hint} onclick={() => { pickSort(s.key); sortOpen = false; }}>
            <span class="ico"></span><span class="t">{s.label}</span>
            {#if store.sortKey === s.key && s.key !== "order"}<span class="n">{store.sortDir === 1 ? "↑" : "↓"}</span>{/if}
          </button>
        {/each}
        <p class="note">Dragging is refused while the list is sorted: a drop between two rows of a list ordered by name writes a position nobody chose.</p>
      </div>
    {/if}
  </div>
  <span class="sp"></span>
  <button class="btn" onclick={() => (store.showImport = true)} title="Import a mod list (Ctrl I)">{@html I.download}<span class="opt-lbl">Import</span></button>
  <button class="btn" onclick={() => store.rescan(false)} title="Read the mod folders again">{@html I.refresh}<span class="opt-lbl">Refresh</span></button>
  {#if store.preview}
    <span class="pair">
      <button class="btn split" class:on={store.splitMode === "halo"} aria-pressed={store.splitMode === "halo"} onclick={() => (store.split = store.split === "halo" ? null : "halo")} title="Show which mods HALO would move, from where to where">
        {@html I.change}<span class="split-lbl">What changes</span>
      </button>
      <button class="btn primary" onclick={() => store.haloApply()} title="Apply the {previewCount} move{previewCount === 1 ? '' : 's'} HALO proposes">{@html I.check}Apply<span class="cnt-lbl"> {previewCount} move{previewCount === 1 ? "" : "s"}</span></button>
      <button class="btn" onclick={() => store.clearPreview()} title="Discard the preview; nothing moves" aria-label="Discard the preview">{@html I.close}<span class="opt-lbl">Discard</span></button>
    </span>
  {:else}
    <button class="btn primary" onclick={() => store.haloPreview()} title="Preview the load order HALO would use, then apply it or discard it">{@html I.halo}Sort with HALO</button>
  {/if}
</div>

<style>
  /* One line, whatever the width: labels give way before anything wraps. */
  .toolbar { display: flex; align-items: center; gap: 8px; flex-wrap: nowrap; container-type: inline-size; min-width: 0; }
  .sp { flex: 1; min-width: 0; }
  .toolbar .btn { height: 32px; flex: none; }
  .toolbar .seg { flex: none; }
  /* The filter is the one control that may shrink, but only to where its label still reads: past
     that the toolbar drops the words below in order of what they are worth, rather than quietly
     squeezing "Show: All mods" down to a lone icon while Import and Refresh keep theirs. */
  .toolbar .filter { flex: 0 1 auto; min-width: 172px; }
  .filter > .btn { max-width: 100%; }
  .seg.arr button { font-size: 12px; padding: 0 10px; }
  .filter > .btn .lbl { max-width: 160px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pair { display: inline-flex; gap: 6px; flex: none; }
  .btn.split.on { background: var(--amber-soft); color: var(--amber); }
  .seg.arr .sm { display: none; }
  /* Narrower toolbars give words up in order of what they are worth, and each step is set where
     the step above it stops fitting rather than at a round number — a toolbar that overflows puts
     Discard under the panel beside it, which is how this was found. Icons and tooltips remain. */
  @container (max-width: 1420px) { .opt-lbl { display: none; } }
  @container (max-width: 1300px) { .seg .num { display: none; } .filter > .btn .lbl { max-width: 90px; } }
  @container (max-width: 1200px) { .btn.split .split-lbl { display: none; } }
  @container (max-width: 980px) { .seg.arr button { padding: 0 8px; } .filter > .btn .lbl { display: none; } .toolbar .filter { min-width: 0; } }
  @container (max-width: 800px) { .seg.arr .lg { display: none; } .seg.arr .sm { display: inline; } .cnt-lbl { display: none; } .toolbar .btn { padding: 0 9px; } }
  .filter { position: relative; }
  .sortm { min-width: 0; }
  .sortm .menu { max-height: 60vh; overflow-y: auto; }
  .sortm .note { margin: 6px 8px 2px; font-size: 11.5px; line-height: 1.45; color: var(--text-3); max-width: 30ch; }
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
  .btn.sortoff { color: var(--amber); }
  .btn.sortoff :global(svg) { width: 13px; height: 13px; }
</style>
