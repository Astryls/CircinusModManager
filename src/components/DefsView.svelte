<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { I } from "$lib/icons";
  import { originColor, type Chain, type DefMatch, type DefModStats, type PatchProblem } from "$lib/types";

  const d = $derived(store.defs);
  const r = $derived(store.defsReport);
  const origins = $derived(store.defsOrigins);
  store.refreshDefs();


  // ---- keeping the place ----
  // Switching views destroys this component, so the position has to live somewhere that
  // outlives it. A report does not change while you are away from it, so the pixel is enough
  // here; the mod list needs an anchor because its content can move underneath you.
  let centre = $state<HTMLElement | null>(null);
  let restored = false;
  $effect(() => {
    if (restored || !centre) return;
    restored = true;
    const y = store.scrollMemory.get("defs");
    if (y) centre.scrollTop = y.top;
  });

  const modOf = (o: number) => origins[o]?.name ?? "an unknown mod";
  const whereOf = (o: number) => (origins[o] ? `${origins[o].name}${origins[o].file ? ` · ${origins[o].file}` : ""}` : "");
  const pct = $derived(d && d.total ? Math.round((d.done / d.total) * 100) : 0);
  const phaseLabel = (p: string) => (p === "defs" ? "Reading defs" : p === "patches" ? "Running patches" : p === "inheritance" ? "Resolving inheritance…" : "");

  /** The histories the backend followed by node: who shipped a value, then everyone who changed it. */
  const chains = $derived(r?.chains ?? []);
  /** Rows opened to show every step, by their place in the filtered list.
   *
   *  By position, for the same reason the rows are: `def` and `path` together do not identify a
   *  chain -- two mods overwriting the same field of the same def make two chains that agree on
   *  both -- so opening one of them used to open the other as well. Filtering resets what is
   *  open, which is the right behaviour anyway: the rows underneath are different rows. */
  let opened = $state(new Set<number>());
  function toggle(i: number) {
    const next = new Set(opened);
    next.has(i) ? next.delete(i) : next.add(i);
    opened = next;
  }
  $effect(() => {
    // What is open is a set of positions, so it has to be dropped when the positions move.
    void shownChains;
    opened = new Set();
  });
  const opName = (how: string) => (how === "Defs" ? "shipped it" : how.replace(/^PatchOperation/, "").replace(/([a-z])([A-Z])/g, "$1 $2").toLowerCase());
  let contestedQuery = $state("");
  const shownChains = $derived.by(() => {
    const q = contestedQuery.trim().toLowerCase();
    if (!q) return chains;
    return chains.filter((c) => c.def.toLowerCase().includes(q) || c.path.toLowerCase().includes(q) || c.steps.some((s) => modOf(s.origin).toLowerCase().includes(q)));
  });

  const isUnknown = (p: PatchProblem) => p.reason.startsWith("Circinus does not know");
  const failed = $derived((r?.problems ?? []).filter((p) => !p.tolerated && !isUnknown(p)));
  const tolerated = $derived((r?.problems ?? []).filter((p) => p.tolerated && !isUnknown(p)));
  const unknown = $derived((r?.problems ?? []).filter(isUnknown));
  /** Problems by the mod whose patch file they are in, biggest first. */
  function byMod(list: PatchProblem[]) {
    const map = new Map<string, { name: string; items: PatchProblem[] }>();
    for (const p of list) {
      const key = origins[p.origin]?.uid ?? String(p.origin);
      if (!map.has(key)) map.set(key, { name: modOf(p.origin), items: [] });
      map.get(key)!.items.push(p);
    }
    return [...map.values()].sort((a, b) => b.items.length - a.items.length || a.name.localeCompare(b.name));
  }
  const unknownClasses = $derived([...new Set(unknown.map((p) => p.class))]);

  // ---- per mod ----
  type Key = keyof DefModStats;
  let sortKey = $state<Key>("values");
  let sortDir = $state<1 | -1>(-1);
  function sortBy(k: Key) {
    if (sortKey === k) sortDir = sortDir === 1 ? -1 : 1;
    else {
      sortKey = k;
      sortDir = k === "name" ? 1 : -1;
    }
  }
  const perMod = $derived.by(() => {
    const rows = [...(r?.perMod ?? [])];
    return rows.sort((a, b) => {
      const x = a[sortKey], y = b[sortKey];
      const c = typeof x === "string" && typeof y === "string" ? x.localeCompare(y) : Number(x) - Number(y);
      return c * sortDir || a.name.localeCompare(b.name);
    });
  });
  const cols: { k: Key; label: string; title: string }[] = [
    { k: "defs", label: "Defs", title: "Defs of this mod's that survive to the end" },
    { k: "values", label: "Values", title: "Values in the finished document that came from this mod" },
    { k: "wins", label: "Wins", title: "Values of other mods this mod took over" },
    { k: "losses", label: "Losses", title: "Values of this mod's that another mod took over" },
    { k: "operations", label: "Operations", title: "Patch operations this mod ran" },
    { k: "failedOperations", label: "Failed", title: "Operations that changed nothing" }
  ];

  // ---- look at a def ----
  let search = $state("");
  let timer: ReturnType<typeof setTimeout> | null = null;
  function onSearch() {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => store.searchDefs(search), 220);
  }
  const tree = $derived(store.defsTree);
  const focus = $derived(store.defsFocus);
  /** The mods that own something in the def on screen, for the colour legend. */
  const treeMods = $derived([...new Set((tree?.nodes ?? []).map((n) => n.origin))]);
  function open(defType: string, defName: string, path?: string) {
    store.openDef(defType, defName, path);
    document.querySelector(".look")?.scrollIntoView({ behavior: "smooth", block: "start" });
  }
  function openMatch(m: DefMatch) {
    open(m.defType, m.defName, m.path);
  }
  let xpath = $state('Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints');
  const query = $derived(store.defsQuery);
