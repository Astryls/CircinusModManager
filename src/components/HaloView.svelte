<script lang="ts">
  import { onMount, tick } from "svelte";
  import { store } from "$lib/store.svelte";
  import { I } from "$lib/icons";
  import { EMPTY_HALO, PHASES, type BuiltinRule, type Phase } from "$lib/types";

  store.loadHaloRules();
  const rules = $derived(store.haloRules);
  const halo = $derived(store.halo);
  const phaseOf = (id: Phase) => PHASES.find((p) => p.id === id)!;
  /** Where a built-in rule files mods right now: its own phase, or where the user sent it. */
  const targetOf = (r: BuiltinRule): Phase => halo.retarget[r.key] ?? r.phase;
  const isOff = (r: BuiltinRule) => halo.off.includes(r.key);
  /** Active mods per phase, as HALO files them today. */
  const counts = $derived.by(() => {
    const c = new Map<Phase, number>();
    for (const uid of store.active) {
      const p = store.placementByUid.get(uid)?.phase ?? "content";
      c.set(p, (c.get(p) ?? 0) + 1);
    }
    return c;
  });
  const edits = $derived(Object.keys(halo.packagePhases).length + halo.namePhases.length + halo.off.length + Object.keys(halo.retarget).length);

  // ---- edits ----
  function toggle(r: BuiltinRule, on: boolean) {
    store.updateHalo((h) => ({ ...h, off: on ? h.off.filter((k) => k !== r.key) : [...new Set([...h.off, r.key])] }));
  }
  function retarget(r: BuiltinRule, phase: Phase) {
    store.updateHalo((h) => {
      const retarget = { ...h.retarget };
      if (phase === r.phase) delete retarget[r.key];
      else retarget[r.key] = phase;
      return { ...h, retarget };
    });
  }
  let newId = $state("");
  let newIdPhase = $state<Phase>("framework");
  function addId() {
    const id = newId.trim().toLowerCase();
    if (!id) return;
    const phase = newIdPhase;
    store.updateHalo((h) => ({ ...h, packagePhases: { ...h.packagePhases, [id]: phase } }));
    newId = "";
  }
  function setId(id: string, phase: Phase) {
    store.updateHalo((h) => ({ ...h, packagePhases: { ...h.packagePhases, [id]: phase } }));
  }
  function dropId(id: string) {
    store.updateHalo((h) => {
      const packagePhases = { ...h.packagePhases };
      delete packagePhases[id];
      return { ...h, packagePhases };
    });
  }
  let newNeedle = $state("");
  let newNeedlePhase = $state<Phase>("content");
  function addNeedle() {
    const needle = newNeedle.trim();
    if (!needle) return;
    const phase = newNeedlePhase;
    store.updateHalo((h) => ({ ...h, namePhases: [...h.namePhases, { needle, phase }] }));
    newNeedle = "";
  }
  function setNeedle(i: number, phase: Phase) {
    store.updateHalo((h) => ({ ...h, namePhases: h.namePhases.map((n, k) => (k === i ? { ...n, phase } : n)) }));
  }
  function dropNeedle(i: number) {
    store.updateHalo((h) => ({ ...h, namePhases: h.namePhases.filter((_, k) => k !== i) }));
  }
  let confirmReset = $state(false);
  function resetAll() {
    store.updateHalo(() => structuredClone(EMPTY_HALO));
    confirmReset = false;
  }
  const perMod = $derived(Object.entries(store.snap?.user.phaseOverrides ?? {}).map(([uid, phase]) => ({ uid, phase, name: store.byUid.get(uid)?.name ?? uid })).sort((a, b) => a.name.localeCompare(b.name)));
  const installedIds = $derived([...store.byPackage.keys()].sort());

  // ---- the wires between signals and phases ----
  let board = $state<HTMLDivElement | null>(null);
  let rowEls: Record<string, HTMLElement> = {};
  let phaseEls: Partial<Record<Phase, HTMLElement>> = {};
  let wires = $state<{ d: string; color: string; key: string; off: boolean }[]>([]);
  function measure() {
    if (!board) return;
    const b = board.getBoundingClientRect();
    const out: typeof wires = [];
    for (const r of rules) {
      const a = rowEls[r.key]?.getBoundingClientRect();
      const p = phaseEls[targetOf(r)]?.getBoundingClientRect();
      if (!a || !p) continue;
      const x1 = a.right - b.left, y1 = a.top + a.height / 2 - b.top;
      const x2 = p.left - b.left, y2 = p.top + p.height / 2 - b.top;
      const mx = (x1 + x2) / 2;
      out.push({ d: `M${x1},${y1} C${mx},${y1} ${mx},${y2} ${x2},${y2}`, color: `var(--${phaseOf(targetOf(r)).color})`, key: r.key, off: isOff(r) });
    }
    wires = out;
  }
  $effect(() => {
    // Re-measure whenever the rules, the edits or the counts change shape.
    void rules; void halo; void counts;
    tick().then(measure);
  });
  onMount(() => {
    const ro = new ResizeObserver(() => measure());
    if (board) ro.observe(board);
    window.addEventListener("resize", measure);
    return () => { ro.disconnect(); window.removeEventListener("resize", measure); };
  });
