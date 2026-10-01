<script lang="ts">
  import { store, type ShowOnly } from "$lib/store.svelte";
  import { I } from "$lib/icons";
  import { describe } from "$lib/describe";
  import { severityOf } from "$lib/types";

  const s = $derived(store.stats);
  const errorIssue = $derived(store.issues.find((i) => severityOf(i) === "error" && (i.kind === "aboveOfficial" || i.kind === "incompatible" || i.kind === "missingDependency" || i.kind === "cycle")));
  const errorText = $derived.by(() => {
    if (!errorIssue) return "Nothing blocks a launch";
    const n = (uid: string) => store.byUid.get(uid)?.name ?? uid;
    if (errorIssue.kind === "aboveOfficial") return `${n(errorIssue.uid)} sits above ${n(errorIssue.officialUid)}`;
    if (errorIssue.kind === "incompatible") return `${n(errorIssue.uid)} conflicts with ${n(errorIssue.otherUid)}`;
    if (errorIssue.kind === "missingDependency") return `${n(errorIssue.uid)} needs ${errorIssue.displayName ?? errorIssue.dependency}`;
    if (errorIssue.kind === "cycle") return errorIssue.chain;
    return "";
  });
  // "green" aliases to the bone bar: a figure that is fine does not need a colour.
  const weightColor = $derived(s.share < 10 ? "green" : s.share < 25 ? "amber" : "red");
  const n = (uid: string) => store.byUid.get(uid)?.name ?? uid;

  // ---- what the tiles say on hover, in full, and where a click takes you ----
  const violations = $derived(store.issues.filter((i) => i.kind === "orderViolation"));
  const rulesTip = $derived.by(() => {
    const head = `${s.satisfied.toLocaleString()} of ${s.orderRules.toLocaleString()} load order rules between active mods are met.`;
    if (!s.violations) return `${head}\n\nClick to open the Analyzer.`;
    const lines = violations.slice(0, 6).map((i) => (i.kind === "orderViolation" ? `• ${n(i.uid)} should load ${i.rule === "loadAfter" ? "after" : "before"} ${n(i.targetUid)}${i.comment ? ` (${i.comment})` : ""}` : "")).join("\n");
    return `${head}\n\nNot met:\n${lines}${violations.length > 6 ? `\n• and ${violations.length - 6} more` : ""}\n\nClick to show only the mods with rule problems.`;
  });
  const errorsTip = $derived.by(() => {
    if (!s.errors) return "Nothing blocks a launch.\n\nClick to open the Analyzer.";
    const errs = store.issues.filter((i) => severityOf(i) === "error").slice(0, 6).map((i) => `• ${describe(i, store.byUid)}`).join("\n");
    return `${s.errors} error${s.errors === 1 ? "" : "s"}. RimWorld resets or refuses the list over these.\n\n${errs}${s.errors > 6 ? `\n• and ${s.errors - 6} more` : ""}\n\nClick to show only the mods with errors.`;
  });
  const texTip = $derived.by(() => {
    if (!s.collisions) return "No texture is replaced by two active mods.\n\nClick to open the Textures view.";
    const wins = new Map<string, number>();
    for (const i of store.issues) if (i.kind === "textureCollision") wins.set(i.winnerUid, (wins.get(i.winnerUid) ?? 0) + 1);
    const top = [...wins.entries()].sort((a, b) => b[1] - a[1]).slice(0, 5).map(([u, c]) => `• ${n(u)} wins ${c.toLocaleString()} file${c === 1 ? "" : "s"}`).join("\n");
    const total = s.collisions + (store.snap?.issuesTruncated ?? 0);
    return `${total.toLocaleString()} texture${total === 1 ? "" : "s"} ${total === 1 ? "is" : "are"} shipped by more than one active mod; ${s.collidingMods} mods are involved. The mod that loads later wins each file, so this is a note, not a problem, unless the wrong one wins.${store.snap?.issuesTruncated ? ` Only the first ${s.collisions.toLocaleString()} are listed in the Analyzer.` : ""}\n\nWinning most often:\n${top}\n\nClick to show only the mods involved.`;
  });
  const perfTip = $derived.by(() => {
    if (!store.showWeight) return `${s.warnings} warning${s.warnings === 1 ? "" : "s"}: order, version and duplicate notes.\n\nClick to show only the mods with warnings.`;
    if (!s.measured && !s.withShare) return "No performance figures loaded yet. Circinus shows each mod's share of frame time as measured by the Circinus profiler.\n\nClick to open Settings, where you can fetch them from circinus.sh.";
    const heavy = store.active.map((u) => store.byUid.get(u)).filter((m) => m && store.passesShowOnly(m.uid, "heavy")).length;
    const body = s.withShare ? `The ${s.withShare} of ${store.active.length} active mods with a number add up to ${s.share.toFixed(1)} % of frame time (a rough guide: long lists spread the total thin).` : `${s.measured} of ${store.active.length} active mods have a rating but no number yet; a number needs 25 clean runs from 10 players.`;
    return `${body}\n${heavy} active mod${heavy === 1 ? " is" : "s are"} rated heavy or very heavy.\n\nClick to show only the heavy ones.`;
  });
  const warnTip = $derived(perfTip);

  // ---- how long the game takes to start ----
  //
  // The other four cards state facts, and so does this one now. It used to state either a
  // measurement or a model made of nine hand-rounded coefficients, and lean on the caption to
  // say which -- which is a guess set in 17px bold with the hedge underneath it, and people
  // quote the number.
  //
  // So the model is gone from here entirely. One source: what the Loading Progress mod, by
  // ilyvion, recorded. On a 1,078-mod install the old fallback read "6m 39s" and nothing had
  // ever observed it. There is no figure to show without a measurement, so the card shows no
  // figure, and says what to install.
  //
  // `loadRun` -- the total scraped out of the game's own log -- is deliberately not a fallback
  // either. It is a real measurement of a real start-up, but it is not per mod and it is not
  // what this card is for; it lives on the Load times page. A card whose whole rule is that it
  // holds one kind of number cannot have a second kind for when the first is missing.
  //
  // Nothing here comes from circinus.sh and nothing should. Pooled medians are other people's
  // machines: useful for ranking one mod against another on its own page, worthless as a claim
  // about how long YOUR game takes to start, and putting them in a card headed with this
  // machine's name would be passing somebody else's measurement off as yours.
  const run = $derived(store.loadRun);
  const impact = $derived(store.startupImpact);
  /* Which of the three states this card is in: measured and current, measured but the list
   * has moved on since, or nothing measured at all. */
  const kind = $derived.by<"measured" | "lastrun" | "none">(() => {
    if (!impact) return "none";
    return store.loadRunMatchesList ? "measured" : "lastrun";
  });
  /** Seconds, or null when nothing has measured one. Null is a real state here, not a zero. */
  const loadSecs = $derived(impact ? impact.totalMs / 1000 : null);
  /** Minutes and seconds past a minute: "7m 43s" reads; "462.99 s" does not. */
  const clock = (secs: number) => (secs >= 60 ? `${Math.floor(secs / 60)}m ${Math.round(secs % 60)}s` : `${Math.round(secs)}s`);
  const ago = (at: number) => {
    if (!at) return "";
    const days = Math.floor((Date.now() / 1000 - at) / 86400);
    return days <= 0 ? "today" : days === 1 ? "yesterday" : `${days} days ago`;
  };
  /** `n` is already the uid-to-name helper in this file, so the number formatter is `num`. */
  const num = (x: number) => x.toLocaleString();
  /** The previous kept report, for "and the one before that". */
  const prev = $derived(store.startupHistory[1] ?? null);
  /* Why there is no figure, which has four different cures and used to have one sentence.
   * "Loading Progress has not written a report" is true of a machine where the mod is not
   * installed, one where it is installed and inactive, one where it is active and not
   * tracking, and one where everything is on and the game has simply not been started since
   * -- and only the last of those needs no action at all. */
  const lp = $derived(store.loadTracking);
  const loadCap = $derived.by(() => {
    if (kind === "measured") return `Measured ${ago(store.startupImpactAt)} by Loading Progress, every mod in this list`;
    // Still a measurement, and the caption says what of: the last start-up, not this list.
    if (kind === "lastrun") return `Your last start-up, ${ago(store.startupImpactAt)}, measured by Loading Progress. Your list has changed since`;
    if (!lp.installed) return "Not measured. Loading Progress, by ilyvion, is the mod that measures this";
    if (!store.loadTrackingOn) return "Not measured. Loading Progress is installed but not tracking \u2014 switch it on in Settings";
    if (!lp.active) return "Tracking is on, but Loading Progress is not in your active list";
    return "Tracking is on. Start the game once and the figure appears";
  });
  const loadTip = $derived.by(() => {
    const click = "\n\nClick to show the mods slowest to load.";
    const since = prev ? `\n\nThe report before it, ${ago(prev.at)}, was ${clock(prev.totalMs / 1000)} over ${num(prev.mods)} mods. Circinus keeps the last ${num(store.startupHistory.length)}.` : "";
    if (kind === "measured" && impact) {
      return `${clock(impact.totalMs / 1000)} the last time the game started, ${ago(store.startupImpactAt)}.\n\nEvery mod in this list was timed individually by Loading Progress, by ilyvion. Circinus read the figures it wrote and did not time anything itself; the measuring is entirely that mod's work, and this card would have nothing to show without it.${since}${click}`;
    }
    if (kind === "lastrun" && impact) {
      return `${clock(impact.totalMs / 1000)} the last time the game started, ${ago(store.startupImpactAt)}, timed by Loading Progress, by ilyvion.\n\nThat is what the start-up took, not what this list would take: ${num(impact.mods.length)} mods were loaded then and ${num(store.active.length)} are active now. Launch again and the figure will describe the list you have.${since}${click}`;
    }
    const head = "Nothing has measured your start-up.\n\nThis card shows one thing: what the Loading Progress mod, by ilyvion, recorded. Circinus does not time loading and will not put a guess here -- a modelled figure in this spot is the sort of number people quote back as a fact.";
    if (!lp.installed) return `${head}\n\nInstall Loading Progress, then switch on "Track startup loading impact" and "Auto-save startup impact report" -- Circinus can do both from Settings -- and start the game once.\n\nClick to open Settings.`;
    if (!store.loadTrackingOn) return `${head}\n\nLoading Progress is installed and is not tracking: "Track startup loading impact" and "Auto-save startup impact report" are both off, which is how it ships. Circinus can switch both on for you from Settings, while the game is closed.\n\nClick to open Settings.`;
    if (!lp.active) return `${head}\n\nTracking is on, but Loading Progress is not in your active list, so nothing runs it. Activate it and start the game once.\n\nClick to open Settings.`;
    return `${head}\n\nTracking is on and the next start-up will be measured. Start the game once; if you already have, press the refresh mark to read the report again.\n\nClick to open Settings.`;
  });
  /* The refresh mark. Reading is automatic at launch and whenever the window comes back to the
   * front, which covers playing and alt-tabbing back. What it cannot cover is the launch you
   * did *before* switching tracking on: the file is new, and the window has no reason to think
   * anything changed. There is a Re-read button on the Load times page and in Settings already;
   * this is the same call on the card people are actually looking at when they wonder. */
  const rereadTip = "Read the start-up report again.\n\nCircinus already does this at launch and when you come back to the window. Press it after a launch you did before switching tracking on, or if the figure looks older than your last game.\n\nThe figure itself only changes when RimWorld starts: that is when Loading Progress writes it.";
  // Nothing about seconds is a percentage, so the meter needs a scale invented for it. Ten
  // minutes is the top: past that the bar is full and the number is the thing being read anyway.
  const loadPct = $derived(loadSecs == null ? 0 : Math.min(100, (loadSecs / 600) * 100));
  const loadColor = $derived(loadSecs == null ? "slate" : loadSecs < 120 ? "green" : loadSecs < 360 ? "amber" : "red");

  function narrow(what: ShowOnly) {
    store.view = "order";
    store.showOnly = store.showOnly === what ? null : what;
  }
  function onRules() {
    if (s.violations) narrow("conflict");
    else store.view = "analyzer";
  }
  function onErrors() {
    if (s.errors) { narrow("error"); store.reviewNext(); }
    else store.view = "analyzer";
  }
  /* With a measurement, the card narrows the list to the slowest mods, which is what it is for.
   * With none, narrowing to "slow" shows nothing and explains nothing, so the card goes where
   * the cure is -- the same bargain the Performance card already makes when it has no figures. */
  function onLoad() {
    if (loadSecs == null) store.view = "settings";
    else narrow("slow");
  }
  function onTextures() {
    if (s.collisions) narrow("collision");
    else store.view = "textures";
  }
  function onPerf() {
    if (!store.showWeight) { if (s.warnings) narrow("warning"); else store.view = "analyzer"; return; }
    if (!s.measured && !s.withShare) store.view = "settings";
    else narrow("heavy");
  }
