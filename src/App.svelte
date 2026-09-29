<script lang="ts">
  import { onMount } from "svelte";
  import { store } from "$lib/store.svelte";
  import { api } from "$lib/api";
  import { actions, chord, matches } from "$lib/keys.svelte";
  import Rail from "./components/Rail.svelte";
  import SearchLine from "./components/SearchLine.svelte";
  import Stats from "./components/Stats.svelte";
  import Banner from "./components/Banner.svelte";
  import Toolbar from "./components/Toolbar.svelte";
  import ModList from "./components/ModList.svelte";
  import Inspector from "./components/Inspector.svelte";
  import ImportDialog from "./components/ImportDialog.svelte";
  import ChangesDialog from "./components/ChangesDialog.svelte";
  import SaveDialog from "./components/SaveDialog.svelte";
  import SharingCard from "./components/SharingCard.svelte";
  import SettingsView from "./components/SettingsView.svelte";
  import AnalyzerView from "./components/AnalyzerView.svelte";
  import LoadTimesView from "./components/LoadTimesView.svelte";
  import DownloadsView from "./components/DownloadsView.svelte";
  import TexturesView from "./components/TexturesView.svelte";
  import DefsView from "./components/DefsView.svelte";
  import PatchesView from "./components/PatchesView.svelte";
  import AnnouncementsDialog from "./components/AnnouncementsDialog.svelte";
  import CommandPalette from "./components/CommandPalette.svelte";
  import ShortcutsDialog from "./components/ShortcutsDialog.svelte";
  import HaloView from "./components/HaloView.svelte";
  import Toast from "./components/Toast.svelte";
  import ContextMenu from "./components/ContextMenu.svelte";
  import CollectionDialog from "./components/CollectionDialog.svelte";
  import InstancesDialog from "./components/InstancesDialog.svelte";
  import MovesView from "./components/MovesView.svelte";
  import Panel from "./components/Panel.svelte";

  onMount(() => {
    store.load();
  });

  /** Ask about sharing once, after the window has something on it.
   *
   *  Not during the loading spinner: a question about data policy over an empty window is one
   *  people dismiss to get to the app. And not at all until the snapshot has arrived, because
   *  until then we cannot tell "never asked" from "not loaded yet". */
  $effect(() => {
    if (store.loading || !store.snap) return;
    if (store.sharingAnswered) return;
    store.showConsent = true;
  });

  /** The game writes how long it took to start; Circinus is not in front while it does.
   *
   *  So the one moment that figure changes is the one moment this window is not looking, and
   *  reading it only at launch meant playing, coming back, and being shown the run before the
   *  one you just did. Asking on focus costs two `stat` calls when nothing has changed. */
  function onFocus() {
    api.refreshLastRun().catch(() => {
      /* the figure on screen stays as it was, which is the right failure */
    });
  }

  // A pane still has to hold a row worth reading: the number, a mod name of 180px, the six badges
  // and the Move column come to 440px, so two of them and the gap between need 892. Measured
  // against the centre column rather than the window, because the inspector is what takes the
  // room: at 1440 it is still there and one pane fits, at 1180 it is gone and two do.
  const TWO_UP = 892;
  let centre = $state(1200);
  let only = $state(0);
  const twoUp = $derived(centre >= TWO_UP);
  const panes = $derived(store.splitMode !== "library" ? [] : [
    { pane: "inactive" as const, title: "Inactive", count: `${store.visibleInactive.length}`, note: "the mods you are not using" },
    { pane: "active" as const, title: "Active", count: `${store.visibleActive.length}`, note: "your active list" }
  ]);

  /** Is the caret in something a keystroke would type into?
   *
   *  The old guard asked only for the tag name and bailed on every INPUT, which is why Escape
   *  did nothing in the search box and Ctrl+S did not save while you were typing -- the moment
   *  you most want to save. What matters is not whether an input has focus but whether this
   *  keystroke would otherwise be a character: a chord is safe in a text field, a bare key is
   *  not. `isContentEditable` is here because the tag check never covered it. */
  function typing(e: KeyboardEvent) {
    const el = e.target as HTMLElement | null;
    if (!el) return false;
    return el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT" || el.isContentEditable;
  }

  function onKey(e: KeyboardEvent) {
    // Escape belongs to whatever is on top; the store keeps the order.
    if (e.key === "Escape") {
      // A text field handles its own Escape first -- clearing what you typed is what you meant,
      // not closing the window behind it.
      if (typing(e)) return;
      if (store.escape()) e.preventDefault();
      return;
    }
    const inText = typing(e);
    for (const a of actions()) {
      if (!a.keys || !matches(e, chord(a.keys))) continue;
      // A bare key inside a text field is a character. A chord is not, and stays live.
      if (inText && a.scope !== "always") return;
      if (a.enabled && !a.enabled()) return;
      e.preventDefault();
      a.run();
      return;
    }
  }
