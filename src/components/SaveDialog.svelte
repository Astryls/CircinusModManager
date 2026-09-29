<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { I } from "$lib/icons";
  import { t } from "$lib/i18n.svelte";

  // What Save is about to write, grouped by the kind of change rather than listed flat.
  //
  // The kinds are not cosmetic. Activating a mod and deactivating one are opposite risks -- one
  // adds something you can undo by looking at it, the other takes something out of a list that
  // may be the only record you had of it -- and a reorder is a third thing again, usually a
  // consequence of the first two rather than a decision. Sorting them apart is what makes the
  // dialog answerable at a glance instead of something to read.
  const diff = $derived(store.pendingSave);
  const added = $derived(diff?.added ?? []);
  const removed = $derived(diff?.removed ?? []);
  const moves = $derived(diff?.moves ?? []);
  const nothing = $derived(!added.length && !removed.length && !moves.length);

  /** A package id back to the name on the row, since the diff speaks in ids. */
  const nameOf = (pkg: string) => store.byPackage.get(pkg.replace(/_steam$/, ""))?.name ?? pkg;

  const groups = $derived.by(() =>
    [
      { id: "added", icon: I.plus, title: t("save.added", { n: added.length }), hint: t("save.added.hint"), items: added },
      { id: "removed", icon: I.minus, title: t("save.removed", { n: removed.length }), hint: t("save.removed.hint"), items: removed }
    ].filter((g) => g.items.length)
  );

  $effect(() => store.onEscape("save", 30, () => store.showSave, close));

  function close() {
    store.showSave = false;
  }
  function confirm() {
    store.showSave = false;
    store.save();
  }
</script>

<div class="scrim" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) close(); }}>
  <div class="dlg card" role="dialog" aria-modal="true" aria-labelledby="save-title">
    <div class="hd">
      <b id="save-title">{t("save.title")}</b>
      <span class="sub">{t("save.sub")}</span>
      <button class="x" aria-label={t("common.close")} onclick={close}>{@html I.close}</button>
    </div>

    <div class="body">
      {#if nothing}
        <!-- `dirty` is sticky: it goes true on the first edit and never goes back, so the Save
             button is lit after you activate a mod and deactivate it again. This dialog
             compares rather than remembers, so it is the one thing in the app that can say
             the list is already what is on disk. -->
        <p class="empty">{t("save.nothing")}</p>
      {:else}
        {#each groups as g (g.id)}
          <section class="grp">
            <h4>{@html g.icon}{g.title}</h4>
            <p class="hint">{g.hint}</p>
            <div class="chips">
              {#each g.items as pkg, i (i)}
                <span class="chip {g.id}">{nameOf(pkg)}</span>
              {/each}
            </div>
          </section>
        {/each}
        {#if moves.length}
          <section class="grp">
            <h4>{@html I.over}{t("save.moved", { n: moves.length })}</h4>
            <p class="hint">{t("save.moved.hint")}</p>
            <div class="rows">
              {#each moves.slice(0, 40) as m, i (i)}
                <div class="mv">
                  <span class="nm">{nameOf(m.packageId)}</span>
                  <span class="pos mono">#{m.from} &rarr; #{m.to}</span>
                </div>
              {/each}
              {#if moves.length > 40}<p class="hint">{t("save.moved.more", { n: moves.length - 40 })}</p>{/if}
            </div>
          </section>
        {/if}
      {/if}
    </div>

    <div class="ft">
      <span class="count mono">{t("save.count", { n: store.active.length })}</span>
      <span class="sp"></span>
      <button class="btn" onclick={close}>{t("save.cancel")}</button>
      <button class="btn primary" onclick={confirm}>{nothing ? t("save.anyway") : t("save.confirm")}</button>
    </div>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: var(--scrim); display: grid; place-items: center; z-index: 40; }
  .dlg { width: min(680px, calc(100vw - 40px)); max-height: calc(100vh - 40px); display: flex; flex-direction: column; padding: 0; box-shadow: var(--shadow-float); }
  .hd { display: flex; align-items: baseline; gap: 10px; padding: 14px 16px; border-bottom: 1px solid var(--surface-3); }
  .hd b { font-size: 15px; }
  .hd .sub { color: var(--text-3); font-size: 12.5px; flex: 1; min-width: 0; }
  .x { width: 26px; height: 26px; display: grid; place-items: center; color: var(--text-3); flex: none; }
  .x:hover { color: var(--text); }
  .x :global(svg) { width: 14px; height: 14px; }

  .body { overflow: hidden auto; padding: 4px 16px 12px; }
  .empty { color: var(--text-2); font-size: 13.5px; padding: 18px 0; margin: 0; }
  .grp { padding: 14px 0; border-bottom: 1px solid var(--surface-3); }
  .grp:last-child { border-bottom: 0; }
  .grp h4 { margin: 0 0 3px; font-size: 13px; font-weight: 600; display: flex; align-items: center; gap: 8px; }
  .grp h4 :global(svg) { width: 14px; height: 14px; flex: none; }
  .hint { margin: 0 0 9px; color: var(--text-3); font-size: 12px; }

  .chips { display: flex; flex-wrap: wrap; gap: 5px; }
  .chip { font-size: 12px; padding: 3px 8px; color: var(--text-2); box-shadow: inset 0 0 0 1px var(--surface-3); max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  /* Removal is the one that can lose you something, so it is the one that carries the colour. */
  .chip.removed { color: var(--red); box-shadow: inset 0 0 0 1px var(--red); }

  .rows { display: flex; flex-direction: column; }
  .mv { display: flex; align-items: baseline; gap: 10px; padding: 4px 0; font-size: 12.5px; }
  .mv .nm { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .mv .pos { color: var(--text-3); font-size: 11.5px; flex: none; }

  .ft { display: flex; align-items: center; gap: 8px; padding: 12px 16px; border-top: 1px solid var(--surface-3); }
  .ft .count { color: var(--text-3); font-size: 11.5px; }
  .ft .sp { flex: 1; }
</style>
