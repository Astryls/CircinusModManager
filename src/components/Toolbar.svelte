<script lang="ts">
  import { SORTS, sortLabel as sortLabelOf, sortHint, store, type ShowOnly, type SortKey, type Tab } from "$lib/store.svelte";
  import { t } from "$lib/i18n.svelte";
  import { keyLabel } from "$lib/keys.svelte";
  import { I } from "$lib/icons";
  import { SOURCE_LABEL, type Source } from "$lib/types";

  // New is only there when there is something in it: a tab that always reads zero is a tab
  // nobody presses, and it costs the other three the width it takes.
  const tabs = $derived.by((): { id: Tab; label: string; count: number }[] => [
    { id: "active", label: t("toolbar.tab.active"), count: store.active.length },
    { id: "inactive", label: t("toolbar.tab.inactive"), count: store.inactive.length },
    { id: "all", label: t("toolbar.tab.all"), count: store.mods.length },
    ...(store.newUids.length ? [{ id: "new" as Tab, label: t("toolbar.tab.new"), count: store.newUids.length }] : [])
  ]);
  const previewCount = $derived(store.preview?.moves.length ?? 0);
  const counts = $derived(store.showOnlyCounts);
  const choices = $derived.by((): { id: ShowOnly; label: string; n?: number; icon?: string }[] => [
    { id: null, label: t("toolbar.show.all") },
    { id: "attention", label: t("toolbar.show.attention"), n: counts.attention, icon: I.warn },
    { id: "error", label: t("toolbar.show.error"), n: counts.error, icon: I.error },
    { id: "warning", label: t("toolbar.show.warning"), n: counts.warning, icon: I.warn },
    { id: "conflict", label: t("toolbar.show.conflict"), n: counts.conflict, icon: I.error },
    { id: "note", label: t("toolbar.show.note"), n: counts.note, icon: I.note },
    { id: "collision", label: t("toolbar.show.collision"), n: counts.collision, icon: I.image },
    ...(store.showWeight ? [{ id: "heavy" as ShowOnly, label: t("toolbar.show.heavy"), n: counts.heavy, icon: I.gauge }] : []),
    { id: "slow", label: t("toolbar.show.slow"), n: counts.slow, icon: I.halo },
    { id: "changed", label: t("toolbar.show.changed"), n: counts.changed, icon: I.change },
    ...(store.preview ? [{ id: "moved" as ShowOnly, label: t("toolbar.show.moved"), n: counts.moved, icon: I.halo }] : [])
  ]);
  const sources: Source[] = ["workshop", "local", "steamcmd", "git", "ludeon"];
  const columns = [
    { key: "time", label: t("toolbar.col.time"), hint: t("toolbar.col.time.title") },
    { key: "load", label: t("toolbar.col.load"), hint: t("toolbar.col.load.title") },
    { key: "versions", label: t("toolbar.col.versions"), hint: t("toolbar.col.versions.title") },
    { key: "phase", label: t("toolbar.col.phase"), hint: t("toolbar.col.phase.title") },
    { key: "group", label: t("toolbar.col.group"), hint: t("toolbar.col.group.title") }
  ];
  const version = $derived(store.snap?.gameVersion.majorMinor ?? "this version");
  let open = $state(false);
  let menu = $state<HTMLElement | null>(null);
  let sortOpen = $state(false);
  let sortMenu = $state<HTMLElement | null>(null);
  const sortLabel = $derived(sortLabelOf(store.sortKey));
  /** The same three states a column heading has: pick it, turn it round, then back to the load
   *  order. The store owns that cycle, so the menu just hands it the key. */
  function pickSort(k: SortKey) {
    if (k === "order") store.clearSort();
    else store.sortBy(k);
  }
  /** Filters other than "all": how many are narrowing the list right now. */
  const narrowing = $derived((store.showOnly ? 1 : 0) + (store.onlyCurrentVersion ? 1 : 0) + (store.sources.length < sources.length ? 1 : 0));
  const currentLabel = $derived(choices.find((c) => c.id === store.showOnly)?.label ?? t("toolbar.show.all"));
  function toggleSource(s: Source) {
    store.sources = store.sources.includes(s) ? store.sources.filter((x) => x !== s) : [...store.sources, s];
  }
  function clearAll() {
    store.showOnly = null;
    store.onlyCurrentVersion = false;
    store.sources = [...sources];
  }
  $effect(() => store.onEscape("toolbar-menus", 40, () => open || sortOpen, () => { open = false; sortOpen = false; }));
  function onWindowClick(e: MouseEvent) {
    if (open && menu && !menu.contains(e.target as Node)) open = false;
    if (sortOpen && sortMenu && !sortMenu.contains(e.target as Node)) sortOpen = false;
  }
