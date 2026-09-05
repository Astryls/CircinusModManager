<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { I } from "$lib/icons";

  const total = $derived(store.mods.length);
  const q = $derived(store.downloads);
  let now = $state(Math.floor(Date.now() / 1000));
  $effect(() => {
    const t = setInterval(() => (now = Math.floor(Date.now() / 1000)), 1000);
    return () => clearInterval(t);
  });
  /** One glance at the download manager: installed? busy? cooling down? */
  const dl = $derived.by(() => {
    if (store.downloadsError) return { color: "var(--red)", label: "Downloads", title: `The download manager did not answer: ${store.downloadsError}`, pulse: false };
    if (!q) return { color: "var(--text-4)", label: "Downloads", title: "Connecting to the download manager", pulse: false };
    const queued = store.queueCounts.queued;
    const cool = q.throttle.cooldownUntil ? Math.max(0, q.throttle.cooldownUntil - now) : 0;
    if (q.installing) return { color: "var(--blue)", label: "Installing SteamCMD", title: "SteamCMD is downloading and updating itself", pulse: true };
    if (!q.steamcmdInstalled) return { color: "var(--amber)", label: "SteamCMD not set up", title: "SteamCMD is not set up yet. Click to set it up. It downloads Workshop mods without the Steam client.", pulse: false };
    if (cool) return { color: "var(--amber)", label: `Cooling down ${cool}s`, title: "Steam refused downloads. Circinus is waiting, then tries a smaller batch.", pulse: false };
    if (q.running) return { color: "var(--blue)", label: `Downloading · ${queued} left`, title: `SteamCMD is fetching a batch of ${q.currentBatch.length}`, pulse: true };
    if (q.paused && queued) return { color: "var(--text-3)", label: `Paused · ${queued} queued`, title: "Downloads are paused", pulse: false };
    if (queued) return { color: "var(--blue)", label: `${queued} queued`, title: "Waiting to start the next batch", pulse: false };
    if (store.queueCounts.failed) return { color: "var(--red)", label: `${store.queueCounts.failed} failed`, title: "Some downloads failed. Open Downloads to retry.", pulse: false };
    return { color: "var(--green)", label: "SteamCMD ready", title: "SteamCMD is ready. Paste Workshop links in Downloads, or use Re-download on a mod.", pulse: false };
  });
  const changesN = $derived(store.changeCounts.total);

</script>

