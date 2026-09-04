<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { api, pickFile, inTauri } from "$lib/api";
  import { I } from "$lib/icons";
  import type { SavedList } from "$lib/types";

  let text = $state("");
  let link = $state("");
  const p = $derived(store.importPreview);
  const c = $derived(store.collectionPreview);
  const r = $derived(store.rentryPreview);

  async function chooseFile() {
    const path = await pickFile([
      { name: "Mod lists and saves", extensions: ["xml", "rws", "rml", "json", "txt"] },
      { name: "All files", extensions: ["*"] }
    ]);
    if (path) await store.importFrom(path);
  }
  function fetchLink() {
    const l = link.trim();
    if (!l) return;
    if (/rentry\.(co|org)/i.test(l) || (!l.includes("/") && !/^\d+$/.test(l) && !l.includes("."))) store.importRentry(l);
    else store.importCollection(l);
  }
  function close() {
    store.showImport = false;
    store.importPreview = null;
    store.collectionPreview = null;
    store.rentryPreview = null;
  }
  const missingIds = $derived(c ? c.missing : r ? r.missingWorkshopIds : []);
  let saved = $state<SavedList[]>([]);
  api.savedLists().then((l) => (saved = l)).catch(() => {});
  const labels: Record<string, string> = { saved: "saved by Circinus", seen: "found on disk", "before-reset": "rescued before a reset" };
  const when = (t: number) => new Date(t * 1000).toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" });
</script>

