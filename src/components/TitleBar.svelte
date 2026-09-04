<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { I } from "$lib/icons";
  import { openUrl } from "$lib/api";

  const total = $derived(store.mods.length);
  const gameDir = $derived(store.snap?.locations.gameDir ?? null);
  const q = $derived(store.downloads);
  let now = $state(Math.floor(Date.now() / 1000));
  $effect(() => {
    const t = setInterval(() => (now = Math.floor(Date.now() / 1000)), 1000);
    return () => clearInterval(t);
  });
  /** One glance at the download manager: installed? busy? cooling down? */
  const dl = $derived.by(() => {
    if (store.downloadsError) return { color: "var(--red)", label: "Downloads", title: `The download manager did not answer: ${store.downloadsError}`, pulse: false };
    if (!q) return { color: "var(--text-4)", label: "Downloads", title: "Connecting to the download manager…", pulse: false };
    const queued = store.queueCounts.queued;
    const cool = q.throttle.cooldownUntil ? Math.max(0, q.throttle.cooldownUntil - now) : 0;
    if (q.installing) return { color: "var(--blue)", label: "Installing SteamCMD", title: "SteamCMD is downloading and updating itself", pulse: true };
    if (!q.steamcmdInstalled) return { color: "var(--amber)", label: "SteamCMD not set up", title: "SteamCMD is not installed yet — click to set it up (needed to download Workshop mods without the Steam client)", pulse: false };
    if (cool) return { color: "var(--amber)", label: `Cooling down ${cool}s`, title: "Steam refused downloads; Circinus is waiting before trying a smaller batch", pulse: false };
    if (q.running) return { color: "var(--blue)", label: `Downloading · ${queued} left`, title: `SteamCMD is fetching a batch of ${q.currentBatch.length}`, pulse: true };
    if (q.paused && queued) return { color: "var(--text-3)", label: `Paused · ${queued} queued`, title: "Downloads are paused", pulse: false };
    if (queued) return { color: "var(--blue)", label: `${queued} queued`, title: "Waiting to start the next batch", pulse: false };
    if (store.queueCounts.failed) return { color: "var(--red)", label: `${store.queueCounts.failed} failed`, title: "Some downloads failed — open Downloads to retry", pulse: false };
    return { color: "var(--green)", label: "SteamCMD ready", title: "SteamCMD is installed; paste Workshop links in Downloads or right-click a mod to re-download it", pulse: false };
  });
  const changesN = $derived(store.changeCounts.total);

  function play() {
    // RimWorld launches through Steam so the overlay and workshop sync behave.
    openUrl("steam://rungameid/294100");
  }
</script>

<header class="title">
  <div class="brand">
    <span class="mark" aria-hidden="true"><i></i></span><b>Circinus</b>
    <span class="pill" title={gameDir ?? "RimWorld not found — set the game folder in Settings"}>
      <span class="dot" style="--c: {gameDir ? 'var(--green)' : 'var(--red)'}"></span>{store.snap ? `RimWorld ${store.snap.gameVersion.majorMinor}` : "…"}{store.snap?.dirty ? " · unsaved" : ""}
    </span>
  </div>
  <div class="search">
    {@html I.search}
    <input id="search" type="search" placeholder="Search {total} mods by name, author, packageId or workshop id" aria-label="Search mods" bind:value={store.query} />
    <kbd>Ctrl K</kbd>
  </div>
  <div class="actions">
    {#if store.busy}<span class="busy">{store.busy}</span>{/if}
    {#if changesN}
      <button class="chip warn" title="{changesN} mods changed since you last opened Circinus — click to see what" onclick={() => (store.showChanges = true)}>{@html I.bell}<span>{changesN} changed</span></button>
    {/if}
    <button class="chip" class:on={store.view === "downloads"} title={dl.title} aria-label="Downloads (Ctrl D)" onclick={() => (store.view = store.view === "downloads" ? "order" : "downloads")}>
      <span class="dot" class:pulse={dl.pulse} style="--c: {dl.color}"></span>{@html I.download}<span>{dl.label}</span>
    </button>
    <button class="ib" class:on={store.view === "settings"} aria-label="Settings" title="Settings" onclick={() => (store.view = store.view === "settings" ? "order" : "settings")}>{@html I.gear}</button>
    <button class="btn" class:save={store.snap?.dirty} disabled={!store.snap?.dirty} onclick={() => store.save()} title="Write ModsConfig.xml (Ctrl S)">{#if store.snap?.dirty}<i></i>{/if}Save</button>
    <button class="btn" onclick={play} title="Launch RimWorld through Steam">{@html I.play}Play</button>
  </div>
</header>

<style>
  .title { display: grid; grid-template-columns: 1fr minmax(320px, 520px) 1fr; align-items: center; gap: 16px; padding: 0 18px; background: var(--bg); }
  .brand { display: flex; align-items: center; gap: 10px; min-width: 0; }
  .mark { width: 26px; height: 26px; border-radius: 8px; background: linear-gradient(160deg, #2a2a30, #161619); box-shadow: var(--shadow-card); display: grid; place-items: center; }
  .mark i { width: 12px; height: 12px; border-radius: 50%; border: 3px solid var(--amber); box-sizing: border-box; display: block; }
  .brand b { font-weight: 800; font-size: 15px; letter-spacing: -0.01em; }
  .search { position: relative; }
  .search input { width: 100%; height: 34px; border: 0; border-radius: 10px; background: var(--surface); color: var(--text); padding: 0 64px 0 36px; font-size: 13.5px; box-shadow: var(--shadow-card); }
  .search input::placeholder { color: var(--text-3); }
  .search :global(svg) { position: absolute; left: 12px; top: 9px; width: 16px; height: 16px; color: var(--text-3); pointer-events: none; }
  .search kbd { position: absolute; right: 10px; top: 8px; font: 600 11px var(--mono); color: var(--text-3); background: var(--surface-3); border-radius: 5px; padding: 2px 6px; }
  .actions { display: flex; justify-content: flex-end; align-items: center; gap: 8px; }
  .busy { font-size: 12px; color: var(--text-3); font-weight: 600; margin-right: 6px; white-space: nowrap; }
  .ib { width: 34px; height: 34px; border-radius: 10px; display: grid; place-items: center; color: var(--text-2); }
  .ib:hover, .ib.on { background: var(--surface-2); color: var(--text); }
  .ib :global(svg) { width: 18px; height: 18px; }
  .chip { height: 34px; padding: 0 12px 0 10px; border-radius: 10px; display: inline-flex; align-items: center; gap: 8px; color: var(--text-2); font-weight: 600; font-size: 12.5px; white-space: nowrap; background: var(--surface); box-shadow: var(--shadow-card); }
  .chip:hover, .chip.on { background: var(--surface-2); color: var(--text); }
  .chip :global(svg) { width: 15px; height: 15px; }
  .chip.warn { color: var(--amber); }
  .chip .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--c); box-shadow: 0 0 0 3px color-mix(in srgb, var(--c) 22%, transparent); }
  .chip .dot.pulse { animation: pulse 1.2s ease-in-out infinite; }
  @keyframes pulse { 50% { box-shadow: 0 0 0 6px color-mix(in srgb, var(--c) 10%, transparent); } }
</style>
