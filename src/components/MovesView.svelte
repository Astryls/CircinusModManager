<script lang="ts">
  import { onMount, tick } from "svelte";
  import { store } from "$lib/store.svelte";
  import { I } from "$lib/icons";
  import { analyseMoves, type Relocation } from "$lib/moves";
  import { PHASES, type Phase } from "$lib/types";

  const phaseOf = (id: Phase) => PHASES.find((p) => p.id === id) ?? PHASES[3];
  const nameOf = (uid: string) => store.byUid.get(uid)?.name ?? uid;
  const nowPhase = (uid: string): Phase => store.placementByUid.get(uid)?.phase ?? "content";
  const nextPhase = (uid: string): Phase => store.proposedPlacementByUid.get(uid)?.phase ?? "content";

  const current = $derived(store.active);
  const proposed = $derived(store.preview?.order ?? []);
  const report = $derived(analyseMoves(current, proposed, nowPhase, nextPhase, (uid) => store.proposedPlacementByUid.get(uid)?.reason ?? ""));

  /** Half a pair: a phase on one side and nothing on the other narrows to everything touching it. */
  type Side = { from: Phase | null; to: Phase | null };
  const holds = (s: Side | null, r: Relocation) => !s || ((s.from == null || r.fromPhase === s.from) && (s.to == null || r.toPhase === s.to));
  /** Clicked: the view narrows to it. Hovered: the view dims everything else but keeps it. */
  let pick = $state<Side | null>(null);
  let hover = $state<Side | null>(null);
  const same = (a: Side | null, b: Side | null) => !!a && !!b && a.from === b.from && a.to === b.to;

  /** The search box narrows the columns too, so one mod can be found without reading the groups. */
  const q = $derived(store.query.trim().toLowerCase());
  const moves = $derived(
    report.relocations.filter((r) => {
      if (!holds(pick, r)) return false;
      if (!q) return true;
      const m = store.byUid.get(r.uid);
      return !!m && (m.name.toLowerCase().includes(q) || m.packageId.toLowerCase().includes(q));
    })
  );
  const narrowed = $derived(moves.length !== report.relocations.length);

  // ---- the two columns: what leaves each phase, and what arrives in each ----
  type Group = { phase: Phase; moves: Relocation[] };
  function groups(list: Relocation[], side: "from" | "to"): Group[] {
    const out: Group[] = [];
    for (const p of PHASES) {
      const inIt = list.filter((r) => (side === "from" ? r.fromPhase : r.toPhase) === p.id);
      if (inIt.length) out.push({ phase: p.id, moves: inIt.sort((a, b) => (side === "from" ? a.from - b.from : a.to - b.to)) });
    }
    return out;
  }
  const leaving = $derived(groups(moves, "from"));
  const arriving = $derived(groups(moves, "to"));
  /** One connector per pair of phases mods travel between; the count is what gives it its weight. */
  const legs = $derived.by(() => {
    const by = new Map<string, { from: Phase; to: Phase; n: number }>();
    for (const r of moves) {
      const k = `${r.fromPhase}>${r.toPhase}`;
      const leg = by.get(k) ?? { from: r.fromPhase, to: r.toPhase, n: 0 };
      leg.n++;
      by.set(k, leg);
    }
    return [...by.values()].sort((a, b) => b.n - a.n);
  });
  /** Groups open past the tenth mod one at a time: a phase can hold two hundred of them. */
  const SHOWN = 10;
  let opened = $state(new Set<string>());
  function openGroup(key: string) {
    const next = new Set(opened);
    next.has(key) ? next.delete(key) : next.add(key);
    opened = next;
  }

  // ---- the connectors ----
  let sheet = $state<SVGSVGElement | null>(null);
  let pair = $state<HTMLDivElement | null>(null);
  let outEl: Partial<Record<Phase, HTMLElement>> = {};
  let inEl: Partial<Record<Phase, HTMLElement>> = {};
  let wires = $state<{ key: string; d: string; color: string; w: number; n: number; x: number; y: number; from: Phase; to: Phase }[]>([]);
  function measure() {
    if (!sheet || !pair) return;
    const b = sheet.getBoundingClientRect();
    if (!b.width) return;
    const out: typeof wires = [];
    for (const leg of legs) {
      const a = outEl[leg.from]?.getBoundingClientRect();
      const z = inEl[leg.to]?.getBoundingClientRect();
      if (!a || !z) continue;
      // Level with the phase's name rather than the middle of its card: a phase can hold two
      // hundred mods, and an arrow pointing at the middle of that points at nothing in particular.
      const ah = outEl[leg.from]?.querySelector(".ghead")?.getBoundingClientRect() ?? a;
      const zh = inEl[leg.to]?.querySelector(".ghead")?.getBoundingClientRect() ?? z;
      const x1 = a.right - b.left, y1 = ah.top + ah.height / 2 - b.top;
      const x2 = z.left - b.left, y2 = zh.top + zh.height / 2 - b.top;
      const mx = (x1 + x2) / 2;
      out.push({
        key: `${leg.from}>${leg.to}`,
        d: `M${x1},${y1} C${mx},${y1} ${mx},${y2} ${x2 - 9},${y2}`,
        // Weighted by how many mods travel it, so the big migrations read as the big ones.
        color: phaseOf(leg.to).color,
        w: Math.min(7, 1.6 + Math.sqrt(leg.n) * 1.1),
        n: leg.n,
        x: mx,
        y: (y1 + y2) / 2,
        from: leg.from,
        to: leg.to
      });
    }
    // Two journeys between neighbouring phases meet in the middle of the lane, so their counts
    // would sit on top of one another: nudge them apart, keeping the order they are drawn in.
    let last = -Infinity;
    for (const w of [...out].sort((a, z) => a.y - z.y)) {
      if (w.y - last < 22) w.y = last + 22;
      last = w.y;
    }
    wires = out;
  }
  $effect(() => {
    // Re-draw whenever the groups, the narrowing or an opened group change the shape of a column.
    void legs; void leaving; void arriving; void opened;
    tick().then(measure);
  });
  onMount(() => {
    const ro = new ResizeObserver(() => measure());
    if (pair) ro.observe(pair);
    window.addEventListener("resize", measure);
    return () => { ro.disconnect(); window.removeEventListener("resize", measure); };
  });

  const label = (p: Phase) => phaseOf(p).name;
  const chip = $derived(pick ? (pick.from && pick.to ? `${label(pick.from)} → ${label(pick.to)}` : pick.from ? `Leaving ${label(pick.from)}` : `Arriving in ${label(pick.to!)}`) : "");
  function choose(s: Side) {
    pick = same(pick, s) ? null : s;
  }
  /** Where a phase's mods are going, said in one line under the group's name. */
  function destinations(g: Group, side: "from" | "to") {
    const seen = new Map<Phase, number>();
    for (const r of g.moves) {
      const p = side === "from" ? r.toPhase : r.fromPhase;
      seen.set(p, (seen.get(p) ?? 0) + 1);
    }
    return [...seen.entries()].sort((a, b) => b[1] - a[1]).map(([p, n]) => (p === g.phase ? `${n} stay here` : `${n} ${side === "from" ? "→" : "←"} ${label(p)}`)).join(" · ");
  }
