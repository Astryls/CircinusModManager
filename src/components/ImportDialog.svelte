<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { pickFile, inTauri } from "$lib/api";
  import { I } from "$lib/icons";

  let text = $state("");
  const p = $derived(store.importPreview);

  async function chooseFile() {
    const path = await pickFile([
      { name: "Mod lists and saves", extensions: ["xml", "rws", "rml", "json", "txt"] },
      { name: "All files", extensions: ["*"] }
    ]);
    if (path) await store.importFrom(path);
  }
  function close() {
    store.showImport = false;
    store.importPreview = null;
  }
</script>

<div class="scrim" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) close(); }}>
  <div class="dlg card" role="dialog" aria-modal="true" aria-labelledby="imp-title">
    <div class="hd"><b id="imp-title">Import a mod list</b><button class="x" aria-label="Close" onclick={close}>{@html I.close}</button></div>
    <p class="lead">ModsConfig.xml, a save (.rws, compressed or not), an .rml list, RimSort/RimPy JSON, or pasted text with package ids. Steam collections and Rentry links arrive in the next milestone.</p>
    <div class="row">
      <button class="btn" onclick={chooseFile} disabled={!inTauri}>{@html I.folder}Choose file…</button>
      <span class="or">or paste below</span>
    </div>
    <textarea class="input" rows="6" placeholder="brrainz.harmony&#10;unlimitedhugs.hugslib&#10;Better Pawn Control [voult.betterpawncontrol]" bind:value={text}></textarea>
    <div class="row">
      <button class="btn" disabled={!text.trim()} onclick={() => store.importFrom(undefined, text)}>Read list</button>
      {#if p}
        <span class="res">{p.list.format}: <b>{p.uids.length}</b> installed{p.missing.length ? `, ${p.missing.length} not installed` : ""}{p.list.gameVersion ? ` · made for ${p.list.gameVersion}` : ""}</span>
      {/if}
    </div>
    {#if p?.missing.length}
      <div class="missing">
        <div class="label">Not installed</div>
        <div class="ids">{#each p.missing as id}<span class="mono">{id}</span>{/each}</div>
      </div>
    {/if}
    <div class="ft">
      <button class="btn" onclick={close}>Cancel</button>
      <span class="sp"></span>
      <button class="btn" disabled={!p?.uids.length} onclick={() => store.applyImport(true)}>Append to active</button>
      <button class="btn primary" disabled={!p?.uids.length} onclick={() => store.applyImport(false)}>Replace active list</button>
    </div>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); backdrop-filter: blur(6px); display: grid; place-items: center; z-index: 40; }
  .dlg { width: min(640px, calc(100vw - 40px)); padding: 18px 20px; box-shadow: var(--shadow-float); display: flex; flex-direction: column; gap: 12px; }
  .hd { display: flex; justify-content: space-between; align-items: center; }
  .hd b { font-size: 16px; font-weight: 800; }
  .x { width: 28px; height: 28px; border-radius: 8px; display: grid; place-items: center; color: var(--text-3); }
  .x:hover { background: var(--surface-2); color: var(--text); }
  .x :global(svg) { width: 14px; height: 14px; }
  .lead { margin: 0; color: var(--text-2); font-size: 13px; line-height: 1.45; }
  .row { display: flex; align-items: center; gap: 12px; }
  .or { color: var(--text-3); font-size: 12.5px; }
  .res { font-size: 13px; color: var(--text-2); }
  .missing .ids { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 8px; max-height: 120px; overflow: auto; }
  .missing .mono { background: var(--surface-2); padding: 2px 7px; border-radius: 6px; color: var(--text-2); }
  .ft { display: flex; gap: 8px; margin-top: 6px; }
  .sp { flex: 1; }
</style>
