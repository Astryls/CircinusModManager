<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { t } from "$lib/i18n.svelte";
  import { api, pickFile, pickFolder, inTauri, openUrl } from "$lib/api";
  import { I } from "$lib/icons";
  import { theme } from "$lib/theme.svelte";
  import { palette, GROUPS, contrast } from "$lib/palette.svelte";
  import { chrome } from "$lib/chrome.svelte";
  import { SORTS, sortLabel, VIEWS, type SortKey, type View } from "$lib/store.svelte";

  /** The optional list columns, in the order they appear in the list. Typical and Yours are
   *  absent on purpose: both follow `showWeight`, because both need figures Circinus only has
   *  once they are fetched. */
  const LIST_COLUMNS = [
    { key: "startup", label: "Start-up", hint: "Seconds this mod added to the last start-up, measured by the Loading Progress mod. Never estimated" },
    { key: "loadmedian", label: "Median", hint: "What the mod typically adds to a start-up on everyone else's machine, from circinus.sh" },
    { key: "versions", label: "Version", hint: "The newest game version the mod says it supports, marked when that is behind the game installed here" },
    { key: "phase", label: "Phase", hint: "Where HALO files the mod" },
    { key: "group", label: "Group", hint: "The group the mod is in" }
  ];
  /** Settings is left out: opening on the settings page would be a window that never shows you
   *  your mods, and anyone who set it would have to come back here to undo it. */
  const VIEW_LABEL: Record<View, string> = {
    order: "Load order",
    library: "Library",
    downloads: "Downloads",
    textures: "Textures",
    patches: "Patches",
    analyzer: "Analyzer",
    loadtimes: "Load times",
    defs: "Defs",
    halo: "HALO",
    settings: "Settings"
  };
  const VIEW_CHOICES = VIEWS.filter((v) => v !== "settings").map((v) => ({ key: v, label: VIEW_LABEL[v] }));
  import { DISCORD, type LaunchInfo, type LaunchMethod, type Locations } from "$lib/types";

  const s = $derived(store.snap?.settings);
  /** Same shape the summary card and the load times page use, so the three never disagree
   *  about how long the last start-up took. "463.0s" is a number you have to convert first. */
  const clock = (secs: number) => (secs >= 60 ? `${Math.floor(secs / 60)}m ${Math.round(secs % 60)}s` : `${Math.round(secs)}s`);
  const loc = $derived(store.snap?.locations);
  const q = $derived(store.downloads);
  const st = $derived(store.steamcmd);
  store.refreshDownloads();
  /** Said out loud when a colour is refused, because a picker that silently snaps back looks
   *  broken rather than careful. */
  let badTry = $state<string | null>(null);

  /** `<input type="color">` only speaks `#rrggbb`. A token is usually already one; anything
   *  else is resolved through the browser so the picker opens on the colour actually showing
   *  rather than on black. */
  function toHex(v: string): string {
    const t = v.trim();
    if (/^#[0-9a-fA-F]{6}$/.test(t)) return t;
    if (/^#[0-9a-fA-F]{3}$/.test(t)) return "#" + [...t.slice(1)].map((c) => c + c).join("");
    try {
      const probe = document.createElement("span");
      probe.style.color = t;
      document.body.appendChild(probe);
      const m = getComputedStyle(probe).color.match(/\d+/g);
      probe.remove();
      if (m && m.length >= 3) return "#" + m.slice(0, 3).map((x) => Number(x).toString(16).padStart(2, "0")).join("");
    } catch {
      /* fall through */
    }
    return "#808080";
  }

  function pick(key: string, value: string) {
    if (palette.set(key, value)) {
      badTry = null;
      return;
    }
    // The only reason `set` refuses a colour the picker produced is the legibility floor.
    const bg = getComputedStyle(document.documentElement).getPropertyValue("--bg").trim();
    const c = contrast(value, bg);
    badTry = `That colour was not applied: it would leave the window too hard to read${c != null ? ` (about ${c.toFixed(1)}:1 against the background, and 3:1 is the floor)` : ""}. Everything else is unchanged.`;
  }

  let diagCopied = $state(false);
  let diagFailed = $state<string | null>(null);
  async function copyDiagnostics() {
    diagFailed = null;
    try {
      await navigator.clipboard.writeText(await api.diagnostics());
      diagCopied = true;
      setTimeout(() => (diagCopied = false), 4000);
    } catch (e) {
      diagFailed = e instanceof Error ? e.message : String(e);
    }
  }
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
  // "Nothing loaded yet. Press Update now." was said whether nothing had been fetched or
  // everything was deliberately off, which reads as a thing gone wrong rather than a choice.
  const dbState = $derived.by(() => {
    const loaded = store.snap?.dbLoaded ?? [];
    if (loaded.length) return `Loaded: ${loaded.join(", ")}.`;
    const on = (s?.dbSources ?? []).filter((d) => d.enabled);
    if (!on.length) return "All off, so nothing here is affecting your load order.";
    return `${on.length === 1 ? on[0].label : `${on.length} switched on`}, but nothing downloaded yet. Press Update now.`;
  });
  const linked = $derived((store.snap?.mods ?? []).filter((m) => m.linkTarget).length);
  const unreadable = $derived(store.snap?.unreadable ?? []);
  const withNumber = $derived(Object.values(store.snap?.weights ?? {}).filter((w) => w.share != null).length);
  /** The other half of the same fetch: mods somebody has shared a start-up for. Counted apart
   *  from `withNumber` because they are different measurements and one can arrive without the
   *  other -- the site has served frame shares for months and start-up medians since this week. */
  const withLoad = $derived(Object.values(store.snap?.weights ?? {}).filter((w) => w.loadMsMedian != null).length);
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
          <div class="lv"><span class="path mono" title={st?.exe ?? ""}>{st?.exe ?? "…"}</span>{#if st}<button class="btn sm" onclick={() => store.openFolder(st.root)}>Open</button>{/if}</div>
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
      <p class="hint">{dbState} Switching one off deletes the copy Circinus downloaded and stops using its rules straight away. The RimSort databases have no licence, so Circinus fetches them when you ask and never bundles them.</p>
      <p class="hint">
        These are other people's collections of what should load before what. They are useful, and often right, but a rule from one of them outranks what the mod's own
        author wrote in About.xml — so they start off, and turning one on is your call to make.
      </p>
    </section>

    <section class="card">
      <h3>Sharing <span class="aside">{store.sharing?.shareLoadRuns ? "on" : "off"}</span></h3>
      <label class="switch"><input type="checkbox" checked={store.sharing?.shareLoadRuns ?? false} onchange={(e) => store.setSharing(e.currentTarget.checked)} />{t("sharing.settings.on")}</label>
      <p class="hint">{store.sharing?.shareLoadRuns ? t("sharing.fine") : t("sharing.settings.off")}</p>
      <button class="btn sm" onclick={() => (store.showConsent = true)}>{t("sharing.settings.review")}</button>

      <!-- The id is shown whether sharing is on or off, because it is what a player needs in
           order to ask for a deletion, and needing it is not conditional on still sharing. -->
      {#if store.sharing?.installId}
        <div class="fld idrow">
          <label for="installid">{t("sharing.settings.id")}</label>
          <input id="installid" class="input mono" readonly value={store.sharing.installId} onclick={(e) => e.currentTarget.select()} />
          <p class="hint">{t("sharing.settings.idhint")}</p>
          <p class="hint">{t("sharing.settings.separate")}</p>
          <button class="btn sm" onclick={() => store.rotateInstallId()}>{t("sharing.settings.rotate")}</button>
          <p class="hint">{t("sharing.settings.rotatehint")}</p>
        </div>
      {/if}
    </section>

    <section class="card">
      <h3>Performance figures</h3>
      <p class="hint">How much frame time each mod costs, measured by the <button class="lnk" onclick={() => openUrl("https://circinus.sh")}>Circinus profiler</button>. Shown as a share of frame time, the median across clean runs. A mod is ranked after 25 runs from 10 players. Long lists spread the total thin, so compare mods rather than adding them up.</p>
      <p class="hint">Two columns, because they are two measurements of the same thing in two places. <b>Typical</b> is what the mod costs everybody else, pooled here. <b>Yours</b> is what it costs on this machine, from your own profiler runs. Neither is ever filled in from the other, so a gap between them is real and is about this install.</p>
      <label class="switch"><input type="checkbox" checked={s?.showWeight ?? false} onchange={(e) => store.updateSettings({ showWeight: e.currentTarget.checked })} />Show Typical and Yours next to each mod</label>
      <label class="switch"><input type="checkbox" checked={s?.includeLocalRuns ?? true} onchange={(e) => store.updateSettings({ includeLocalRuns: e.currentTarget.checked })} />Read my own runs, for the Yours column (the Circinus/Runs folder next to the config folder)</label>
      <label class="switch"><input type="checkbox" checked={s?.refreshWeightsOnSort ?? true} onchange={(e) => store.updateSettings({ refreshWeightsOnSort: e.currentTarget.checked })} />Fetch fresh figures before Sort with HALO</label>
      <p class="hint">This updates what the columns say. It does not change the order: HALO sorts from rules, the mod authors&rsquo; own declarations and your overrides, and never reads these figures. The lists that <em>can</em> change the order are the rule databases above.</p>
      <div class="row">
        <button class="btn" onclick={() => store.refreshWeights()}>{@html I.gauge}Fetch from circinus.sh</button>
        <span class="hint">{Object.keys(store.snap?.weights ?? {}).length.toLocaleString()} mods have figures ({withNumber.toLocaleString()} with a frame share, {withLoad.toLocaleString()} timed at start-up) · fetched {fetchedAgo}</span>
      </div>
      {#if store.snap?.weightsSample}
        <details class="sample">
          <summary>One record as circinus.sh sends it</summary>
          <pre class="mono">{store.snap.weightsSample}</pre>
        </details>
      {/if}
    </section>

    <!-- Start-up timing. Separate from Performance figures on purpose: these are two different
         measurements of two different things, and a player who reads them as one number will
         add a frame share to a start-up cost. -->
    <section class="card">
      <h3>Start-up times <span class="aside">{store.startupImpact ? `${clock(store.startupImpact.totalMs / 1000)} last read` : "nothing measured"}</span></h3>
      <p class="hint">
        How long each mod took while the game loaded, measured by the
        <button class="lnk" onclick={() => openUrl("https://steamcommunity.com/sharedfiles/filedetails/?id=3535481557")}>Loading Progress</button>
        mod by ilyvion. Circinus times nothing itself: it reads the report that mod writes.
        <b>Both of that mod's tracking settings are off out of the box</b> — turn on Track startup
        loading impact, then Auto-save startup impact report, and start the game once.
      </p>
      <div class="row">
        <button class="btn" onclick={() => store.rereadLoadRun()} disabled={!!store.busy}>{@html I.refresh}Re-read now</button>
        <span class="hint">
          {#if store.startupImpact}
            {store.startupImpact.mods.length.toLocaleString()} mods timed, {store.measuredCount.toLocaleString()} of them in your list now
          {:else}
            No report found beside Player.log
          {/if}
        </span>
      </div>
      <p class="hint">Circinus already re-reads this when it starts and whenever you come back to the window, so this button is for the launch you did before switching the mod's settings on.</p>
      <button class="btn sm" onclick={() => (store.view = "loadtimes")}>Open Load times</button>
    </section>

    <section class="card">
      <h3>Appearance <span class="aside">{theme.paper === "light" ? "paper" : theme.darkVariant === "oled" ? "OLED black" : "dark"}</span></h3>
      <label class="switch"><input type="checkbox" checked={theme.paper === "light"} onchange={(e) => theme.set(e.currentTarget.checked ? "light" : "dark")} />Use the light paper</label>
      <label class="switch"><input type="checkbox" checked={theme.darkVariant === "oled"} onchange={(e) => theme.setDark(e.currentTarget.checked ? "oled" : "normal")} />Use OLED black instead of the normal dark</label>
      <p class="hint">
        OLED black takes the ground to pure black, so the pixels behind it are switched off rather
        than driven dim. Everything else is the same paper: the ink stays bone rather than going
        white, because white on black is what makes text smear when an OLED panel scrolls. Only
        the separating rules are lifted, since a hairline that reads over near-black disappears
        over black.
      </p>
      <p class="hint">This applies to the dark paper only, and takes effect the moment you switch back to it. The theme is remembered on this machine and never travels with a mod list.</p>
      {#if chrome.available}
        <label class="switch"><input type="checkbox" checked={chrome.on} onchange={(e) => chrome.set(e.currentTarget.checked)} />Draw the window buttons in the window</label>
        <p class="hint">
          Puts minimise, maximise and close at the end of the search line, in these colours, instead of Windows drawing a caption bar above them in its own. Dragging,
          snapping and Win+Arrow all still work. <b>Snap Layouts do not</b> &mdash; the flyout when you hover the maximise button needs the system frame, so leave this off if
          you use it. The change happens the moment you click, both ways, so nothing here can leave you stuck with a window you cannot work.
        </p>
      {/if}
    </section>

    <!-- Every colour, for anyone who wants different ones.
         The card is built around getting back out of it: see the Reset button's own comment,
         and `lib/palette.svelte.ts` for why an override is a removable inline property rather
         than a written-down default. -->
    <section class="card">
      <h3>Colours <span class="aside">{palette.changed ? `${palette.changed} changed` : "as shipped"}</span></h3>
      <p class="hint">
        Each paper keeps its own set, so a colour chosen here applies to the {theme.paper === "light" ? "light paper" : theme.darkVariant === "oled" ? "OLED black" : "dark paper"} and switching papers
        swaps the whole set. Everything is remembered on this machine only and never travels with a mod list.
      </p>
      {#if palette.refused}
        <p class="hint warnline">
          The colours saved for this paper left the text unreadable against its background, so they were not applied and the shipped ones are showing. Nothing was lost: change one and it is saved again.
        </p>
      {/if}
      {#if badTry}
        <p class="hint warnline">{badTry}</p>
      {/if}

      {#each GROUPS as g (g.title)}
        <div class="pal">
          <div class="palhead"><b>{g.title}</b><span>{g.note}</span></div>
          <div class="swatches">
            {#each g.slots as slot (slot.key)}
              {@const set = palette.current[slot.key]}
              <label class="sw" title={slot.hint}>
                <input
                  type="color"
                  value={toHex(set ?? palette.shipped(slot.key))}
                  oninput={(e) => pick(slot.key, e.currentTarget.value)}
                  aria-label={slot.label}
                />
                <span class="nm">{slot.label}</span>
                {#if set}<button class="undo" title="Put {slot.label} back to the shipped colour" onclick={() => palette.clear(slot.key)}>&times;</button>{/if}
              </label>
            {/each}
          </div>
        </div>
      {/each}

      <!-- RESET.
           **This button paints itself with literal colours, and that is on purpose.** The
           house rule is that a literal hex in a component is a bug, because a colour that only
           works on one paper is exactly what the token system exists to prevent. This is the
           one place the rule has to be broken: the button's job is to be found and pressed
           when the tokens have been made unusable, and a button drawn in `--surface` and
           `--text` is invisible precisely when it is needed. The pair below is fixed, carries
           its own border, and reads on any ground a player can produce.
           Do not "fix" this by giving it tokens. -->
      <div class="resetrow">
        <button class="hardreset" onclick={() => palette.reset()}>Reset these colours</button>
        <button class="hardreset ghost" onclick={() => palette.resetAll()}>Reset every paper</button>
        <span class="hint">Puts the shipped colours back. It writes nothing and reads nothing you have set, so it works even if the window has become unreadable.</span>
      </div>
    </section>

    <!-- The list itself: what it shows, and where it starts. Kept apart from Sorting below,
         which is about how HALO arranges the order rather than about this window. -->
    <section class="card">
      <h3>The list <span class="aside">{store.listColumns.length} optional column{store.listColumns.length === 1 ? "" : "s"}</span></h3>

      <p class="hint">Columns beyond the mod name and its package id. A narrow window drops them from the right rather than squeezing the names past reading, so the order here is the order they go.</p>
      <div class="chips">
        {#each LIST_COLUMNS as c}
          <button class="chip" class:on={store.listColumns.includes(c.key)} title={c.hint} onclick={() => store.setListColumn(c.key, !store.listColumns.includes(c.key))}>{c.label}</button>
        {/each}
      </div>
      <p class="hint">Cost is not in here: it follows <b>Show the figure next to each mod</b> under Performance figures, because it needs figures Circinus only has once they are fetched.</p>

      <div class="fld">
        <label for="defview">Open on</label>
        <select id="defview" class="input" value={s?.defaultView || "order"} onchange={(e) => store.updateSettings({ defaultView: e.currentTarget.value })}>
          {#each VIEW_CHOICES as v}<option value={v.key}>{v.label}</option>{/each}
        </select>
        <p class="hint">Which page Circinus shows when it starts. Applied once, when the window opens, so it is a starting position rather than a page you keep being sent back to.</p>
      </div>

      <div class="fld">
        <label for="defsort">Open sorted by</label>
        <div class="row">
          <select id="defsort" class="input" value={s?.defaultSort || "order"} onchange={(e) => store.updateSettings({ defaultSort: e.currentTarget.value })}>
            {#each SORTS as k}<option value={k}>{sortLabel(k)}</option>{/each}
          </select>
          {#if (s?.defaultSort || "order") !== "order"}
            <select class="input dir" aria-label="Sort direction" value={s?.defaultSortAsc === false ? "desc" : "asc"} onchange={(e) => store.updateSettings({ defaultSortAsc: e.currentTarget.value === "asc" })}>
              <option value="asc">Smallest or A first</option>
              <option value="desc">Largest or Z first</option>
            </select>
          {/if}
        </div>
        <p class="hint">
          {#if (s?.defaultSort || "order") === "order"}
            The load order is the only ordering that is true of anything outside this window, which is why it is the default. Dragging is refused while the list is sorted by anything else, since a drop between two rows of an alphabetical list writes a position nobody chose.
          {:else}
            Circinus will open sorted by {sortLabel((s?.defaultSort ?? "order") as SortKey).toLowerCase()}. Dragging is refused while a list is sorted, so clear the sort from the column heading before rearranging.
          {/if}
        </p>
      </div>

      <div class="fld">
        <label for="dismissed">Notices you have put down</label>
        <p class="hint" id="dismissed">
          Each row carries one mark for the most serious thing it has to say, and clicking it shows the rest and offers to put any of them down. A dismissal only hides the mark: the Analyzer still lists every issue, because it is the complete index.
        </p>
        <div class="row">
          <button class="btn sm" disabled={!store.dismissedNotices.size} onclick={() => store.clearDismissedNotices()}>Bring them all back</button>
          <span class="hint">{store.dismissedNotices.size ? `${store.dismissedNotices.size} put down` : "Nothing put down"}</span>
        </div>
      </div>
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
        <button class="btn" onclick={copyDiagnostics}>{diagCopied ? "Copied" : "Copy diagnostics"}</button>
        {#if store.hiddenWarnings}
          <button class="btn" onclick={() => store.clearHiddenWarnings()}>Show {store.hiddenWarnings} hidden {store.hiddenWarnings === 1 ? "warning" : "warnings"}</button>
        {/if}
      </div>
      <p class="hint">
        Diagnostics are what turns "it went wrong" into something anyone can act on: the version, your folders and whether they are there, what the scan found, and
        the end of the log. Your user folder is written as <span class="mono">~</span>, so your name does not go with it. Read it before you paste it if you like -
        it is plain text.{#if diagFailed}<span class="att"> Could not reach the clipboard: {diagFailed}</span>{/if}
      </p>
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
  .path { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-2); background: var(--surface-2); padding: 5px 8px; border-radius: 0; user-select: text; }
  .row { display: flex; align-items: center; gap: 12px; margin-top: 12px; flex-wrap: wrap; }
  .unread { margin-top: 10px; padding: 10px 12px; border-radius: 0; background: var(--surface-2); display: flex; flex-direction: column; gap: 4px; font-size: 12.5px; }
  .unread b { color: var(--amber); }
  .unread .hint { margin: 0 0 4px; }
  .unread .ur { display: grid; grid-template-columns: minmax(0, 220px) minmax(0, 1fr); gap: 10px; color: var(--text-2); overflow-wrap: anywhere; }
  .unread .ur .mono { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .hint { color: var(--text-3); font-size: 12.5px; line-height: 1.45; margin: 6px 0 0; }
  /* ---- the colour picker --------------------------------------------------------------- */
  .warnline { color: var(--amber); box-shadow: inset 2px 0 0 var(--amber); padding-left: 9px; }
  .pal { margin: 14px 0 0; }
  .palhead { display: flex; flex-direction: column; gap: 2px; margin: 0 0 8px; }
  .palhead b { font: 700 10.5px var(--font); letter-spacing: 0.06em; text-transform: uppercase; color: var(--text-2); }
  .palhead span { font-size: 12px; color: var(--text-3); line-height: 1.4; }
  .swatches { display: flex; flex-wrap: wrap; gap: 8px; }
  .sw { position: relative; display: flex; flex-direction: column; align-items: center; gap: 4px; width: 84px; cursor: pointer; }
  /* A real border rather than an inset shadow. The native colour well paints over the whole
     control, so an inset shadow is hidden under it -- which made the "Card" swatch, whose
     colour is the card it sits on, draw as nothing at all. */
  .sw input[type="color"] { width: 100%; height: 30px; padding: 0; border: 1px solid var(--surface-4); border-radius: 0; background: none; cursor: pointer; }
  .sw input[type="color"]::-webkit-color-swatch-wrapper { padding: 0; }
  .sw input[type="color"]::-webkit-color-swatch { border: 0; border-radius: 0; }
  .sw .nm { font-size: 11px; color: var(--text-3); text-align: center; line-height: 1.2; }
  /* A changed slot gets a way back of its own, so putting one colour right does not mean
     resetting the lot. */
  .sw .undo { position: absolute; top: -5px; right: -5px; width: 16px; height: 16px; padding: 0; border: 0; border-radius: 0;
    background: var(--amber); color: var(--amber-ink); font: 700 11px var(--font); line-height: 1; cursor: pointer; }
  .resetrow { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; margin: 18px 0 0; padding: 12px 0 0; border-top: 1px solid var(--surface-3); }
  .resetrow .hint { margin: 0; flex: 1 1 220px; }
  /* LITERAL COLOURS, DELIBERATELY. See the comment on the button in the markup: this is the
     control for undoing a palette, so it cannot be drawn in the palette. Fixed brick on fixed
     white, with its own border, legible on any ground somebody can produce -- including one
     where --surface and --text have been set to the same colour.
     Do not replace these with tokens. */
  .hardreset { background: #8c1d16; color: #fff7f5; border: 1px solid #f3d9d4; border-radius: 0;
    font: 600 12.5px var(--font); padding: 7px 14px; cursor: pointer; }
  .hardreset:hover { background: #a62a21; }
  .hardreset.ghost { background: #fff7f5; color: #8c1d16; border-color: #8c1d16; }
  .hardreset.ghost:hover { background: #f3d9d4; }
  .fld { margin-top: 14px; }
  .fld > label { display: block; font-size: 12.5px; font-weight: 600; color: var(--text-2); margin-bottom: 6px; }
  /* Wide enough for the longest sort name and no wider: a select stretched across the card
     reads as a text field somebody forgot to fill in. */
  .fld select.input { max-width: 260px; }
  .fld select.dir { max-width: 200px; }
  .fld .row { margin-top: 0; }
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
  .sample pre { margin: 6px 0 0; max-height: 240px; overflow: hidden auto; background: var(--surface-2); border-radius: 0; padding: 8px 10px; font-size: 11px; line-height: 1.4; user-select: text; white-space: pre-wrap; word-break: break-all; }
  .path.bad { color: var(--red); }
  .opt { display: flex; align-items: center; gap: 10px; margin: 6px 0 10px; font-size: 13px; }
  .opt .l { width: 100px; color: var(--text-2); }
  .opt .hint { margin: 0; }
  .seg { display: inline-flex; background: var(--surface-2); border-radius: 0; padding: 3px; gap: 2px; }
  .seg button { padding: 5px 10px; border-radius: 0; font-size: 12.5px; font-weight: 600; color: var(--text-2); }
  .seg button.on { background: var(--surface-4); color: var(--text); }
  .lv .input { height: 30px; font-size: 12.5px; flex: 1; }
  @media (max-width: 980px) { .settings { columns: 1; } }
  .idrow { margin-top: 12px; gap: 5px; }
  .idrow .input { max-width: 340px; }
  .idrow .btn { align-self: flex-start; margin-top: 4px; }
</style>
