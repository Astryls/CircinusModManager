<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { describe } from "$lib/describe";
  import { sevIcon } from "$lib/icons";
  import { primaryUid, severityOf, type Issue } from "$lib/types";
  import GameLog from "./GameLog.svelte";

  const n = (uid: string) => store.byUid.get(uid)?.name ?? uid;
  const groups = $derived.by(() => {
    const order: Issue["kind"][] = ["aboveOfficial", "cycle", "incompatible", "missingDependency", "misplacedOptimization", "orderViolation", "versionMismatch", "duplicatePackageId", "missingPackageId", "invalid", "ruleIgnored", "textureCollision"];
    const titles: Record<Issue["kind"], string> = {
      aboveOfficial: "Above the game or a DLC, where a def cannot inherit from it",
      cycle: "Rules that contradict each other",
      incompatible: "Mods that do not work together, both active",
      missingDependency: "Missing dependencies",
      misplacedOptimization: "Performance mods not at the end",
      orderViolation: "Load order rules not met",
      versionMismatch: "Not made for this game version",
      duplicatePackageId: "Installed more than once",
      missingPackageId: "No packageId",
      invalid: "Folders that are not working mods",
      ruleIgnored: "Rules HALO set aside",
      textureCollision: "Textures replaced by more than one mod"
    };
    return order.map((k) => ({ kind: k, title: titles[k], items: store.issues.filter((i) => i.kind === k) })).filter((g) => g.items.length);
  });
  /** Texture collisions grouped by (winner, others) pair so 2,000 files read as a few lines. */
  const collisionPairs = $derived.by(() => {
    const map = new Map<string, { winner: string; losers: string[]; paths: string[] }>();
    for (const i of store.issues) {
      if (i.kind !== "textureCollision") continue;
      const losers = i.uids.filter((u) => u !== i.winnerUid);
      const key = `${i.winnerUid}|${losers.join(",")}`;
      if (!map.has(key)) map.set(key, { winner: i.winnerUid, losers, paths: [] });
      map.get(key)!.paths.push(i.path);
    }
    return [...map.values()].sort((a, b) => b.paths.length - a.paths.length);
  });
  function go(i: Issue) {
    const uid = primaryUid(i);
    if (uid) { store.view = "order"; store.scrollTo(uid); }
  }
</script>

<main class="center">
  <GameLog />
  {#if !groups.length}
    <section class="card"><h3>Analyzer</h3><p class="lead">Nothing needs attention. Rules, versions, dependencies and texture packs all check out.</p></section>
  {/if}
  {#each groups as g}
    <section class="card">
      <h3>{g.title} <span class="aside">{g.items.length}</span></h3>
      {#if g.kind === "textureCollision"}
        {#each collisionPairs as p}
          <details class="pair">
            <summary><span class="flag note">{@html sevIcon.note}</span><b>{n(p.winner)}</b> wins {p.paths.length} texture{p.paths.length === 1 ? "" : "s"} over {p.losers.map(n).join(", ")}</summary>
            <div class="paths">{#each p.paths.slice(0, 300) as path}<span class="mono">{path}</span>{/each}{#if p.paths.length > 300}<span class="mono">… {p.paths.length - 300} more</span>{/if}</div>
          </details>
        {/each}
      {:else}
        {#each g.items.slice(0, 200) as i}
          <button class="it" onclick={() => go(i)}>
            <span class="flag {severityOf(i)}">{@html sevIcon[severityOf(i)]}</span>
            <span class="txt"><b>{i.kind === "cycle" ? "Loop" : n(primaryUid(i) ?? "")}</b>: {describe(i, store.byUid)}</span>
          </button>
        {/each}
      {/if}
    </section>
  {/each}
</main>

<style>
  .center { display: flex; flex-direction: column; gap: 12px; min-height: 0; min-width: 0; overflow: hidden auto; padding-bottom: 14px; }
  .lead { margin: 0; color: var(--text-2); font-size: 13.5px; line-height: 1.5; max-width: 70ch; }
  .it { display: flex; gap: 10px; align-items: flex-start; width: 100%; text-align: left; padding: 8px 6px; border-radius: 0; font-size: 13px; color: var(--text-2); line-height: 1.4; }
  .it:hover { background: var(--surface-2); color: var(--text); }
  .it b { color: var(--text); }
  .it .flag { margin-top: 1px; }
  .pair { padding: 6px 6px; border-radius: 0; }
  .pair summary { cursor: pointer; display: flex; gap: 10px; align-items: center; font-size: 13px; color: var(--text-2); }
  .pair summary b { color: var(--text); }
  .paths { display: flex; flex-wrap: wrap; gap: 4px 10px; padding: 8px 0 4px 28px; max-height: 240px; overflow: hidden auto; }
  .paths .mono { color: var(--text-3); font-size: 11.5px; }
</style>
