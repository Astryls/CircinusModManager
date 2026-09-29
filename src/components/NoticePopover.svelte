<script lang="ts">
  /*
   * Everything one row is saying, and a way to stop it saying any of it.
   *
   * The column shows the most serious notice and nothing else, which is the right answer for a
   * list of two thousand rows and the wrong answer the moment somebody wants to know what *else*
   * is there. This is where the rest lives.
   *
   * Dismissing hides the mark on the row and nothing else: the Analyzer still lists every issue,
   * because it is the complete index and one that quietly drops rows is worse than none. That is
   * the bargain `muted` already makes for incompatible pairs, and for the same reason -- a
   * warning nobody can dismiss is one people learn to look past, along with the true ones beside
   * it. What keeps it honest is that this panel also shows what has been put down and offers it
   * straight back, so a dismissal is never a thing you cannot find again.
   */
  import { tick } from "svelte";
  import { store } from "$lib/store.svelte";
  import { I } from "$lib/icons";
  import { describe } from "$lib/describe";
  import { NOTICE_ICON, noticesFor, type Notice } from "$lib/notices";
  import { describeChange, severityOf } from "$lib/types";
  import { t } from "$lib/i18n.svelte";

  const at = $derived(store.noticePopover);
  const mod = $derived(at ? store.byUid.get(at.uid) : undefined);
  const inactive = $derived(mod ? !store.activeSet.has(mod.uid) : false);

  const nt = $derived.by(() => {
    if (!mod) return { shown: [] as Notice[], hidden: [] as Notice[] };
    return noticesFor({
      mod,
      issues: store.issuesByUid.get(mod.uid) ?? [],
      describe: (i) => describe(i, store.byUid, mod.uid),
      severityOf,
      update: store.updateByUid.get(mod.uid),
      change: store.changeByUid.get(mod.uid),
      describeChange,
      isNew: store.isNew(mod.uid),
      pinned: store.pinned.has(mod.uid),
      inactive,
      dismissed: store.dismissedNotices
    });
  });

  /** A function rather than a table, because a module-level constant that calls `t()` captures
   *  whatever locale happened to be current when the file first loaded and never changes again. */
  const title = (k: Notice["kind"]) => t(`notice.${k}`);

  let el = $state<HTMLElement | null>(null);
  let pos = $state({ x: 0, y: 0 });

  $effect(() => {
    const p = store.noticePopover;
    if (!p) return;
    pos = { x: p.x, y: p.y };
    clamp();
  });

  /** Keep it inside the window, also after dismissing a row shortens it. */
  async function clamp() {
    await tick();
    if (!el) return;
    const r = el.getBoundingClientRect();
    pos = {
      x: Math.max(8, Math.min(pos.x, innerWidth - r.width - 8)),
      y: Math.max(8, Math.min(pos.y, innerHeight - r.height - 8))
    };
  }

  function close() {
    store.noticePopover = null;
  }
  $effect(() => store.onEscape("notices", 55, () => !!store.noticePopover, close));
  function onWindowDown(e: MouseEvent) {
    if (at && el && !el.contains(e.target as Node)) close();
  }
</script>

<svelte:window onmousedown={onWindowDown} />

{#if at && mod}
  <div class="pop card" bind:this={el} style="left: {pos.x}px; top: {pos.y}px" role="dialog" aria-label={t("notice.aria", { name: mod.name ?? mod.uid })}>
    <div class="head"><b>{mod.name ?? mod.uid}</b><span class="mono">{mod.packageId}</span></div>

    {#each nt.shown as n (n.kind)}
      <div class="it" class:err={n.kind === "error"}>
        <span class="ico">{@html I[NOTICE_ICON[n.kind]]}</span>
        <div class="tx"><b>{title(n.kind)}</b><span>{n.text}</span></div>
        {#if n.dismissKey}
          <button class="x" title={t("notice.dismiss")} aria-label={t("notice.dismiss.aria")} onclick={() => store.dismissNotice(n.dismissKey!, true)?.then(clamp)}>{@html I.close}</button>
        {:else}
          <!-- A pin is a thing the user did, and the way to stop seeing it is to unpin. A
               dismissal would leave the list behaving in a way nothing on screen explained. -->
          <button class="x" title={t("notice.unpin")} aria-label={t("notice.unpin.aria")} onclick={() => store.setPinned([mod.uid], false)}>{@html I.pin}</button>
        {/if}
      </div>
    {:else}
      <p class="none">{t("notice.none")}</p>
    {/each}

    {#if nt.hidden.length}
      <div class="sep"></div>
      <p class="lbl">{nt.shown.length ? t("notice.putdown.also") : t("notice.putdown")}</p>
      {#each nt.hidden as n (n.kind)}
        <div class="it gone">
          <span class="ico">{@html I[NOTICE_ICON[n.kind]]}</span>
          <div class="tx"><b>{title(n.kind)}</b><span>{n.text}</span></div>
          <button class="x" title={t("notice.restore")} aria-label={t("notice.restore.aria")} onclick={() => store.dismissNotice(n.dismissKey!, false)?.then(clamp)}>{@html I.up}</button>
        </div>
      {/each}
    {/if}
  </div>
{/if}

<style>
  .pop { position: fixed; z-index: 40; width: 340px; max-height: 70vh; overflow: auto; padding: 8px; box-shadow: var(--shadow-float); background: var(--surface); }
  .head { display: flex; flex-direction: column; gap: 1px; padding: 4px 6px 8px; border-bottom: 1px solid var(--surface-3); margin-bottom: 6px; }
  .head b { font-size: 13.5px; font-weight: 700; color: var(--text); }
  .head .mono { font-family: var(--mono); font-size: 11px; color: var(--text-3); }
  .it { display: grid; grid-template-columns: 18px minmax(0, 1fr) 24px; gap: 8px; align-items: start; padding: 6px; }
  .it + .it { border-top: 1px solid var(--surface-2); }
  .ico { display: grid; place-items: center; height: 18px; color: var(--text-3); }
  .ico :global(svg) { width: 15px; height: 15px; }
  .it.err .ico { color: var(--red); }
  .tx { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .tx b { font-size: 11px; font-weight: 700; letter-spacing: 0.06em; text-transform: uppercase; color: var(--text-3); }
  .tx span { font-size: 12.5px; line-height: 1.45; color: var(--text-2); white-space: pre-wrap; overflow-wrap: anywhere; }
  .x { width: 24px; height: 24px; display: grid; place-items: center; color: var(--text-4); border-radius: 0; }
  .x:hover { background: var(--surface-3); color: var(--text); }
  .x :global(svg) { width: 12px; height: 12px; }
  .it.gone { opacity: 0.6; }
  .it.gone .tx span { text-decoration: line-through; text-decoration-color: var(--text-4); }
  .sep { height: 1px; background: var(--surface-3); margin: 6px 0; }
  .lbl { margin: 0 0 2px; padding: 0 6px; font-size: 10.5px; font-weight: 700; letter-spacing: 0.07em; text-transform: uppercase; color: var(--text-4); }
  .none { margin: 0; padding: 6px; font-size: 12.5px; color: var(--text-3); }
</style>
