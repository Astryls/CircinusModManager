<script lang="ts">
  import { onMount } from "svelte";
  import { store } from "$lib/store.svelte";
  import TitleBar from "./components/TitleBar.svelte";
  import Rail from "./components/Rail.svelte";
  import Stats from "./components/Stats.svelte";
  import Banner from "./components/Banner.svelte";
  import Toolbar from "./components/Toolbar.svelte";
  import ModList from "./components/ModList.svelte";
  import Inspector from "./components/Inspector.svelte";
  import ImportDialog from "./components/ImportDialog.svelte";
  import ChangesDialog from "./components/ChangesDialog.svelte";
  import SettingsView from "./components/SettingsView.svelte";
  import AnalyzerView from "./components/AnalyzerView.svelte";
  import DownloadsView from "./components/DownloadsView.svelte";
  import TexturesView from "./components/TexturesView.svelte";
  import DefsView from "./components/DefsView.svelte";
  import HaloView from "./components/HaloView.svelte";
  import Toast from "./components/Toast.svelte";
  import ContextMenu from "./components/ContextMenu.svelte";
  import CollectionDialog from "./components/CollectionDialog.svelte";
  import Panel from "./components/Panel.svelte";

  onMount(() => {
    store.load();
  });

  function onKey(e: KeyboardEvent) {
    const tag = (e.target as HTMLElement)?.tagName;
    if (tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT") return;
    const meta = e.ctrlKey || e.metaKey;
    if (meta && e.key.toLowerCase() === "s") { e.preventDefault(); store.save(); }
    else if (meta && e.key.toLowerCase() === "k") { e.preventDefault(); (document.getElementById("search") as HTMLInputElement)?.focus(); }
    else if (meta && e.key.toLowerCase() === "i") { e.preventDefault(); store.showImport = true; }
    else if (meta && e.key.toLowerCase() === "d") { e.preventDefault(); store.view = store.view === "downloads" ? "order" : "downloads"; }
    else if (e.altKey && e.key === "ArrowUp") { e.preventDefault(); store.moveSelected(-1); }
    else if (e.altKey && e.key === "ArrowDown") { e.preventDefault(); store.moveSelected(1); }
    else if (e.key === "F8") { e.preventDefault(); store.reviewNext(); }
    else if (e.key === "Delete" || e.key === "Backspace") {
      const sel = store.selected.filter((u) => store.activeSet.has(u));
      if (sel.length) { e.preventDefault(); store.deactivate(sel); }
    } else if (e.key === "Escape") { if (store.showChanges) store.showChanges = false; else { store.selected = []; store.preview = null; } }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="app">
  <TitleBar />
  {#if store.view === "settings"}
    <Panel name="Settings"><SettingsView /></Panel>
  {:else if store.view === "analyzer"}
    <div class="frame two"><Panel name="Sidebar"><Rail /></Panel><Panel name="Analyzer"><AnalyzerView /></Panel></div>
  {:else if store.view === "downloads"}
    <div class="frame two"><Panel name="Sidebar"><Rail /></Panel><Panel name="Downloads"><DownloadsView /></Panel></div>
  {:else if store.view === "textures"}
    <div class="frame two"><Panel name="Sidebar"><Rail /></Panel><Panel name="Textures"><TexturesView /></Panel></div>
  {:else if store.view === "defs"}
    <div class="frame two"><Panel name="Sidebar"><Rail /></Panel><Panel name="Defs"><DefsView /></Panel></div>
  {:else if store.view === "halo"}
    <div class="frame two"><Panel name="Sidebar"><Rail /></Panel><Panel name="HALO"><HaloView /></Panel></div>
  {:else}
    <div class="frame">
      <Panel name="Sidebar"><Rail /></Panel>
      <main class="center">
        <Panel name="Summary"><Stats /></Panel>
        <Panel name="Attention banner"><Banner /></Panel>
        <Panel name="Toolbar"><Toolbar /></Panel>
        <Panel name="Mod list"><ModList /></Panel>
      </main>
      <Panel name="Inspector"><Inspector /></Panel>
      <div class="foot">
        {#if store.snap}
          {store.snap.mods.length} mods installed · {store.active.length} active · {store.snap.inspecting ? `reading ${store.snap.inspecting} folders in the background · ` : ""}{store.snap.missing.length ? `${store.snap.missing.length} in ModsConfig.xml but not installed · ` : ""}
          {store.snap.dbLoaded.length ? `Rule databases: ${store.snap.dbLoaded.join(", ")}` : "No rule databases loaded yet: Settings, Update now"}
        {/if}
      </div>
    </div>
  {/if}
  {#if store.showImport}<ImportDialog />{/if}
  {#if store.showChanges}<Panel name="Changes"><ChangesDialog /></Panel>{/if}
  {#if store.showCollection != null}<Panel name="Collection"><CollectionDialog /></Panel>{/if}
  <Panel name="Menu"><ContextMenu /></Panel>
  <Toast />
  {#if store.loading}
    <div class="loading"><div class="spin"></div><span>{store.progress ? `Reading your mods… ${store.progress.done.toLocaleString()} of ${store.progress.total.toLocaleString()}` : store.error ? store.error : `${store.step}…`}</span></div>
  {:else if store.error && !store.snap}
    <div class="loading"><span class="err">{store.error}<br /><button class="btn" onclick={() => store.load()}>Try again</button></span></div>
  {/if}
</div>

<style>
  .app { height: 100vh; display: grid; grid-template-rows: 56px minmax(0, 1fr); overflow: hidden; }
  .frame { display: grid; grid-template-columns: 236px minmax(0, 1fr) 340px; grid-template-rows: minmax(0, 1fr) auto; gap: 12px 14px; padding: 0 14px 10px; min-height: 0; }
  .frame.two { grid-template-columns: 236px minmax(0, 1fr); }
  .frame > :global(*) { min-width: 0; min-height: 0; }
  .center { display: flex; flex-direction: column; gap: 12px; min-height: 0; min-width: 0; }
  .foot { grid-column: 1 / -1; font-size: 11.5px; color: var(--text-3); text-align: center; padding: 0 4px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; margin-top: -4px; }
  .loading { position: fixed; inset: 0; display: grid; place-items: center; background: rgba(11, 11, 13, 0.7); backdrop-filter: blur(4px); z-index: 50; color: var(--text-2); font-weight: 600; }
  .loading > * { grid-area: 1 / 1; }
  .loading span { margin-top: 70px; max-width: 60ch; text-align: center; line-height: 1.5; user-select: text; }
  .loading .err { color: #ffb4ae; margin-top: 0; }
  .loading .err .btn { margin-top: 12px; }
  .spin { width: 28px; height: 28px; border-radius: 50%; border: 3px solid var(--surface-4); border-top-color: var(--amber); animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 1240px) { .frame { grid-template-columns: 220px minmax(0, 1fr); } .frame :global(.inspector) { display: none; } }
  @media (min-width: 1700px) { .frame { grid-template-columns: 250px minmax(0, 1fr) 380px; } }
  @media (max-width: 980px) { .frame { grid-template-columns: minmax(0, 1fr); } .frame :global(.rail) { display: none; } }
</style>
