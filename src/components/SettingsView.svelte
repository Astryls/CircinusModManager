<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { api, pickFile, pickFolder, inTauri, openUrl, revealPath } from "$lib/api";
  import { I } from "$lib/icons";
  import type { LaunchInfo, LaunchMethod, Locations } from "$lib/types";

  const s = $derived(store.snap?.settings);
  const loc = $derived(store.snap?.locations);
  const q = $derived(store.downloads);
  const st = $derived(store.steamcmd);
  store.refreshDownloads();
  let launch = $state<LaunchInfo | null>(null);
  function refreshLaunch() {
    api.launchInfo().then((l) => (launch = l)).catch(() => {});
  }
  refreshLaunch();
  const L = $derived(s?.launch);
  async function updateLaunch(patch: Partial<NonNullable<typeof L>>) {
    if (!L) return;
    await store.updateSettings({ launch: { ...L, ...patch } });
    refreshLaunch();
  }
  async function chooseExe() {
    const p = await pickFile([{ name: "RimWorld", extensions: ["exe", "app", "sh", "x86_64", "*"] }]);
    if (p) await updateLaunch({ executable: p, method: "executable" });
  }
  const methods: { id: LaunchMethod; label: string; hint: string }[] = [
    { id: "auto", label: "Auto", hint: "Steam when the game lives in a Steam library, otherwise the executable" },
    { id: "steam", label: "Steam", hint: "steam://rungameid/294100 — overlay and Workshop sync as usual" },
    { id: "executable", label: "Executable", hint: "Start the game directly: GOG, DRM-free, or a copy outside Steam" }
  ];
  let appData = $state("");
  api.appDataDir().then((d) => (appData = d)).catch(() => {});

  type Key = keyof Locations;
  const fields: { key: Key; label: string; hint: string }[] = [
    { key: "gameDir", label: "RimWorld folder", hint: "Contains Version.txt and Data/ (on macOS, the RimWorldMac.app bundle)" },
    { key: "configDir", label: "Config folder", hint: "Contains ModsConfig.xml" },
    { key: "localModsDir", label: "Local mods folder", hint: "Usually <RimWorld>/Mods; SteamCMD downloads land here" },
    { key: "workshopDir", label: "Workshop folder", hint: "steamapps/workshop/content/294100" }
  ];
  async function choose(key: Key) {
    const p = await pickFolder(fields.find((f) => f.key === key)?.label);
    if (!p || !s) return;
    await store.updateSettings({ locations: { ...s.locations, [key]: p } });
    await store.rescan(false);
  }
  async function clear(key: Key) {
    if (!s) return;
    await store.updateSettings({ locations: { ...s.locations, [key]: null } });
    await store.rescan(false);
  }
  async function toggleSource(id: string) {
    if (!s) return;
    await store.updateSettings({ dbSources: s.dbSources.map((d) => (d.id === id ? { ...d, enabled: !d.enabled } : d)) });
  }
  const fetchedAgo = $derived.by(() => {
    const at = store.snap?.weightsFetchedAt ?? 0;
    if (!at) return "never";
    const h = Math.round((Date.now() / 1000 - at) / 3600);
    return h < 1 ? "just now" : h < 48 ? `${h} h ago` : `${Math.round(h / 24)} days ago`;
  });
</script>

