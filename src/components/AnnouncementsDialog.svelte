<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { t } from "$lib/i18n.svelte";
  import { openUrl } from "$lib/api";
  import { I } from "$lib/icons";
  import type { Announcement } from "$lib/types";

  // Everything on this screen was written by somebody who is not us, and that decides most of
  // the design: the curator's name sits on every post rather than once at the top, the text is
  // rendered as text and never as markup, and the lead says plainly that Circinus is carrying
  // the message rather than making it. A curator asking a player to delete a folder is ordinary
  // and fine; the same words with Circinus's voice behind them would not be.
  const posts = $derived(store.announcements);
  const byPack = $derived.by(() => {
    const m = new Map<number, Announcement[]>();
    for (const a of posts) {
      const list = m.get(a.pack) ?? [];
      list.push(a);
      m.set(a.pack, list);
    }
    return [...m.entries()].sort((a, b) => (b[1][0]?.at ?? 0) - (a[1][0]?.at ?? 0));
  });
  const unread = (a: Announcement) => a.at > (store.packsRead[String(a.pack)] ?? 0);
  const when = (secs: number) => (secs ? new Date(secs * 1000).toLocaleString(undefined, { month: "short", day: "numeric", hour: "numeric", minute: "2-digit" }) : "");
  const checked = $derived(store.snap?.announcementsCheckedAt ?? 0);
  // Muted packs are gone from the feed by the time it reaches here, so unmuting needs a row of
  // its own -- otherwise muting a curator also hides the only button that undoes it.
  const mutedPacks = $derived(store.collections.filter((c) => store.packsMuted.has(c.id)));

  // Escape closes the topmost thing; the store keeps the order. Import and Collection
  // had no Escape at all before this.
  $effect(() => store.onEscape("announcements", 30, () => store.showAnnouncements, close));

  function close() {
    store.showAnnouncements = false;
  }
</script>

<div class="scrim" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) close(); }}>
  <div class="dlg card" role="dialog" aria-modal="true" aria-labelledby="packs-title">
    <div class="hd">
      <div>
        <b id="packs-title">{t("packs.title")}</b>
        <span class="sub">{checked ? t("packs.checked", { when: when(checked) }) : t("packs.checked.never")}</span>
      </div>
      <button class="x" aria-label={t("packs.close")} onclick={close}>{@html I.close}</button>
    </div>

    <p class="lead">{t("packs.lead")}</p>

    <div class="body">
      {#if !posts.length && !mutedPacks.length}
        <p class="hint">{store.collections.length ? t("packs.empty") : t("packs.empty.none")}</p>
      {/if}

      {#each byPack as [pack, list] (pack)}
        <section class="pack">
          <h4>
            {store.packName(pack)}
            <span class="tools"><button class="lnk" onclick={() => store.mutePack(pack, true)}>{t("packs.mute")}</button></span>
          </h4>
          <div class="posts">
            {#each list as a (a.id)}
              <article class="post" class:new={unread(a)}>
                <div class="meta">
                  <b class="who">{a.author || store.packName(pack)}</b>
                  <span class="at">{when(a.at)}</span>
                  {#if unread(a)}<span class="pip">{t("packs.unread")}</span>{/if}
                </div>
                <!-- Text, never markup: this string came off the network from a third party. -->
                <p class="body-text">{a.text}</p>
                {#if a.link}
                  <button class="lnk go" onclick={() => openUrl(a.link!)}>{@html I.link}{t("packs.open")}</button>
                {/if}
              </article>
            {/each}
          </div>
        </section>
      {/each}

      {#each mutedPacks as c (c.id)}
        <section class="pack muted">
          <h4>{c.name}</h4>
          <p class="hint">{t("packs.muted.note")}</p>
          <button class="btn sm" onclick={() => store.mutePack(c.id, false)}>{t("packs.unmute")}</button>
        </section>
      {/each}
    </div>

    <div class="ft">
      <button class="btn" onclick={() => store.refreshAnnouncements()}>{@html I.refresh}{t("packs.refresh")}</button>
      {#if store.unreadAnnouncements.length}<button class="btn primary" onclick={() => store.readAllAnnouncements()}>{t("packs.markread")}</button>{/if}
    </div>
  </div>
</div>

<style>
  .scrim { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); backdrop-filter: blur(6px); display: grid; place-items: center; z-index: 40; }
  .dlg { width: min(760px, calc(100vw - 40px)); max-height: calc(100vh - 40px); padding: 18px 20px; box-shadow: var(--shadow-float); display: flex; flex-direction: column; gap: 14px; }
  .hd { display: flex; justify-content: space-between; align-items: flex-start; gap: 12px; }
  .hd b { font-size: 16px; font-weight: 800; display: block; }
  .hd .sub { display: block; font-size: 12.5px; color: var(--text-3); margin-top: 2px; line-height: 1.4; }
  .x { width: 28px; height: 28px; border-radius: 8px; display: grid; place-items: center; color: var(--text-3); flex: none; }
  .x:hover { background: var(--surface-2); color: var(--text); }
  .x :global(svg) { width: 14px; height: 14px; }
  .lead { margin: 0; color: var(--text-2); font-size: 12.5px; line-height: 1.5; }
  .hint { margin: 0; color: var(--text-3); font-size: 12.5px; line-height: 1.45; }

  .body { overflow: hidden auto; min-height: 0; padding-right: 4px; display: flex; flex-direction: column; gap: 16px; }
  h4 { margin: 0 0 8px; font-size: 12px; font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase; color: var(--text-2); display: flex; align-items: center; gap: 8px; }
  .tools { margin-left: auto; font-weight: 500; text-transform: none; letter-spacing: 0; }
  .lnk { color: var(--blue); font-weight: 600; font-size: 12.5px; white-space: nowrap; }
  .lnk:hover { text-decoration: underline; }

  .posts { display: flex; flex-direction: column; gap: 8px; }
  .post { padding: 10px 12px; border-radius: 10px; background: var(--surface-2); }
  /* The unread mark is a left edge rather than a background, so a post stays readable the moment
     after it is marked read instead of the whole card changing colour under the cursor. */
  .post.new { box-shadow: inset 3px 0 0 var(--amber); }
  .meta { display: flex; align-items: baseline; gap: 10px; flex-wrap: wrap; margin-bottom: 5px; }
  .who { font-size: 13px; font-weight: 700; color: var(--text); }
  .at { font-size: 11.5px; color: var(--text-3); }
  .pip { font-size: 10px; font-weight: 700; letter-spacing: 0.06em; text-transform: uppercase; color: var(--amber); }
  /* `pre-wrap` because a curator's paragraph breaks are theirs; `anywhere` because a pasted
     folder path with no spaces in it must not push the dialog wider than the window. */
  .body-text { margin: 0; font-size: 13px; line-height: 1.55; color: var(--text-2); white-space: pre-wrap; overflow-wrap: anywhere; }
  .go { display: inline-flex; align-items: center; gap: 6px; margin-top: 8px; }
  .go :global(svg) { width: 12px; height: 12px; flex: none; }

  .muted { opacity: 0.75; display: flex; flex-direction: column; align-items: flex-start; gap: 8px; }
  .ft { display: flex; gap: 8px; justify-content: flex-end; }
</style>
