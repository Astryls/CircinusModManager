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
    <div class="meter c-amber" style="--v:{Math.min(100, s.collisions * 4)}%"><i></i></div>
    <div class="cap">{#if s.collisions}<span class="flag">{@html I.note}</span>{/if}<span class="t">{s.collisions ? `${s.collidingMods} mods replace the same files. The later mod wins.${store.snap?.issuesTruncated ? ` Only the first ${s.collisions.toLocaleString()} are listed.` : ""}` : "No texture is replaced by two mods"}</span></div>
  </button>
  {#if store.showWeight}
    <button class="card stat" class:on={store.showOnly === "heavy"} title={perfTip} onclick={onPerf}>
      <div class="l"><span>Performance</span><span class="v num" class:att={s.share >= 10} class:neg={s.share >= 25}>{s.withShare ? `${s.share.toFixed(1)} %` : s.measured ? `${s.measured} rated` : "no figures"}</span></div>
      <div class="meter" style="--c: var(--{weightColor}); --v:{Math.min(100, s.share)}%"><i></i></div>
      <div class="cap"><span class="t">{s.withShare ? `of frame time, adding up the ${s.withShare} of ${store.active.length} active mods with a number` : s.measured ? `${s.measured} of ${store.active.length} active mods have a rating but no number yet` : "Fetch figures from circinus.sh in Settings"}</span></div>
    </button>
  {:else}
    <button class="card stat" class:on={store.showOnly === "warning"} title={warnTip} onclick={onPerf}>
      <div class="l"><span>Warnings</span><span class="v num" class:att={s.warnings > 0}>{s.warnings}</span></div>
      <div class="meter c-amber" style="--v:{Math.min(100, s.warnings * 8)}%"><i></i></div>
      <div class="cap">{#if s.warnings}<span class="flag">{@html I.warn}</span>{/if}<span class="t">{s.warnings ? "Order, version and duplicate notes" : "Nothing to review"}</span></div>
    </button>
  {/if}
</div>

<style>
  .stats { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 12px; container-type: inline-size; }
  .stat { padding: 12px 14px; min-width: 0; overflow: hidden; display: block; text-align: left; color: inherit; font: inherit; cursor: pointer; transition: background 0.12s, box-shadow 0.12s; }
  .stat:hover { background: var(--surface-2); }
  .stat.on { box-shadow: 0 0 0 1.5px var(--amber) inset; }
  .stat .l { display: flex; justify-content: space-between; align-items: baseline; gap: 4px 10px; flex-wrap: wrap; }
  .stat .l span:first-child { font-weight: 600; font-size: 13px; }
  .stat .v { font-weight: 800; font-size: 17px; letter-spacing: -0.02em; white-space: nowrap; margin-left: auto; }
  @container (max-width: 900px) { .stats { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
  .stat .meter { margin: 8px 0 7px; }
  .stat .cap { font-size: 12px; color: var(--text-3); display: flex; align-items: flex-start; gap: 6px; min-height: 34px; line-height: 1.4; }
  .stat .cap .flag { margin-top: 1px; }
  .stat .cap .t { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }

</style>
