<script lang="ts">
  import { store, type View } from "$lib/store.svelte";
  import { I } from "$lib/icons";
  import type { Source } from "$lib/types";
  import { SOURCE_LABEL } from "$lib/types";

  const snap = $derived(store.snap);
  const groupCounts = $derived.by(() => {
    const c = new Map<string, number>();
    for (const [uid, g] of Object.entries(snap?.user.modGroups ?? {})) if (store.byUid.has(uid)) c.set(g, (c.get(g) ?? 0) + 1);
    return c;
  });
  const nav: { id: View; label: string; icon: string; count?: () => string | number; att?: boolean }[] = [
    { id: "order", label: "Load order", icon: I.list, count: () => store.active.length },
    { id: "library", label: "Library", icon: I.library, count: () => store.mods.length },
    { id: "downloads", label: "Downloads", icon: I.download, count: () => store.queueCounts.queued || "", att: true },
    { id: "analyzer", label: "Analyzer", icon: I.layers, count: () => store.stats.errors + store.stats.warnings, att: true },
    { id: "settings", label: "Settings", icon: I.sliders }
  ];
  const sources: Source[] = ["workshop", "local", "steamcmd", "git", "ludeon"];
  function toggleSource(s: Source) {
    store.sources = store.sources.includes(s) ? store.sources.filter((x) => x !== s) : [...store.sources, s];
  }
  let newGroup = $state(false);
  let newName = $state("");
  const palette = ["blue", "teal", "green", "pink", "amber", "coral", "violet"];
  function addGroup() {
    const name = newName.trim();
    if (!name) return;
    const id = name.toLowerCase().replace(/[^a-z0-9]+/g, "-") + "-" + Math.random().toString(36).slice(2, 6);
    store.updateUser((u) => ({ ...u, groups: [...u.groups, { id, name, color: palette[u.groups.length % palette.length] }] }));
    newName = "";
    newGroup = false;
  }
  const pct = $derived(store.mods.length ? Math.round((store.active.length / store.mods.length) * 100) : 0);
</script>

<aside class="rail">
  <section class="card inst">
    <div class="ver">RimWorld {snap?.gameVersion.majorMinor ?? "—"}</div>
    <div class="sub">{snap?.gameVersion.full ?? "not found"}{snap?.locations.workshopDir ? " · Steam" : ""}</div>
    <div class="row"><span>{store.mods.length} mods installed</span><span class="num">{store.active.length} active</span></div>
    <div class="meter c-blue" style="--v:{pct}%"><i></i></div>
    <div class="row"><span>ModsConfig.xml</span><span>{snap?.dirty ? "unsaved changes" : "saved"}</span></div>
  </section>

  <nav class="card nav" aria-label="Views">
    {#each nav as n}
      <button class:on={store.view === n.id || (n.id === "order" && store.view === "library")} onclick={() => { store.view = n.id; if (n.id === "library") store.tab = "all"; if (n.id === "order") store.tab = "active"; }}>
        {@html n.icon}{n.label}
        {#if n.count}<span class="count num" class:att={n.att && Number(n.count()) > 0}>{n.count()}</span>{/if}
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
      <button class="g" class:on={store.group === g.id} onclick={() => (store.group = store.group === g.id ? null : g.id)}>
        <span class="dot c-{g.color}"></span><span class="n">{g.name}</span>
        {#if g.phase}<span class="ov" title="Members are placed in the {store.phaseInfo(g.phase).name} phase">{@html I.over}</span>{/if}
        <span class="c num">{groupCounts.get(g.id) ?? 0}</span>
      </button>
    {/each}
    {#if store.group}<button class="clear" onclick={() => (store.group = null)}>Show all groups</button>{/if}
  </section>

  <section class="card filters">
    <div class="label">Source</div>
    <div class="chips">
      {#each sources as s}<button class="chip" class:on={store.sources.includes(s)} onclick={() => toggleSource(s)}>{SOURCE_LABEL[s]}</button>{/each}
    </div>
    <div class="label">Version</div>
    <div class="chips">
      <button class="chip" class:on={store.onlyCurrentVersion} onclick={() => (store.onlyCurrentVersion = !store.onlyCurrentVersion)}>Only {snap?.gameVersion.majorMinor ?? "current"}</button>
    </div>
    <div class="label">Show only</div>
    <div class="chips">
      <button class="chip" class:on={store.showOnly === "warning"} onclick={() => (store.showOnly = store.showOnly === "warning" ? null : "warning")}>▲ Warnings</button>
      <button class="chip err" class:on={store.showOnly === "error"} onclick={() => (store.showOnly = store.showOnly === "error" ? null : "error")}>✕ Errors</button>
      <button class="chip note" class:on={store.showOnly === "note"} onclick={() => (store.showOnly = store.showOnly === "note" ? null : "note")}>◆ HALO notes</button>
    </div>
  </section>
</aside>

<style>
  .rail { display: flex; flex-direction: column; gap: 12px; min-height: 0; overflow: auto; padding-bottom: 4px; }
  .inst .ver { font-size: 20px; font-weight: 800; letter-spacing: -0.02em; line-height: 1.1; }
  .inst .sub { color: var(--text-2); font-size: 12.5px; margin-top: 3px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .inst .row { display: flex; justify-content: space-between; margin-top: 12px; font-size: 12px; color: var(--text-3); }
  .inst .meter { margin-top: 8px; }
  .nav { padding: 8px; display: flex; flex-direction: column; }
  .nav button { display: flex; align-items: center; gap: 10px; height: 34px; padding: 0 10px; border-radius: 9px; color: var(--text-2); font-weight: 600; font-size: 13.5px; text-align: left; }
  .nav button :global(svg) { width: 16px; height: 16px; flex: none; }
  .nav button:hover { background: var(--surface-2); color: var(--text); }
  .nav button.on { background: var(--surface-3); color: var(--text); }
  .nav .count { margin-left: auto; font-size: 11.5px; font-weight: 700; color: var(--text-3); }
  .nav .count.att { color: var(--amber); }
  .plus { color: var(--text-3); font-size: 16px; line-height: 1; font-weight: 500; }
  .plus:hover { color: var(--text); }
  .newg { display: flex; gap: 6px; margin-bottom: 8px; }
  .newg .input { height: 28px; font-size: 12.5px; }
  .groups .g { display: flex; align-items: center; gap: 9px; height: 32px; padding: 0 8px; margin: 0 -6px; border-radius: 8px; font-weight: 600; font-size: 13px; width: calc(100% + 12px); text-align: left; }
  .groups .g:hover { background: var(--surface-2); }
  .groups .g.on { background: var(--surface-3); }
  .groups .g .n { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .groups .g .c { color: var(--text-3); font-size: 12px; }
  .groups .g .ov { color: var(--text-3); display: grid; }
  .groups .g .ov :global(svg) { width: 13px; height: 13px; }
  .clear { margin-top: 6px; font-size: 12px; color: var(--text-3); font-weight: 600; }
  .clear:hover { color: var(--text); }
  .filters .label { margin: 12px 0 8px; }
  .filters .label:first-child { margin-top: 0; }
</style>
