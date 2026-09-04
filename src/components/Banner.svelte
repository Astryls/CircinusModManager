<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { headline } from "$lib/describe";
  import { I } from "$lib/icons";
  import { primaryUid } from "$lib/types";

  type Notice = { id: string; kind: "error" | "warning" | "note" | "info"; title: string; detail: string; action: string; run: () => void; dismissable: boolean };

  const head = $derived(headline(store.issues, store.byUid));
  const missing = $derived(store.snap?.missing ?? []);
  const since = $derived.by(() => {
    const t = store.snap?.changesSince ?? 0;
    return t ? new Date(t * 1000).toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" }) : "";
  });

  const notices = $derived.by((): Notice[] => {
    const out: Notice[] = [];
    const reset = store.snap?.listReset ?? null;
    if (reset) {
      const from = reset.restoreFrom;
      const when = from ? new Date(from.savedAt * 1000).toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" }) : "";
      out.push({
        id: "reset",
        kind: "error",
        title: `RimWorld reset your mod list to the game and DLC only, because it failed to load last time`,
        detail: from ? `Your list of ${from.count} mods from ${when} is saved (${from.label}) and can go straight back` : `${reset.previousCount} mods were in the list before. No saved copy is available, so import one from a file.`,
        action: from ? "Restore and save" : "Import a list",
        run: () => (from ? store.restoreList(from.path, true) : (store.showImport = true)),
        dismissable: true
      });
    }
    const c = store.changeCounts;
    const l = store.listChange;
    const listBits = l ? [l.added.length ? `${l.added.length} added` : "", l.removed.length ? `${l.removed.length} removed` : "", l.reordered ? "order changed" : ""].filter(Boolean).join(", ") : "";
    if (c.total) {
      const activeN = store.changes.filter((x) => x.active).length;
      out.push({
        id: "changes",
        kind: "warning",
        title: `${c.total} mod${c.total === 1 ? "" : "s"} changed since you last opened Circinus`,
        detail: `${store.changeSummary}${activeN ? ` · ${activeN} in your active list` : ""}${l ? ` · the active list was also edited outside Circinus (${listBits})` : ""}${since ? ` · since ${since}` : ""}`,
        action: "What changed",
        run: () => (store.showChanges = true),
        dismissable: true
      });
    } else if (l) {
      out.push({ id: "changes", kind: "warning", title: "Your active list was changed outside Circinus", detail: `${listBits} in ModsConfig.xml, by RimWorld or another manager`, action: "Show", run: () => (store.showChanges = true), dismissable: true });
    }
    if (missing.length) {
      const ready = store.steamcmdReady;
      out.push({
        id: "missing",
        kind: "warning",
        title: `${missing.length} mod${missing.length === 1 ? " in your list isn't" : "s in your list aren't"} installed`,
        detail: ready ? "SteamCMD can fetch them from the Workshop without the Steam client" : "Set up SteamCMD once, then Circinus can download them for you",
        action: ready ? "Download with SteamCMD" : "Set up SteamCMD",
        run: () => (ready ? store.queueMissing().then(() => (store.view = "downloads")) : store.installSteamCmd()),
        dismissable: true
      });
    }
    if (head) {
      const notice: Notice = { id: "issues", kind: head.kind, title: head.title, detail: head.detail, action: "Review", run: review, dismissable: false };
      // Something that will make the game reset the list outranks housekeeping notices.
      if (head.kind === "error") out.splice(reset ? 1 : 0, 0, notice);
      else out.push(notice);
    }
    return out.filter((n) => !store.dismissed.includes(n.id));
  });

  function review() {
    const wanted = head?.kind === "error" ? ["aboveOfficial", "cycle", "incompatible", "missingDependency"] : ["misplacedOptimization"];
    const target = store.issues.find((i) => wanted.includes(i.kind)) ?? store.issues[0];
    const uid = target && primaryUid(target);
    if (uid) store.scrollTo(uid);
  }
</script>

{#each notices as n, i (n.id)}
  <div class="banner {n.kind}" class:compact={i > 0} role="status">
    <span class="ico">{@html n.kind === "error" ? I.error : n.kind === "warning" ? I.warn : n.id === "changes" ? I.bell : I.note}</span>
    <span class="t">{n.title}{#if i === 0}<small>{n.detail}</small>{:else}<span class="inline">{n.detail}</span>{/if}</span>
    <button onclick={n.run}>{n.action}</button>
    {#if n.dismissable}<button class="x" aria-label="Dismiss" title="Hide until next launch" onclick={() => store.dismiss(n.id)}>{@html I.close}</button>{/if}
  </div>
{/each}

<style>
  .banner { display: flex; align-items: center; gap: 12px; border-radius: 12px; padding: 10px 14px; font-weight: 700; font-size: 13.5px; box-shadow: var(--shadow-raised); }
  .banner + .banner { margin-top: -4px; }
  .banner.compact { padding: 5px 10px 5px 12px; font-size: 13px; }
  .banner.compact .ico { width: 20px; height: 20px; }
  .banner.compact .ico :global(svg) { width: 10px; height: 10px; }
  .banner.compact button { padding: 4px 9px; font-size: 12.5px; }
  .banner.compact button.x { width: 24px; height: 24px; }
  .inline { font-weight: 500; opacity: 0.8; font-size: 12.5px; margin-left: 6px; }
  .banner.note, .banner.warning { background: var(--amber-deep); color: var(--amber-ink); }
  .banner.info { background: var(--surface-3); color: var(--text); }
  .banner.error { background: #b8433c; color: #fff4f3; }
  .ico { width: 24px; height: 24px; border-radius: 50%; background: rgba(0, 0, 0, 0.18); display: grid; place-items: center; flex: none; }
  .ico :global(svg) { width: 12px; height: 12px; }
  .t { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .t small { display: block; font-weight: 500; opacity: 0.8; font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  button { font-weight: 800; padding: 6px 10px; border-radius: 8px; background: rgba(0, 0, 0, 0.16); color: inherit; white-space: nowrap; }
  button:hover { background: rgba(0, 0, 0, 0.26); }
  button.x { width: 28px; height: 28px; padding: 0; display: grid; place-items: center; opacity: 0.7; }
  button.x :global(svg) { width: 12px; height: 12px; }
</style>
