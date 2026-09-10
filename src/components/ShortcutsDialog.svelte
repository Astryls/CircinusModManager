<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { t } from "$lib/i18n.svelte";
  import { actions, keyLabel } from "$lib/keys.svelte";
  import { I } from "$lib/icons";

  // The reference, as against the palette's search box. Both read the same table, so a binding
  // cannot be listed here and be wrong -- which is the failure the five hardcoded "Ctrl S"
  // strings were heading towards, one of them telling a Mac user to press a key Macs do not use
  // for this.
  const bound = $derived(actions().filter((a) => a.keys));
  const sections = $derived.by(() => {
    const m = new Map<string, typeof bound>();
    for (const a of bound) m.set(a.section, [...(m.get(a.section) ?? []), a]);
    return [...m.entries()];
  });

  function close() {
    store.showKeys = false;
  }
  $effect(() => store.onEscape("shortcuts", 30, () => store.showKeys, close));
</script>

<div class="scrim" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) close(); }}>
  <div class="dlg card" role="dialog" aria-modal="true" aria-labelledby="keys-title">
    <div class="hd">
      <b id="keys-title">{t("shortcuts.title")}</b>
      <button class="x" aria-label={t("packs.close")} onclick={close}>{@html I.close}</button>
    </div>
    <p class="lead">{t("shortcuts.lead", { palette: keyLabel("Mod+Shift+p") })}</p>
    <div class="body">
      {#each sections as [name, list] (name)}
        <section>
          <h4>{name}</h4>
          {#each list as a (a.id)}
            <div class="row"><span class="nm">{a.name}</span><kbd>{keyLabel(a.keys!)}</kbd></div>
          {/each}
        </section>
      {/each}
      <section>
        <h4>{t("shortcuts.section.inlist")}</h4>
        <p class="note">{t("shortcuts.list")}</p>
      </section>
    </div>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); backdrop-filter: blur(6px); display: grid; place-items: center; z-index: 40; }
  .dlg { width: min(620px, calc(100vw - 40px)); max-height: calc(100vh - 40px); padding: 18px 20px; box-shadow: var(--shadow-float); display: flex; flex-direction: column; gap: 12px; }
  .hd { display: flex; justify-content: space-between; align-items: flex-start; gap: 12px; }
  .hd b { font-size: 16px; font-weight: 800; }
  .x { width: 28px; height: 28px; border-radius: 8px; display: grid; place-items: center; color: var(--text-3); flex: none; }
  .x:hover { background: var(--surface-2); color: var(--text); }
  .x :global(svg) { width: 14px; height: 14px; }
  .lead { margin: 0; color: var(--text-2); font-size: 12.5px; line-height: 1.5; }
  .body { overflow: hidden auto; min-height: 0; display: flex; flex-direction: column; gap: 16px; padding-right: 4px; }
  h4 { margin: 0 0 6px; font-size: 11px; font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase; color: var(--text-3); }
  .row { display: flex; align-items: center; gap: 12px; height: 28px; }
  .row .nm { flex: 1; min-width: 0; font-size: 13px; color: var(--text-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  kbd { font-family: inherit; font-size: 11px; font-weight: 700; color: var(--text-2); background: var(--surface-3); border-radius: 5px; padding: 3px 7px; white-space: nowrap; }
  .note { margin: 0; font-size: 12.5px; line-height: 1.55; color: var(--text-3); }
</style>
