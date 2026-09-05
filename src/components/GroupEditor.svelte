<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { I } from "$lib/icons";
  import { GROUP_COLORS, PHASES, type Group, type Phase } from "$lib/types";

  let { group, onclose }: { group: Group; onclose: () => void } = $props();
  // The name field follows the group until the user types; a rename is saved on change.
  let name = $state("");
  $effect(() => {
    name = group.name;
  });
  const groups = $derived(store.snap?.user.groups ?? []);
  const index = $derived(groups.findIndex((g) => g.id === group.id));
  const members = $derived(Object.values(store.snap?.user.modGroups ?? {}).filter((g) => g === group.id).length);
  /** The one control for where members go: nothing, a phase, or a section of their own. */
  const sortAs = $derived(group.section && group.phase ? "section" : (group.phase ?? ""));
  const after = $derived(PHASES.find((p) => p.id === (group.phase ?? "content"))!);

  function rename() {
    const n = name.trim();
    if (n && n !== group.name) store.updateGroup(group.id, { name: n });
    else name = group.name;
  }
  function setSortAs(v: string) {
    if (v === "section") store.updateGroup(group.id, { section: true, phase: group.phase ?? "content" });
    else if (v) store.updateGroup(group.id, { section: false, phase: v as Phase });
    else store.updateGroup(group.id, { section: false, phase: null });
  }
  function remove() {
    store.deleteGroup(group.id);
    store.say(`Group ${group.name} removed. Its ${members} mod${members === 1 ? " keeps its place" : "s keep their places"}.`, "ok");
    onclose();
  }
</script>

<div class="ged" role="group" aria-label="Edit group {group.name}">
  <div class="top">
    <input class="input" bind:value={name} onchange={rename} aria-label="Group name" />
    <button class="x" aria-label="Close" title="Close" onclick={onclose}>{@html I.close}</button>
  </div>
  <div class="colors" role="radiogroup" aria-label="Colour">
    {#each GROUP_COLORS as c}<button class="sw c-{c}" class:on={group.color === c} role="radio" aria-checked={group.color === c} aria-label={c} title={c} onclick={() => store.updateGroup(group.id, { color: c })}></button>{/each}
  </div>
  <label class="fld"><span>Sort members as</span>
    <select class="sel" value={sortAs} onchange={(e) => setSortAs(e.currentTarget.value)}>
      <option value="">Not by group</option>
      {#each PHASES as p}<option value={p.id}>{p.name}</option>{/each}
      <option value="section">Own section</option>
    </select>
  </label>
  {#if group.section}
    <label class="fld"><span>Placed after</span>
      <select class="sel" value={group.phase ?? "content"} onchange={(e) => store.updateGroup(group.id, { phase: e.currentTarget.value as Phase })}>
        {#each PHASES as p}<option value={p.id}>{p.name}</option>{/each}
      </select>
    </label>
    <p class="hint">Members sort together in a section of their own, right after the ordinary {after.name.toLowerCase()} mods. Rules between mods still hold. Up and Down set the order of sections that share a place.</p>
  {:else if group.phase}
    <p class="hint">Members are sorted as {after.name.toLowerCase()} mods, unless a mod has its own Sort it as.</p>
  {:else}
    <p class="hint">A label and a filter only. HALO sorts members by what they contain.</p>
  {/if}
  <div class="acts">
    <button class="btn sm" disabled={index <= 0} onclick={() => store.moveGroup(group.id, -1)}>Up</button>
    <button class="btn sm" disabled={index < 0 || index >= groups.length - 1} onclick={() => store.moveGroup(group.id, 1)}>Down</button>
    <span class="sp"></span>
    <button class="btn sm danger" title="Remove the group; its mods keep their places" onclick={remove}>Delete</button>
  </div>
</div>

<style>
  .ged { display: flex; flex-direction: column; gap: 8px; padding: 10px; margin: 2px -6px 8px; border-radius: 10px; background: var(--surface-2); }
  .top { display: flex; gap: 6px; align-items: center; }
  .ged .input { height: 28px; font-size: 12.5px; background: var(--surface-3); }
  .x { width: 26px; height: 26px; border-radius: 7px; display: grid; place-items: center; color: var(--text-3); flex: none; }
  .x:hover { background: var(--surface-3); color: var(--text); }
  .x :global(svg) { width: 12px; height: 12px; }
  .colors { display: flex; gap: 6px; flex-wrap: wrap; }
  .sw { width: 18px; height: 18px; border-radius: 50%; background: var(--c); border: 2px solid transparent; box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.25) inset; }
  .sw.on { border-color: var(--text); }
  .sw:hover { transform: scale(1.1); }
  .fld { display: grid; grid-template-columns: 1fr; gap: 4px; font-size: 12px; color: var(--text-2); font-weight: 600; }
  .sel { height: 26px; border: 0; border-radius: 7px; background: var(--surface-3); color: var(--text); font-size: 12px; padding: 0 6px; min-width: 0; width: 100%; }
  .hint { color: var(--text-3); font-size: 11.5px; line-height: 1.4; margin: 0; }
  .acts { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
  .acts .sp { flex: 1; }
</style>