</script>

<svelte:window onclick={onWindowClick} />

<div class="toolbar">
  {#if !store.splitMode}
    <!-- Side by side, the panes decide what is shown and these would decide nothing. -->
    <div class="seg" role="tablist">
      {#each tabs as tab}<button role="tab" class:on={store.tab === tab.id} aria-selected={store.tab === tab.id} onclick={() => (store.tab = tab.id)}>{tab.label} <span class="num">{tab.count}</span></button>{/each}
    </div>
  {/if}
  {#if !store.splitMode && store.tab === "new"}
    <button class="btn sm" onclick={() => store.markNewSeen()} title={t("toolbar.seenThem.title")}>{t("toolbar.seenThem")}</button>
  {/if}
  {#if !store.splitMode && store.sorted}
    <!-- A sorted list is not the load order, and the way back has to be visible from the list
         rather than remembered as "click the same heading twice more". -->
    <button class="btn sm sortoff" onclick={() => store.clearSort()} title={t("toolbar.sorted.title")}>{@html I.close}<span class="lbl">{t("toolbar.sorted")}</span></button>
  {/if}
  {#if store.splitMode === "library" || (!store.splitMode && store.tab !== "inactive")}
    <div class="seg arr" role="radiogroup" aria-label={t("toolbar.arrangement")}>
      <button role="radio" class:on={!store.byPhase} aria-checked={!store.byPhase} title={t("toolbar.arr.order.title")} onclick={() => store.byPhase && store.setByPhase(false)}><span class="lg">{t("toolbar.arr.order")}</span><span class="sm">{t("toolbar.arr.order.short")}</span></button>
      <button role="radio" class:on={store.byPhase} aria-checked={store.byPhase} title={t("toolbar.arr.phase.title")} onclick={() => !store.byPhase && store.setByPhase(true)}><span class="lg">{t("toolbar.arr.phase")}</span><span class="sm">{t("toolbar.arr.phase.short")}</span></button>
    </div>
  {/if}
  <button class="btn split" class:on={store.splitMode === "library"} aria-pressed={store.splitMode === "library"} onclick={() => (store.split = store.split === "library" ? null : "library")} title={t("toolbar.split.title")}>
    {@html I.split}<span class="split-lbl">{t("toolbar.split.label")}</span>
  </button>
  <div class="filter" bind:this={menu}>
    <button class="btn" class:on={narrowing > 0} onclick={() => (open = !open)} aria-haspopup="menu" aria-expanded={open} title={t("toolbar.show.title")}>
      {@html I.search}<span class="lbl">{t("toolbar.show.label", { what: currentLabel })}</span>{#if narrowing > 1}<span class="cnt">+{narrowing - 1}</span>{/if}
    </button>
    {#if open}
      <div class="menu card" role="menu">
        <div class="label">{t("toolbar.show.only")}</div>
        {#each choices as c}
          <button class="opt" class:on={store.showOnly === c.id} role="menuitemradio" aria-checked={store.showOnly === c.id} onclick={() => { store.showOnly = c.id; open = false; }}>
            <span class="ico">{#if c.icon}{@html c.icon}{/if}</span><span class="t">{c.label}</span>{#if c.n != null}<span class="n num">{c.n}</span>{/if}
          </button>
        {/each}
        <div class="label">{t("toolbar.columns")}</div>
        <div class="chips">
          <button class="chip" class:on={store.showWeight} onclick={() => store.updateSettings({ showWeight: !store.showWeight })} title={t("toolbar.col.cost.title")}>{t("toolbar.col.cost")}</button>
          {#each columns as c}<button class="chip" class:on={store.listColumns.includes(c.key)} onclick={() => store.setListColumn(c.key, !store.listColumns.includes(c.key))} title={c.hint}>{c.label}</button>{/each}
        </div>
        <div class="label">{t("toolbar.from")}</div>
        <div class="chips">
          {#each sources as s}<button class="chip" class:on={store.sources.includes(s)} onclick={() => toggleSource(s)}>{t(`source.${s}`)}</button>{/each}
        </div>
        <div class="label">{t("toolbar.version")}</div>
        <label class="switch sm"><input type="checkbox" bind:checked={store.onlyCurrentVersion} />{t("toolbar.onlyThisVersion", { version })}</label>
        {#if narrowing}<button class="btn sm clear" onclick={() => { clearAll(); open = false; }}>{t("toolbar.showEverything")}</button>{/if}
      </div>
    {/if}
  </div>
  <div class="filter sortm" bind:this={sortMenu}>
    <button class="btn" class:on={store.sorted} onclick={() => (sortOpen = !sortOpen)} aria-haspopup="menu" aria-expanded={sortOpen} title={t("toolbar.sort.title")}>
      {@html I.list}<span class="lbl">{t("toolbar.sort.label", { what: sortLabel })}</span>{#if store.sorted}<span class="cnt">{store.sortDir === 1 ? "↑" : "↓"}</span>{/if}
    </button>
    {#if sortOpen}
      <div class="menu card" role="menu">
        <div class="label">{t("toolbar.sort.by")}</div>
        {#each SORTS as k}
          <button class="opt" class:on={store.sortKey === k} role="menuitemradio" aria-checked={store.sortKey === k} title={sortHint(k)} onclick={() => { pickSort(k); sortOpen = false; }}>
            <span class="ico"></span><span class="t">{sortLabelOf(k)}</span>
            {#if store.sortKey === k && k !== "order"}<span class="n">{store.sortDir === 1 ? "↑" : "↓"}</span>{/if}
          </button>
        {/each}
        <p class="note">{t("toolbar.sort.dragNote")}</p>
      </div>
    {/if}
  </div>
  <span class="sp"></span>
  <button class="btn" onclick={() => (store.showImport = true)} title={t("toolbar.import.title", { keys: keyLabel("Mod+i") })}>{@html I.download}<span class="opt-lbl">{t("toolbar.import")}</span></button>
  <button class="btn" onclick={() => store.rescan(false)} title={t("toolbar.refresh.title")}>{@html I.refresh}<span class="opt-lbl">{t("toolbar.refresh")}</span></button>
  {#if store.preview}
    <span class="pair">
      <button class="btn split" class:on={store.splitMode === "halo"} aria-pressed={store.splitMode === "halo"} onclick={() => (store.split = store.split === "halo" ? null : "halo")} title={t("toolbar.whatChanges.title")}>
        {@html I.change}<span class="split-lbl">{t("toolbar.whatChanges")}</span>
      </button>
      <button class="btn strong" onclick={() => store.haloApply()} title={t("toolbar.apply.title", { n: previewCount })}>{@html I.check}{t("toolbar.apply")}<span class="cnt-lbl">{t("toolbar.apply.moves", { n: previewCount })}</span></button>
      <button class="btn" onclick={() => store.clearPreview()} title={t("toolbar.discard.title")} aria-label={t("toolbar.discard.aria")}>{@html I.close}<span class="opt-lbl">{t("toolbar.discard")}</span></button>
    </span>
  {:else}
    <button class="btn strong" onclick={() => store.haloPreview()} title={t("toolbar.sortWithHalo.title")}>{@html I.halo}{t("toolbar.sortWithHalo")}</button>
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
  .opt { display: flex; align-items: center; gap: 8px; height: 30px; padding: 0 8px; border-radius: 0; font-size: 13px; color: var(--text-2); text-align: left; width: 100%; }
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
