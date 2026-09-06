<script lang="ts">
  // Every instance in one place: which folders each one points at, how it starts the game, and
  // the four things you can do to one — switch to it, copy it, rename it, forget it.
  import { store } from "$lib/store.svelte";
  import { I } from "$lib/icons";
  import { pickFile, pickFolder, inTauri } from "$lib/api";
  import type { Instance, LaunchMethod, Locations } from "$lib/types";

  let picked = $state<string | null>(null);
  const chosen = $derived(store.instances.find((i) => i.id === picked) ?? store.instances.find((i) => i.id === store.instance?.id) ?? store.instances[0]);
  const isCurrent = $derived(!!chosen && chosen.id === store.instance?.id);

  let naming = $state(false);
  let newName = $state("");
  let fromCurrent = $state(true);
  let renaming = $state(false);
  let renameTo = $state("");
  let confirmDelete = $state(false);
  let confirmSwitch = $state(false);

  type Key = keyof Locations;
  const fields: { key: Key; label: string; hint: string }[] = [
    { key: "gameDir", label: "RimWorld folder", hint: "Has Version.txt and Data in it. On macOS pick the folder RimWorldMac.app sits in - the Finder will not let you choose the bundle itself, and Circinus looks inside it" },
    { key: "configDir", label: "Config folder", hint: "Has ModsConfig.xml in it. A folder of its own is what gives this instance its own saves" },
    { key: "localModsDir", label: "Local mods folder", hint: "Usually RimWorld/Mods. SteamCMD downloads go here" },
    { key: "workshopDir", label: "Workshop folder", hint: "steamapps/workshop/content/294100" }
  ];
  const methods: { id: LaunchMethod; label: string; hint: string }[] = [
    { id: "auto", label: "Auto", hint: "Through Steam when the game lives in a Steam library, otherwise from the executable" },
    { id: "steam", label: "Steam", hint: "Asks Steam to start the game, so the overlay and Workshop updates work as usual" },
    { id: "executable", label: "Executable", hint: "Starts the game file directly: GOG, DRM-free, or a copy outside Steam" }
  ];

  function saveLocations(loc: Locations) {
    if (chosen) store.updateInstance(chosen.id, loc, chosen.launch);
  }
  function saveLaunch(patch: Partial<Instance["launch"]>) {
    if (chosen) store.updateInstance(chosen.id, chosen.locations, { ...chosen.launch, ...patch });
  }
  async function choose(key: Key) {
    const p = await pickFolder(fields.find((f) => f.key === key)?.label);
    if (p && chosen) saveLocations({ ...chosen.locations, [key]: p });
  }
  function clear(key: Key) {
    if (chosen) saveLocations({ ...chosen.locations, [key]: null });
  }
  async function chooseExe() {
    const p = await pickFile([{ name: "RimWorld", extensions: ["exe", "app", "sh", "x86_64", "*"] }]);
    if (p) saveLaunch({ executable: p, method: "executable" });
  }
  async function make() {
    const name = newName.trim();
    if (!name) return;
    const inst = await store.createInstance(name, fromCurrent);
    if (inst) picked = inst.id;
    newName = "";
    naming = false;
  }
  async function rename() {
    const name = renameTo.trim();
    if (!name || !chosen) return;
    await store.renameInstance(chosen.id, name);
    renaming = false;
  }
  async function duplicate() {
    if (!chosen) return;
    const inst = await store.duplicateInstance(chosen.id);
    if (inst) picked = inst.id;
  }
  async function remove() {
    if (!chosen) return;
    confirmDelete = false;
    await store.deleteInstance(chosen.id);
    picked = null;
  }
  function switchTo(sure: boolean) {
    if (!chosen) return;
    if (store.snap?.dirty && !sure) { confirmSwitch = true; return; }
    confirmSwitch = false;
    store.switchInstance(chosen.id, true);
    close();
  }
  /** What the game will be told, when the config folder is not the one it uses by default. */
  const saveData = $derived.by(() => {
    const cfg = chosen?.locations.configDir;
    if (!cfg) return null;
    if (isCurrent) return store.snap?.settings.launch.args.toLowerCase().includes("-savedatafolder") ? null : /[\\/]config$/i.test(cfg) ? cfg.replace(/[\\/]config$/i, "") : cfg;
    return /[\\/]config$/i.test(cfg) ? cfg.replace(/[\\/]config$/i, "") : cfg;
  });
  const when = (t: number) => (t ? new Date(t * 1000).toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" }) : "");

  function close() {
    store.showInstances = false;
  }
</script>

<div class="scrim" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) close(); }}>
  <div class="dlg card" role="dialog" aria-modal="true" aria-labelledby="inst-title">
    <div class="hd">
      <div>
        <b id="inst-title">Instances</b>
        <span class="sub">An instance is a set of folders: a game folder, a config folder, a local mods folder and a Workshop folder, with its own launch settings and its own lists. Switching re-points Circinus and reads the mods again.</span>
      </div>
      <button class="x" aria-label="Close" onclick={close}>{@html I.close}</button>
    </div>

    <div class="body">
      <div class="side">
        {#each store.instances as i (i.id)}
          <button class="row" class:on={chosen?.id === i.id} onclick={() => (picked = i.id)} title={i.locations.gameDir ?? "no game folder set"}>
            <span class="dot" style="--c: var(--{i.locations.gameDir ? "green" : "amber"})"></span>
            <span class="nm">
              <b>{i.name}</b>
              <span>{i.id === store.instance?.id ? "open now" : `made ${when(i.createdAt)}`}</span>
            </span>
          </button>
        {/each}
        {#if naming}
          <form class="newg" onsubmit={(e) => { e.preventDefault(); make(); }}>
            <input class="input" placeholder="Instance name" bind:value={newName} />
            <button class="btn sm primary" type="submit">Add</button>
          </form>
          <label class="switch sm"><input type="checkbox" bind:checked={fromCurrent} />Start from the folders open now</label>
        {:else}
          <button class="add" onclick={() => { naming = true; newName = ""; }}>{@html I.plus}New instance</button>
        {/if}
      </div>

      {#if chosen}
        <div class="detail">
          <div class="dh">
            {#if renaming}
              <form class="newg" onsubmit={(e) => { e.preventDefault(); rename(); }}>
                <input class="input" placeholder="New name" bind:value={renameTo} />
                <button class="btn sm primary" type="submit">Rename</button>
                <button class="btn sm" type="button" onclick={() => (renaming = false)}>Keep</button>
              </form>
            {:else}
              <h4>{chosen.name}{#if isCurrent}<span class="tag">open now</span>{/if}</h4>
              <div class="acts">
                {#if !isCurrent}<button class="btn sm primary" onclick={() => switchTo(false)}>{@html I.change}Switch to this one</button>{/if}
                <button class="btn sm" onclick={() => { renaming = true; renameTo = chosen.name; }}>Rename…</button>
                <button class="btn sm" title="Another instance pointing at the same folders" onclick={duplicate}>Duplicate</button>
                <button class="btn sm danger" onclick={() => (confirmDelete = true)}>Delete…</button>
              </div>
            {/if}
          </div>
          {#if confirmSwitch}
            <div class="ask">Your list has unsaved changes. Switching to {chosen.name} loses them.<div><button class="btn sm primary" onclick={() => switchTo(true)}>Switch anyway</button><button class="btn sm" onclick={() => (confirmSwitch = false)}>Keep editing</button></div></div>
          {/if}
          {#if confirmDelete}
            <div class="ask">Delete {chosen.name}? Circinus forgets the instance and nothing else: the mods, the saves and the config folder stay exactly where they are, and its lists stay in Circinus's data folder.<div><button class="btn sm danger" onclick={remove}>Delete the instance</button><button class="btn sm" onclick={() => (confirmDelete = false)}>Keep it</button></div></div>
          {/if}

          <section>
            <div class="label">Where this instance's RimWorld lives</div>
            {#each fields as f}
              <div class="loc">
                <div class="lt"><b>{f.label}</b><span>{f.hint}</span></div>
                <div class="lv">
                  <span class="path mono" title={chosen.locations[f.key] ?? ""}>{chosen.locations[f.key] ?? (isCurrent ? (store.snap?.locations[f.key] ?? "found automatically") : "found automatically")}</span>
                  <button class="btn sm" disabled={!inTauri} onclick={() => choose(f.key)}>Choose…</button>
                  {#if chosen.locations[f.key]}<button class="btn sm" onclick={() => clear(f.key)}>Auto</button>{/if}
                </div>
              </div>
            {/each}
            {#if saveData}
              <p class="hint">This instance has a config folder of its own, so Play starts the game with <span class="mono">-savedatafolder={saveData}</span>. Without that the game would read and rewrite the usual ModsConfig.xml instead of this one, and the two playthroughs would share their saves.</p>
            {:else}
              <p class="hint">This instance uses RimWorld's usual config folder. Give it one of its own to keep its saves and its mod list apart from the others.</p>
            {/if}
          </section>

          <section>
            <div class="label">How this instance starts</div>
            <div class="opt">
              <span class="l">Play starts</span>
              <div class="seg">{#each methods as m}<button class:on={chosen.launch.method === m.id} title={m.hint} onclick={() => saveLaunch({ method: m.id })}>{m.label}</button>{/each}</div>
            </div>
            <div class="loc">
              <div class="lt"><b>Executable</b><span>{chosen.launch.executable ? "chosen by you" : "detected from the RimWorld folder"}</span></div>
              <div class="lv">
                <span class="path mono" title={chosen.launch.executable ?? ""}>{chosen.launch.executable ?? "found automatically"}</span>
                <button class="btn sm" disabled={!inTauri} onclick={chooseExe}>Choose…</button>
                {#if chosen.launch.executable}<button class="btn sm" onclick={() => saveLaunch({ executable: null })}>Auto</button>{/if}
              </div>
            </div>
            <div class="loc">
              <div class="lt"><b>Extra arguments</b><span>For example -popupwindow or -screen-width 1920</span></div>
              <div class="lv"><input class="input mono" value={chosen.launch.args} placeholder="none" onchange={(e) => saveLaunch({ args: e.currentTarget.value })} /></div>
            </div>
            <label class="switch"><input type="checkbox" checked={chosen.launch.saveFirst} onchange={(e) => saveLaunch({ saveFirst: e.currentTarget.checked })} />Save ModsConfig.xml first when there are unsaved changes</label>
          </section>

          <section>
            <div class="label">Its lists</div>
            {#if isCurrent}
              <p class="hint">{store.namedLists.length ? `${store.namedLists.length} named ${store.namedLists.length === 1 ? "list" : "lists"}: ${store.namedLists.map((l) => l.name).join(", ")}.` : "No named lists yet."} Lists belong to the instance they were saved in; switch instance and its own lists are in the rail. {store.currentList ? `Working on ${store.currentList}.` : "The active list is ModsConfig.xml."}</p>
            {:else}
              <p class="hint">Switch to this instance to see and edit its lists. Every instance keeps its own, and remembers which one it was working on.</p>
            {/if}
          </section>
        </div>
      {/if}
    </div>

    <div class="ft">
      <span class="hint">Two instances may share a game folder and differ only in config, or share a Workshop folder. Nothing here copies or moves a mod.</span>
      <span class="sp"></span>
      <button class="btn" onclick={close}>Done</button>
    </div>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); backdrop-filter: blur(6px); display: grid; place-items: center; z-index: 40; }
  .dlg { width: min(900px, calc(100vw - 40px)); max-height: calc(100vh - 40px); padding: 18px 20px; box-shadow: var(--shadow-float); display: flex; flex-direction: column; gap: 14px; }
  .hd { display: flex; justify-content: space-between; align-items: flex-start; gap: 12px; }
  .hd b { font-size: 16px; font-weight: 800; display: block; }
  .hd .sub { display: block; font-size: 12.5px; color: var(--text-3); margin-top: 2px; line-height: 1.45; max-width: 76ch; }
  .x { width: 28px; height: 28px; border-radius: 8px; display: grid; place-items: center; color: var(--text-3); flex: none; }
  .x:hover { background: var(--surface-2); color: var(--text); }
  .x :global(svg) { width: 14px; height: 14px; }
  .body { display: grid; grid-template-columns: 210px minmax(0, 1fr); gap: 16px; min-height: 0; overflow: hidden; }
  .side { display: flex; flex-direction: column; gap: 2px; overflow: hidden auto; padding-right: 4px; }
  .side .row { display: flex; align-items: center; gap: 9px; padding: 7px 8px; border-radius: 9px; text-align: left; width: 100%; min-width: 0; }
  .side .row:hover { background: var(--surface-2); }
  .side .row.on { background: var(--surface-3); }
  .side .nm { display: flex; flex-direction: column; min-width: 0; }
  .side .nm b { font-size: 13px; font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .side .nm span { font-size: 11px; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .add { display: flex; align-items: center; gap: 8px; padding: 7px 8px; border-radius: 9px; color: var(--text-2); font-weight: 600; font-size: 12.5px; margin-top: 4px; }
  .add:hover { background: var(--surface-2); color: var(--text); }
  .add :global(svg) { width: 14px; height: 14px; }
  .detail { overflow: hidden auto; min-width: 0; display: flex; flex-direction: column; gap: 14px; padding-right: 4px; }
  .dh { display: flex; align-items: center; justify-content: space-between; gap: 10px; flex-wrap: wrap; }
  h4 { margin: 0; font-size: 15px; font-weight: 700; display: flex; align-items: center; gap: 8px; }
  .tag { font-size: 11px; font-weight: 700; color: var(--green); background: var(--green-soft); border-radius: 6px; padding: 2px 7px; }
  .acts { display: flex; gap: 6px; flex-wrap: wrap; }
  .label { margin-bottom: 4px; }
  .loc { display: grid; grid-template-columns: 170px minmax(0, 1fr); gap: 10px; align-items: center; padding: 7px 0; border-bottom: 1px solid rgba(255, 255, 255, 0.05); }
  .loc:last-of-type { border-bottom: 0; }
  .lt b { display: block; font-size: 13px; }
  .lt span { display: block; font-size: 11.5px; color: var(--text-3); overflow-wrap: anywhere; }
  .lv { display: flex; gap: 6px; align-items: center; min-width: 0; }
  .path { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--text-2); background: var(--surface-2); padding: 5px 8px; border-radius: 7px; user-select: text; }
  .lv .input { height: 30px; font-size: 12.5px; flex: 1; }
  .hint { color: var(--text-3); font-size: 12.5px; line-height: 1.45; margin: 8px 0 0; }
  .hint .mono { color: var(--text-2); user-select: text; overflow-wrap: anywhere; }
  .ask { color: var(--text-2); background: var(--surface-2); border-radius: 10px; padding: 10px 12px; font-size: 12.5px; line-height: 1.45; }
  .ask div { display: flex; gap: 6px; margin-top: 8px; }
  .opt { display: flex; align-items: center; gap: 10px; margin: 6px 0 10px; font-size: 13px; }
  .opt .l { width: 100px; color: var(--text-2); }
  .seg { display: inline-flex; background: var(--surface-2); border-radius: 9px; padding: 3px; gap: 2px; }
  .seg button { padding: 5px 10px; border-radius: 7px; font-size: 12.5px; font-weight: 600; color: var(--text-2); }
  .seg button.on { background: var(--surface-4); color: var(--text); }
  .switch { margin: 6px 0; align-items: center; display: flex; }
  .switch.sm { font-size: 12px; color: var(--text-3); font-weight: 600; margin: 2px 0 0; }
  .newg { display: flex; gap: 6px; }
  .newg .input { height: 28px; font-size: 12.5px; }
  .ft { display: flex; gap: 12px; align-items: center; flex-wrap: wrap; }
  .ft .hint { margin: 0; max-width: 68ch; }
  .sp { flex: 1; }
  @media (max-width: 860px) { .body { grid-template-columns: minmax(0, 1fr); } }
</style>
