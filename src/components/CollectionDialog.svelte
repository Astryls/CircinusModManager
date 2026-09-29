<script lang="ts">
  // One followed Steam collection: what it holds, what is installed, what changed since the
  // user last looked, and what to do about it.
  import { store } from "$lib/store.svelte";
  import { I } from "$lib/icons";
  import { openUrl } from "$lib/api";

  const c = $derived(store.collections.find((x) => x.id === store.showCollection));
  const view = $derived(c ? store.collectionView(c) : null);
  const when = (t: number) => (t ? new Date(t * 1000).toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" }) : "never");
  const nameOf = (id: number) => store.byPfid.get(id)?.name ?? c?.names[String(id)] ?? String(id);
  const rows = $derived.by(() => {
    if (!c || !view) return [];
    return c.items.map((id) => {
      const m = store.byPfid.get(id);
      const state = m ? (store.activeSet.has(m.uid) ? "active" : "installed") : "missing";
      return { id, name: nameOf(id), m, state, added: view.added.includes(id) };
    });
  });
  let filter = $state<"all" | "missing" | "changed">("all");
  const shown = $derived(filter === "all" ? rows : filter === "missing" ? rows.filter((r) => r.state === "missing") : rows.filter((r) => r.added));

  // Escape closes the topmost thing; the store keeps the order. Import and Collection
  // had no Escape at all before this.
  $effect(() => store.onEscape("collection", 30, () => store.showCollection != null, close));

  function close() {
    store.showCollection = null;
  }
  function show(uid: string) {
    close();
    store.reveal(uid);
  }
</script>

{#if c && view}
  <div class="scrim" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) close(); }}>
    <div class="dlg card" role="dialog" aria-modal="true" aria-labelledby="col-title">
      <div class="hd">
        <div>
          <b id="col-title">{c.name}</b>
          <span class="sub">by {c.creator || "unknown"} · {c.items.length} mods · {view.installed.length} installed, {view.active.length} active, {view.missing.length} missing · checked {when(c.checkedAt)}</span>
        </div>
        <button class="x" aria-label="Close" onclick={close}>{@html I.close}</button>
      </div>

      {#if view.added.length || view.removed.length}
        <section class="changes">
          <h4>Changed since you last looked <span class="aside">{view.added.length} added, {view.removed.length} removed</span></h4>
          <div class="pills">
            {#each view.added as id}<span class="pill add" title={String(id)}>{@html I.plus}{nameOf(id)}</span>{/each}
            {#each view.removed as id}<span class="pill rm" title={String(id)}>{@html I.minus}{nameOf(id)}</span>{/each}
          </div>
          <p class="hint">Removed mods stay installed and active on your side; the curator just dropped them from the collection.</p>
        </section>
      {/if}

      <div class="seg" role="tablist">
        <button role="tab" class:on={filter === "all"} aria-selected={filter === "all"} onclick={() => (filter = "all")}>All <span class="num">{rows.length}</span></button>
        <button role="tab" class:on={filter === "missing"} aria-selected={filter === "missing"} onclick={() => (filter = "missing")}>Not installed <span class="num">{view.missing.length}</span></button>
        <button role="tab" class:on={filter === "changed"} aria-selected={filter === "changed"} onclick={() => (filter = "changed")}>Added <span class="num">{view.added.length}</span></button>
      </div>

      <div class="body">
        <div class="rows">
          {#each shown as r (r.id)}
            <div class="row {r.state}">
              <span class="k">{#if r.state === "active"}{@html I.check}{:else if r.state === "installed"}{@html I.minus}{:else}{@html I.download}{/if}</span>
              <span class="nm">
                <b>{r.name}</b>
                <span>{r.state === "active" ? "installed and active" : r.state === "installed" ? "installed, not in your list" : "not installed"}{r.added ? " · new in the collection" : ""}</span>
              </span>
              <span class="acts">
                <button class="ib" title="Workshop page" onclick={() => openUrl(`https://steamcommunity.com/sharedfiles/filedetails/?id=${r.id}`)}>{@html I.link}</button>
                {#if r.m}
                  <button class="btn sm" onclick={() => show(r.m!.uid)}>Show</button>
                  {#if r.state === "installed"}<button class="btn sm" onclick={() => store.activate([r.m!.uid])}>Activate</button>{/if}
                {:else}
                  <button class="btn sm" title="Download with SteamCMD" onclick={() => store.queueIds([r.id])}>Download</button>
                {/if}
              </span>
            </div>
          {/each}
          {#if !shown.length}<p class="hint">Nothing here.</p>{/if}
        </div>
      </div>

      <div class="ft">
        <button class="btn" onclick={() => store.refreshCollections(c.id)}>{@html I.refresh}Check for changes</button>
        <button class="btn" onclick={() => openUrl(`https://steamcommunity.com/sharedfiles/filedetails/?id=${c.id}`)}>{@html I.cloud}Open on Steam</button>
        <button class="btn danger" title="Stop following it; nothing is uninstalled" onclick={() => store.untrackCollection(c.id)}>Stop following</button>
        <span class="sp"></span>
        {#if view.missing.length}<button class="btn" title="Queue everything not installed for SteamCMD" onclick={() => store.queueIds(view.missing)}>{@html I.download}Download {view.missing.length} missing</button>{/if}
        {#if view.installed.length > view.active.length}<button class="btn" title="Add the installed mods to your active list, in the collection's order" onclick={() => store.activateCollection(c)}>{@html I.plus}Activate installed</button>{/if}
        <button class="btn" title="Make a named list of the collection's installed mods, in its order, after the game and DLC" onclick={() => { store.listFromCollection(c); close(); }}>{@html I.save}Make a list</button>
        {#if view.added.length || view.removed.length}<button class="btn primary" onclick={() => store.acknowledgeCollection(c.id)}>{@html I.check}Got it</button>{/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .scrim { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); backdrop-filter: blur(6px); display: grid; place-items: center; z-index: 40; }
  .dlg { width: min(760px, calc(100vw - 40px)); max-height: calc(100vh - 40px); padding: 18px 20px; box-shadow: var(--shadow-float); display: flex; flex-direction: column; gap: 14px; }
  .hd { display: flex; justify-content: space-between; align-items: flex-start; gap: 12px; }
  .hd b { font-size: 16px; font-weight: 800; display: block; }
  .hd .sub { display: block; font-size: 12.5px; color: var(--text-3); margin-top: 2px; line-height: 1.4; }
  .x { width: 28px; height: 28px; border-radius: 0; display: grid; place-items: center; color: var(--text-3); flex: none; }
  .x:hover { background: var(--surface-2); color: var(--text); }
  .x :global(svg) { width: 14px; height: 14px; }
  h4 { margin: 0 0 6px; font-size: 12px; font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase; color: var(--text-2); display: flex; align-items: center; gap: 8px; }
  .aside { color: var(--text-3); font-weight: 600; letter-spacing: 0; text-transform: none; }
  .hint { margin: 6px 0 0; color: var(--text-3); font-size: 12.5px; line-height: 1.45; }
  .pills { display: flex; flex-wrap: wrap; gap: 6px; }
  .pill { display: inline-flex; align-items: center; gap: 5px; height: 24px; padding: 0 9px; border-radius: 0; background: var(--surface-3); font-size: 12px; font-weight: 600; color: var(--text-2); max-width: 100%; }
  .pill :global(svg) { width: 10px; height: 10px; flex: none; }
  .pill.add { color: var(--green); background: var(--green-soft); }
  .pill.rm { color: var(--red); background: var(--red-soft); }
  .seg { display: inline-flex; background: var(--surface-2); border-radius: 0; padding: 3px; gap: 2px; align-self: flex-start; }
  .seg button { padding: 5px 10px; border-radius: 0; font-size: 12.5px; font-weight: 600; color: var(--text-2); display: flex; gap: 6px; }
  .seg button.on { background: var(--surface-4); color: var(--text); }
  .seg .num { color: var(--text-3); }
  .body { overflow: hidden auto; min-height: 0; padding-right: 4px; }
  .rows { display: flex; flex-direction: column; gap: 2px; }
  .row { display: grid; grid-template-columns: 24px minmax(0, 1fr) auto; gap: 10px; align-items: center; padding: 6px 8px; border-radius: 0; }
  .row:hover { background: var(--surface-2); }
  .k { width: 22px; height: 22px; border-radius: 0; display: grid; place-items: center; background: var(--surface-3); color: var(--text-2); }
  .k :global(svg) { width: 11px; height: 11px; }
  .row.active .k { background: var(--green-soft); color: var(--green); }
  .row.missing .k { background: var(--amber-soft); color: var(--amber); }
  .nm { min-width: 0; display: flex; flex-direction: column; }
  .nm b { font-size: 13.5px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .nm span { font-size: 11.5px; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .acts { display: flex; gap: 4px; align-items: center; }
  .ib { width: 28px; height: 28px; border-radius: 0; display: grid; place-items: center; color: var(--text-3); }
  .ib:hover { background: var(--surface-3); color: var(--text); }
  .ib :global(svg) { width: 14px; height: 14px; }
  .ft { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .sp { flex: 1; }
</style>