<header class="title">
  <div class="brand">
    <button class="mark" title="Back to the load order" aria-label="Circinus: back to the load order" onclick={() => { store.view = "order"; store.tab = "active"; }}>
      <svg viewBox="0 0 64 64" aria-hidden="true">
        <defs><linearGradient id="ctile" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#2b2b30"/><stop offset="1" stop-color="#17171a"/></linearGradient></defs>
        <rect x="2" y="2" width="60" height="60" rx="14" fill="url(#ctile)" stroke="#3a3a40" stroke-width="1.5"/>
        <circle cx="32" cy="32" r="19" fill="none" stroke="#e9a23b" stroke-width="4"/>
        <path d="M32 8v4M32 52v4M8 32h4M52 32h4" stroke="#e9a23b" stroke-width="3" stroke-linecap="round"/>
        <path d="M32 15 38 32H26z" fill="#e9a23b"/>
        <path d="M32 49 26 32h12z" fill="#7d7f86"/>
        <circle cx="32" cy="32" r="4" fill="#f3f3f5"/>
        <circle cx="32" cy="32" r="1.6" fill="#17171a"/>
      </svg>
    </button>
  </div>
  <div class="search">
    {@html I.search}
    <input id="search" type="search" placeholder="Search {total} mods by name, author, packageId or workshop id" aria-label="Search mods" bind:value={store.query} />
    <kbd>Ctrl K</kbd>
  </div>
  <div class="actions">
    {#if store.busy}<span class="busy">{store.busy}</span>{/if}
    {#if changesN}
      <button class="chip warn" title="{changesN} mods changed since you last opened Circinus. Click to see what changed." onclick={() => (store.showChanges = true)}>{@html I.bell}<span>{changesN} changed</span></button>
    {/if}
    <button class="chip" class:on={store.view === "downloads"} title={dl.title} aria-label="Downloads (Ctrl D)" onclick={() => (store.view = store.view === "downloads" ? "order" : "downloads")}>
      <span class="dot" class:pulse={dl.pulse} style="--c: {dl.color}"></span>{@html I.cloud}<span class="lbl">{dl.label}</span>
    </button>
    <button class="ib" class:on={store.view === "settings"} aria-label="Settings" title="Settings" onclick={() => (store.view = store.view === "settings" ? "order" : "settings")}>{@html I.gear}</button>
    <button class="btn" class:save={store.snap?.dirty} disabled={!store.snap?.dirty} onclick={() => store.save()} title="Write ModsConfig.xml (Ctrl S)">{@html I.save}Save</button>
    <button class="btn" onclick={() => store.launch()} disabled={!!store.busy} title={store.snap?.settings.launch.method === "executable" ? "Start RimWorld from its executable (see Settings, Launching RimWorld)" : "Start RimWorld. Through Steam when it lives in a Steam library, otherwise from its executable."}>{@html I.play}Play</button>
  </div>
</header>

<style>
  .title { display: flex; align-items: center; gap: 16px; padding: 0 18px; background: var(--bg); min-width: 0; }
  .brand { display: flex; align-items: center; gap: 10px; flex: 1 1 0; min-width: 0; }
  .mark { width: 36px; height: 36px; border-radius: 10px; display: grid; place-items: center; transition: transform 0.12s; }
  .mark svg { width: 34px; height: 34px; display: block; filter: drop-shadow(0 4px 10px rgba(0, 0, 0, 0.45)); }
  .mark:hover { transform: scale(1.06); }
  .mark:active { transform: scale(0.98); }
  .search { position: relative; flex: 1 1 520px; max-width: 520px; min-width: 160px; }
  .search input { width: 100%; height: 34px; border: 0; border-radius: 10px; background: var(--surface); color: var(--text); padding: 0 64px 0 36px; font-size: 13.5px; box-shadow: var(--shadow-card); }
  .search input::placeholder { color: var(--text-3); }
  .search :global(svg) { position: absolute; left: 11px; top: 8px; width: 18px; height: 18px; pointer-events: none; opacity: 0.9; }
  .search kbd { position: absolute; right: 10px; top: 8px; font: 600 11px var(--mono); color: var(--text-3); background: var(--surface-3); border-radius: 5px; padding: 2px 6px; }
  .actions { display: flex; justify-content: flex-end; align-items: center; gap: 8px; flex: 1 1 0; min-width: max-content; }
  @media (max-width: 1500px) { .chip .lbl { display: none; } .chip { padding-right: 10px; } .busy { display: none; } }
  .busy { font-size: 12px; color: var(--text-3); font-weight: 600; margin-right: 6px; white-space: nowrap; }
  .ib { width: 34px; height: 34px; border-radius: 10px; display: grid; place-items: center; color: var(--text-2); }
  .ib:hover, .ib.on { background: var(--surface-2); color: var(--text); }
  .ib :global(svg) { width: 20px; height: 20px; }
  .chip { height: 34px; padding: 0 12px 0 10px; border-radius: 10px; display: inline-flex; align-items: center; gap: 8px; color: var(--text-2); font-weight: 600; font-size: 12.5px; white-space: nowrap; background: var(--surface); box-shadow: var(--shadow-card); }
  .chip:hover, .chip.on { background: var(--surface-2); color: var(--text); }
  .chip :global(svg) { width: 16px; height: 16px; }
  .chip.warn { color: var(--amber); }
  .chip .dot { width: 8px; height: 8px; border-radius: 50%; background: var(--c); box-shadow: 0 0 0 3px color-mix(in srgb, var(--c) 22%, transparent); }
  .chip .dot.pulse { animation: pulse 1.2s ease-in-out infinite; }
  @keyframes pulse { 50% { box-shadow: 0 0 0 6px color-mix(in srgb, var(--c) 10%, transparent); } }
</style>