<div class="scrim" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) close(); }}>
  <div class="dlg card" role="dialog" aria-modal="true" aria-labelledby="imp-title">
    <div class="hd"><b id="imp-title">Import a mod list</b><button class="x" aria-label="Close" onclick={close}>{@html I.close}</button></div>

    <div class="sec">
      <div class="label">Steam collection or Rentry link</div>
      <div class="row">
        <input class="input" placeholder="https://steamcommunity.com/sharedfiles/filedetails/?id=…  or  https://rentry.co/…" bind:value={link} onkeydown={(e) => e.key === "Enter" && fetchLink()} />
        <button class="btn" disabled={!link.trim()} onclick={fetchLink}>Fetch</button>
      </div>
    </div>

    <div class="sec">
      <div class="label">File or text</div>
      <p class="lead">ModsConfig.xml, a save (.rws, compressed or not), an .rml list, RimSort/RimPy JSON, or pasted text with package ids.</p>
      <div class="row">
        <button class="btn" onclick={chooseFile} disabled={!inTauri}>{@html I.folder}Choose file…</button>
        <span class="or">or paste below</span>
      </div>
      <textarea class="input" rows="4" placeholder="brrainz.harmony&#10;unlimitedhugs.hugslib&#10;Better Pawn Control [voult.betterpawncontrol]" bind:value={text}></textarea>
      <div class="row">
        <button class="btn" disabled={!text.trim()} onclick={() => { store.collectionPreview = null; store.rentryPreview = null; store.importFrom(undefined, text); }}>Read list</button>
      </div>
    </div>

    {#if saved.length}
      <div class="sec">
        <div class="label">Previous lists <span class="or">every list Circinus saved or found in ModsConfig.xml</span></div>
        <div class="hist">
          {#each saved.slice(0, 12) as l (l.path)}
            <div class="hrow">
              <span class="hw">{when(l.savedAt)}</span>
              <span class="hn"><b class="num">{l.count}</b> mods · {labels[l.label] ?? l.label}{l.gameVersion ? ` · ${l.gameVersion.split(" ")[0]}` : ""}</span>
              <button class="btn sm" onclick={() => store.restoreList(l.path, false)}>Use as the list</button>
            </div>
          {/each}
        </div>
      </div>
    {/if}

    {#if c}
      <div class="res">
        <b>Collection:</b> {c.ids.length} items · <b>{c.installed.length}</b> installed · <b>{c.missing.length}</b> not installed
        {#if c.missing.length}
          <div class="ids">{#each c.missing.slice(0, 40) as id}<span class="mono" title={String(id)}>{c.names[String(id)] ?? id}</span>{/each}{#if c.missing.length > 40}<span class="mono">…</span>{/if}</div>
        {/if}
      </div>
    {:else if p}
      <div class="res">
        <b>{p.list.format}:</b> <b>{p.uids.length}</b> installed{p.missing.length ? `, ${p.missing.length} not installed` : ""}{p.list.gameVersion ? ` · made for ${p.list.gameVersion}` : ""}
        {#if p.missing.length}
          <div class="ids">{#each p.missing.slice(0, 40) as id}<span class="mono">{id}</span>{/each}{#if p.missing.length > 40}<span class="mono">…</span>{/if}</div>
        {/if}
        {#if r && r.missingWorkshopIds.length}<div class="hint">{r.missingWorkshopIds.length} of the missing ones have Workshop links in the paste and can be queued.</div>{/if}
      </div>
    {/if}

    <div class="ft">
      <button class="btn" onclick={close}>Cancel</button>
      <span class="sp"></span>
      {#if missingIds.length}
        <button class="btn" onclick={() => store.queueIds(missingIds)}>{@html I.download}Queue {missingIds.length} download{missingIds.length === 1 ? "" : "s"}</button>
      {/if}
      {#if c}
        <button class="btn" disabled={!c.installed.length} onclick={() => store.applyCollection(true)}>Append installed</button>
        <button class="btn primary" disabled={!c.installed.length} onclick={() => store.applyCollection(false)}>Activate installed as the list</button>
      {:else}
        <button class="btn" disabled={!p?.uids.length} onclick={() => store.applyImport(true)}>Append to active</button>
        <button class="btn primary" disabled={!p?.uids.length} onclick={() => store.applyImport(false)}>Replace active list</button>
      {/if}
    </div>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); backdrop-filter: blur(6px); display: grid; place-items: center; z-index: 40; }
  .dlg { width: min(680px, calc(100vw - 40px)); max-height: calc(100vh - 40px); overflow: auto; padding: 18px 20px; box-shadow: var(--shadow-float); display: flex; flex-direction: column; gap: 14px; }
  .hd { display: flex; justify-content: space-between; align-items: center; }
  .hd b { font-size: 16px; font-weight: 800; }
  .x { width: 28px; height: 28px; border-radius: 8px; display: grid; place-items: center; color: var(--text-3); }
  .x:hover { background: var(--surface-2); color: var(--text); }
  .x :global(svg) { width: 14px; height: 14px; }
  .sec { display: flex; flex-direction: column; gap: 8px; }
  .lead { margin: 0; color: var(--text-2); font-size: 12.5px; line-height: 1.45; }
  .row { display: flex; align-items: center; gap: 10px; }
  .or { color: var(--text-3); font-size: 12.5px; }
  .res { font-size: 13px; color: var(--text-2); background: var(--surface-2); border-radius: 10px; padding: 10px 12px; }
  .res b { color: var(--text); }
  .ids { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 8px; max-height: 120px; overflow: auto; }
  .ids .mono { background: var(--surface-3); padding: 2px 7px; border-radius: 6px; color: var(--text-2); }
  .hint { margin-top: 8px; font-size: 12px; color: var(--text-3); }
  .hist { display: flex; flex-direction: column; gap: 2px; max-height: 190px; overflow: auto; }
  .hrow { display: grid; grid-template-columns: 130px minmax(0, 1fr) auto; gap: 10px; align-items: center; padding: 4px 6px; border-radius: 8px; font-size: 12.5px; }
  .hrow:hover { background: var(--surface-2); }
  .hw { color: var(--text-3); }
  .hn { color: var(--text-2); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .hn b { color: var(--text); }
  .ft { display: flex; gap: 8px; margin-top: 4px; flex-wrap: wrap; }
  .sp { flex: 1; }
</style>
