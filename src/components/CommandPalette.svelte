<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { t } from "$lib/i18n.svelte";
  import { actions, keyLabel } from "$lib/keys.svelte";
  import { I } from "$lib/icons";
  import { tick } from "svelte";

  // The honest answer to "let me reach things by keyboard". Thirty chords is thirty things to
  // memorise and thirty chances to collide with something the webview wants; one chord and a
  // name is neither. It also solves discovery, which a keymap alone never does: every row shows
  // its own binding, so the way you find a command is also the way you learn its shortcut.
  let q = $state("");
  let at = $state(0);
  let field = $state<HTMLInputElement | null>(null);

  const all = $derived(actions().filter((a) => !a.hidden));
  const rows = $derived.by(() => {
    const needle = q.trim().toLowerCase();
    if (!needle) return all;
    // Every letter of the query in order, not necessarily together: "sml" finds "Save the mod
    // list". Ranked so a run of adjacent letters beats one scattered across the words.
    const scored = all.map((a) => ({ a, s: score(a.name.toLowerCase(), needle) })).filter((r) => r.s >= 0);
    scored.sort((x, y) => y.s - x.s || x.a.name.localeCompare(y.a.name));
    return scored.map((r) => r.a);
  });
  function score(text: string, needle: string): number {
    let i = 0;
    let points = 0;
    let last = -2;
    for (const ch of needle) {
      const found = text.indexOf(ch, i);
      if (found < 0) return -1;
      if (found === last + 1) points += 3;
      if (found === 0 || text[found - 1] === " ") points += 2;
      points += 1;
      last = found;
      i = found + 1;
    }
    return points;
  }
  // Filtering moves the ground under the cursor, so it goes back to the top rather than staying
  // on a row number that now means something else.
  $effect(() => {
    void q;
    at = 0;
  });
  $effect(() => {
    field?.focus();
  });
  $effect(() => store.onEscape("palette", 50, () => store.showPalette, close));

  function close() {
    store.showPalette = false;
    q = "";
  }
  function run(i: number) {
    const a = rows[i];
    if (!a || (a.enabled && !a.enabled())) return;
    close();
    // After the palette is gone: an action that moves focus (search, reveal) must not be undone
    // by this dialog tearing down around it.
    tick().then(() => a.run());
  }
  function key(e: KeyboardEvent) {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (!rows.length) return;
      at = (at + (e.key === "ArrowDown" ? 1 : -1) + rows.length) % rows.length;
    } else if (e.key === "Enter") {
      e.preventDefault();
      run(at);
    }
  }
</script>

<div class="scrim" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) close(); }}>
  <div class="dlg card" role="dialog" aria-modal="true" aria-label={t("palette.title")}>
    <div class="q">
      {@html I.search}
      <input bind:this={field} bind:value={q} onkeydown={key} placeholder={t("palette.placeholder")} aria-label={t("palette.title")} />
    </div>
    <div class="rows" role="listbox" aria-label={t("palette.title")}>
      {#each rows as a, i (a.id)}
        {@const off = !!a.enabled && !a.enabled()}
        <button class="it" class:on={i === at} class:off role="option" aria-selected={i === at} onclick={() => run(i)} onmousemove={() => (at = i)}>
          <span class="nm">{a.name}</span>
          <span class="sec">{off ? t("palette.unavailable") : a.section}</span>
          {#if a.keys}<kbd>{keyLabel(a.keys)}</kbd>{/if}
        </button>
      {:else}
        <p class="hint">{t("palette.none")}</p>
      {/each}
    </div>
    <p class="hint foot">{t("palette.hint")}</p>
  </div>
</div>

<style>
  /* Higher than the other dialogs: the palette can be opened over one of them, and a command
     palette behind the thing it was summoned from would be a trick. */
  .scrim { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.5); backdrop-filter: blur(4px); display: grid; place-items: start center; padding-top: 12vh; z-index: 45; }
  .dlg { width: min(620px, calc(100vw - 40px)); max-height: 70vh; padding: 10px; box-shadow: var(--shadow-float); display: flex; flex-direction: column; gap: 8px; }
  /* Not `.field`: that is a global form-row class that stacks its children, and a scoped rule
     that only adds to it inherits the stacking. Two different ideas should not share a name. */
  .q { display: flex; flex-direction: row; align-items: center; gap: 8px; padding: 0 10px; height: 40px; border-radius: 10px; background: var(--surface-2); flex: none; }
  .q :global(svg) { width: 17px; height: 17px; color: var(--text-3); flex: none; }
  .q input { flex: 1; min-width: 0; background: none; border: 0; outline: none; color: var(--text); font-size: 14px; font-family: inherit; }
  .rows { overflow: hidden auto; min-height: 0; display: flex; flex-direction: column; gap: 1px; }
  .it { display: grid; grid-template-columns: minmax(0, 1fr) auto auto; gap: 10px; align-items: center; text-align: left; height: 34px; padding: 0 10px; border-radius: 8px; }
  .it.on { background: var(--surface-3); }
  .it .nm { font-size: 13px; font-weight: 600; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .it .sec { font-size: 11.5px; color: var(--text-3); white-space: nowrap; }
  /* Greyed rather than hidden: the palette should say the command exists and is not available
     now, instead of leaving somebody searching for a word that has quietly stopped matching. */
  .it.off { opacity: 0.5; }
  .it.off .sec { color: var(--amber); }
  kbd { font-family: inherit; font-size: 11px; font-weight: 700; color: var(--text-3); background: var(--surface-3); border-radius: 5px; padding: 2px 6px; white-space: nowrap; }
  .it.on kbd { color: var(--text-2); background: var(--surface-4); }
  .hint { color: var(--text-3); font-size: 12px; margin: 0; padding: 8px 10px; }
  .foot { border-top: 1px solid var(--surface-3); padding: 8px 10px 4px; flex: none; }
</style>