</script>

<main class="center halo">
  <section class="card caution">
    <div class="ic">{@html I.warn}</div>
    <div class="txt">
      <b>For advanced users.</b> HALO's defaults are safe: they keep the game's own content on top, libraries before what needs them, patches after what they change, performance mods last. A mapping changed here applies the next time you press <b>Sort with HALO</b>, and a wrong one can put a mod above the game (which resets your list) or after the mods it patches. Every edit can be undone; the game's own content cannot be moved at all.
    </div>
    {#if confirmReset}
      <span class="ask">Put every mapping back?<button class="btn sm danger" onclick={resetAll}>Reset</button><button class="btn sm" onclick={() => (confirmReset = false)}>Keep</button></span>
    {:else}
      <button class="btn sm" disabled={!edits} onclick={() => (confirmReset = true)}>Reset to defaults{edits ? ` (${edits} edit${edits === 1 ? "" : "s"})` : ""}</button>
    {/if}
  </section>

  <section class="card how">
    <h3>How HALO orders a list</h3>
    <div class="strip">
      {#each PHASES as p, i}
        <div class="ph"><span class="dot c-{p.color}"></span><b>{i + 1}. {p.name}</b><span>{p.note}</span></div>
      {/each}
    </div>
    <p>Every active mod is filed under one of eight phases, from what its folder contains and what is known about it: the phases are the soft order, top to bottom. Rules are the hard order: every <span class="mono">loadAfter</span>, <span class="mono">loadBefore</span> and dependency from About.xml, Manifest.xml, the community database and your own rules becomes an edge, and no phase preference is allowed to break one. Where rules contradict each other, the more trusted source wins — yours, then the community's, then the mod's own — and a true loop is cut at its weakest rule and reported.</p>
    <p>Two things outrank every rule. The game and its DLC load first, in release order, and everything that ships Defs loads after them, because a def can only inherit from mods above it; a rule that would break this is set aside and shown as a note. And a mod that must load after a late loader or a performance mod is pulled down with it, so add-ons follow the mod they extend.</p>
    <p>Within a phase your arrangement is kept where the rules allow it (or sorted by name, in Settings). A mod's own <b>Sort it as</b> beats everything below; a group can sort its members as a phase or hold a section of its own; a pinned mod does not move. The page below shows what HALO looks at when nothing of yours applies, and lets you change where each signal leads.</p>
  </section>

  <section class="card map">
    <h3>What HALO looks at, and where it files a mod <span class="aside">tried top to bottom; the first match wins</span></h3>
    <div class="board" bind:this={board}>
      <div class="signals">
        {#each rules as r (r.key)}
          {@const off = isOff(r)}
          <div class="sig" class:off bind:this={rowEls[r.key]}>
            <label class="switch sm" title={r.editable ? (off ? "Switched off: this signal is ignored" : "On") : "Always on"}><input type="checkbox" checked={!off} disabled={!r.editable} onchange={(e) => toggle(r, e.currentTarget.checked)} /></label>
            <div class="txt">
              <b>{r.signal}</b>
              <span class="detail">{r.detail}</span>
              {#if r.ids.length}
                <details><summary>{r.ids.length} known package id{r.ids.length === 1 ? "" : "s"}</summary><div class="ids">{#each r.ids as id}<span class="mono" class:have={store.byPackage.has(id)} title={store.byPackage.get(id)?.name ?? "not installed"}>{id}</span>{/each}</div></details>
              {/if}
            </div>
            {#if r.editable}
              <select class="sel" value={targetOf(r)} disabled={off} onchange={(e) => retarget(r, e.currentTarget.value as Phase)} title="Where this signal files a mod">
                {#each PHASES as p}<option value={p.id}>{p.name}{p.id === r.phase ? " (default)" : ""}</option>{/each}
              </select>
            {:else}
              <span class="fixed"><span class="dot c-{phaseOf(r.phase).color}"></span>{phaseOf(r.phase).name}</span>
            {/if}
          </div>
        {/each}
        {#if !rules.length}<p class="hint">Loading HALO's rule table…</p>{/if}
      </div>
      <svg class="wires" aria-hidden="true">
        {#each wires as w (w.key)}<path d={w.d} stroke={w.color} class:off={w.off} />{/each}
      </svg>
      <div class="phases">
        {#each PHASES as p}
          <div class="phase" bind:this={phaseEls[p.id]}>
            <span class="dot c-{p.color}"></span>
            <b>{p.name}</b>
            <span class="n num">{counts.get(p.id) ?? 0}</span>
            <span class="note">{p.note}</span>
          </div>
        {/each}
      </div>
    </div>
  </section>

  <section class="card yours">
    <h3>Your mappings <span class="aside">checked before the signals above; a mod's own Sort it as still wins</span></h3>
    <div class="two">
      <div>
        <h4>Mods filed by package id</h4>
        <p class="hint">Portable: the id, not the folder, so it holds across reinstalls and can be shared.</p>
        <form class="add" onsubmit={(e) => { e.preventDefault(); addId(); }}>
          <input class="input mono" list="halo-ids" placeholder="packageId, e.g. oskarpotocki.vfe.empire" bind:value={newId} />
          <datalist id="halo-ids">{#each installedIds as id}<option value={id}></option>{/each}</datalist>
          <select class="sel" bind:value={newIdPhase}>{#each PHASES.filter((p) => p.id !== "core") as p}<option value={p.id}>{p.name}</option>{/each}</select>
          <button class="btn sm primary" type="submit" disabled={!newId.trim()}>Add</button>
        </form>
        <div class="rows">
          {#each Object.entries(halo.packagePhases).sort((a, b) => a[0].localeCompare(b[0])) as [id, phase] (id)}
            <div class="row">
              <span class="nm"><b>{store.byPackage.get(id)?.name ?? "(not installed)"}</b><span class="mono">{id}</span></span>
              <select class="sel" value={phase} onchange={(e) => setId(id, e.currentTarget.value as Phase)}>{#each PHASES.filter((p) => p.id !== "core") as p}<option value={p.id}>{p.name}</option>{/each}</select>
              <button class="x" aria-label="Remove" title="Remove" onclick={() => dropId(id)}>{@html I.close}</button>
            </div>
          {:else}
            <p class="hint">None yet.</p>
          {/each}
        </div>
      </div>
      <div>
        <h4>Names that mean a phase</h4>
        <p class="hint">Any mod whose name contains the text, in the order listed; the first match wins.</p>
        <form class="add" onsubmit={(e) => { e.preventDefault(); addNeedle(); }}>
          <input class="input" placeholder="Part of a name, e.g. Retexture" bind:value={newNeedle} />
          <select class="sel" bind:value={newNeedlePhase}>{#each PHASES.filter((p) => p.id !== "core") as p}<option value={p.id}>{p.name}</option>{/each}</select>
          <button class="btn sm primary" type="submit" disabled={!newNeedle.trim()}>Add</button>
        </form>
        <div class="rows">
          {#each halo.namePhases as n, i (i + ":" + n.needle)}
            <div class="row">
              <span class="nm"><b>“{n.needle}”</b><span>{store.mods.filter((m) => m.name.toLowerCase().includes(n.needle.trim().toLowerCase())).length} installed mods match</span></span>
              <select class="sel" value={n.phase} onchange={(e) => setNeedle(i, e.currentTarget.value as Phase)}>{#each PHASES.filter((p) => p.id !== "core") as p}<option value={p.id}>{p.name}</option>{/each}</select>
              <button class="x" aria-label="Remove" title="Remove" onclick={() => dropNeedle(i)}>{@html I.close}</button>
            </div>
          {:else}
            <p class="hint">None yet.</p>
          {/each}
        </div>
      </div>
    </div>
    {#if perMod.length}
      <h4>Mods with their own Sort it as <span class="aside">{perMod.length}</span></h4>
      <p class="hint">Set from the right-click menu or the panel; these win over everything on this page. Tied to the folder, not the id.</p>
      <div class="rows chips">
        {#each perMod as m (m.uid)}
          <span class="pill"><span class="dot c-{phaseOf(m.phase).color}"></span>{m.name} → {phaseOf(m.phase).name}<button class="x" aria-label="Let HALO decide for {m.name}" title="Let HALO decide" onclick={() => store.setPhaseOverride(m.uid, null)}>{@html I.close}</button></span>
        {/each}
      </div>
    {/if}
  </section>
</main>

<style>
  .halo { display: flex; flex-direction: column; gap: 12px; min-height: 0; overflow: hidden auto; padding-right: 2px; }
  .card { padding: 16px 18px; }
  h3 { display: flex; align-items: center; gap: 10px; }
  h3 .aside { font-weight: 500; text-transform: none; letter-spacing: 0; color: var(--text-3); }
  h4 { margin: 14px 0 4px; font-size: 13px; font-weight: 700; color: var(--text); display: flex; align-items: center; gap: 8px; }
  h4 .aside { color: var(--text-3); font-weight: 600; }
  p { margin: 0 0 10px; color: var(--text-2); font-size: 13px; line-height: 1.55; }
  p:last-child { margin-bottom: 0; }
  .hint { color: var(--text-3); font-size: 12px; line-height: 1.45; margin: 0 0 8px; }
  .caution { display: flex; align-items: center; gap: 14px; background: var(--amber-soft); box-shadow: inset 0 0 0 1px rgba(233, 162, 59, 0.35); }
  .caution .ic :global(svg) { width: 26px; height: 26px; }
  .caution .txt { flex: 1; font-size: 13px; line-height: 1.5; color: var(--text); }
  .caution .ask { display: inline-flex; align-items: center; gap: 6px; font-size: 12.5px; color: var(--text-2); }
  .strip { display: grid; grid-template-columns: repeat(8, minmax(0, 1fr)); gap: 6px; margin: 4px 0 12px; }
  .strip .ph { background: var(--surface-2); border-radius: 9px; padding: 8px 10px; display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .strip .ph b { font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .strip .ph span:not(.dot) { font-size: 11px; color: var(--text-3); line-height: 1.35; }
  .strip .dot { width: 7px; height: 7px; }
  /* The board: signals on the left, phases on the right, wires between them. */
  .board { position: relative; display: grid; grid-template-columns: minmax(0, 1fr) 150px minmax(200px, 240px); gap: 0; margin-top: 6px; }
  .signals { display: flex; flex-direction: column; gap: 6px; }
  .sig { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; gap: 10px; padding: 8px 12px; border-radius: 10px; background: var(--surface-2); }
  .sig.off { opacity: 0.5; }
  .sig .txt { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .sig b { font-size: 13px; }
  .sig .detail { font-size: 11.5px; color: var(--text-3); line-height: 1.4; }
  .sig details { font-size: 11.5px; color: var(--text-3); }
  .sig summary { cursor: pointer; }
  .sig .ids { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 4px; }
  .sig .ids .mono { font-size: 11px; padding: 1px 6px; border-radius: 5px; background: var(--surface-3); color: var(--text-3); }
  .sig .ids .mono.have { color: var(--text); }
  .sig .fixed { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; font-weight: 600; color: var(--text-2); white-space: nowrap; }
  .sel { height: 26px; border: 0; border-radius: 7px; background: var(--surface-3); color: var(--text); font-size: 12px; padding: 0 6px; max-width: 170px; }
  .wires { grid-column: 1 / -1; grid-row: 1; position: absolute; inset: 0; width: 100%; height: 100%; pointer-events: none; overflow: visible; }
  .wires path { fill: none; stroke-width: 1.6px; opacity: 0.85; }
  .wires path.off { stroke-dasharray: 3 4; opacity: 0.3; }
  .signals { grid-column: 1; grid-row: 1; }
  .phases { grid-column: 3; grid-row: 1; display: flex; flex-direction: column; justify-content: space-between; gap: 6px; }
  .phase { display: grid; grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; column-gap: 8px; padding: 8px 12px; border-radius: 10px; background: var(--surface-2); box-shadow: inset 0 0 0 1px var(--surface-3); }
  .phase b { font-size: 12.5px; }
  .phase .n { color: var(--text-3); font-size: 12px; }
  .phase .note { grid-column: 2 / -1; font-size: 11px; color: var(--text-3); line-height: 1.35; }
  .two { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 18px; }
  .add { display: flex; gap: 6px; margin-bottom: 8px; }
  .add .input { flex: 1; min-width: 0; height: 30px; font-size: 12.5px; }
  .rows { display: flex; flex-direction: column; gap: 4px; }
  .row { display: grid; grid-template-columns: minmax(0, 1fr) auto auto; gap: 8px; align-items: center; padding: 6px 8px; border-radius: 9px; background: var(--surface-2); }
  .row .nm { min-width: 0; display: flex; flex-direction: column; }
  .row .nm b { font-size: 13px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .row .nm span { font-size: 11px; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .x { width: 24px; height: 24px; border-radius: 6px; display: grid; place-items: center; color: var(--text-3); }
  .x:hover { background: var(--surface-3); color: var(--text); }
  .x :global(svg) { width: 11px; height: 11px; }
  .chips { flex-direction: row; flex-wrap: wrap; gap: 6px; }
  .pill { display: inline-flex; align-items: center; gap: 6px; height: 26px; padding: 0 4px 0 9px; border-radius: 7px; background: var(--surface-2); font-size: 12px; font-weight: 600; color: var(--text-2); }
  @media (max-width: 1240px) { .strip { grid-template-columns: repeat(4, minmax(0, 1fr)); } .two { grid-template-columns: 1fr; } .board { grid-template-columns: minmax(0, 1fr) 90px 190px; } }
</style>