</script>

<div class="strip">
  <div class="stats">
  <button class="card stat" class:on={store.showOnly === "conflict"} title={rulesTip} onclick={onRules}>
    <div class="l"><span>Rules met</span><span class="v num" class:pos={s.pct === 100} class:att={s.pct < 100}>{s.pct} %</span></div>
    <div class="meter" style="--c: var(--{s.pct === 100 ? 'green' : s.pct >= 90 ? 'amber' : 'red'}); --v:{s.pct}%"><i></i></div>
    <div class="cap"><span class="t">{s.satisfied.toLocaleString()} of {s.orderRules.toLocaleString()} load order rules{s.violations ? `, ${s.violations} not met` : ""}</span></div>
  </button>
  <button class="card stat" class:on={store.showOnly === "error"} title={errorsTip} onclick={onErrors}>
    <div class="l"><span>Errors</span><span class="v num" class:neg={s.errors > 0}>{s.errors}</span></div>
    <div class="meter c-red" style="--v:{Math.min(100, s.errors * 12)}%"><i></i></div>
    <div class="cap">{#if s.errors}<span class="flag">{@html I.error}</span>{/if}<span class="t">{errorText}</span></div>
  </button>
  <button class="card stat" class:on={store.showOnly === "collision"} title={texTip} onclick={onTextures}>
    <div class="l"><span>Overlapping textures</span><span class="v num" class:att={s.collisions > 0}>{(s.collisions + (store.snap?.issuesTruncated ?? 0)).toLocaleString()}</span></div>
    <div class="meter" style="--v:{Math.min(100, s.collisions * 4)}%"><i></i></div>
    <div class="cap">{#if s.collisions}<span class="flag">{@html I.note}</span>{/if}<span class="t">{s.collisions ? `${s.collidingMods} mods replace the same files. The later mod wins.${store.snap?.issuesTruncated ? ` Only the first ${s.collisions.toLocaleString()} are listed.` : ""}` : "No texture is replaced by two mods"}</span></div>
  </button>
  <!-- The one card with a control of its own, so it is a cell holding two buttons rather than
       one button: an interactive element inside another is invalid, and a refresh mark that is
       part of the card's own click would narrow the list every time somebody pressed it. -->
  <div class="cell">
    <button class="card stat" class:on={store.showOnly === "slow"} title={loadTip} onclick={onLoad}>
      <div class="l pad"><span>Load time</span><span class="v num" class:att={loadSecs != null && loadSecs >= 120} class:neg={loadSecs != null && loadSecs >= 360} class:none={loadSecs == null}>{loadSecs == null ? "\u2014" : clock(loadSecs)}</span></div>
      <div class="meter" style="--c: var(--{loadColor}); --v:{loadPct}%"><i></i></div>
      <div class="cap">{#if kind !== "measured"}<span class="flag">{@html I.note}</span>{/if}<span class="t">{loadCap}</span></div>
    </button>
    <button class="rf" title={rereadTip} aria-label="Read the start-up report again" disabled={!!store.busy} onclick={() => store.rereadLoadRun()}>{@html I.refresh}</button>
  </div>
  {#if store.showWeight}
    <button class="card stat" class:on={store.showOnly === "heavy"} title={perfTip} onclick={onPerf}>
      <div class="l"><span>Performance</span><span class="v num" class:att={s.share >= 10} class:neg={s.share >= 25}>{s.withShare ? `${s.share.toFixed(1)} %` : s.measured ? `${s.measured} rated` : "no figures"}</span></div>
      <div class="meter" style="--c: var(--{weightColor}); --v:{Math.min(100, s.share)}%"><i></i></div>
      <div class="cap"><span class="t">{s.withShare ? `of frame time, adding up the ${s.withShare} of ${store.active.length} active mods with a number` : s.measured ? `${s.measured} of ${store.active.length} active mods have a rating but no number yet` : "Fetch figures from circinus.sh in Settings"}</span></div>
    </button>
  {:else}
    <button class="card stat" class:on={store.showOnly === "warning"} title={warnTip} onclick={onPerf}>
      <div class="l"><span>Warnings</span><span class="v num" class:att={s.warnings > 0}>{s.warnings}</span></div>
      <div class="meter" style="--v:{Math.min(100, s.warnings * 8)}%"><i></i></div>
      <div class="cap">{#if s.warnings}<span class="flag">{@html I.warn}</span>{/if}<span class="t">{s.warnings ? "Order, version and duplicate notes" : "Nothing to review"}</span></div>
    </button>
  {/if}
  </div>
</div>

<style>
  /* The wrapper is the container, not the grid itself. A container query matches *descendants*
     of a container, so `.stats` carrying `container-type` and then querying `.stats` -- which is
     what this did -- never matched anything, and the one breakpoint it declared never fired.
     One element exists solely so the grid has something to ask about its own width. */
  .strip { container-type: inline-size; }
  /* One row, always. A second row is not free: it costs the mod list under it about 113 pixels,
     at every width, for ever -- and the list is what the window is for. So the strip never wraps,
     and the cards give way instead. Each is its own container (a container query matches
     *descendants*, so a card can ask about its own width and style what is inside it) and sheds
     what it can afford to as it narrows.
     What it never sheds is the caption, because on the load-time card that is the word
     "Estimated" or "Measured" -- the difference between a fact and a model. It wraps to as many
     lines as it needs; it is never clamped and never hidden. */
  .stats { display: grid; grid-template-columns: repeat(5, minmax(0, 1fr)); gap: 12px; }
  .stat { padding: 12px 14px; min-width: 0; overflow: hidden; display: block; text-align: left; color: inherit; font: inherit; cursor: pointer; transition: background 0.12s, box-shadow 0.12s; container-type: inline-size; }
  .stat:hover { background: var(--surface-2); }
  .stat.on { box-shadow: 0 0 0 1.5px var(--amber) inset; }
  .stat .l { display: flex; justify-content: space-between; align-items: baseline; gap: 4px 10px; flex-wrap: wrap; }
  .stat .l span:first-child { font-weight: 600; font-size: 13px; min-width: 0; overflow-wrap: anywhere; }
  .stat .v { font-weight: 800; font-size: 17px; letter-spacing: -0.02em; white-space: nowrap; margin-left: auto; }
  /* Nothing measured. The dash takes the weight of the figure it stands in for, so the card
     keeps its shape, but in the ink of an absence rather than of a reading. */
  .stat .v.none { color: var(--text-4); }

  .stat .meter { margin: 8px 0 7px; }
  .stat .cap { font-size: 12px; color: var(--text-3); display: flex; align-items: flex-start; gap: 6px; min-height: 34px; line-height: 1.4; }
  .stat .cap .flag { margin-top: 1px; flex: none; }
  /* Wrapped, never cut. A caption clamped to two lines with an ellipsis is a sentence the reader
     has to hover to finish, and the thing it cuts off is usually the half that says what the
     number means -- "Measured yesterday, from…" loses the name of whoever measured it. Cards in a
     grid row are all as tall as the tallest, so the strip grows a little instead; it is still one
     row, which is the part that costs the list underneath. `anywhere` because a mod name or a
     path has no spaces to break at and would otherwise push the card wider than its column. */
  .stat .cap .t { min-width: 0; overflow-wrap: anywhere; }
  /* Room for the refresh mark in the corner, taken out of the value row rather than out of the
     caption. Both would have worked and only one of them is free: the caption is six or seven
     wrapped lines on this card, so 22px off its width is an extra line at most widths and 17px
     of strip height for ever -- which is the cost this whole file exists to refuse. The value
     row is one short label and one short figure with slack between them, so the same 22px
     changes nothing until the card is very narrow. `localize.cjs` is what measured that: it
     counts the rows the list can draw, and the caption version cost it one. */
  .stat .l.pad { padding-right: 22px; }

  /* The load card's own cell: the card fills it, the refresh mark floats in its corner. The
     wrapper is the grid item, so `.stat`'s container query still measures the card itself. */
  .cell { position: relative; min-width: 0; display: grid; }
  .cell > .card { height: 100%; }
  .rf { position: absolute; right: 7px; top: 9px; width: 22px; height: 22px; display: grid; place-items: center; background: transparent; color: var(--text-4); box-shadow: none; }
  .rf:hover { color: var(--text); background: var(--surface-3); }
  .rf:disabled { opacity: 0.4; cursor: default; }
  .rf :global(svg) { width: 14px; height: 14px; }

  /* Narrowing, in the order a card can least afford to lose things. The label wraps above its
     value on its own (the row is `flex-wrap: wrap`), which is why nothing here has to touch it.
     These widths are the card's *content* box, which is what `container-type: inline-size`
     measures -- 28px narrower than the card, at this padding. Reading them as card widths is how
     the caption came to be clamped to one line a full 30px earlier than intended. */
  @container (max-width: 150px) {
    .stat { padding: 11px 12px; }
    .stat .cap { min-height: 0; }
  }
  @container (max-width: 120px) {
    .stat { padding: 10px 10px; }
    .stat .l span:first-child { font-size: 12px; }
    .stat .v { font-size: 15px; }
    .stat .meter { margin: 7px 0 6px; }
    .stat .cap { font-size: 11.5px; }
    /* The flag icon is a second way of saying what the caption says; the caption is the one that
       survives, since a coloured dot at this size is decoration. */
    .stat .cap .flag { display: none; }
  }

</style>