</script>

<svelte:window onkeydown={onKey} onfocus={onFocus} />

<div class="app">
  <!-- The Directory layout: one fixed column is the whole navigation -- instance, views, groups,
       collections, and the two committed actions at its foot -- and there is no title bar above
       it. The column is outside the view switch because it is the constant; only what sits
       beside it changes. -->
  <div class="frame" class:two={store.view !== "order"}>
    <Panel name="Sidebar"><Rail /></Panel>
    <main class="center" bind:clientWidth={centre}>
      <SearchLine />
      {#if store.view === "settings"}
        <Panel name="Settings"><SettingsView /></Panel>
      {:else if store.view === "analyzer"}
        <Panel name="Analyzer"><AnalyzerView /></Panel>
      {:else if store.view === "loadtimes"}
        <Panel name="Load times"><LoadTimesView /></Panel>
      {:else if store.view === "downloads"}
        <Panel name="Downloads"><DownloadsView /></Panel>
      {:else if store.view === "textures"}
        <Panel name="Textures"><TexturesView /></Panel>
      {:else if store.view === "defs"}
        <Panel name="Defs"><DefsView /></Panel>
      {:else if store.view === "patches"}
        <Panel name="Patches"><PatchesView /></Panel>
      {:else if store.view === "halo"}
        <Panel name="HALO"><HaloView /></Panel>
      {:else}
        <Panel name="Summary"><Stats /></Panel>
        <Panel name="Attention banner"><Banner /></Panel>
        <Panel name="Toolbar"><Toolbar /></Panel>
        {#if store.splitMode === "halo"}
          <Panel name="What HALO would change"><MovesView /></Panel>
        {:else if panes.length}
          <div class="panes" class:one={!twoUp}>
            {#each twoUp ? panes : [panes[only]] as p (p.pane)}
              <Panel name={p.title}>
                <section class="pane">
                  <div class="phead"><span class="t">{p.title}</span><span class="sep">·</span><span class="n num">{p.count}</span></div>
                  <ModList pane={p.pane} />
                </section>
              </Panel>
            {/each}
          </div>
          {#if !twoUp}
            <div class="narrow">Not enough width for two panels, so this is {panes[only].note} on its own. <button onclick={() => (only = only ? 0 : 1)}>Show {panes[only ? 0 : 1].note}</button></div>
          {/if}
        {:else}
          <Panel name="Mod list"><ModList /></Panel>
        {/if}
      {/if}
    </main>
    {#if store.view === "order"}<Panel name="Inspector"><Inspector /></Panel>{/if}
    <div class="foot">
      {#if store.snap}
        {store.snap.mods.length} mods installed · {store.active.length} active · {store.snap.inspecting ? `reading ${store.snap.inspecting} folders in the background · ` : ""}{store.snap.missing.length ? `${store.snap.missing.length} in ModsConfig.xml but not installed · ` : ""}
        {store.snap.dbLoaded.length ? `Rule databases: ${store.snap.dbLoaded.join(", ")}` : "No rule databases loaded yet: Settings, Update now"}
      {/if}
    </div>
  </div>
  {#if store.showImport}<ImportDialog />{/if}
  {#if store.showChanges}<Panel name="Changes"><ChangesDialog /></Panel>{/if}
  {#if store.showSave}<Panel name="Save"><SaveDialog /></Panel>{/if}
  {#if store.showConsent}<Panel name="Sharing"><SharingCard /></Panel>{/if}
  {#if store.showCollection != null}<Panel name="Collection"><CollectionDialog /></Panel>{/if}
  <!-- A dialog rather than a view with a rail row of its own: most installs follow no packs, and
       a permanent nav entry reading zero is the thing the New tab was deliberately not. -->
  {#if store.showAnnouncements}<Panel name="Modpack updates"><AnnouncementsDialog /></Panel>{/if}
  {#if store.showInstances}<Panel name="Instances"><InstancesDialog /></Panel>{/if}
  {#if store.showKeys}<Panel name="Shortcuts"><ShortcutsDialog /></Panel>{/if}
  {#if store.showPalette}<Panel name="Commands"><CommandPalette /></Panel>{/if}
  <Panel name="Menu"><ContextMenu /></Panel>
  <Toast />
  {#if store.loading}
    <div class="loading"><div class="spin"></div><span>{store.progress ? `Reading your mods… ${store.progress.done.toLocaleString()} of ${store.progress.total.toLocaleString()}` : store.error ? store.error : `${store.step}…`}</span></div>
  {:else if store.error && !store.snap}
    <div class="loading"><span class="err">{store.error}<br /><button class="btn" onclick={() => store.load()}>Try again</button></span></div>
  {/if}
</div>

<style>
  /* No title bar row: the directory column runs the full height of the window, and its width is
     the site's own --dirw. Panels are separated by rules, not by gaps, so the gap is gone too --
     a gap between flat squares reads as a mistake, where between shadowed cards it read as depth. */
  .app { height: 100vh; display: grid; grid-template-rows: minmax(0, 1fr); overflow: hidden; }
  .frame { display: grid; grid-template-columns: 266px minmax(0, 1fr) 340px; grid-template-rows: minmax(0, 1fr) auto; min-height: 0; }
  .frame.two { grid-template-columns: 266px minmax(0, 1fr); }
  .frame > :global(.rail), .frame > :global(*:first-child) { border-right: 1px solid var(--surface-3); }
  .frame > :global(.inspector) { border-left: 1px solid var(--surface-3); }
  .frame > :global(*) { min-width: 0; min-height: 0; }
  .center { display: flex; flex-direction: column; min-height: 0; min-width: 0; }
  .center > :global(*) { min-width: 0; }
  .panes { flex: 1; min-height: 0; display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .panes.one { grid-template-columns: minmax(0, 1fr); }
  .panes > :global(*) { min-width: 0; min-height: 0; display: flex; flex-direction: column; }
  .pane { display: flex; flex-direction: column; gap: 8px; min-height: 0; min-width: 0; flex: 1; }
  .phead { display: flex; align-items: baseline; gap: 6px; padding: 0 4px; white-space: nowrap; overflow: hidden; }
  .phead .t { font-size: 11px; font-weight: 700; letter-spacing: 0.09em; text-transform: uppercase; color: var(--text-2); }
  .phead .sep, .phead .n { font-size: 11.5px; font-weight: 600; color: var(--text-3); }
  .narrow { font-size: 11.5px; color: var(--text-3); padding: 0 4px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .narrow button { color: var(--text-2); font-size: 11.5px; font-weight: 600; text-decoration: underline; }
  .narrow button:hover { color: var(--text); }
  .foot { grid-column: 1 / -1; font-family: var(--mono); font-size: 10.5px; color: var(--text-3); padding: 7px 14px; border-top: 1px solid var(--surface-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .loading { position: fixed; inset: 0; display: grid; place-items: center; background: var(--scrim); backdrop-filter: blur(4px); z-index: 50; color: var(--text-2); font-weight: 600; }
  .loading > * { grid-area: 1 / 1; }
  .loading span { margin-top: 70px; max-width: 60ch; text-align: center; line-height: 1.5; user-select: text; }
  .loading .err { color: var(--red); margin-top: 0; }
  .loading .err .btn { margin-top: 12px; }
  .spin { width: 28px; height: 28px; border-radius: 50%; border: 3px solid var(--surface-3); border-top-color: var(--amber); animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 1240px) { .frame { grid-template-columns: 240px minmax(0, 1fr); } .frame :global(.inspector) { display: none; } }
  @media (min-width: 1700px) { .frame { grid-template-columns: 266px minmax(0, 1fr) 380px; } }
  @media (max-width: 980px) { .frame { grid-template-columns: minmax(0, 1fr); } .frame :global(.rail) { display: none; } }
</style>