<main class="settings">
  <div class="col">
    <section class="card">
      <h3>Where RimWorld lives</h3>
      {#each fields as f}
        <div class="loc">
          <div class="lt"><b>{f.label}</b><span>{f.hint}</span></div>
          <div class="lv">
            <span class="path mono" title={loc?.[f.key] ?? ""}>{loc?.[f.key] ?? "not found"}</span>
            <button class="btn sm" disabled={!inTauri} onclick={() => choose(f.key)}>Choose…</button>
            {#if s?.locations[f.key]}<button class="btn sm" onclick={() => clear(f.key)}>Auto</button>{/if}
          </div>
        </div>
      {/each}
      <div class="row">
        <button class="btn" onclick={() => store.rescan(true)}>Re-read every mod</button>
        <span class="hint">Drops the parse cache. Normal refreshes only re-read folders that changed.</span>
      </div>
    </section>

    <section class="card">
      <h3>Launching RimWorld <span class="aside">{launch ? (launch.steamInstall ? "Steam install" : "not a Steam install") : ""}</span></h3>
      <div class="opt">
        <span class="l">Play starts</span>
        <div class="seg">{#each methods as m}<button class:on={L?.method === m.id} title={m.hint} onclick={() => updateLaunch({ method: m.id })}>{m.label}</button>{/each}</div>
        {#if L?.method === "auto" && launch}<span class="hint">→ {launch.autoResolvesTo === "steam" ? "Steam" : "the executable"}</span>{/if}
      </div>
      <div class="loc">
        <div class="lt"><b>Executable</b><span>{L?.executable ? "chosen by you" : "detected from the RimWorld folder"}</span></div>
        <div class="lv">
          <span class="path mono" class:bad={launch && !launch.executableExists} title={launch?.executable ?? ""}>{launch?.executable ?? "not found — choose it"}</span>
          <button class="btn sm" disabled={!inTauri} onclick={chooseExe}>Choose…</button>
          {#if L?.executable}<button class="btn sm" onclick={() => updateLaunch({ executable: null })}>Auto</button>{/if}
        </div>
      </div>
      <div class="loc">
        <div class="lt"><b>Arguments</b><span>e.g. -popupwindow, -screen-width 1920</span></div>
        <div class="lv"><input class="input mono" value={L?.args ?? ""} placeholder="none" onchange={(e) => updateLaunch({ args: e.currentTarget.value })} /></div>
      </div>
      <label class="switch"><input type="checkbox" checked={L?.saveFirst ?? true} onchange={(e) => updateLaunch({ saveFirst: e.currentTarget.checked })} />Save ModsConfig.xml before starting when there are unsaved changes</label>
      <div class="row">
        <button class="btn primary" onclick={() => store.launch()}>{@html I.play}Play now</button>
        <span class="hint">Non-standard installs: set the RimWorld folder above (Version.txt and Data/ live in it), then choose the executable here if it isn't picked up.</span>
      </div>
    </section>

    <section class="card">
      <h3>SteamCMD <span class="aside">{q?.steamcmdInstalled ? "ready" : q?.installing ? "installing…" : store.downloadsError ? "unavailable" : "not installed"}</span></h3>
      <p class="hint">Valve's command-line Steam client. Circinus uses it to download Workshop mods without the Steam client (whole collections, missing mods from a list, fresh copies of broken ones) with an anonymous login and a throttle that backs off when Steam pushes back. Valve's terms don't allow shipping it, so Circinus fetches it once into its own data folder.</p>
      {#if store.downloadsError}
        <p class="hint bad">The download manager did not answer: <span class="mono">{store.downloadsError}</span></p>
        <div class="row"><button class="btn" onclick={() => store.refreshDownloads()}>{@html I.refresh}Try again</button></div>
      {:else if q?.steamcmdInstalled}
        <div class="loc">
          <div class="lt"><b>Installed at</b><span>steamcmd.exe / steamcmd.sh</span></div>
          <div class="lv"><span class="path mono" title={st?.exe ?? ""}>{st?.exe ?? "…"}</span>{#if st}<button class="btn sm" onclick={() => revealPath(st.root)}>Open</button>{/if}</div>
        </div>
        <div class="loc">
          <div class="lt"><b>Downloads go to</b><span>then move into your Mods folder</span></div>
          <div class="lv"><span class="path mono" title={st?.modsDir ?? ""}>{st?.modsDir ?? "no Mods folder — set the RimWorld folder above"}</span></div>
        </div>
        <div class="row">
          <button class="btn primary" onclick={() => (store.view = "downloads")}>{@html I.download}Open Downloads</button>
          <button class="btn" disabled={q.running || q.installing} onclick={() => store.testSteamCmd().then(() => (store.view = "downloads"))}>{@html I.terminal}Test SteamCMD</button>
          <button class="btn" disabled={q.running || q.installing} onclick={() => store.installSteamCmd()}>{@html I.refresh}Reinstall</button>
          <span class="hint">Batch size {q.throttle.batchSize}/25 · {store.queueCounts.queued} queued · {store.queueCounts.failed} failed</span>
        </div>
      {:else if q?.installing}
        <div class="row"><span class="hint">Installing — SteamCMD downloads itself and updates on first run. Watch the output in Downloads.</span><button class="btn" onclick={() => (store.view = "downloads")}>Open Downloads</button></div>
      {:else}
        <div class="row">
          <button class="btn primary" onclick={() => store.installSteamCmd()}>{@html I.download}Install SteamCMD</button>
          <span class="hint">About 5 MB, then a self-update. Nothing is sent to Steam beyond an anonymous login.</span>
        </div>
      {/if}
    </section>

    <section class="card">
      <h3>Rule and workshop databases</h3>
      {#each s?.dbSources ?? [] as d}
        <label class="switch dbrow"><input type="checkbox" checked={d.enabled} onchange={() => toggleSource(d.id)} /><span class="dbt"><b>{d.label}</b><span class="mono">{d.url}</span></span></label>
      {/each}
      <div class="row">
        <button class="btn primary" onclick={() => store.updateDatabases()}>Update now</button>
        <label class="switch"><input type="checkbox" checked={s?.updateDatabasesOnStart ?? false} onchange={(e) => store.updateSettings({ updateDatabasesOnStart: e.currentTarget.checked })} />Update when Circinus starts</label>
      </div>
      <p class="hint">Loaded: {store.snap?.dbLoaded.join(" · ") || "nothing yet — press Update now"}. The RimSort databases carry no licence, so Circinus fetches them on your request and never ships them.</p>
    </section>
  </div>

  <div class="col">
    <section class="card">
      <h3>Circinus weight</h3>
      <p class="hint">Per-mod frame-time share measured by the <button class="lnk" onclick={() => openUrl("https://circinus.sh")}>Circinus profiler</button>. Median across clean runs; ranked only after 25 runs from 10 installs. Shares dilute on long lists, so compare mods, not totals.</p>
      <label class="switch"><input type="checkbox" checked={s?.showWeight ?? false} onchange={(e) => store.updateSettings({ showWeight: e.currentTarget.checked })} />Show weight next to each mod</label>
      <label class="switch"><input type="checkbox" checked={s?.includeLocalRuns ?? true} onchange={(e) => store.updateSettings({ includeLocalRuns: e.currentTarget.checked })} />Include my own runs (Circinus/Runs beside the config folder)</label>
      <div class="row">
        <button class="btn" onclick={() => store.refreshWeights()}>Fetch from circinus.sh</button>
        <span class="hint">{Object.keys(store.snap?.weights ?? {}).length} mods with figures · fetched {fetchedAgo}</span>
      </div>
    </section>

    <section class="card">
      <h3>Ordering</h3>
      <label class="switch"><input type="checkbox" checked={s?.alphabeticalWithinPhase ?? false} onchange={(e) => store.updateSettings({ alphabeticalWithinPhase: e.currentTarget.checked })} />Alphabetical within a phase (otherwise HALO keeps your arrangement where rules allow)</label>
      <p class="hint">Rules are hard constraints. Your rules beat community rules, which beat a mod's own About.xml. A loop between equal rules is reported and the weakest link set aside rather than refusing to sort.</p>
    </section>

    <section class="card">
      <h3>About</h3>
      <p class="hint">Circinus Mod Manager 0.1 · MIT licence · data folder <span class="mono">{appData || "…"}</span></p>
      <p class="hint">Reads: About.xml (with ByVersion blocks), Fluffy's Manifest.xml, LoadFolders.xml, PublishedFileId.txt. Writes: ModsConfig.xml (keeps a .bak), your rules in dbs/userRules.json.</p>
      <button class="btn" onclick={() => (store.view = "order")}>Back to load order</button>
    </section>
  </div>
</main>

<style>
  .settings { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; padding: 0 14px 14px; min-height: 0; overflow: auto; align-content: start; }
  .col { display: flex; flex-direction: column; gap: 12px; min-width: 0; }
  .loc { display: grid; grid-template-columns: 200px minmax(0, 1fr); gap: 10px; align-items: center; padding: 8px 0; border-bottom: 1px solid rgba(255, 255, 255, 0.05); }
  .loc:last-of-type { border-bottom: 0; }
  .lt b { display: block; font-size: 13px; }
  .lt span { display: block; font-size: 11.5px; color: var(--text-3); }
  .lv { display: flex; gap: 6px; align-items: center; min-width: 0; }
  .path { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-2); background: var(--surface-2); padding: 5px 8px; border-radius: 7px; user-select: text; }
  .row { display: flex; align-items: center; gap: 12px; margin-top: 12px; flex-wrap: wrap; }
  .hint { color: var(--text-3); font-size: 12.5px; line-height: 1.5; margin: 8px 0 0; }
  .dbrow { display: flex; align-items: center; gap: 12px; padding: 6px 0; font-weight: 500; }
  .dbt { display: flex; flex-direction: column; min-width: 0; }
  .dbt b { font-size: 13px; font-weight: 600; }
  .dbt .mono { color: var(--text-3); font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .switch { margin: 6px 0; align-items: center; display: flex; }
  .lnk { color: var(--blue); font-weight: 600; }
  .hint.bad { color: var(--red); }
  .path.bad { color: var(--red); }
  .opt { display: flex; align-items: center; gap: 10px; margin: 6px 0 10px; font-size: 13px; }
  .opt .l { width: 100px; color: var(--text-2); }
  .opt .hint { margin: 0; }
  .seg { display: inline-flex; background: var(--surface-2); border-radius: 9px; padding: 3px; gap: 2px; }
  .seg button { padding: 5px 10px; border-radius: 7px; font-size: 12.5px; font-weight: 600; color: var(--text-2); }
  .seg button.on { background: var(--surface-4); color: var(--text); }
  .lv .input { height: 30px; font-size: 12.5px; flex: 1; }
  @media (max-width: 980px) { .settings { grid-template-columns: 1fr; } }
</style>
