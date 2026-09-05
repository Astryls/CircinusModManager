<script lang="ts">
  import { store, type View } from "$lib/store.svelte";
  import { I } from "$lib/icons";
  import GroupEditor from "./GroupEditor.svelte";
  import type { Group } from "$lib/types";

  const snap = $derived(store.snap);
  const groupCounts = $derived(store.groupCounts);
  const nav: { id: View; label: string; icon: string; count?: () => string | number; att?: boolean }[] = [
    { id: "order", label: "Load order", icon: I.list, count: () => store.active.length },
    { id: "library", label: "Library", icon: I.library, count: () => store.mods.length },
    { id: "downloads", label: "Downloads", icon: I.cloud, count: () => store.queueCounts.queued || (store.downloads && !store.downloads.steamcmdInstalled && !store.downloads.installing ? "set up" : ""), att: true },
    { id: "textures", label: "Textures", icon: I.image, count: () => (store.tex?.running ? (store.tex.phase === "converting" && store.tex.progress.total ? `${Math.round((store.tex.progress.done / store.tex.progress.total) * 100)}%` : "busy") : store.ddsTotals.mods || ""), att: false },
    { id: "patches", label: "Patches", icon: I.halo, count: () => (store.patchJob?.running ? "busy" : store.patchReport ? store.patchReport.contested || "" : ""), att: true },
    { id: "analyzer", label: "Analyzer", icon: I.analyze, count: () => store.stats.errors + store.stats.warnings, att: true },
    { id: "halo", label: "HALO", icon: I.halo, count: () => Object.keys(store.halo.packagePhases).length + store.halo.namePhases.length + store.halo.off.length + Object.keys(store.halo.retarget).length || "" },
    { id: "settings", label: "Settings", icon: I.gear }
  ];
  let newGroup = $state(false);
  let newName = $state("");
  let editing = $state<string | null>(null);
  function addGroup() {
    if (!newName.trim()) return;
    editing = store.addGroup(newName);
    newName = "";
    newGroup = false;
  }
  function marker(g: Group): string {
    const parts: string[] = [];
    if (g.auto) parts.push(autoLabel(g));
    if (g.section && g.phase) parts.push(`Has its own section in the load order, after ${store.phaseInfo(g.phase).name.toLowerCase()}`);
    else if (g.phase) parts.push(`Hand-picked members are sorted as ${store.phaseInfo(g.phase).name.toLowerCase()}`);
    return parts.join(". ");
  }
  /** What a self-filling group takes in. */
  function autoLabel(g: Group): string {
    const r = g.auto;
    if (!r) return "";
    if (r.kind === "official") return "Fills itself with the game and its DLC";
    if (r.kind === "phase") return `Fills itself with HALO: ${store.phaseInfo(r.phase).name}`;
    return r.name.trim() ? `Fills itself with mods by ${r.name.trim()}` : "Fills itself with mods by an author (none named yet)";
  }
  const pct = $derived(store.mods.length ? Math.round((store.active.length / store.mods.length) * 100) : 0);

  // ---- the list switcher ----
  let listOpen = $state(false);
  let listMenu = $state<HTMLElement | null>(null);
  let listBtn = $state<HTMLElement | null>(null);
  /** The menu is fixed to the window (the rail scrolls and would clip it otherwise). */
  let menuAt = $state({ x: 0, y: 0 });
  function toggleListMenu() {
    if (!listOpen && listBtn) {
      const r = listBtn.getBoundingClientRect();
      menuAt = { x: Math.max(8, r.left), y: Math.min(r.bottom + 4, innerHeight - 420) };
    }
    listOpen = !listOpen;
  }
  let naming = $state<"new" | "rename" | null>(null);
  let newListName = $state("");
  let confirmLoad = $state<string | null>(null);
  let confirmDelete = $state(false);
  function loadList(name: string, sure: boolean) {
    if (name === store.currentList && !snap?.dirty) { listOpen = false; return; }
    if (snap?.dirty && !sure) { confirmLoad = name; return; }
    confirmLoad = null;
    listOpen = false;
    store.loadNamedList(name);
  }
  function saveAs() {
    const name = newListName.trim();
    if (!name) return;
    if (naming === "rename" && store.currentList) store.renameNamedList(store.currentList, name);
    else store.saveNamedList(name);
    naming = null;
    listOpen = false;
  }
  function onWindowClick(e: MouseEvent) {
    const t = e.target as Node | null;
    // A click inside the menu may replace the clicked element before this runs (an option
    // turning into a form): a detached target was inside.
    if (listOpen && listMenu && t?.isConnected && !listMenu.contains(t)) { listOpen = false; naming = null; confirmLoad = null; confirmDelete = false; }
  }

  // ---- collections ----
  let newCol = $state(false);
  let colText = $state("");
  async function follow() {
    if (!colText.trim()) return;
    await store.trackCollection(colText);
    colText = "";
    newCol = false;
  }
