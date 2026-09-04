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
  import SettingsView from "./components/SettingsView.svelte";
  import AnalyzerView from "./components/AnalyzerView.svelte";
  import DownloadsView from "./components/DownloadsView.svelte";
  import Toast from "./components/Toast.svelte";

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
    else if (e.altKey && e.key === "ArrowUp") { e.preventDefault(); store.moveSelected(-1); }
    else if (e.altKey && e.key === "ArrowDown") { e.preventDefault(); store.moveSelected(1); }
    else if (e.key === "Delete" || e.key === "Backspace") {
      const sel = store.selected.filter((u) => store.activeSet.has(u));
      if (sel.length) { e.preventDefault(); store.deactivate(sel); }
    } else if (e.key === "Escape") { store.selected = []; store.preview = null; }
  }
</script>

<svelte:window onkeydown={onKey} />

<div class="app">
  <TitleBar />
  {#if store.view === "settings"}
    <SettingsView />
  {:else if store.view === "analyzer"}
    <div class="frame two"><Rail /><AnalyzerView /></div>
  {:else if store.view === "downloads"}
    <div class="frame two"><Rail /><DownloadsView /></div>
  {:else}
    <div class="frame">
      <Rail />
      <main class="center">
        <Stats />
        <Banner />
        <Toolbar />
        <ModList />
        <div class="foot">
          {#if store.snap}
            {store.snap.mods.length} installed · {store.active.length} active · {store.snap.inspecting ? `inspecting ${store.snap.inspecting} folders in the background · ` : ""}{store.snap.missing.length ? `${store.snap.missing.length} in ModsConfig.xml but not installed · ` : ""}
            {store.snap.dbLoaded.length ? store.snap.dbLoaded.join(" · ") : "no rule databases loaded yet"}
          {/if}
        </div>
      </main>
      <Inspector />
    </div>
  {/if}
  {#if store.showImport}<ImportDialog />{/if}
  <Toast />
  {#if store.loading}
    <div class="loading"><div class="spin"></div><span>{store.progress ? `Reading your mods… ${store.progress.done.toLocaleString()} of ${store.progress.total.toLocaleString()}` : store.error ? store.error : `${store.step}…`}</span></div>
  {:else if store.error && !store.snap}
    <div class="loading"><span class="err">{store.error}<br /><button class="btn" onclick={() => store.load()}>Try again</button></span></div>
  {/if}
</div>

<style>
  .app { height: 100vh; display: grid; grid-template-rows: 56px minmax(0, 1fr); overflow: hidden; }
  .frame { display: grid; grid-template-columns: 236px minmax(0, 1fr) 316px; grid-template-rows: minmax(0, 1fr); gap: 14px; padding: 0 14px 14px; min-height: 0; }
  .frame.two { grid-template-columns: 236px minmax(0, 1fr); }
  .center { display: flex; flex-direction: column; gap: 12px; min-height: 0; min-width: 0; }
  .foot { font-size: 11px; color: var(--text-4); text-align: right; padding: 0 4px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .loading { position: fixed; inset: 0; display: grid; place-items: center; background: rgba(11, 11, 13, 0.7); backdrop-filter: blur(4px); z-index: 50; color: var(--text-2); font-weight: 600; }
  .loading > * { grid-area: 1 / 1; }
  .loading span { margin-top: 70px; max-width: 60ch; text-align: center; line-height: 1.5; user-select: text; }
  .loading .err { color: #ffb4ae; margin-top: 0; }
  .loading .err .btn { margin-top: 12px; }
  .spin { width: 28px; height: 28px; border-radius: 50%; border: 3px solid var(--surface-4); border-top-color: var(--amber); animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 1240px) { .frame { grid-template-columns: 220px minmax(0, 1fr); } .frame :global(.inspector) { display: none; } }
  @media (max-width: 980px) { .frame { grid-template-columns: minmax(0, 1fr); } .frame :global(.rail) { display: none; } }
</style>
