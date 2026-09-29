<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { I } from "$lib/icons";
  import { t } from "$lib/i18n.svelte";

  // The one time the player is asked. Two buttons and no default: closing it with Escape is
  // not an answer, so the card comes back rather than being taken as a yes or a no.
  //
  // What it must do is describe the trade honestly enough that the answer means something. A
  // dialog that says "help us improve Circinus" and nothing else collects consent without
  // informing anybody, and the privacy page this links to would then be a document nobody had
  // a reason to open.
  $effect(() => store.onEscape("consent", 30, () => store.showConsent, () => (store.showConsent = false)));

  const rows = [
    { k: t("sharing.row.costs"), v: t("sharing.row.costs.v") },
    { k: t("sharing.row.ids"), v: t("sharing.row.ids.v") },
    { k: t("sharing.row.machine"), v: t("sharing.row.machine.v") },
    { k: t("sharing.row.id"), v: t("sharing.row.id.v") }
  ];
  const never = [t("sharing.never.paths"), t("sharing.never.names"), t("sharing.never.you"), t("sharing.never.world")];
</script>

<div class="scrim" role="presentation">
  <div class="dlg card" role="dialog" aria-modal="true" aria-labelledby="sh-title">
    <div class="hd">
      <b id="sh-title">{t("sharing.title")}</b>
      <span class="sub">{t("sharing.sub")}</span>
    </div>

    <div class="body">
      <p class="lede">{t("sharing.lede")}</p>
      <!-- Said before the table rather than after it. Somebody being asked to share a
           measurement should know whose measurement it is, and the answer is not ours. -->
      <p class="credit">{t("sharing.thanks")}</p>

      <h4>{@html I.up}{t("sharing.sent")}</h4>
      <table>
        <tbody>
          {#each rows as r, i (i)}
            <tr><th scope="row">{r.k}</th><td>{r.v}</td></tr>
          {/each}
        </tbody>
      </table>

      <h4>{@html I.minus}{t("sharing.nevers")}</h4>
      <ul>
        {#each never as n, i (i)}<li>{n}</li>{/each}
      </ul>

      <!-- Said out loud rather than left to be inferred from a list of omissions. "Mod names:
           never sent" next to "your username: never sent" reads as "the site cannot tell what I
           run", which is false and is the opposite of what the payload is for. Anyone can read
           brrainz.harmony. The honest line is which of the two kinds of thing is protected. -->
      <p class="plain">{t("sharing.plain")}</p>
      <p class="fine">{t("sharing.fine")}</p>
    </div>

    <div class="ft">
      <!-- Keeping it local is the left button and the plain one: the quiet answer should not
           look like the wrong one. -->
      <button class="btn" onclick={() => store.setSharing(false)}>{t("sharing.no")}</button>
      <span class="sp"></span>
      <button class="btn primary" onclick={() => store.setSharing(true)}>{t("sharing.yes")}</button>
    </div>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: var(--scrim); display: grid; place-items: center; z-index: 45; }
  .dlg { width: min(620px, calc(100vw - 40px)); max-height: calc(100vh - 40px); display: flex; flex-direction: column; padding: 0; box-shadow: var(--shadow-float); }
  .hd { padding: 16px 18px 12px; border-bottom: 1px solid var(--surface-3); display: flex; flex-direction: column; gap: 3px; }
  .hd b { font-size: 16px; }
  .hd .sub { color: var(--text-3); font-size: 12.5px; }
  .body { overflow: hidden auto; padding: 14px 18px 16px; }
  .lede { margin: 0 0 14px; font-size: 13.5px; color: var(--text-2); }
  h4 { margin: 14px 0 7px; font-size: 12px; font-weight: 600; display: flex; align-items: center; gap: 8px; }
  h4 :global(svg) { width: 13px; height: 13px; flex: none; color: var(--text-3); }
  table { width: 100%; border-collapse: collapse; font-size: 12.5px; }
  th, td { text-align: left; padding: 5px 0; vertical-align: top; border-bottom: 1px solid var(--surface-3); }
  th { font-weight: 500; color: var(--text); width: 38%; padding-right: 12px; }
  td { color: var(--text-2); }
  tbody tr:last-child th, tbody tr:last-child td { border-bottom: 0; }
  ul { margin: 0; padding-left: 18px; font-size: 12.5px; color: var(--text-2); }
  li { padding: 2px 0; }
  .credit { margin: -8px 0 14px; font-size: 12.5px; color: var(--text-3); }
  .plain { margin: 14px 0 0; font-size: 12.5px; color: var(--text-2); }
  .fine { margin: 10px 0 0; font-size: 12px; color: var(--text-3); }
  .ft { display: flex; align-items: center; gap: 8px; padding: 12px 18px; border-top: 1px solid var(--surface-3); }
  .ft .sp { flex: 1; }
</style>