</script>

<svelte:window onclick={onWindowClick} onkeydown={(e) => e.key === "Escape" && (listOpen = false)} />

<aside class="rail">
  <section class="card inst">
    <div class="ver"><span class="dot" style="--c: {snap?.locations.gameDir ? 'var(--green)' : 'var(--red)'}"></span>RimWorld {snap?.gameVersion.majorMinor ?? "not found"}</div>
    <div class="sub" title={snap?.locations.gameDir ?? ""}>{snap?.locations.gameDir ? `${snap.gameVersion.full}${snap.locations.workshopDir ? " · Steam" : ""}` : "Set the game folder in Settings"}</div>
    <div class="row"><span class="num">{store.active.length} active</span><span class="num">{store.mods.length} installed</span></div>
    <div class="meter c-blue" style="--v:{pct}%"><i></i></div>
    <div class="row lists" bind:this={listMenu}>
      <button class="lbtn" bind:this={listBtn} title={store.currentList ? `Working on the list ${store.currentList}. Save writes it and ModsConfig.xml.` : "The active list is ModsConfig.xml. Save it under a name to keep several lists."} onclick={toggleListMenu} aria-haspopup="menu" aria-expanded={listOpen}>{@html I.list}<span class="n">{store.currentList ?? "ModsConfig.xml"}</span><span class="car">▾</span></button>
      <span class:att={snap?.dirty}>{snap?.dirty ? "not saved" : "saved"}</span>
      {#if listOpen}
        <div class="menu card" role="menu" style="left: {menuAt.x}px; top: {menuAt.y}px">
          <div class="label">Your lists</div>
          {#each store.namedLists as l (l.name)}
            {#if confirmLoad === l.name}
              <div class="ask">Load {l.name}? Unsaved changes to the current list are lost.<div><button class="btn sm primary" onclick={() => loadList(l.name, true)}>Load</button><button class="btn sm" onclick={() => (confirmLoad = null)}>Keep editing</button></div></div>
            {:else}
              <button class="opt" class:on={store.currentList === l.name} role="menuitem" onclick={() => loadList(l.name, false)}>
                <span class="t">{l.name}</span><span class="c num">{l.count}</span>
              </button>
            {/if}
          {/each}
          {#if !store.namedLists.length}<div class="hint">No lists yet. Save the active list under a name and it appears here; switch between lists any time.</div>{/if}
          <div class="label">This list</div>
          {#if naming}
            <form class="newg" onsubmit={(e) => { e.preventDefault(); saveAs(); }}>
              <input class="input" placeholder={naming === "rename" ? "New name" : "List name"} bind:value={newListName} />
              <button class="btn sm primary" type="submit">{naming === "rename" ? "Rename" : "Save"}</button>
            </form>
          {:else}
            {#if store.currentList}<button class="opt" role="menuitem" onclick={() => { store.save(); listOpen = false; }}><span class="t">Save {store.currentList} and ModsConfig.xml</span><span class="kbd">Ctrl S</span></button>{/if}
            <button class="opt" role="menuitem" onclick={() => { naming = "new"; newListName = ""; }}><span class="t">Save as a new list…</span></button>
            {#if store.currentList}
              <button class="opt" role="menuitem" onclick={() => { naming = "rename"; newListName = store.currentList ?? ""; }}><span class="t">Rename…</span></button>
              <button class="opt" role="menuitem" title="Keep working on ModsConfig.xml alone; the named list stays as it is" onclick={() => { store.detachList(); listOpen = false; }}><span class="t">Leave {store.currentList}</span></button>
              {#if confirmDelete}
                <div class="ask">Delete the list {store.currentList}? Your active mods stay as they are.<div><button class="btn sm danger" onclick={() => { store.deleteNamedList(store.currentList!); confirmDelete = false; listOpen = false; }}>Delete</button><button class="btn sm" onclick={() => (confirmDelete = false)}>Keep</button></div></div>
              {:else}
                <button class="opt danger" role="menuitem" onclick={() => (confirmDelete = true)}><span class="t">Delete {store.currentList}…</span></button>
              {/if}
            {/if}
            <button class="opt" role="menuitem" title="Every list Circinus read or wrote, newest first" onclick={() => { store.showImport = true; listOpen = false; }}><span class="t">Previous versions…</span></button>
          {/if}
        </div>
      {/if}
    </div>
  </section>

  <nav class="card nav" aria-label="Views">
    {#each nav as n}
      <button class:on={store.view === n.id || (n.id === "order" && store.view === "library")} onclick={() => { store.view = n.id; if (n.id === "library") store.tab = "all"; if (n.id === "order") store.tab = "active"; }}>
        {@html n.icon}{n.label}
        {#if n.count}<span class="count num" class:att={n.att && (Number(n.count()) > 0 || n.count() === "set up")}>{n.count()}</span>{/if}
      </button>
    {/each}
  </nav>

  <section class="card groups">
    <h3>Groups <button class="plus" aria-label="New group" onclick={() => (newGroup = !newGroup)}>+</button></h3>
    {#if newGroup}
      <form class="newg" onsubmit={(e) => { e.preventDefault(); addGroup(); }}>
        <input class="input" placeholder="Group name" bind:value={newName} />
        <button class="btn sm primary" type="submit">Add</button>
      </form>
    {/if}
    {#each snap?.user.groups ?? [] as g (g.id)}
      <div class="grow" class:on={store.group === g.id} class:editing={editing === g.id}>
        <button class="g" onclick={() => (store.group = store.group === g.id ? null : g.id)} title={marker(g) || "Show only this group"}>
          <span class="dot c-{g.color}"></span><span class="n">{g.name}</span>
          {#if g.auto}<span class="ov auto" title={autoLabel(g)}>{@html I.halo}</span>{/if}
          {#if g.phase}<span class="ov" title={marker(g)}>{@html I.over}</span>{/if}
          <span class="c num">{groupCounts.get(g.id) ?? 0}</span>
        </button>
        <button class="edit" aria-label="Edit group {g.name}" title="Rename, colour, where it sorts, delete" onclick={() => (editing = editing === g.id ? null : g.id)}>{@html I.gear}</button>
      </div>
      {#if editing === g.id}<GroupEditor group={g} onclose={() => (editing = null)} />{/if}
    {/each}
    {#if store.group}<button class="clear" onclick={() => (store.group = null)}>Show all groups</button>{/if}
    <p class="ghint">Click a group to show only its mods. The gear sets what a group takes in by itself and where its members sort: a phase, or a section of their own.</p>
  </section>

  <section class="card cols">
    <h3>Collections <button class="plus" aria-label="Follow a collection" title="Follow a Steam collection: paste its link" onclick={() => (newCol = !newCol)}>+</button></h3>
    {#if newCol}
      <form class="newg" onsubmit={(e) => { e.preventDefault(); follow(); }}>
        <input class="input" placeholder="Collection link or id" bind:value={colText} />
        <button class="btn sm primary" type="submit">Follow</button>
      </form>
    {/if}
    {#each store.collections as c (c.id)}
      {@const v = store.collectionView(c)}
      {@const changed = v.added.length + v.removed.length}
      <button class="col" onclick={() => (store.showCollection = c.id)} title="{c.name} by {c.creator}: {c.items.length} mods, {v.installed.length} installed, {v.missing.length} missing{changed ? `, ${v.added.length} added and ${v.removed.length} removed since you last looked` : ""}">
        <span class="dot" style="--c: var(--{v.missing.length ? 'amber' : 'green'})"></span>
        <span class="n">{c.name}</span>
        {#if changed}<span class="chg num" title="Changed since you last looked">{v.added.length ? `+${v.added.length}` : ""}{v.removed.length ? ` −${v.removed.length}` : ""}</span>{/if}
        <span class="c num">{v.installed.length}/{c.items.length}</span>
      </button>
    {/each}
    {#if !store.collections.length && !newCol}
      <p class="ghint">Follow a Steam collection to see what of it you have, download the rest, and hear when the curator adds or removes mods.</p>
    {:else if store.collections.length}
      <button class="clear" onclick={() => store.refreshCollections()}>Check all for changes</button>
    {/if}
  </section>

</aside>

<style>
  .rail { display: flex; flex-direction: column; gap: 10px; min-height: 0; overflow: hidden auto; }
  .rail .card { padding: 12px; flex: none; }
  .inst .ver { font-size: 18px; font-weight: 800; letter-spacing: -0.02em; line-height: 1.1; display: flex; align-items: center; gap: 8px; }
  .inst .ver .dot { width: 8px; height: 8px; box-shadow: 0 0 0 3px color-mix(in srgb, var(--c) 22%, transparent); }
  .inst .sub { color: var(--text-2); font-size: 12px; margin-top: 3px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .inst .row { display: flex; justify-content: space-between; margin-top: 9px; font-size: 12px; color: var(--text-3); }
  .inst .row .att { color: var(--amber); font-weight: 700; }
  .inst .lists { position: relative; align-items: center; }
  .lbtn { display: flex; align-items: center; gap: 6px; min-width: 0; color: var(--text-2); font-weight: 600; font-size: 12px; padding: 3px 6px; margin-left: -6px; border-radius: 7px; }
  .lbtn:hover { background: var(--surface-2); color: var(--text); }
  .lbtn :global(svg) { width: 14px; height: 14px; flex: none; }
  .lbtn .n { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 130px; }
  .lbtn .car { color: var(--text-3); font-size: 10px; }
  .menu { position: fixed; z-index: 30; width: 280px; max-height: 70vh; overflow: hidden auto; padding: 8px; box-shadow: var(--shadow-float); display: flex; flex-direction: column; gap: 2px; font-size: 13px; }
  .menu .label { margin: 6px 0 4px 6px; }
  .menu .label:first-child { margin-top: 0; }
  .opt { display: flex; align-items: center; gap: 8px; height: 30px; padding: 0 8px; border-radius: 8px; color: var(--text-2); text-align: left; width: 100%; font-size: 13px; }
  .opt:hover { background: var(--surface-2); color: var(--text); }
  .opt.on { background: var(--surface-3); color: var(--text); font-weight: 600; }
  .opt.danger { color: var(--red); }
  .opt .t { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .opt .c { color: var(--text-3); font-size: 12px; }
  .opt .kbd { color: var(--text-3); font-size: 11px; font-family: var(--mono); }
  .menu .hint, .ask { color: var(--text-3); font-size: 12px; line-height: 1.4; padding: 4px 6px; }
  .ask { color: var(--text-2); background: var(--surface-2); border-radius: 8px; padding: 8px; }
  .ask div { display: flex; gap: 6px; margin-top: 6px; }
  .menu .newg { margin: 4px 0; }
  .cols .col { display: flex; align-items: center; gap: 9px; height: 30px; padding: 0 8px; margin: 0 -6px; border-radius: 8px; font-weight: 600; font-size: 13px; width: calc(100% + 12px); text-align: left; }
  .cols .col:hover { background: var(--surface-2); }
  .cols .col .n { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .cols .col .c { color: var(--text-3); font-size: 12px; }
  .cols .col .chg { color: var(--amber); font-size: 11.5px; font-weight: 700; }
  .cols h3 { margin-bottom: 4px; }
  .inst .meter { margin-top: 7px; }
  .nav { padding: 6px !important; display: flex; flex-direction: column; }
  .nav button { display: flex; align-items: center; gap: 10px; height: 32px; padding: 0 10px; border-radius: 9px; color: var(--text-2); font-weight: 600; font-size: 13.5px; text-align: left; }
  .nav button :global(svg) { width: 18px; height: 18px; flex: none; }
  .nav button:hover { background: var(--surface-2); color: var(--text); }
  .nav button.on { background: var(--surface-3); color: var(--text); }
  .nav .count { margin-left: auto; font-size: 11.5px; font-weight: 700; color: var(--text-3); }
  .nav .count.att { color: var(--amber); }
  .plus { color: var(--text-3); font-size: 16px; line-height: 1; font-weight: 500; }
  .plus:hover { color: var(--text); }
  .newg { display: flex; gap: 6px; margin-bottom: 8px; }
  .newg .input { height: 28px; font-size: 12.5px; }
  .groups h3 { margin-bottom: 4px; }
  .grow { display: flex; align-items: center; margin: 0 -6px; border-radius: 8px; }
  .grow:hover, .grow.editing { background: var(--surface-2); }
  .grow.on { background: var(--surface-3); }
  .groups .g { display: flex; align-items: center; gap: 9px; height: 28px; padding: 0 8px; border-radius: 8px; font-weight: 600; font-size: 13px; flex: 1; min-width: 0; text-align: left; }
  .grow .edit { width: 24px; height: 24px; margin-right: 2px; border-radius: 6px; display: grid; place-items: center; opacity: 0; color: var(--text-3); flex: none; }
  .grow .edit :global(svg) { width: 14px; height: 14px; }
  .grow:hover .edit, .grow.editing .edit, .grow .edit:focus-visible { opacity: 1; }
  .grow .edit:hover { background: var(--surface-3); color: var(--text); }
  .ghint { color: var(--text-3); font-size: 11.5px; line-height: 1.4; margin: 8px 0 0; }
  .groups .g .n { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .groups .g .c { color: var(--text-3); font-size: 12px; }
  .groups .g .ov { color: var(--text-3); display: grid; }
  .groups .g .ov :global(svg) { width: 13px; height: 13px; }
  .groups .g .ov.auto :global(svg) { width: 12px; height: 12px; opacity: 0.8; }
  .clear { margin-top: 6px; font-size: 12px; color: var(--text-3); font-weight: 600; }
  .clear:hover { color: var(--text); }
</style>
