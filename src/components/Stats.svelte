<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { I } from "$lib/icons";

  const s = $derived(store.stats);
  const errorIssue = $derived(store.issues.find((i) => i.kind === "aboveOfficial" || i.kind === "incompatible" || i.kind === "missingDependency" || i.kind === "cycle"));
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
</script>

<div class="stats">
  <section class="card stat">
    <div class="l"><span>Rules met</span><span class="v num" class:pos={s.pct === 100} class:att={s.pct < 100}>{s.pct} %</span></div>
    <div class="meter" style="--c: var(--{s.pct === 100 ? 'green' : s.pct >= 90 ? 'amber' : 'red'}); --v:{s.pct}%"><i></i></div>
    <div class="cap"><span class="t">{s.satisfied.toLocaleString()} of {s.orderRules.toLocaleString()} load order rules{s.violations ? `, ${s.violations} not met` : ""}</span></div>
  </section>
  <section class="card stat">
    <div class="l"><span>Errors</span><span class="v num" class:neg={s.errors > 0}>{s.errors}</span></div>
    <div class="meter c-red" style="--v:{Math.min(100, s.errors * 12)}%"><i></i></div>
    <div class="cap">{#if s.errors}<span class="flag">{@html I.error}</span>{/if}<span class="t">{errorText}</span></div>
  </section>
  <section class="card stat">
    <div class="l"><span>Overlapping textures</span><span class="v num" class:att={s.collisions > 0}>{(s.collisions + (store.snap?.issuesTruncated ?? 0)).toLocaleString()}</span></div>
    <div class="meter c-amber" style="--v:{Math.min(100, s.collisions * 4)}%"><i></i></div>
    <div class="cap">{#if s.collisions}<span class="flag">{@html I.note}</span>{/if}<span class="t">{s.collisions ? `${s.collidingMods} mods replace the same files. The later mod wins.${store.snap?.issuesTruncated ? ` Only the first ${s.collisions.toLocaleString()} are listed.` : ""}` : "No texture is replaced by two mods"}</span></div>
  </section>
  {#if store.showWeight}
    <section class="card stat" title="Sum of the median frame time shares from circinus.sh for active mods that have figures. Long lists spread the total thin, so use it as a rough guide.">
      <div class="l"><span>Performance</span><span class="v num" class:att={s.share >= 10} class:neg={s.share >= 25}>{s.withShare ? `${s.share.toFixed(1)} %` : s.measured ? `${s.measured} rated` : "no figures"}</span></div>
      <div class="meter" style="--c: var(--{weightColor}); --v:{Math.min(100, s.share)}%"><i></i></div>
      <div class="cap"><span class="t">{s.withShare ? `of frame time, adding up the ${s.withShare} of ${store.active.length} active mods with a number` : s.measured ? `${s.measured} of ${store.active.length} active mods have a rating but no number yet` : "Fetch figures from circinus.sh in Settings"}</span></div>
    </section>
  {:else}
    <section class="card stat">
      <div class="l"><span>Warnings</span><span class="v num" class:att={s.warnings > 0}>{s.warnings}</span></div>
      <div class="meter c-amber" style="--v:{Math.min(100, s.warnings * 8)}%"><i></i></div>
      <div class="cap">{#if s.warnings}<span class="flag">{@html I.warn}</span>{/if}<span class="t">{s.warnings ? "Order, version and duplicate notes" : "Nothing to review"}</span></div>
    </section>
  {/if}
</div>

<style>
  .stats { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 12px; container-type: inline-size; }
  .stat { padding: 12px 14px; min-width: 0; overflow: hidden; }
  .stat .l { display: flex; justify-content: space-between; align-items: baseline; gap: 4px 10px; flex-wrap: wrap; }
  .stat .l span:first-child { font-weight: 600; font-size: 13px; }
  .stat .v { font-weight: 800; font-size: 17px; letter-spacing: -0.02em; white-space: nowrap; margin-left: auto; }
  @container (max-width: 900px) { .stats { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
  .stat .meter { margin: 8px 0 7px; }
  .stat .cap { font-size: 12px; color: var(--text-3); display: flex; align-items: flex-start; gap: 6px; min-height: 34px; line-height: 1.4; }
  .stat .cap .flag { margin-top: 1px; }
  .stat .cap .t { display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }

</style>