</script>

<section class="moves">
  <header class="sum">
    <span class="lead">{@html I.halo}</span>
    <p>
      {#if !report.relocations.length}
        HALO would leave every mod where it is.
      {:else}
        HALO would move <b>{report.relocations.length} mod{report.relocations.length === 1 ? "" : "s"}</b>{#if report.crossPhase}, <b>{report.crossPhase}</b> of them into a different phase{/if}.
        {#if report.drift}<span class="muted">{report.drift.toLocaleString()} other mods keep their places and only change number, because the moved ones pass them.</span>{/if}
      {/if}
    </p>
    {#if narrowed}<span class="of num">{moves.length} shown</span>{/if}
    {#if pick}<button class="chip on" onclick={() => (pick = null)} title="Show every move again">{chip}<span class="x">{@html I.close}</span></button>{/if}
  </header>

  {#if report.relocations.length}
    <div class="board">
      <div class="cap l">Leaving<span class="num">{leaving.length} phase{leaving.length === 1 ? "" : "s"}</span></div>
      <div class="cap r">Arriving<span class="num">{arriving.length} phase{arriving.length === 1 ? "" : "s"}</span></div>
      <div class="pair" bind:this={pair}>
        <div class="col">
          {#each leaving as g (g.phase)}
            {@const key = `out:${g.phase}`}
            {@const side = { from: g.phase, to: null }}
            <div class="grp" class:dim={hover != null && hover.from != null && hover.from !== g.phase} bind:this={outEl[g.phase]}>
              <button class="ghead" class:on={same(pick, side)} onclick={() => choose(side)}
                onmouseenter={() => (hover = side)} onmouseleave={() => (hover = null)}
                title="Only the moves that leave {label(g.phase)}">
                <span class="dot c-{phaseOf(g.phase).color}"></span>
                <b>{label(g.phase)}</b>
                <span class="where">{destinations(g, "from")}</span>
                <span class="n num">{g.moves.length}</span>
              </button>
              {#each opened.has(key) ? g.moves : g.moves.slice(0, SHOWN) as m (m.uid)}
                <button class="mv" class:sel={store.selected.includes(m.uid)} class:fresh={store.isNew(m.uid)} class:dim={!holds(hover, m)}
                  onclick={() => store.select(m.uid)} title={m.reason}>
                  <span class="num">#{m.from + 1}</span>
                  <span class="dot c-{phaseOf(m.fromPhase).color}"></span>
                  <span class="nm">{nameOf(m.uid)}</span>
                  <span class="to num">→ #{m.to + 1}</span>
                </button>
              {/each}
              {#if g.moves.length > SHOWN}
                <button class="more" onclick={() => openGroup(key)}>{opened.has(key) ? "Show fewer" : `${g.moves.length - SHOWN} more leaving ${label(g.phase)}`}</button>
              {/if}
            </div>
          {/each}
        </div>

        <div class="lane">
          <svg bind:this={sheet} aria-hidden="true">
            <defs>
              {#each PHASES as p (p.id)}
                <marker id="ln-{p.color}" viewBox="0 0 8 8" refX="7.4" refY="4" markerWidth="6" markerHeight="6" orient="auto">
                  <path d="M0.4,0.6 L8,4 L0.4,7.4 Z" fill="var(--{p.color})" />
                </marker>
              {/each}
            </defs>
            {#each wires as w (w.key)}
              <path d={w.d} stroke="var(--{w.color})" stroke-width={w.w} marker-end="url(#ln-{w.color})"
                class:on={same(pick, { from: w.from, to: w.to })} class:cold={(hover != null && !holds(hover, { fromPhase: w.from, toPhase: w.to } as Relocation))} />
            {/each}
          </svg>
          {#each wires as w (w.key)}
            <button class="tally" style="left: {w.x}px; top: {w.y}px" onclick={() => choose({ from: w.from, to: w.to })}
              onmouseenter={() => (hover = { from: w.from, to: w.to })} onmouseleave={() => (hover = null)}
              class:on={same(pick, { from: w.from, to: w.to })}
              title="{w.n} mod{w.n === 1 ? '' : 's'} {w.from === w.to ? `move within ${label(w.to)}` : `go from ${label(w.from)} to ${label(w.to)}`}">{w.n}</button>
          {/each}
        </div>

        <div class="col">
          {#each arriving as g (g.phase)}
            {@const key = `in:${g.phase}`}
            {@const side = { from: null, to: g.phase }}
            <div class="grp" class:dim={hover != null && hover.to != null && hover.to !== g.phase} bind:this={inEl[g.phase]}>
              <button class="ghead" class:on={same(pick, side)} onclick={() => choose(side)}
                onmouseenter={() => (hover = side)} onmouseleave={() => (hover = null)}
                title="Only the moves that land in {label(g.phase)}">
                <span class="dot c-{phaseOf(g.phase).color}"></span>
                <b>{label(g.phase)}</b>
                <span class="where">{destinations(g, "to")}</span>
                <span class="n num">{g.moves.length}</span>
              </button>
              {#each opened.has(key) ? g.moves : g.moves.slice(0, SHOWN) as m (m.uid)}
                <button class="mv" class:sel={store.selected.includes(m.uid)} class:fresh={store.isNew(m.uid)} class:dim={!holds(hover, m)}
                  onclick={() => store.select(m.uid)} title={m.reason}>
                  <span class="num">#{m.to + 1}</span>
                  <span class="dot c-{phaseOf(m.toPhase).color}"></span>
                  <span class="nm">{nameOf(m.uid)}</span>
                  <span class="to num">from #{m.from + 1}</span>
                </button>
              {/each}
              {#if g.moves.length > SHOWN}
                <button class="more" onclick={() => openGroup(key)}>{opened.has(key) ? "Show fewer" : `${g.moves.length - SHOWN} more arriving in ${label(g.phase)}`}</button>
              {/if}
            </div>
          {/each}
          {#if !arriving.length}<p class="empty">{q ? `No mod HALO would move matches “${store.query.trim()}”.` : "Nothing matches."}</p>{/if}
        </div>
      </div>
      <p class="hint">Each arrow is a group of mods making the same journey, and its weight is how many. Click an arrow or a phase to keep only those moves; click a mod to see why in the panel.</p>
    </div>
  {:else}
    <p class="empty card">Your order already does what HALO would do. There is nothing to apply.</p>
  {/if}
</section>

<style>
  .moves { display: flex; flex-direction: column; gap: 10px; min-height: 0; flex: 1; }
  .sum { display: flex; align-items: center; gap: 10px; padding: 0 4px; }
  .sum .lead :global(svg) { width: 17px; height: 17px; display: block; }
  .sum p { margin: 0; font-size: 13px; line-height: 1.45; color: var(--text); }
  .sum .muted { color: var(--text-3); }
  .sum .of { flex: none; font-size: 11.5px; font-weight: 600; color: var(--text-3); white-space: nowrap; }
  .sum .chip { flex: none; height: 24px; gap: 5px; }
  .sum .chip .x :global(svg) { width: 9px; height: 9px; display: block; }

  /* The columns stop being the two orders and become what leaves and what arrives. Both scroll
     together, so an arrow drawn between two groups stays on them. */
  .board { flex: 1; min-height: 0; display: grid; grid-template-rows: auto minmax(0, 1fr) auto; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); column-gap: 12px; row-gap: 8px; }
  .cap { grid-row: 1; font-size: 11px; font-weight: 700; letter-spacing: 0.07em; text-transform: uppercase; color: var(--text-2); display: flex; align-items: baseline; gap: 6px; white-space: nowrap; padding: 0 4px; }
  .cap .num { font-size: 11px; font-weight: 600; color: var(--text-3); letter-spacing: 0; }
  .cap.l { grid-column: 1; }
  .cap.r { grid-column: 2; justify-content: flex-end; }
  .pair { grid-row: 2; grid-column: 1 / -1; overflow: hidden auto; display: grid; grid-template-columns: minmax(0, 1fr) 116px minmax(0, 1fr); align-items: start; padding-right: 2px; }
  .col { display: flex; flex-direction: column; gap: 10px; min-width: 0; }
  .lane { position: relative; align-self: stretch; }
  .lane svg { position: absolute; inset: 0; width: 100%; height: 100%; overflow: visible; pointer-events: none; }
  .lane path { fill: none; opacity: 0.8; transition: opacity 0.12s; }
  .lane path.on { opacity: 1; }
  .lane path.cold { opacity: 0.15; }
  .tally { position: absolute; transform: translate(-50%, -50%); min-width: 22px; height: 19px; padding: 0 6px; border-radius: 0; background: var(--surface-3); color: var(--text-2); font-size: 11px; font-weight: 700; font-variant-numeric: tabular-nums; box-shadow: 0 0 0 3px var(--surface); }
  .tally:hover, .tally.on { background: var(--amber); color: var(--amber-ink); }

  /* one phase's worth of departures or arrivals */
  .grp { background: var(--surface); border-radius: 0; padding: 7px 8px 8px; display: flex; flex-direction: column; gap: 3px; min-width: 0; transition: opacity 0.12s; }
  .grp.dim { opacity: 0.35; }
  .ghead { display: grid; grid-template-columns: 8px auto minmax(0, 1fr) auto; align-items: center; gap: 8px; width: 100%; height: 28px; padding: 0 6px; border-radius: 0; text-align: left; }
  .ghead:hover { background: var(--surface-2); }
  .ghead.on { background: var(--amber-soft); }
  .ghead b { font-size: 12.5px; font-weight: 700; color: var(--text); white-space: nowrap; }
  .ghead .where { font-size: 11px; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .ghead .n { font-size: 11.5px; font-weight: 700; color: var(--text-2); }

  .mv { display: grid; grid-template-columns: 46px 8px minmax(0, 1fr) auto; align-items: center; gap: 9px; width: 100%; height: 26px; padding: 0 6px; border-radius: 0; background: var(--bg-2); text-align: left; transition: opacity 0.12s; }
  .mv:hover { background: var(--surface-2); }
  .mv.sel { box-shadow: inset 0 0 0 1px var(--amber); }
  .mv.dim { opacity: 0.28; }
  .mv .num { font-size: 11px; font-variant-numeric: tabular-nums; color: var(--text-3); text-align: right; }
  .mv .nm { font-size: 12.5px; font-weight: 600; color: var(--text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .mv .to { font-size: 10.5px; color: var(--text-3); white-space: nowrap; }
  .more { height: 22px; margin-top: 1px; border-radius: 0; font-size: 11px; font-weight: 600; color: var(--text-3); background: repeating-linear-gradient(115deg, var(--bg-2) 0 7px, var(--surface) 7px 14px); }
  .more:hover { color: var(--text); }

  .board .hint { grid-row: 3; grid-column: 1 / -1; margin: 0; padding: 0 4px; font-size: 11.5px; color: var(--text-3); line-height: 1.4; }
  .empty { font-size: 13px; color: var(--text-3); padding: 14px; margin: 0; }
  @media (max-width: 1240px) { .pair { grid-template-columns: minmax(0, 1fr) 78px minmax(0, 1fr); } .ghead .where { display: none; } }
  /* The same green edge the list uses, so a mod that arrived and a mod that moved read the
     same way wherever you are looking at them. */
  .mv.fresh { box-shadow: inset -2px 0 0 var(--green); background: var(--green-soft); }
</style>
