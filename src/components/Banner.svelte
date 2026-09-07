<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { headline, headlineFor } from "$lib/describe";
  import { severityOf } from "$lib/types";
  import { I } from "$lib/icons";

  type Notice = { id: string; kind: "error" | "warning" | "note" | "info"; title: string; detail: string; pos?: string; action: string; run: () => void; dismissable: boolean; closeTitle?: string; close?: () => void; alt?: { action: string; title: string; run: () => void } };

  // The mod Review is standing on, if it is standing on one.
  const reviewing = $derived(store.reviewPos.at >= 0 ? (store.selected[0] ?? null) : null);
  // While it is, the banner is about that mod. It used to keep naming whichever issue was worst
  // in the whole list, so Next scrolled the list to the next mod and the words above it did not
  // change — the button looked broken and there was no way to read what you had been sent to.
  const head = $derived.by(() => (reviewing ? headlineFor(reviewing, store.issuesByUid.get(reviewing) ?? [], store.byUid) : null) ?? headline(store.issues, store.byUid));
  // Following the selection must not let a serious problem drop out of sight: a note about the
  // mod under review says so where there are still errors behind it.
  const errorsLeft = $derived(store.issues.filter((i) => severityOf(i) === "error").length);
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
        // The two routes differ in one thing each, so say that one thing.
        detail: "Steam keeps them updated; SteamCMD needs no client",
        action: ready ? "Download with SteamCMD" : "Set up SteamCMD",
        run: () => (ready ? store.queueMissing().then(() => (store.view = "downloads")) : store.installSteamCmd()),
        alt: { action: "Subscribe in Steam", title: store.steamClient?.detail ?? "Opens each mod's page in Steam, where Subscribe is one press", run: () => store.subscribeMissing() },
        dismissable: true
      });
    }
    if (head) {
      // Errors stay until fixed; warnings and notes can be closed for this session. Review walks
      // the mods with something to look at, one press each, most serious first.
      const { at, total } = store.reviewPos;
      // "Next (6 of 166)" counted the mod it was about to go to while the banner described the
      // one before it, so the two numbers never agreed and neither was the one being read. The
      // position belongs beside the words it describes; the button just says what it does.
      const action = total <= 1 ? "Review" : at < 0 ? `Review (${total})` : "Next";
      // How far along, kept out of the detail: the detail is elided when the window is narrow,
      // and where you are is the one thing that must not be the part that disappears.
      const pos = at >= 0 && total > 1 ? `${at + 1} of ${total}` : "";
      const behind = at >= 0 && head.kind !== "error" && errorsLeft ? `${errorsLeft} error${errorsLeft === 1 ? "" : "s"} still to come` : "";
      const detail = [head.detail, behind].filter(Boolean).join(" · ");
      const notice: Notice = { id: `issues:${head.title}`, kind: head.kind, title: head.title, detail, pos, action, run: () => store.reviewNext(), dismissable: head.kind !== "error" || at >= 0 };
      // Closing a banner about the mod under review leaves the review, rather than hiding that
      // one mod for the session: come back round to it and the banner would vanish with the only
      // button that walks the list.
      if (at >= 0) {
        notice.close = () => (store.selected = []);
        notice.closeTitle = "Stop reviewing";
      }
      // Something that will make the game reset the list outranks housekeeping notices.
      if (head.kind === "error") out.splice(reset ? 1 : 0, 0, notice);
      else out.push(notice);
    }
    return out.filter((n) => !store.dismissed.includes(n.id));
  });

  // ---- a newer Circinus ----
  // Its own row rather than a Notice: while the install runs the row turns into a progress
  // bar, and a failed install has to say why and offer another go.
  const update = $derived(store.update);
  const prog = $derived(store.updateProgress);
  const showUpdate = $derived(!!update && (!!prog || !store.dismissed.includes("update")));
  const mb = (n: number) => `${(n / 1e6).toFixed(n < 1e7 ? 1 : 0)} MB`;
  const pct = $derived(prog?.total ? Math.min(100, Math.round((prog.downloaded / prog.total) * 100)) : null);
  const upText = $derived.by(() => {
    const v = update?.version ?? "";
    if (!prog) {
      // One line: the first sentence of the notes; the whole text is in Settings.
      const first = (update?.notes ?? "").trim().split(/(?<=[.!?])\s+/)[0] ?? "";
      const have = update?.current ?? store.appVersion ?? "";
      return { title: `Circinus ${v} is available`, detail: first ? `${first} · You have ${have}; the full notes are in Settings` : `You have ${have} · installing takes about a minute and restarts Circinus` };
    }
    switch (prog.phase) {
      case "downloading": return { title: `Downloading Circinus ${v}`, detail: prog.total ? `${mb(prog.downloaded)} of ${mb(prog.total)}` : mb(prog.downloaded) };
      case "installing": return { title: `Installing Circinus ${v}`, detail: "The installer is running. Circinus restarts on its own when it is done." };
      case "restarting": return { title: `Circinus is restarting with ${v}`, detail: "If this window stays open, close it and start Circinus again." };
      case "failed": return { title: `Circinus ${v} was not installed`, detail: prog.error ?? "Something went wrong. Try again." };
    }
  });
  const upBusy = $derived(!!prog && prog.phase !== "failed");
