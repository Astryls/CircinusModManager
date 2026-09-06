<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { api, pickFile, pickFolder, inTauri, openUrl, revealPath } from "$lib/api";
  import { I } from "$lib/icons";
  import { DISCORD, type LaunchInfo, type LaunchMethod, type Locations } from "$lib/types";

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
    { id: "auto", label: "Auto", hint: "Through Steam when the game lives in a Steam library, otherwise from the executable" },
    { id: "steam", label: "Steam", hint: "Asks Steam to start the game, so the overlay and Workshop updates work as usual" },
    { id: "executable", label: "Executable", hint: "Starts the game file directly: GOG, DRM-free, or a copy outside Steam" }
  ];
  let appData = $state("");
  api.appDataDir().then((d) => (appData = d)).catch(() => {});

  type Key = keyof Locations;
  const fields: { key: Key; label: string; hint: string }[] = [
    { key: "gameDir", label: "RimWorld folder", hint: "Has Version.txt and Data in it. On macOS pick the folder RimWorldMac.app sits in - the Finder will not let you choose the bundle itself, and Circinus looks inside it" },
    { key: "configDir", label: "Config folder", hint: "Has ModsConfig.xml in it" },
    { key: "localModsDir", label: "Local mods folder", hint: "Usually RimWorld/Mods. SteamCMD downloads go here" },
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
  const linked = $derived((store.snap?.mods ?? []).filter((m) => m.linkTarget).length);
  const unreadable = $derived(store.snap?.unreadable ?? []);
  const withNumber = $derived(Object.values(store.snap?.weights ?? {}).filter((w) => w.share != null).length);
  const fetchedAgo = $derived.by(() => {
    const at = store.snap?.weightsFetchedAt ?? 0;
    if (!at) return "never";
    const h = Math.round((Date.now() / 1000 - at) / 3600);
    return h < 1 ? "just now" : h < 48 ? `${h} h ago` : `${Math.round(h / 24)} days ago`;
  });
</script>

<main class="scroll">
  <div class="settings">
    <section class="card">
      <h3>Where RimWorld lives <span class="aside">{store.instance?.name ?? ""}</span></h3>
      <p class="hint">These are the folders of the instance you have open{store.instance ? `, ${store.instance.name}` : ""}. Changing one changes that instance, not the others. <button class="lnk" onclick={() => (store.showInstances = true)}>All instances…</button></p>
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
        <button class="btn" onclick={() => store.rescan(true)}>{@html I.refresh}Read every mod again</button>
        <span class="hint">Forgets what it knows and reads every folder. A normal Refresh only reads folders that changed.</span>
      </div>
      {#if linked}<p class="hint">{linked} {linked === 1 ? "entry is a link" : "entries are links"} to a folder kept elsewhere (Modmixer, a dev tool, or a link you made). Circinus reads them the way RimWorld does; the mod's details say where the files are.</p>{/if}
      {#if unreadable.length}
        <div class="unread">
          <b>{unreadable.length} {unreadable.length === 1 ? "entry" : "entries"} left out</b>
          <span class="hint">RimWorld cannot load these either. A link whose target was moved or deleted is the usual cause.</span>
          {#each unreadable.slice(0, 20) as u}<div class="ur"><span class="mono" title={u.path}>{u.path.split(/[\\/]/).pop()}</span><span>{u.reason}</span></div>{/each}
          {#if unreadable.length > 20}<span class="hint">and {unreadable.length - 20} more</span>{/if}
        </div>
      {/if}
    </section>

    <section class="card">
      <h3>Launching RimWorld <span class="aside">{launch ? (launch.steamInstall ? "Steam install" : "not a Steam install") : ""}</span></h3>
      <div class="opt">
        <span class="l">Play starts</span>
        <div class="seg">{#each methods as m}<button class:on={L?.method === m.id} title={m.hint} onclick={() => updateLaunch({ method: m.id })}>{m.label}</button>{/each}</div>
        {#if L?.method === "auto" && launch}<span class="hint">right now: {launch.autoResolvesTo === "steam" ? "Steam" : "the executable"}</span>{/if}
      </div>
      <div class="loc">
        <div class="lt"><b>Executable</b><span>{L?.executable ? "chosen by you" : "detected from the RimWorld folder"}</span></div>
        <div class="lv">
          <span class="path mono" class:bad={launch && !launch.executableExists} title={launch?.executable ?? ""}>{launch?.executable ?? "not found, choose it"}</span>
          <button class="btn sm" disabled={!inTauri} onclick={chooseExe}>Choose…</button>
          {#if L?.executable}<button class="btn sm" onclick={() => updateLaunch({ executable: null })}>Auto</button>{/if}
        </div>
      </div>
      <div class="loc">
        <div class="lt"><b>Extra arguments</b><span>For example -popupwindow or -screen-width 1920</span></div>
        <div class="lv"><input class="input mono" value={L?.args ?? ""} placeholder="none" onchange={(e) => updateLaunch({ args: e.currentTarget.value })} /></div>
      </div>
      <label class="switch"><input type="checkbox" checked={L?.saveFirst ?? true} onchange={(e) => updateLaunch({ saveFirst: e.currentTarget.checked })} />Save ModsConfig.xml first when there are unsaved changes</label>
      {#if launch?.saveDataFolder}
        <p class="hint">This instance keeps its config and saves in <span class="mono">{launch.saveDataFolder}</span>, so Play starts the game with <span class="mono">-savedatafolder</span>. Without it the game would use the usual folder and rewrite the wrong ModsConfig.xml.</p>
      {/if}
      <div class="row">
        <button class="btn primary" onclick={() => store.launch()}>{@html I.play}Play now</button>
        <span class="hint">Game installed somewhere unusual? Set the RimWorld folder above, then choose the executable here if it is not found.</span>
      </div>
    </section>
  

    <section class="card">
      <h3>SteamCMD <span class="aside">{q?.steamcmdInstalled ? "ready" : q?.installing ? "installing" : store.downloadsError ? "not available" : "not installed"}</span></h3>
      <p class="hint">Valve's command line Steam tool. Circinus uses it to download Workshop mods without the Steam client: whole collections, missing mods from a list, fresh copies of broken ones. It logs in anonymously and slows down when Steam pushes back. Valve does not allow it to be bundled, so Circinus fetches it once into its own data folder.</p>
      {#if store.downloadsError}
        <p class="hint bad">The download manager did not answer: <span class="mono">{store.downloadsError}</span></p>
        <div class="row"><button class="btn" onclick={() => store.refreshDownloads()}>{@html I.refresh}Try again</button></div>
      {:else if q?.steamcmdInstalled}
        <div class="loc">
          <div class="lt"><b>Installed at</b><span>steamcmd.exe / steamcmd.sh</span></div>
          <div class="lv"><span class="path mono" title={st?.exe ?? ""}>{st?.exe ?? "…"}</span>{#if st}<button class="btn sm" onclick={() => revealPath(st.root)}>Open</button>{/if}</div>
        </div>
        <div class="loc">
          <div class="lt"><b>Downloads go to</b><span>they move into your Mods folder when done</span></div>
          <div class="lv"><span class="path mono" title={st?.modsDir ?? ""}>{st?.modsDir ?? "no Mods folder, set the RimWorld folder first"}</span></div>
        </div>
        <div class="row">
          <button class="btn primary" onclick={() => (store.view = "downloads")}>{@html I.download}Open Downloads</button>
          <button class="btn" disabled={q.running || q.installing} onclick={() => store.testSteamCmd().then(() => (store.view = "downloads"))}>{@html I.terminal}Test SteamCMD</button>
          <button class="btn" disabled={q.running || q.installing} onclick={() => store.installSteamCmd()}>{@html I.refresh}Reinstall</button>
          <span class="hint">Batch size {q.throttle.batchSize} of 25 · {store.queueCounts.queued} queued · {store.queueCounts.failed} failed</span>
        </div>
      {:else if q?.installing}
        <div class="row"><span class="hint">Installing. SteamCMD downloads itself and updates on first run. Watch the output in Downloads.</span><button class="btn" onclick={() => (store.view = "downloads")}>Open Downloads</button></div>
      {:else}
        <div class="row">
          <button class="btn primary" onclick={() => store.installSteamCmd()}>{@html I.download}Install SteamCMD</button>
          <span class="hint">About 5 MB, then it updates itself. It only logs in to Steam anonymously.</span>
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
      <p class="hint">{store.snap?.dbLoaded.length ? `Loaded: ${store.snap.dbLoaded.join(", ")}.` : "Nothing loaded yet. Press Update now."} The RimSort databases have no licence, so Circinus fetches them when you ask and never bundles them.</p>
    </section>

    <section class="card">
      <h3>Performance figures</h3>
      <p class="hint">How much frame time each mod costs, measured by the <button class="lnk" onclick={() => openUrl("https://circinus.sh")}>Circinus profiler</button>. Shown as a share of frame time, the median across clean runs. A mod is ranked after 25 runs from 10 players. Long lists spread the total thin, so compare mods rather than adding them up.</p>
      <label class="switch"><input type="checkbox" checked={s?.showWeight ?? false} onchange={(e) => store.updateSettings({ showWeight: e.currentTarget.checked })} />Show the figure next to each mod</label>
      <label class="switch"><input type="checkbox" checked={s?.includeLocalRuns ?? true} onchange={(e) => store.updateSettings({ includeLocalRuns: e.currentTarget.checked })} />Include my own runs (the Circinus/Runs folder next to the config folder)</label>
      <div class="row">
        <button class="btn" onclick={() => store.refreshWeights()}>{@html I.gauge}Fetch from circinus.sh</button>
        <span class="hint">{Object.keys(store.snap?.weights ?? {}).length.toLocaleString()} mods have figures ({withNumber.toLocaleString()} with a number) · fetched {fetchedAgo}</span>
      </div>
      {#if store.snap?.weightsSample}
        <details class="sample">
          <summary>One record as circinus.sh sends it</summary>
          <pre class="mono">{store.snap.weightsSample}</pre>
        </details>
      {/if}
    </section>

    <section class="card">
      <h3>Sorting</h3>
      <label class="switch"><input type="checkbox" checked={s?.alphabeticalWithinPhase ?? false} onchange={(e) => store.updateSettings({ alphabeticalWithinPhase: e.currentTarget.checked })} />Sort alphabetically inside each phase</label>
      <p class="hint">Off: HALO keeps your own arrangement wherever the rules allow. Rules always win. Your rules beat community rules, which beat a mod's own About.xml. When rules form a loop, the weakest one is set aside and reported.</p>
    </section>

    <section class="card updates">
      <h3>Updates <span class="aside">Circinus {store.appVersion ?? ""}</span></h3>
      <label class="switch"><input type="checkbox" checked={s?.checkForUpdates ?? true} onchange={(e) => store.updateSettings({ checkForUpdates: e.currentTarget.checked })} />Check for a newer Circinus when it starts</label>
      <p class="hint">Asks circinus.sh once, a few seconds after launch, and only tells you when there is one. Nothing is installed without you pressing the button.</p>
      <div class="row">
        <button class="btn" disabled={store.updateChecking || !!store.updateProgress} onclick={() => store.checkForUpdates()}>{@html I.refresh}{store.updateChecking ? "Checking…" : "Check now"}</button>
        {#if store.update && !store.updateProgress}
          <button class="btn primary" onclick={() => { store.installUpdate(); store.view = "order"; }}>{@html I.up}Install {store.update.version} and restart</button>
        {/if}
        <span class="hint result" class:bad={store.updateStatus?.kind === "err"}>{store.updateStatus?.text ?? (store.appVersion ? `Circinus ${store.appVersion}. Not checked yet this launch.` : "")}</span>
      </div>
      {#if store.update?.notes}<p class="hint notes">{store.update.notes}</p>{/if}
    </section>

    <section class="card">
      <h3>Help and about</h3>
      <p class="hint">Something wrong, or a mod sorted somewhere odd? The Discord is where to say so — bring the mod's name and what you expected instead.</p>
      <div class="row">
        <button class="btn" onclick={() => openUrl(DISCORD)}>{@html I.link}Circinus on Discord</button>
        <button class="btn" onclick={() => openUrl("https://circinus.sh")}>{@html I.cloud}circinus.sh</button>
      </div>
      <p class="hint">Circinus Mod Manager {store.appVersion ?? ""} · MIT licence · data folder <span class="mono">{appData || "loading"}</span></p>
      <p class="hint">Reads About.xml (with ByVersion blocks), Fluffy's Manifest.xml, LoadFolders.xml and PublishedFileId.txt. Writes ModsConfig.xml (and keeps a .bak) and your rules in dbs/userRules.json.</p>
    </section>
  </div>
</main>

<style>
  /* The page scrolls up and down only, never sideways. The multi-column box inside grows as tall
     as it needs (it must not be height-limited: a limited multi-column box spills extra columns
     out to the right), and balances the cards so the columns end at about the same height. */
  .scroll { min-height: 0; height: 100%; overflow: hidden auto; }
  .settings { columns: 2; column-gap: 14px; padding: 0 14px 14px; }
  .settings > .card { break-inside: avoid; margin-bottom: 12px; display: block; min-width: 0; }
  @media (min-width: 1600px) { .settings { columns: 3; } }
  .loc { display: grid; grid-template-columns: 170px minmax(0, 1fr); gap: 10px; align-items: center; padding: 7px 0; border-bottom: 1px solid rgba(255, 255, 255, 0.05); }
  .loc:last-of-type { border-bottom: 0; }
  .lt b { display: block; font-size: 13px; }
  .lt span { display: block; font-size: 11.5px; color: var(--text-3); overflow-wrap: anywhere; }
  .lv { display: flex; gap: 6px; align-items: center; min-width: 0; }
  .path { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-2); background: var(--surface-2); padding: 5px 8px; border-radius: 7px; user-select: text; }
  .row { display: flex; align-items: center; gap: 12px; margin-top: 12px; flex-wrap: wrap; }
  .unread { margin-top: 10px; padding: 10px 12px; border-radius: 10px; background: var(--surface-2); display: flex; flex-direction: column; gap: 4px; font-size: 12.5px; }
  .unread b { color: var(--amber); }
  .unread .hint { margin: 0 0 4px; }
  .unread .ur { display: grid; grid-template-columns: minmax(0, 220px) minmax(0, 1fr); gap: 10px; color: var(--text-2); overflow-wrap: anywhere; }
  .unread .ur .mono { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .hint { color: var(--text-3); font-size: 12.5px; line-height: 1.45; margin: 6px 0 0; }
  .dbrow { display: flex; align-items: center; gap: 12px; padding: 6px 0; font-weight: 500; }
  .dbt { display: flex; flex-direction: column; min-width: 0; }
  .dbt b { font-size: 13px; font-weight: 600; }
  .dbt .mono { color: var(--text-3); font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .switch { margin: 6px 0; align-items: center; display: flex; }
  .lnk { color: var(--blue); font-weight: 600; }
  .hint.bad { color: var(--red); }
  .updates .result { margin: 0; flex: 1 1 200px; min-width: 0; overflow-wrap: anywhere; }
  .updates .notes { white-space: pre-wrap; }
  .sample { margin-top: 8px; font-size: 12.5px; color: var(--text-3); }
  .sample summary { cursor: pointer; font-weight: 600; }
  .sample pre { margin: 6px 0 0; max-height: 240px; overflow: hidden auto; background: var(--surface-2); border-radius: 8px; padding: 8px 10px; font-size: 11px; line-height: 1.4; user-select: text; white-space: pre-wrap; word-break: break-all; }
  .path.bad { color: var(--red); }
  .opt { display: flex; align-items: center; gap: 10px; margin: 6px 0 10px; font-size: 13px; }
  .opt .l { width: 100px; color: var(--text-2); }
  .opt .hint { margin: 0; }
  .seg { display: inline-flex; background: var(--surface-2); border-radius: 9px; padding: 3px; gap: 2px; }
  .seg button { padding: 5px 10px; border-radius: 7px; font-size: 12.5px; font-weight: 600; color: var(--text-2); }
  .seg button.on { background: var(--surface-4); color: var(--text); }
  .lv .input { height: 30px; font-size: 12.5px; flex: 1; }
  @media (max-width: 980px) { .settings { columns: 1; } }
</style>