</script>

<main class="center" bind:this={centre} onscroll={() => centre && store.scrollMemory.set("defs", { anchor: null, delta: 0, top: centre.scrollTop })}>
  <section class="card run">
    <h3>The merged defs <span class="aside">{d?.running ? phaseLabel(d.phase) : r ? `${r.defs.toLocaleString()} defs` : `${store.active.length} active mods`}</span></h3>
    <p class="lead">Circinus builds the document the game builds: every active mod's <span class="mono">Defs</span> merged in load order, every <span class="mono">PatchOperation</span> run in load order, then inheritance resolved. Every value then has an owner, so "which mod wins this" has an answer.</p>
    {#if d?.running}
      <div class="prog">
        <div class="bar"><i style="width: {Math.max(3, pct)}%"></i></div>
        <div class="pl">
          <b class="num">{phaseLabel(d.phase)} {d.total ? `${Math.min(d.done + 1, d.total).toLocaleString()} of ${d.total.toLocaleString()}` : ""}</b>
          <span class="cur mono">{d.current}</span>
        </div>
        <button class="btn" onclick={() => store.stopDefs()}>Stop</button>
      </div>
    {:else}
      <div class="acts">
        <button class="btn primary" onclick={() => store.startDefs()}>{@html I.analyze}{r ? "Work it out again" : "Work out the merged defs"}</button>
        {#if !r}<span class="hint">Reading every def and patch of a large list takes a few seconds to a minute.</span>{/if}
      </div>
      {#if r}
        <div class="report">
          <b class="num">{r.defs.toLocaleString()}</b> defs and <b class="num">{r.values.toLocaleString()}</b> values, after <b class="num">{r.operations.toLocaleString()}</b> patch operations, in {(r.elapsedMs / 1000).toFixed(1)}s.
        </div>
      {/if}
      {#if d?.stopped}<p class="hint">The last run was stopped part-way, so there is nothing to look at.</p>{/if}
      {#if d?.error}<p class="hint att">{d.error}</p>{/if}
    {/if}
  </section>

  {#if r}
    <section class="card contested">
      <h3>Contested values <span class="aside">{chains.length.toLocaleString()} value{chains.length === 1 ? "" : "s"} more than one mod set</span>
        <span class="tools"><input class="input sm" placeholder="Filter by def, path or mod" bind:value={contestedQuery} /></span>
      </h3>
      {#if !chains.length}
        <p class="hint">No mod overwrote another mod's value. Every value in the list belongs to the mod that shipped it.</p>
      {:else}
        <div class="hdr ch"><span>Def</span><span>Value</span><span>Shipped by</span><span></span><span>What the game gets</span></div>
        <div class="rows">
          <!-- Keyed by position, not by the row's contents. Two overwrites of the same field of
               the same def are two chains with the same `def` and the same `path`, and gluing
               those together made a key that was not unique -- which Svelte treats as fatal, so
               the whole report went to the error screen on any real load order. The rows are
               replaced wholesale whenever the report changes, so position *is* their identity. -->
          {#each shownChains.slice(0, 200) as c, i (i)}
            {@const first = c.steps[0]}
            {@const last = c.steps[c.steps.length - 1]}
            {@const between = c.steps.length - 2}
            {@const isOpen = opened.has(i)}
            <div class="ch-row" class:open={isOpen}>
              <button class="row ch" onclick={() => toggle(i)} title={isOpen ? "Hide the history" : "Show every step"} aria-expanded={isOpen}>
                <span class="def"><b>{c.defName || c.def}</b><span class="ty">{c.defType}</span></span>
                <span class="mono path">{c.path}{#if c.removed?.length}<span class="cnt">{c.removed.length} items</span>{/if}</span>
                <span class="step" title={whereOf(first.origin)}><span class="dot c-{originColor(first.origin)}"></span><i>{modOf(first.origin)}</i>{#if !c.removed?.length}<b class="num">{first.value}</b>{/if}</span>
                <span class="via" title={between > 0 ? `${between} other mod${between === 1 ? "" : "s"} changed it in between` : ""}>{#if between > 0}<em>+{between}</em>{/if}<span class="arr">→</span></span>
                <span class="step win" title="{whereOf(last.origin)} · {opName(last.how)}"><span class="dot c-{originColor(last.origin)}"></span><i>{modOf(last.origin)}</i><b class="num">{last.value || (c.removed?.length ? "removed them" : "removed it")}</b></span>
              </button>
              {#if isOpen}
                <div class="history">
                  {#each c.steps as s, i}
                    <div class="hs" class:last={i === c.steps.length - 1}>
                      <span class="n num">{i + 1}</span>
                      <span class="dot c-{originColor(s.origin)}"></span>
                      <span class="who"><b>{modOf(s.origin)}</b><span class="file">{origins[s.origin]?.file ?? ""}</span></span>
                      <span class="did">{opName(s.how)}</span>
                      <b class="num val">{s.value || "(removed)"}</b>
                    </div>
                  {/each}
                  {#if c.removed?.length}
                    <div class="gone">{#each c.removed as v}<span class="mono">{v}</span>{/each}</div>
                  {/if}
                  <button class="btn sm" onclick={() => open(c.defType, c.defName, c.path)}>Open {c.defName || c.def} in the inspector</button>
                </div>
              {/if}
            </div>
          {/each}
          {#if shownChains.length > 200}<div class="hint">…{(shownChains.length - 200).toLocaleString()} more; narrow the filter.</div>{/if}
          {#if !shownChains.length}<div class="hint">Nothing contested matches that filter.</div>{/if}
        </div>
      {/if}
    </section>

    <section class="card problems">
      <h3>Patches that did nothing <span class="aside">{failed.length.toLocaleString()} of {r.operations.toLocaleString()} operations</span></h3>
      {#if !failed.length}
        <p class="lead">Every patch found what it was looking for.</p>
      {:else}
        <p class="lead">These operations matched nothing, so whatever they meant to change is not in your game. Usually the mod they patch is not active, or it renamed the def.</p>
        {#each byMod(failed) as g}
          <details class="pm" open={byMod(failed).length <= 5}>
            <summary><b>{g.name}</b><span class="fc">{g.items.length} operation{g.items.length === 1 ? "" : "s"}</span></summary>
            <div class="plist">
              {#each g.items.slice(0, 120) as p}
                <div class="pr"><span class="mono xp">{p.xpath || "(the whole file)"}</span><span class="why">{p.class} · {p.reason}</span></div>
              {/each}
              {#if g.items.length > 120}<div class="hint">…{g.items.length - 120} more</div>{/if}
            </div>
          </details>
        {/each}
      {/if}
      {#if tolerated.length}
        <details class="quiet">
          <summary>{tolerated.length.toLocaleString()} more matched nothing, and the mod says that is fine</summary>
          <p class="hint">These operations carry <span class="mono">success="Always"</span>, so the author expects them to miss when a mod is not there. They cost nothing and break nothing.</p>
          {#each byMod(tolerated) as g}
            <div class="qmod"><b>{g.name}</b><span class="fc">{g.items.length}</span></div>
            <div class="plist">
              {#each g.items.slice(0, 40) as p}<div class="pr"><span class="mono xp">{p.xpath || "(the whole file)"}</span></div>{/each}
              {#if g.items.length > 40}<div class="hint">…{g.items.length - 40} more</div>{/if}
            </div>
          {/each}
        </details>
      {/if}
      {#if unknown.length}
        <div class="unknown">
          <h4>Operations Circinus does not run <span class="fc">{unknown.length.toLocaleString()}</span></h4>
          <p class="hint">Some frameworks ship patch operations of their own ({unknownClasses.slice(0, 4).join(", ")}{unknownClasses.length > 4 ? `, and ${unknownClasses.length - 4} more` : ""}). Circinus applies RimWorld's own operations only, so whatever these would have changed is missing from this view. Everything else here still holds; this part of the document may be incomplete.</p>
          {#each byMod(unknown) as g}
            <div class="qmod"><b>{g.name}</b><span class="fc">{g.items.length}</span></div>
            <div class="plist">
              {#each g.items.slice(0, 20) as p}<div class="pr"><span class="mono xp">{p.class}</span><span class="why">{p.xpath}</span></div>{/each}
            </div>
          {/each}
        </div>
      {/if}
    </section>

    <section class="card dups">
      <h3>Defs shipped by more than one mod <span class="aside">{r.duplicates.length.toLocaleString()}</span></h3>
      {#if !r.duplicates.length}
        <p class="hint">No def name is used by two mods. Nothing is being replaced wholesale.</p>
      {:else}
        <p class="lead">The same def name from two mods is not merged: the one that loads last replaces the other outright, with everything it does not repeat lost.</p>
        <div class="rows">
          {#each r.duplicates.slice(0, 200) as dup, i (i)}
            <button class="row dp" onclick={() => open(dup.defType, dup.defName)} title="Open {dup.defType}/{dup.defName} in the inspector below">
              <span class="def"><b>{dup.defName}</b><span class="ty">{dup.defType}</span></span>
              <span class="from">{dup.origins.map(modOf).join(", ")}</span>
              <span class="win"><span class="dot c-{originColor(dup.winner)}"></span>{modOf(dup.winner)} wins</span>
            </button>
          {/each}
          {#if r.duplicates.length > 200}<div class="hint">…{r.duplicates.length - 200} more</div>{/if}
        </div>
      {/if}
    </section>

    {#if r.missingParents.length}
      <section class="card parents">
        <h3>Missing parents <span class="aside">{r.missingParents.length.toLocaleString()}</span></h3>
        <p class="lead">A def that inherits from a name nothing defines never finishes loading. RimWorld says so in the log and drops it.</p>
        <div class="plist">{#each r.missingParents.slice(0, 200) as m}<div class="pr"><span class="why">{m}</span></div>{/each}</div>
        {#if r.missingParents.length > 200}<div class="hint">…{r.missingParents.length - 200} more</div>{/if}
      </section>
    {/if}

    <section class="card permod">
      <h3>Per mod <span class="aside">{perMod.length.toLocaleString()} mods contributed</span></h3>
      <div class="hdr pm2">
        <button class="h" class:on={sortKey === "name"} onclick={() => sortBy("name")}>Mod</button>
        {#each cols as c}<button class="h r" class:on={sortKey === c.k} title={c.title} onclick={() => sortBy(c.k)}>{c.label}</button>{/each}
      </div>
      <div class="rows">
        {#each perMod.slice(0, 400) as m (m.uid)}
          <div class="row pm2">
            <span class="nm"><b>{m.name}</b></span>
            <span class="r num">{m.defs.toLocaleString()}</span>
            <span class="r num">{m.values.toLocaleString()}</span>
            <span class="r num" class:ok={m.wins > 0}>{m.wins.toLocaleString()}</span>
            <span class="r num">{m.losses.toLocaleString()}</span>
            <span class="r num">{m.operations.toLocaleString()}</span>
            <span class="r num" class:bad={m.failedOperations > 0}>{m.failedOperations.toLocaleString()}</span>
          </div>
        {/each}
      </div>
    </section>

    <section class="card look">
      <h3>Look at a def <span class="aside">{tree ? `${tree.defType}/${tree.defName}` : "as the game will have it"}</span>
        <span class="tools"><input class="input sm" id="defsearch" placeholder="Find a def by name" bind:value={search} oninput={onSearch} /></span>
      </h3>
      {#if store.defsFound.length}
        <div class="found">
          {#each store.defsFound.slice(0, 20) as m}
            <button class="chip" class:on={tree && tree.defName === m.defName && tree.defType === m.defType} onclick={() => openMatch(m)}>{m.defName}<span class="ty">{m.defType}</span></button>
          {/each}
        </div>
      {/if}
      {#if tree}
        <div class="legend">
          {#each treeMods as o}<span class="lg" title={whereOf(o)}><span class="dot c-{originColor(o)}"></span>{modOf(o)}</span>{/each}
        </div>
        <div class="tree">
          {#each tree.nodes as n, i (i)}
            <div class="tn" class:hit={focus && n.path === focus} style="padding-left: {n.depth * 16}px">
              <span class="dot c-{originColor(n.origin)}" title={whereOf(n.origin)}></span>
              <span class="tag mono">{n.tag}</span>
              {#each n.attrs as [k, v]}<span class="at mono">{k}="{v}"</span>{/each}
              {#if n.leaf}<span class="val mono">{n.value}</span>{/if}
              {#if n.inherited}<span class="inh" title="Not written here: taken from {n.inheritedFrom || 'a parent def'}">inherited{n.inheritedFrom ? ` from ${n.inheritedFrom}` : ""}</span>{/if}
              <span class="who">{modOf(n.origin)}</span>
            </div>
          {/each}
          {#if tree.truncated}<div class="hint">…{tree.truncated.toLocaleString()} more nodes; this def is unusually large.</div>{/if}
        </div>
      {:else}
        <p class="hint">{store.defsError ?? "Type part of a def name — Wall, Autopistol — then pick one to see what the game ends up with, and which mod wrote each line."}</p>
      {/if}

      <details class="adv">
        <summary>Advanced: run an XPath</summary>
        <p class="hint">The same XPath a patch uses, run against the merged document. <span class="mono">Defs/ThingDef[defName="Wall"]/statBases/*</span> is the shape of it.</p>
        <form class="xq" onsubmit={(e) => { e.preventDefault(); store.runDefsQuery(xpath); }}>
          <input class="input mono" bind:value={xpath} spellcheck="false" />
          <button class="btn" type="submit">Run</button>
        </form>
        {#if store.defsError && !store.defsFound.length}<p class="hint att">{store.defsError}</p>{/if}
        {#if query}
          <div class="qres">
            <div class="hint">{query.total.toLocaleString()} match{query.total === 1 ? "" : "es"}{query.total > query.matches.length ? `, showing ${query.matches.length}` : ""} · {query.elapsedMs} ms</div>
            {#each query.matches as m, i (i)}
              <button class="row qr" onclick={() => openMatch(m)}>
                <span class="def"><b>{m.defName}</b><span class="ty">{m.defType}</span></span>
                <span class="mono path">{m.path}</span>
                <span class="mono val">{m.value}</span>
                <span class="who"><span class="dot c-{originColor(m.origin)}"></span>{modOf(m.origin)}</span>
              </button>
            {/each}
          </div>
        {/if}
      </details>
    </section>
  {/if}
</main>

<style>
  .center { display: flex; flex-direction: column; gap: 12px; min-height: 0; min-width: 0; overflow: hidden auto; padding-bottom: 14px; }
  .lead { margin: 0 0 12px; color: var(--text-2); font-size: 13px; line-height: 1.5; max-width: 92ch; }
  .acts { display: flex; gap: 10px; flex-wrap: wrap; align-items: center; }
  .hint { color: var(--text-3); font-size: 12px; line-height: 1.45; margin: 6px 0 0; }
  .hint.att { color: var(--amber); }
  .report { margin-top: 12px; font-size: 12.5px; color: var(--text-2); line-height: 1.5; }
  .prog { display: grid; grid-template-columns: 1fr auto; gap: 8px 12px; align-items: center; }
  .bar { grid-column: 1 / -1; height: 8px; border-radius: 4px; background: var(--surface-3); overflow: hidden; }
  .bar i { display: block; height: 100%; background: var(--amber); border-radius: 4px; transition: width 0.3s; }
  .pl { display: flex; flex-direction: column; gap: 2px; font-size: 12.5px; color: var(--text-2); min-width: 0; }
  .pl b { font-size: 14px; color: var(--text); }
  .cur { color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .card h3 { gap: 10px; }
  .tools { margin-left: auto; display: flex; gap: 10px; align-items: center; font-weight: 500; text-transform: none; letter-spacing: 0; }
  .input.sm { height: 28px; font-size: 12.5px; width: 220px; }
  .rows { display: flex; flex-direction: column; gap: 1px; }
  .hdr { font-size: 11px; font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase; color: var(--text-3); height: 28px; align-items: center; padding: 0 8px; gap: 12px; }
  .row { height: 34px; border-radius: 9px; font-size: 13px; padding: 0 8px; gap: 12px; align-items: center; text-align: left; width: 100%; color: var(--text-2); }
  .row:hover { background: var(--surface-2); color: var(--text); }
  /* Five columns that cannot collide: the def, the path, who shipped it, an arrow with a count
     for everyone in between, and what the game ends up with. Thirty steps used to go on one line
     and print over each other; now they are a click away, one per line. */
  .hdr.ch, .row.ch { display: grid; grid-template-columns: minmax(0, 1.1fr) minmax(0, 1.3fr) minmax(0, 1fr) 56px minmax(0, 1fr); }
  .ch-row { border-radius: 9px; }
  .ch-row.open { background: var(--surface-2); }
  .ch-row.open .row { background: transparent; color: var(--text); }
  .path .cnt { margin-left: 8px; font-family: var(--sans); font-size: 11px; color: var(--text-3); }
  .step { display: inline-flex; align-items: center; gap: 6px; min-width: 0; }
  .step i { font-style: normal; font-size: 12px; color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .step b { font-size: 12.5px; color: var(--text-3); font-family: var(--mono); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 12ch; flex: none; }
  .step.win b { color: var(--text); font-weight: 700; }
  .via { display: inline-flex; align-items: center; justify-content: center; gap: 4px; color: var(--text-4); white-space: nowrap; }
  .via em { font-style: normal; font-size: 11px; font-weight: 700; color: var(--amber); background: var(--amber-soft); padding: 1px 6px; border-radius: 999px; }
  .history { display: flex; flex-direction: column; gap: 2px; padding: 4px 12px 10px 20px; }
  .hs { display: grid; grid-template-columns: 24px 10px minmax(0, 1.4fr) minmax(0, 0.8fr) minmax(0, 1fr); gap: 10px; align-items: center; height: 26px; font-size: 12.5px; color: var(--text-2); }
  .hs .n { color: var(--text-4); font-size: 11px; text-align: right; }
  .hs .who { min-width: 0; display: flex; align-items: baseline; gap: 8px; overflow: hidden; }
  .hs .who b { color: var(--text); font-weight: 600; white-space: nowrap; }
  .hs .who .file { font-size: 11px; color: var(--text-4); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .hs .did { color: var(--text-3); white-space: nowrap; }
  .hs .val { font-family: var(--mono); color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .hs.last .val { color: var(--text); font-weight: 700; }
  .gone { display: flex; flex-wrap: wrap; gap: 4px 10px; padding: 4px 0 6px 44px; }
  .gone .mono { font-size: 11.5px; color: var(--text-3); }
  .history .btn.sm { align-self: flex-start; margin: 4px 0 0 44px; }
  .row.dp { display: grid; grid-template-columns: minmax(0, 1.1fr) minmax(0, 1.4fr) minmax(0, 1fr); }
  .def { min-width: 0; display: flex; align-items: baseline; gap: 8px; }
  .def b { color: var(--text); font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .def .ty { font-size: 11px; color: var(--text-4); white-space: nowrap; }
  .path { color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dp .from { color: var(--text-3); font-size: 12.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dp .win { display: flex; align-items: center; gap: 6px; color: var(--text-2); font-size: 12.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .pm { border-radius: 9px; }
  .pm[open] { background: var(--surface-2); }
  .pm summary, .quiet summary { cursor: pointer; display: flex; align-items: center; gap: 10px; padding: 6px 10px; font-size: 13px; color: var(--text-2); }
  .pm summary b { color: var(--text); }
  .fc { color: var(--text-3); font-size: 12px; margin-left: auto; }
  .plist { display: flex; flex-direction: column; gap: 5px; padding: 4px 12px 10px 24px; max-height: 280px; overflow: hidden auto; }
  .pr { display: flex; flex-direction: column; gap: 1px; font-size: 12px; min-width: 0; }
  .pr .xp { color: var(--text-2); word-break: break-all; }
  .pr .why { color: var(--text-3); font-size: 11.5px; line-height: 1.4; }
  .quiet { margin-top: 8px; }
  .quiet summary { font-weight: 600; color: var(--text-3); }
  .quiet .plist { max-height: 200px; }
  .qmod { display: flex; align-items: center; gap: 10px; font-size: 12.5px; color: var(--text-2); padding: 6px 10px 0; }
  .qmod b { color: var(--text-2); }
  .unknown { margin-top: 10px; border-top: 1px solid var(--surface-3); padding-top: 8px; }
  .unknown h4 { margin: 4px 0 0; font-size: 12.5px; font-weight: 700; color: var(--text-2); display: flex; align-items: center; gap: 10px; }
  .hdr.pm2, .row.pm2 { display: grid; grid-template-columns: minmax(0, 1fr) 80px 90px 70px 70px 100px 80px; }
  .hdr .h { font: inherit; color: inherit; letter-spacing: inherit; text-transform: inherit; text-align: left; padding: 0; }
  .hdr .h.r { text-align: right; }
  .hdr .h:hover { color: var(--text); }
  .hdr .h.on { color: var(--amber); }
  .nm { min-width: 0; }
  .nm b { color: var(--text); font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; display: block; }
  .r { text-align: right; }
  .r.ok { color: var(--green); font-weight: 700; }
  .r.bad { color: var(--amber); font-weight: 700; }
  .found { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 10px; }
  .found .chip.on { background: var(--amber-soft); color: var(--amber); }
  .found .chip .ty { font-size: 10.5px; color: var(--text-4); }
  .legend { display: flex; flex-wrap: wrap; gap: 6px 14px; padding: 2px 2px 10px; }
  .lg { display: inline-flex; align-items: center; gap: 6px; font-size: 11.5px; color: var(--text-3); }
  .tree { display: flex; flex-direction: column; gap: 1px; max-height: 460px; overflow: auto; }
  .tn { display: flex; align-items: center; gap: 8px; height: 24px; padding-right: 8px; border-radius: 6px; font-size: 12.5px; white-space: nowrap; }
  .tn:hover { background: var(--surface-2); }
  .tn.hit { background: var(--amber-soft); }
  .tn .tag { color: var(--text-2); }
  .tn .at { color: var(--text-3); }
  .tn .val { color: var(--text); font-weight: 500; }
  .tn .inh { font-size: 10.5px; font-weight: 700; letter-spacing: 0.04em; text-transform: uppercase; color: var(--text-4); background: var(--surface-3); padding: 1px 6px; border-radius: 5px; }
  .tn .who { margin-left: auto; padding-left: 16px; font-size: 11px; color: var(--text-3); opacity: 0; }
  .tn:hover .who, .tn.hit .who { opacity: 1; }
  .adv { margin-top: 12px; border-top: 1px solid var(--surface-3); padding-top: 8px; }
  .adv summary { cursor: pointer; font-size: 12.5px; font-weight: 600; color: var(--text-3); }
  .adv summary:hover { color: var(--text); }
  .xq { display: flex; gap: 8px; margin: 8px 0; }
  .xq .input { font-size: 12px; }
  .qres { display: flex; flex-direction: column; gap: 1px; max-height: 320px; overflow: hidden auto; }
  .row.qr { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1.2fr) minmax(0, 0.8fr) minmax(0, 0.9fr); height: 30px; font-size: 12.5px; }
  .qr .val { color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .qr .who { display: flex; align-items: center; gap: 6px; color: var(--text-3); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  @media (max-width: 1240px) {
    .hdr.ch, .row.ch { grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) minmax(0, 2fr); }
    .hdr.pm2, .row.pm2 { grid-template-columns: minmax(0, 1fr) 64px 78px 60px 60px 84px 66px; }
  }
</style>