</script>

{#if showUpdate && update}
  <div class="banner update" class:busy={upBusy} role="status" aria-live="polite">
    <span class="ico">{@html prog?.phase === "failed" ? I.error : I.up}</span>
    <span class="t">{upText.title}<small>{upText.detail}</small></span>
    {#if prog && prog.phase !== "failed"}
      <span class="bar" class:indeterminate={pct == null || prog.phase !== "downloading"} title={pct != null ? `${pct}%` : ""}><i style="width: {prog.phase === "downloading" ? (pct ?? 0) : 100}%"></i></span>
      {#if pct != null && prog.phase === "downloading"}<span class="pct num">{pct}%</span>{/if}
    {:else}
      <button class="alt" title="Hide until next launch. Settings has Check now." onclick={() => { store.updateProgress = null; store.dismiss("update"); }}>Not now</button>
      <button onclick={() => store.installUpdate()}>{prog?.phase === "failed" ? "Try again" : "Install and restart"}</button>
    {/if}
  </div>
{/if}

{#each notices as n, i (n.id)}
  <div class="banner {n.kind}" class:compact={i > 0} role="status">
    <span class="ico">{@html n.kind === "error" ? I.error : n.kind === "warning" ? I.warn : n.id === "changes" ? I.bell : I.note}</span>
    <span class="t">{n.title}{#if i === 0}<small>{n.detail}</small>{:else}<span class="inline">{n.detail}</span>{/if}</span>
    {#if n.pos}<span class="pos num">{n.pos}</span>{/if}
    {#if n.alt}<button class="alt" title={n.alt.title} onclick={n.alt.run}>{n.alt.action}</button>{/if}
    <button onclick={n.run}>{n.action}</button>
    {#if n.dismissable}<button class="x" aria-label={n.closeTitle ?? "Dismiss"} title={n.closeTitle ?? "Hide until next launch"} onclick={() => (n.close ? n.close() : store.dismiss(n.id))}>{@html I.close}</button>{/if}
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
  /* A newer build: good news, in the blue the app uses for "something to fetch". */
  .banner.update { background: #2f63c4; color: #eef3ff; }
  .banner.update .bar { flex: 0 0 200px; height: 8px; border-radius: 4px; background: rgba(0, 0, 0, 0.28); overflow: hidden; }
  .banner.update .bar i { display: block; height: 100%; border-radius: 4px; background: #fff; transition: width 0.12s linear; }
  .banner.update .bar.indeterminate i { background: repeating-linear-gradient(-45deg, #fff 0 10px, rgba(255, 255, 255, 0.55) 10px 20px); background-size: 28px 100%; animation: slide 0.8s linear infinite; }
  @keyframes slide { to { background-position: 28px 0; } }
  .banner.update .pct { width: 38px; text-align: right; font-size: 12.5px; opacity: 0.9; }
  /* Where you are in the review. Beside the button rather than in the detail, which is elided
     the moment the window is narrow. */
  .pos { flex: none; font-weight: 700; font-size: 12.5px; opacity: 0.75; }
  .ico { width: 24px; height: 24px; border-radius: 50%; background: rgba(0, 0, 0, 0.18); display: grid; place-items: center; flex: none; }
  .ico :global(svg) { width: 12px; height: 12px; }
  .t { flex: 1; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .t small { display: block; font-weight: 500; opacity: 0.8; font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  button { font-weight: 800; padding: 6px 10px; border-radius: 8px; background: rgba(0, 0, 0, 0.16); color: inherit; white-space: nowrap; }
  button:hover { background: rgba(0, 0, 0, 0.26); }
  /* The second way of doing the same job: offered, but quieter than the first. */
  button.alt { background: transparent; box-shadow: inset 0 0 0 1.5px rgba(0, 0, 0, 0.22); }
  button.alt:hover { background: rgba(0, 0, 0, 0.16); }
  button.x { width: 28px; height: 28px; padding: 0; display: grid; place-items: center; opacity: 0.7; }
  button.x :global(svg) { width: 12px; height: 12px; }
</style>
