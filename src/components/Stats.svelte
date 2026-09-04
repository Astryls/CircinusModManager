<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { I } from "$lib/icons";

  const s = $derived(store.stats);
  const errorIssue = $derived(store.issues.find((i) => i.kind === "incompatible" || i.kind === "missingDependency" || i.kind === "cycle"));
  const errorText = $derived.by(() => {
    if (!errorIssue) return "Nothing blocks a launch";
    const n = (uid: string) => store.byUid.get(uid)?.name ?? uid;
    if (errorIssue.kind === "incompatible") return `${n(errorIssue.uid)} conflicts with ${n(errorIssue.otherUid)}`;
    if (errorIssue.kind === "missingDependency") return `${n(errorIssue.uid)} needs ${errorIssue.displayName ?? errorIssue.dependency}`;
    if (errorIssue.kind === "cycle") return errorIssue.chain;
    return "";
  });
  const weightColor = $derived(s.share < 10 ? "green" : s.share < 25 ? "amber" : "red");
</script>

<div class="stats">
  <section class="card stat">
    <div class="l"><span>Rules satisfied</span><span class="v num" class:pos={s.pct === 100} class:att={s.pct < 100}>{s.pct}%</span></div>
    <div class="meter" style="--c: var(--{s.pct === 100 ? 'green' : s.pct >= 90 ? 'amber' : 'red'}); --v:{s.pct}%"><i></i></div>
    <div class="cap">{s.satisfied} of {s.orderRules} ordering rules · {s.violations ? `${s.violations} to fix` : "all met"}</div>
  </section>
  <section class="card stat">
    <div class="l"><span>Errors</span><span class="v num" class:neg={s.errors > 0}>{s.errors}</span></div>
    <div class="meter c-red" style="--v:{Math.min(100, s.errors * 12)}%"><i></i></div>
    <div class="cap">{#if s.errors}<span class="flag error">{@html I.error}</span>{/if}<span class="t">{errorText}</span></div>
  </section>
  <section class="card stat">
    <div class="l"><span>Texture collisions</span><span class="v num" class:att={s.collisions > 0}>{s.collisions}</span></div>
    <div class="meter c-amber" style="--v:{Math.min(100, s.collisions * 4)}%"><i></i></div>
    <div class="cap">{#if s.collisions}<span class="flag note">{@html I.note}</span>{/if}<span class="t">{s.collisions ? `${s.collidingMods} mods replace the same files` : "No file is replaced twice"}</span></div>
  </section>
  {#if store.showWeight}
    <section class="card stat" title="Sum of median frame shares from circinus.sh for active mods with figures. Shares dilute on long lists; treat as a rough guide.">
      <div class="l"><span>Circinus weight</span><span class="v num" class:att={s.share >= 10} class:neg={s.share >= 25}>{s.measured ? `${s.share.toFixed(1)}%` : "—"}</span></div>
      <div class="meter" style="--c: var(--{weightColor}); --v:{Math.min(100, s.share)}%"><i></i></div>
      <div class="cap"><span class="t">{s.measured ? `${s.measured} of ${store.active.length} active mods measured` : "Refresh in Settings to fetch figures"}</span></div>
    </section>
  {:else}
    <section class="card stat">
      <div class="l"><span>Warnings</span><span class="v num" class:att={s.warnings > 0}>{s.warnings}</span></div>
      <div class="meter c-amber" style="--v:{Math.min(100, s.warnings * 8)}%"><i></i></div>
      <div class="cap">{#if s.warnings}<span class="flag warning">{@html I.warn}</span>{/if}<span class="t">{s.warnings ? "Order, version and duplicate notes" : "Nothing to review"}</span></div>
    </section>
  {/if}
</div>

<style>
  .stats { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 12px; }
  .stat { padding: 14px 16px; }
  .stat .l { display: flex; justify-content: space-between; align-items: baseline; gap: 8px; }
  .stat .l span:first-child { font-weight: 600; font-size: 13.5px; }
  .stat .v { font-weight: 800; font-size: 18px; letter-spacing: -0.02em; }
  .stat .meter { margin: 10px 0 8px; }
  .stat .cap { font-size: 12px; color: var(--text-3); display: flex; align-items: center; gap: 6px; min-height: 18px; }
  .stat .cap .t { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  @media (max-width: 980px) { .stats { grid-template-columns: repeat(2, minmax(0, 1fr)); } }
</style>
