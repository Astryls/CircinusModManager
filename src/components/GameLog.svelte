<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { pickFile, revealPath } from "$lib/api";
  import { I, sevIcon } from "$lib/icons";
  import { formatBytes, type LogModRef, type XmlProblem } from "$lib/types";

  const a = $derived(store.gameLog);
  const r = $derived(store.gameLog?.report ?? null);
  const files = $derived(store.gameLogFiles);
  const current = $derived(files.find((f) => f.path.endsWith("Player.log")));
  const previous = $derived(files.find((f) => f.path.endsWith("Player-prev.log")));
  store.refreshGameLogFiles();

  const when = (secs: number) => (secs ? new Date(secs * 1000).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" }) : "");
  const n = (uid: string | null | undefined, fallback: string) => (uid ? store.byUid.get(uid)?.name ?? fallback : fallback);
  /** "GravshipSize.HarmonyPatches.X" → "Gravship Size (GravshipSize.HarmonyPatches.X)" when the frame resolved to a mod. */
  function who(frame: string | null | undefined): string {
    if (!frame) return "";
    const root = frame.split(/[.+:]/)[0];
    const name = a?.resolved[frame] ?? a?.resolved[root];
    return name ? `${name} — ${frame}` : frame;
  }
  function owners(list: string[]): string {
    return list.map((o) => a?.resolved[o] ?? o).join(", ");
  }
  async function choose() {
    const p = await pickFile([{ name: "Unity log", extensions: ["log", "txt"] }]);
    if (p) store.analyzeGameLog(p);
  }
  function show(m: LogModRef) {
    if (!m.uid) return;
    store.view = "order";
    store.scrollTo(m.uid);
  }
  /** Missing parents grouped by the mod whose def lacked them. */
  const parentsByMod = $derived.by(() => {
    const map = new Map<string, { names: Set<string>; defs: number }>();
    for (const p of r?.missingParents ?? []) {
      const key = p.sourceMod ?? p.file ?? "unknown source";
      const e = map.get(key) ?? { names: new Set<string>(), defs: 0 };
      if (p.missingParent) e.names.add(p.missingParent);
      e.defs++;
      map.set(key, e);
    }
    return [...map.entries()].sort((x, y) => y[1].defs - x[1].defs);
  });
  const xmlByMod = $derived.by(() => {
    const map = new Map<string, XmlProblem[]>();
    for (const p of r?.xmlErrors ?? []) {
      const key = p.sourceMod ?? p.file ?? "unknown source";
      map.set(key, [...(map.get(key) ?? []), p]);
    }
    return [...map.entries()].sort((x, y) => y[1].length - x[1].length);
  });
  const misplaced = $derived((a?.mods ?? []).filter((m) => m.aboveOfficial && m.missingParents > 0));
  const verdict = $derived.by(() => {
    if (!r) return null;
    if (r.outcome === "crashed") return { kind: "error", title: "RimWorld crashed", sub: r.reset ? "after loading failed and the mod list was reset" : r.crash?.reason ?? "" };
    if (r.outcome === "loadFailedReset") return { kind: "error", title: "Loading failed — RimWorld reset the mod list", sub: r.gaveUp ? "and then gave up on the second attempt" : "and loaded official content only" };
    const bad = r.exceptions.reduce((s, e) => s + e.count, 0);
    return { kind: bad || r.missingParents.length ? "warning" : "note", title: "Loaded without a fatal error", sub: `${r.exceptions.length} exception group${r.exceptions.length === 1 ? "" : "s"}, ${r.missingParents.length + r.xmlErrors.length} XML error${r.missingParents.length + r.xmlErrors.length === 1 ? "" : "s"}, ${r.ddsFailures.length} DDS failure${r.ddsFailures.length === 1 ? "" : "s"}` };
  });
  const flags = (m: LogModRef) => {
    const out: { cls: string; text: string; title: string }[] = [];
    if (m.aboveOfficial) out.push({ cls: "error", text: "above Core", title: "Sits above official content in the current list, so its defs cannot inherit Core's parents" });
    if (m.crashCulprit) out.push({ cls: "error", text: "in the crash", title: "First mod frame in the native crash stack" });
    if (m.loadFailurePatch) out.push({ cls: "error", text: "in the load failure", title: "Its code or Harmony patch is in the exception that made loading fail" });
    if (m.missingParents) out.push({ cls: "warning", text: `${m.missingParents} missing parent${m.missingParents === 1 ? "" : "s"}`, title: "Defs whose ParentName could not be found" });
    if (m.xmlErrors) out.push({ cls: "warning", text: `${m.xmlErrors} XML`, title: "Other XML errors attributed to this mod" });
    if (m.exceptions) out.push({ cls: "warning", text: `${m.exceptions} exception${m.exceptions === 1 ? "" : "s"} · ${m.exceptionHits}×`, title: "Exception groups whose innermost mod frame or patch owner is this mod, and how often they fired" });
    if (m.ddsFailures) out.push({ cls: "note", text: `${m.ddsFailures} DDS`, title: "DDS textures Unity refused to create" });
    if (m.duplicateFolders) out.push({ cls: "note", text: `${m.duplicateFolders} folders`, title: "Installed more than once; RimWorld ignored the duplicates" });
    return out;
  };
</script>

<section class="card gl">
  <h3>
    Last game run
    {#if a}<span class="aside mono">{a.path} · {when(a.modified)} · {formatBytes(a.bytes)}</span>{/if}
  </h3>
  {#if !a}
    <p class="lead">Read RimWorld's <span class="mono">Player.log</span> to see what the last launch did: whether it loaded, which mods' XML broke, what threw, and what crashed — tied to the mods in your list.</p>
    <div class="acts">
      <button class="btn primary" disabled={!current?.exists && files.length > 0} onclick={() => store.analyzeGameLog()}>{@html I.terminal}Read Player.log{#if current?.exists}<span class="cnt">{when(current.modified)}</span>{/if}</button>
      {#if previous?.exists}<button class="btn" onclick={() => store.analyzeGameLog(previous.path)}>Previous run <span class="cnt">{when(previous.modified)}</span></button>{/if}
      <button class="btn" onclick={choose}>Choose a log…</button>
    </div>
    {#if files.length && !current?.exists}<p class="hint">No Player.log at <span class="mono">{current?.path}</span> yet — RimWorld writes it when it runs.</p>{/if}
  {:else if r && verdict}
    <div class="verdict {verdict.kind}">
      <span class="flag {verdict.kind}">{@html sevIcon[verdict.kind as "error" | "warning" | "note"]}</span>
      <div class="vt"><b>{verdict.title}</b>{#if verdict.sub}<span>{verdict.sub}</span>{/if}</div>
      <div class="vacts">
        <button class="btn sm" onclick={() => store.analyzeGameLog(a.path)}>Re-read</button>
        {#if previous?.exists && !a.path.endsWith("Player-prev.log")}<button class="btn sm" onclick={() => store.analyzeGameLog(previous.path)}>Previous run</button>{/if}
        {#if current?.exists && a.path.endsWith("Player-prev.log")}<button class="btn sm" onclick={() => store.analyzeGameLog()}>Current run</button>{/if}
        <button class="btn sm" onclick={choose}>Choose…</button>
        <button class="btn sm" onclick={() => revealPath(a.path)}>Reveal</button>
      </div>
    </div>

    <ol class="chain">
      {#if misplaced.length}
        <li class="error">
          <b>{misplaced.map((m) => m.name).join(", ")} {misplaced.length === 1 ? "sits" : "sit"} above Core in the current list.</b>
          A def can only inherit from mods loaded above it, so {r.missingParents.length} of {misplaced.length === 1 ? "its" : "their"} defs lost their parents ({[...new Set(r.missingParents.map((p) => p.missingParent).filter(Boolean))].slice(0, 6).join(", ")}{new Set(r.missingParents.map((p) => p.missingParent)).size > 6 ? "…" : ""}) and with them their category, thing class and comps. That is what breaks def generation next. Sort with HALO or move them below the DLCs.
        </li>
      {:else if r.missingParents.length}
        <li class="warning">
          <b>{r.missingParents.length} def{r.missingParents.length === 1 ? "" : "s"} could not find {r.missingParents.length === 1 ? "its" : "their"} parent.</b>
          Parents named: {[...new Set(r.missingParents.map((p) => p.missingParent).filter(Boolean))].slice(0, 8).join(", ")}. Usually the mod that defines the parent is missing, outdated, or loads below the mod that needs it.
        </li>
      {/if}
      {#if r.loadFailure}
        <li class="error">
          <b>Loading failed:</b> <span class="mono">{r.loadFailure.message}</span>
          {#if r.loadFailure.modFrame}<span class="det">in {who(r.loadFailure.modFrame)}</span>{:else if r.loadFailure.topFrame}<span class="det">in {r.loadFailure.topFrame}</span>{/if}
          {#if r.loadFailure.patchOwners.length}<span class="det">Harmony patches on that method: {owners(r.loadFailure.patchOwners)}</span>{/if}
        </li>
      {/if}
      {#if r.reset}
        <li class="error"><b>RimWorld reset ModsConfig.xml to official content and tried again</b>{#if r.gaveUp}, then gave up{/if}. Your list on disk is gone; Import can bring it back from a save game or an archived list.</li>
      {/if}
      {#if r.crash}
        <li class="error">
          <b>Native crash</b>{#if r.crash.reason}: <span class="mono">{r.crash.reason}</span>{/if}
          {#if r.crash.culpritFrame}<span class="det">first mod frame: {who(r.crash.culpritFrame)}</span>{/if}
          {#if r.crash.offMainThread}<span class="det">It happened on a worker thread{r.crash.quickstart ? " during a quick-start" : ""} — a texture or graphics call off the main thread. After a reset this is a symptom, not the cause.</span>{/if}
        </li>
      {/if}
      {#if r.prepatcherRestarted}<li class="note">Prepatcher restarted the game{r.prepatcherVanillaLoadSecs ? ` (vanilla load ${Math.round(r.prepatcherVanillaLoadSecs)}s)` : ""}.</li>{/if}
      {#if r.duplicates.length}<li class="note">{r.duplicates.length} packageId{r.duplicates.length === 1 ? " is" : "s are"} installed more than once; RimWorld ignored the duplicates.</li>{/if}
    </ol>

    {#if a.mods.length}
      <div class="sub">Mods implicated <span class="aside">{a.mods.length}</span></div>
      <div class="rows">
        {#each a.mods.slice(0, 60) as m}
          <div class="row" class:off={!m.active && m.uid}>
            <span class="nm"><b>{m.name}</b>{#if !m.uid}<span class="tag">not installed</span>{:else if !m.active}<span class="tag">inactive</span>{/if}</span>
            <span class="fl">{#each flags(m) as f}<span class="chip on {f.cls}" title={f.title}>{f.text}</span>{/each}</span>
            <span class="act">{#if m.uid}<button class="btn sm" onclick={() => show(m)}>Show</button>{/if}</span>
          </div>
        {/each}
        {#if a.mods.length > 60}<div class="hint">…{a.mods.length - 60} more</div>{/if}
      </div>
    {/if}

    <div class="details">
      {#if parentsByMod.length}
        <details>
          <summary>Missing parents <span class="aside">{r.missingParents.length} defs in {parentsByMod.length} mods</span></summary>
          <div class="dl">{#each parentsByMod as [mod, e]}<div><b>{mod}</b> — {e.defs} def{e.defs === 1 ? "" : "s"}: <span class="mono">{[...e.names].join(", ")}</span></div>{/each}</div>
        </details>
      {/if}
      {#if xmlByMod.length}
        <details>
          <summary>Other XML errors <span class="aside">{r.xmlErrors.length}</span></summary>
          <div class="dl">{#each xmlByMod.slice(0, 80) as [mod, list]}<div><b>{mod}</b> — {list.length}<div class="msgs">{#each list.slice(0, 6) as p}<span class="mono">{p.message}</span>{/each}{#if list.length > 6}<span class="mono">… {list.length - 6} more</span>{/if}</div></div>{/each}</div>
        </details>
      {/if}
      {#if r.exceptions.length}
        <details>
          <summary>Exceptions <span class="aside">{r.exceptions.length} groups · {r.exceptions.reduce((s, e) => s + e.count, 0)} thrown</span></summary>
          <div class="dl">
            {#each r.exceptions.slice(0, 120) as e}
              <div>
                <b>{e.count}×</b> <span class="mono">{e.message}</span>
                {#if e.modFrame}<div class="det">in {who(e.modFrame)}</div>{:else if e.topFrame}<div class="det">in {e.topFrame}</div>{/if}
                {#if e.patchOwners.length}<div class="det">patched by {owners(e.patchOwners)}</div>{/if}
              </div>
            {/each}
          </div>
        </details>
      {/if}
      {#if r.ddsFailures.length}
        <details>
          <summary>DDS textures Unity refused <span class="aside">{r.ddsFailures.length}</span></summary>
          <div class="dl">{#each r.ddsFailures as d}<div><span class="mono">{d.path}</span><div class="det">{d.reason}</div></div>{/each}</div>
          {#if Object.keys(r.multipleOf4Warnings).length}<p class="hint">"Requires a texture size that is a multiple of 4": {Object.entries(r.multipleOf4Warnings).map(([f, c]) => `${f} ×${c}`).join(", ")}. These files were shipped by their authors that way; Circinus never writes such a file, and the DDS audit can regenerate them from the PNG.</p>{/if}
        </details>
      {/if}
      {#if r.texturesNotFound.length}
        <details>
          <summary>Textures not found <span class="aside">{r.texturesNotFoundTotal}</span></summary>
          <div class="dl">{#each r.texturesNotFound.slice(0, 60) as [p, c]}<div><span class="mono">{p}</span>{#if c > 1}<span class="det"> ×{c}</span>{/if}</div>{/each}</div>
        </details>
      {/if}
      {#if r.timings.length}
        <details>
          <summary>Timings</summary>
          <div class="dl">{#each r.timings as t}<div><b>{t.seconds >= 60 ? `${(t.seconds / 60).toFixed(1)} min` : `${t.seconds.toFixed(1)}s`}</b> {t.label}</div>{/each}</div>
        </details>
      {/if}
      <details>
        <summary>Environment</summary>
        <div class="dl">
          <div>RimWorld <b>{r.gameVersion ?? "?"}</b> · Unity {r.unityVersion ?? "?"}</div>
          <div>{r.gpu ?? "GPU unknown"}{r.vramMb ? ` · ${Math.round(r.vramMb / 1024)} GB VRAM` : ""}</div>
          {#if r.commandLine}<div class="mono">{r.commandLine}</div>{/if}
          <div>{r.lines.toLocaleString()} lines{r.threadTextureWarnings ? ` · ${r.threadTextureWarnings} off-main-thread texture warnings` : ""}{r.badTextureMaterials ? ` · ${r.badTextureMaterials} bad texture materials` : ""}</div>
        </div>
      </details>
    </div>
  {/if}
</section>

<style>
  .gl h3 .aside { font-size: 11px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .lead { margin: 0 0 12px; color: var(--text-2); font-size: 13.5px; line-height: 1.5; max-width: 72ch; }
  .acts { display: flex; gap: 8px; flex-wrap: wrap; align-items: center; }
  .cnt { margin-left: 4px; font-size: 11px; font-weight: 500; opacity: 0.75; }
  .hint { color: var(--text-3); font-size: 12px; line-height: 1.45; margin: 8px 0 0; }
  .verdict { display: flex; align-items: center; gap: 12px; padding: 10px 12px; border-radius: 10px; background: var(--surface-2); }
  .verdict.error { background: var(--red-soft); }
  .verdict.warning { background: var(--amber-soft); }
  .vt { display: flex; flex-direction: column; gap: 2px; min-width: 0; flex: 1; }
  .vt b { font-size: 15px; }
  .vt span { font-size: 12.5px; color: var(--text-2); }
  .vacts { display: flex; gap: 6px; flex-wrap: wrap; justify-content: flex-end; }
  .chain { list-style: none; margin: 12px 0 0; padding: 0; display: flex; flex-direction: column; gap: 8px; }
  .chain li { position: relative; padding: 8px 10px 8px 26px; border-radius: 9px; background: var(--surface-2); font-size: 13px; line-height: 1.5; color: var(--text-2); }
  .chain li::before { content: ""; position: absolute; left: 11px; top: 14px; width: 7px; height: 7px; border-radius: 50%; background: var(--text-3); }
  .chain li.error::before { background: var(--red); }
  .chain li.warning::before { background: var(--amber); }
  .chain li b { color: var(--text); }
  .det { display: block; color: var(--text-3); font-size: 12px; margin-top: 2px; }
  .sub { margin: 14px 0 6px; font-size: 11px; font-weight: 700; letter-spacing: 0.09em; text-transform: uppercase; color: var(--text-3); display: flex; gap: 8px; align-items: center; }
  .sub .aside { font-weight: 500; letter-spacing: 0; text-transform: none; }
  .rows { display: flex; flex-direction: column; gap: 1px; }
  .row { display: grid; grid-template-columns: minmax(180px, 1fr) minmax(0, 2fr) 70px; gap: 10px; align-items: center; padding: 5px 8px; border-radius: 9px; font-size: 13px; min-height: 36px; }
  .row:hover { background: var(--surface-2); }
  .row.off { opacity: 0.6; }
  .nm { min-width: 0; display: flex; align-items: center; gap: 8px; }
  .nm b { font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .tag { font-size: 10.5px; font-weight: 700; letter-spacing: 0.05em; text-transform: uppercase; color: var(--text-4); background: var(--surface-3); padding: 2px 6px; border-radius: 5px; white-space: nowrap; }
  .fl { display: flex; gap: 4px; flex-wrap: wrap; }
  .fl .chip { height: 22px; font-size: 11px; padding: 0 8px; cursor: default; }
  .fl .chip.warning { background: var(--amber-soft); color: var(--amber); }
  .act { display: flex; justify-content: flex-end; }
  .details { margin-top: 12px; display: flex; flex-direction: column; gap: 4px; }
  details { border-radius: 9px; }
  details[open] { background: var(--surface-2); }
  summary { cursor: pointer; padding: 7px 10px; font-size: 13px; font-weight: 600; color: var(--text-2); display: flex; gap: 8px; align-items: center; }
  summary:hover { color: var(--text); }
  summary .aside { font-weight: 500; color: var(--text-3); }
  .dl { padding: 4px 12px 10px 28px; display: flex; flex-direction: column; gap: 6px; font-size: 12.5px; color: var(--text-2); line-height: 1.45; max-height: 360px; overflow: auto; }
  .dl b { color: var(--text); }
  .dl .mono { color: var(--text-2); word-break: break-all; }
  .msgs { display: flex; flex-direction: column; gap: 2px; padding-left: 12px; }
  .msgs .mono { color: var(--text-3); font-size: 11.5px; }
</style>
