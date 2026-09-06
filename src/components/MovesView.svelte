<script lang="ts">
  import { onMount, tick } from "svelte";
  import { store } from "$lib/store.svelte";
  import { I } from "$lib/icons";
  import { analyseMoves, type Relocation } from "$lib/moves";
  import { PHASES, type Phase } from "$lib/types";

  const phaseOf = (id: Phase) => PHASES.find((p) => p.id === id) ?? PHASES[3];
  const nameOf = (uid?: string) => (uid ? (store.byUid.get(uid)?.name ?? uid) : "");
  const nowPhase = (uid: string): Phase => store.placementByUid.get(uid)?.phase ?? "content";
  const nextPhase = (uid: string): Phase => store.proposedPlacementByUid.get(uid)?.phase ?? "content";

  const current = $derived(store.active);
  const proposed = $derived(store.preview?.order ?? []);
  const report = $derived(analyseMoves(current, proposed, nowPhase, nextPhase, (uid) => store.proposedPlacementByUid.get(uid)?.reason ?? ""));
  /** A phase clicked on either bar: the board and the list narrow to the moves that touch it. */
  let only = $state<Phase | null>(null);
  /** The search box narrows the board too: on a list with hundreds of moves, finding the one mod
   *  you came for beats scrolling for it. */
  const q = $derived(store.query.trim().toLowerCase());
  const moves = $derived(
    report.relocations.filter((r) => {
      if (only && r.fromPhase !== only && r.toPhase !== only) return false;
      if (!q) return true;
      const m = store.byUid.get(r.uid);
      return !!m && (m.name.toLowerCase().includes(q) || m.packageId.toLowerCase().includes(q));
    })
  );
  const narrowed = $derived(moves.length !== report.relocations.length);
  /** The mod under the pointer, on the board or in the list: both ends light up together. */
  let hot = $state<string | null>(null);

  // ---- the two bars: one band per run of mods filed under the same phase ----
  type Band = { phase: Phase; start: number; count: number };
  function bands(order: string[], phase: (uid: string) => Phase): Band[] {
    const out: Band[] = [];
    for (let i = 0; i < order.length; i++) {
      const p = phase(order[i]);
      const last = out[out.length - 1];
      if (last && last.phase === p) last.count++;
      else out.push({ phase: p, start: i, count: 1 });
    }
    return out;
  }
  const leftBands = $derived(bands(current, nowPhase));
  const rightBands = $derived(bands(proposed, nextPhase));
  /** Both bars are the same list, so both are measured against the same total. */
  const n = $derived(Math.max(current.length, proposed.length, 1));
  /** How tall the bars are drawn, so labels can be spaced in pixels rather than in percentages. */
  let barH = $state(260);
  /** The bands worth naming: each phase once, at its longest run, biggest first, and only where
   *  the name will not run into one already placed. A list can hold a hundred runs of a single
   *  mod, and none of those is a label. */
  function labels(all: Band[]): { name: string; top: number }[] {
    const gap = (13 / Math.max(barH, 1)) * 100;
    const out: { name: string; top: number }[] = [];
    const said = new Set<Phase>();
    for (const b of [...all].sort((a, z) => z.count - a.count)) {
      if ((b.count / n) * 100 < gap) break;
      const top = ((b.start + b.count / 2) / n) * 100;
      if (said.has(b.phase) || out.some((o) => Math.abs(o.top - top) < gap)) continue;
      said.add(b.phase);
      out.push({ name: phaseOf(b.phase).name, top });
    }
    return out;
  }
  const leftLabels = $derived(labels(leftBands));
  const rightLabels = $derived(labels(rightBands));

  // ---- the lines between the bars ----
  let board = $state<HTMLDivElement | null>(null);
  let sheet = $state<SVGSVGElement | null>(null);
  let leftBar = $state<HTMLDivElement | null>(null);
  let rightBar = $state<HTMLDivElement | null>(null);
  let wires = $state<{ uid: string; d: string; color: string }[]>([]);
  function measure() {
    if (!sheet || !leftBar || !rightBar) return;
    // Against the sheet the lines are drawn on, not the board: an absolutely placed child of a
    // grid takes its grid area as its origin, and that area starts below the captions.
    const b = sheet.getBoundingClientRect();
    const l = leftBar.getBoundingClientRect();
    const r = rightBar.getBoundingClientRect();
    barH = l.height;
    const x1 = l.right - b.left;
    const x2 = r.left - b.left;
    const mx = (x1 + x2) / 2;
    wires = moves.map((m) => {
      const y1 = l.top - b.top + ((m.from + 0.5) / n) * l.height;
      const y2 = r.top - b.top + ((m.to + 0.5) / n) * r.height;
      // Held flat at both ends so a line leaves the bar it belongs to and arrives at the other.
      return { uid: m.uid, d: `M${x1},${y1} C${mx},${y1} ${mx},${y2} ${x2 - 9},${y2}`, color: phaseOf(m.toPhase).color };
    });
  }
  $effect(() => {
    // Re-draw whenever the proposal, the narrowing or the bars themselves change shape.
    void moves; void leftBands; void rightBands;
    tick().then(measure);
  });
  onMount(() => {
    const ro = new ResizeObserver(() => measure());
    if (board) ro.observe(board);
    window.addEventListener("resize", measure);
    return () => { ro.disconnect(); window.removeEventListener("resize", measure); };
  });

  // ---- the list ----
  /** How far a mod travels, said the way a list is read: up is earlier, down is later. */
  const distance = (m: Relocation) => ({ up: m.to < m.from, n: Math.abs(m.to - m.from) });
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
    {#if only}
      <button class="chip on" onclick={() => (only = null)} title="Show every move again">{phaseOf(only).name}<span class="x">{@html I.close}</span></button>
    {/if}
  </header>

  {#if report.relocations.length}
    <div class="board card" bind:this={board}>
      <div class="cap left">The order you have<span class="num">{current.length}</span></div>
      <div class="cap right">The order HALO proposes<span class="num">{proposed.length}</span></div>

      <div class="side l">
        {#each leftLabels as t (t.name)}<span class="lbl" style="top: {t.top}%">{t.name}</span>{/each}
      </div>
      <div class="bar l" bind:this={leftBar}>
        {#each leftBands as b, i (i)}
          <button class="band c-{phaseOf(b.phase).color}" class:dim={only != null && only !== b.phase} style="top: {(b.start / n) * 100}%; height: {(b.count / n) * 100}%"
            title="{phaseOf(b.phase).name} — {b.count} mod{b.count === 1 ? '' : 's'}, #{b.start + 1} to #{b.start + b.count}"
            aria-label="{phaseOf(b.phase).name}, {b.count} mods" onclick={() => (only = only === b.phase ? null : b.phase)}></button>
        {/each}
        {#each moves as m (m.uid)}<span class="tick" class:hot={hot === m.uid} style="top: {((m.from + 0.5) / n) * 100}%"></span>{/each}
      </div>

      <svg class="wires" class:dense={moves.length > 120} bind:this={sheet} aria-hidden="true">
        <defs>
          {#each PHASES as p (p.id)}
            <marker id="mv-{p.color}" viewBox="0 0 8 8" refX="7.4" refY="4" markerWidth="7.5" markerHeight="7.5" orient="auto">
              <path d="M0.4,0.6 L8,4 L0.4,7.4 Z" fill="var(--{p.color})" />
            </marker>
          {/each}
        </defs>
        {#each wires as w (w.uid)}
          <path d={w.d} stroke="var(--{w.color})" marker-end="url(#mv-{w.color})" class:hot={hot === w.uid} class:cold={hot != null && hot !== w.uid} />
        {/each}
      </svg>

      <div class="bar r" bind:this={rightBar}>
        {#each rightBands as b, i (i)}
          <button class="band c-{phaseOf(b.phase).color}" class:dim={only != null && only !== b.phase} style="top: {(b.start / n) * 100}%; height: {(b.count / n) * 100}%"
            title="{phaseOf(b.phase).name} — {b.count} mod{b.count === 1 ? '' : 's'}, #{b.start + 1} to #{b.start + b.count}"
            aria-label="{phaseOf(b.phase).name}, {b.count} mods" onclick={() => (only = only === b.phase ? null : b.phase)}></button>
        {/each}
        {#each moves as m (m.uid)}<span class="tick" class:hot={hot === m.uid} style="top: {((m.to + 0.5) / n) * 100}%"></span>{/each}
      </div>
      <div class="side r">
        {#each rightLabels as t (t.name)}<span class="lbl" style="top: {t.top}%">{t.name}</span>{/each}
      </div>
      <p class="hint">Each line is one mod lifted out of its place; where it starts and ends is where it sits in each order. Click a phase to keep only the moves that touch it.</p>
    </div>

    <div class="rows" role="list">
      {#each moves as m (m.uid)}
        {@const d = distance(m)}
        <div class="mv" class:hot={hot === m.uid} class:sel={store.selected.includes(m.uid)} role="listitem" data-uid={m.uid}
          onmouseenter={() => (hot = m.uid)} onmouseleave={() => (hot = null)}>
          <button class="hit" onclick={() => store.select(m.uid)} title="Select {nameOf(m.uid)}">
            <span class="dist" class:up={d.up}>{@html d.up ? I.rise : I.fall}<span class="num">{d.n.toLocaleString()}</span></span>
            <span class="body">
              <span class="line">
                <b class="nm">{nameOf(m.uid)}</b>
                {#if m.fromPhase !== m.toPhase}
                  <span class="pill"><span class="dot c-{phaseOf(m.fromPhase).color}"></span>{phaseOf(m.fromPhase).name}</span>
                  <span class="arrow">{@html I.right}</span>
                  <span class="pill"><span class="dot c-{phaseOf(m.toPhase).color}"></span>{phaseOf(m.toPhase).name}</span>
                {:else}
                  <span class="pill"><span class="dot c-{phaseOf(m.toPhase).color}"></span>{phaseOf(m.toPhase).name}</span>
                  <span class="within">a new place within the phase</span>
                {/if}
                <span class="pos num">#{m.from + 1}<span class="arrow">{@html I.right}</span>#{m.to + 1}</span>
              </span>
              <span class="line two">
                <span class="was">{m.wasAfterUid ? `was after ${nameOf(m.wasAfterUid)}` : "was first in the list"}</span>
                <span class="now">{m.afterUid ? `now after ${nameOf(m.afterUid)}` : m.beforeUid ? `now at the top, before ${nameOf(m.beforeUid)}` : "now on its own"}</span>
                {#if m.reason}<span class="why">{m.reason}</span>{/if}
              </span>
            </span>
          </button>
        </div>
      {:else}
        <p class="empty">{q ? `No mod HALO would move matches “${store.query.trim()}”` : `No move touches ${phaseOf(only ?? "content").name.toLowerCase()}`}.</p>
      {/each}
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

  /* The board. Both bars are the same list of mods, top to bottom: the left one in the order the
     user has, the right one in HALO's. A line is one mod, and its slope is the move itself. */
  .board { position: relative; display: grid; grid-template-columns: auto 16px minmax(90px, 1fr) 16px auto; grid-template-rows: auto minmax(0, 1fr) auto; row-gap: 8px; padding: 12px 14px; height: clamp(230px, 34vh, 360px); flex: none; }
  .cap { grid-row: 1; font-size: 11px; font-weight: 700; letter-spacing: 0.07em; text-transform: uppercase; color: var(--text-2); display: flex; align-items: baseline; gap: 6px; white-space: nowrap; }
  .cap .num { font-size: 11px; color: var(--text-3); letter-spacing: 0; }
  .cap.left { grid-column: 1 / 3; }
  .cap.right { grid-column: 4 / 6; justify-content: flex-end; }
  .side { grid-row: 2; position: relative; width: 116px; }
  .side.l { grid-column: 1; }
  .side.r { grid-column: 5; }
  .lbl { position: absolute; transform: translateY(-50%); font-size: 10.5px; font-weight: 600; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; max-width: 100%; }
  .side.l .lbl { right: 8px; text-align: right; }
  .side.r .lbl { left: 8px; }
  .bar { grid-row: 2; position: relative; border-radius: 6px; background: var(--surface-2); overflow: hidden; }
  .bar.l { grid-column: 2; }
  .bar.r { grid-column: 4; }
  .band { position: absolute; left: 0; right: 0; display: block; padding: 0; border-radius: 0; background: var(--c, var(--text-3)); opacity: 0.6; transition: opacity 0.12s; }
  .band:hover { opacity: 0.95; }
  .band.dim { opacity: 0.14; }
  .tick { position: absolute; left: 0; right: 0; height: 2px; margin-top: -1px; background: var(--text); opacity: 0.5; pointer-events: none; }
  .tick.hot { opacity: 1; height: 4px; margin-top: -2px; }
  /* Over the whole board, so a line is measured in the board's own coordinates from one bar to
     the other; the bars sit above it and the lines only ever span the height they cover. */
  .wires { grid-column: 1 / -1; grid-row: 2; position: absolute; inset: 0; width: 100%; height: 100%; pointer-events: none; overflow: visible; }
  .wires path { fill: none; stroke-width: 1.4px; opacity: 0.75; }
  .wires.dense path { stroke-width: 1.1px; opacity: 0.5; }
  .wires path.hot { stroke-width: 2.6px; opacity: 1; }
  .wires path.cold { opacity: 0.15; }
  .board .hint { grid-column: 1 / -1; grid-row: 3; margin: 0; font-size: 11.5px; color: var(--text-3); line-height: 1.4; }

  /* One row per move: what changed on the first line, what it changed against on the second. */
  .rows { flex: 1; min-height: 0; overflow: hidden auto; display: flex; flex-direction: column; gap: 4px; padding-right: 2px; }
  .mv { border-radius: var(--r-row); background: var(--surface); }
  .mv.hot { background: var(--surface-2); }
  .mv.sel { box-shadow: inset 0 0 0 1px var(--amber); }
  .hit { display: grid; grid-template-columns: 58px minmax(0, 1fr); align-items: center; gap: 10px; width: 100%; padding: 7px 12px 7px 8px; text-align: left; border-radius: var(--r-row); }
  .dist { display: inline-flex; align-items: center; justify-content: flex-end; gap: 4px; font-size: 12px; font-weight: 700; color: var(--amber); }
  .dist :global(svg) { width: 13px; height: 13px; display: block; }
  .dist.up { color: var(--blue); }
  .body { min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .line { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .nm { font-size: 13px; font-weight: 650; color: var(--text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; flex: 0 1 auto; }
  .pill { display: inline-flex; align-items: center; gap: 5px; height: 19px; padding: 0 8px; border-radius: var(--r-pill); background: var(--surface-2); font-size: 11px; font-weight: 600; color: var(--text-2); white-space: nowrap; flex: none; }
  .arrow { display: inline-flex; flex: none; color: var(--text-3); }
  .arrow :global(svg) { width: 11px; height: 11px; display: block; }
  .within { font-size: 11px; color: var(--text-3); white-space: nowrap; flex: none; }
  .pos { margin-left: auto; font-size: 11.5px; color: var(--text-3); display: inline-flex; align-items: center; gap: 4px; white-space: nowrap; flex: none; }
  .two { font-size: 11.5px; color: var(--text-3); gap: 10px; }
  .two > span { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .two .was, .two .now { flex: 0 1 auto; }
  .two .why { color: var(--text-2); flex: 1 1 auto; min-width: 0; }
  .empty { font-size: 13px; color: var(--text-3); padding: 14px; margin: 0; }
  @media (max-width: 1240px) { .side { width: 76px; } .lbl { font-size: 10px; } }
</style>
