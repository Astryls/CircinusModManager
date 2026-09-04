<script lang="ts">
  import { store } from "$lib/store.svelte";
  import { I, sevIcon } from "$lib/icons";
  import { describe } from "$lib/describe";
  import { BAND_LABEL, SOURCE_GLYPH, SOURCE_LABEL, severityOf, type ModInfo } from "$lib/types";

  let dragUids: string[] = [];
  let dropAt = $state<{ uid: string; after: boolean } | null>(null);
  let dropEnd = $state(false);

  const versions = $derived.by(() => {
    const cur = store.snap?.gameVersion.majorMinor ?? "1.6";
    const [ma, mi] = cur.split(".").map(Number);
    return [`${ma}.${mi - 1}`, cur];
  });

  function visibleList(): string[] {
    return [...store.visibleActive.map((m) => m.uid), ...(store.tab !== "active" ? store.visibleInactive.map((m) => m.uid) : [])];
  }
  function click(e: MouseEvent, m: ModInfo) {
    store.select(m.uid, { toggle: e.ctrlKey || e.metaKey, range: e.shiftKey, list: visibleList() });
  }
  function toggle(m: ModInfo) {
    if (store.activeSet.has(m.uid)) store.deactivate([m.uid]);
    else store.activate([m.uid]);
  }
  function key(e: KeyboardEvent, m: ModInfo) {
    if (e.key === "Enter" || e.key === " ") { e.preventDefault(); toggle(m); }
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      const list = visibleList();
      const i = list.indexOf(m.uid) + (e.key === "ArrowDown" ? 1 : -1);
      if (i >= 0 && i < list.length) {
        store.select(list[i], { range: e.shiftKey, list });
        (document.querySelector(`[data-uid="${CSS.escape(list[i])}"]`) as HTMLElement)?.focus();
      }
    }
  }
  function dragStart(e: DragEvent, m: ModInfo) {
    if (!store.selected.includes(m.uid)) store.select(m.uid);
    dragUids = store.selected.filter((u) => store.byUid.has(u));
    e.dataTransfer?.setData("text/plain", dragUids.join("\n"));
    if (e.dataTransfer) e.dataTransfer.effectAllowed = "move";
  }
  function dragOver(e: DragEvent, m: ModInfo) {
    if (!store.activeSet.has(m.uid) || !dragUids.length) return;
    e.preventDefault();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    dropAt = { uid: m.uid, after: e.clientY > r.top + r.height / 2 };
    dropEnd = false;
  }
  function dragOverEnd(e: DragEvent) {
    if (!dragUids.length) return;
    e.preventDefault();
    dropEnd = true;
    dropAt = null;
  }
  async function drop(e: DragEvent) {
    e.preventDefault();
    const uids = dragUids;
    const target = dropAt;
    const end = dropEnd;
    dragUids = [];
    dropAt = null;
    dropEnd = false;
    if (!uids.length) return;
    let index = store.active.length;
    if (target) {
      const i = store.indexOf.get(target.uid);
      if (i == null) return;
      index = i + (target.after ? 1 : 0);
    } else if (!end) return;
    const fresh = uids.filter((u) => !store.activeSet.has(u));
    const moving = uids.filter((u) => store.activeSet.has(u));
    if (moving.length) await store.moveTo(moving, index);
    if (fresh.length) await store.activate(fresh, moving.length ? undefined : index);
  }
  function dragEnd() {
    dragUids = [];
    dropAt = null;
    dropEnd = false;
  }
  function weightOf(m: ModInfo) {
    return store.weightOf(m);
  }
</script>

<div class="card list" aria-label="Load order" ondragover={dragOverEnd} ondrop={drop} ondragend={dragEnd} role="listbox" aria-multiselectable="true" tabindex="-1">
  {#if store.tab !== "inactive"}
    {#each store.sections as sec (sec.phase.id)}
      <div class="ph"><span class="dot c-{sec.phase.color}"></span><span class="n">{sec.phase.name}</span><span class="c num">{sec.mods.length}</span><span class="note">{sec.phase.note}</span></div>
      {#each sec.mods as m (m.uid)}
        {@const issues = store.issuesByUid.get(m.uid) ?? []}
        {@const delta = store.moveOf.get(m.uid)}
        {@const w = weightOf(m)}
        <div
          class="row"
          class:sel={store.selected.includes(m.uid)}
          class:drop-before={dropAt?.uid === m.uid && !dropAt.after}
          class:drop-after={dropAt?.uid === m.uid && dropAt.after}
          data-uid={m.uid}
          role="option"
          aria-selected={store.selected.includes(m.uid)}
          tabindex="0"
          draggable="true"
          onclick={(e) => click(e, m)}
          ondblclick={() => toggle(m)}
          onkeydown={(e) => key(e, m)}
          ondragstart={(e) => dragStart(e, m)}
          ondragover={(e) => dragOver(e, m)}
        >
          <span class="idx num">{(store.indexOf.get(m.uid) ?? 0) + 1}</span>
          <span class="grip">{@html I.grip}</span>
          <span class="src {m.source}" title={SOURCE_LABEL[m.source] ?? m.source}>{SOURCE_GLYPH[m.source] ?? "?"}</span>
          <span class="name"><b>{m.name ?? m.uid}</b><span>{(m.authors ?? []).join(", ")}</span></span>
          <span class="pkg">{m.packageId}</span>
          {#if store.showWeight}
            <span class="wt">{#if w && w.share != null}<span class="band {w.band}" title="{BAND_LABEL[w.band]} · median {w.share.toFixed(2)}% of frame · {w.measured ?? '?'} runs · {w.origin === 'local' ? 'your runs' : 'circinus.sh'}">{w.share.toFixed(1)}%</span>{:else if w}<span class="band {w.band}" title={BAND_LABEL[w.band]}>—</span>{/if}</span>
          {/if}
          <span class="vers">{#each versions as v}<span class:off={!(m.supportedVersions ?? []).includes(v)}>{v}</span>{/each}</span>
          <span class="g">{#if store.groupOf(m.uid)}<span class="dot c-{store.groupOf(m.uid)?.color}" title={store.groupOf(m.uid)?.name}></span>{/if}</span>
          <span class="flags">
            {#each issues.slice(0, 3) as i}<span class="flag {severityOf(i)}" title={describe(i, store.byUid, m.uid)}>{@html sevIcon[severityOf(i)] ?? sevIcon.warning}</span>{/each}
            {#if store.pinned.has(m.uid)}<span class="flag pin" title="Pinned: keeps this position">{@html I.pin}</span>{/if}
          </span>
          <span class="delta num" class:down={delta && delta > 0} class:up={delta && delta < 0}>{#if delta}{delta > 0 ? "+" : ""}{delta}{/if}</span>
        </div>
      {/each}
    {/each}
    {#if !store.visibleActive.length}<div class="empty">{store.active.length ? "No active mod matches the search or filters." : "No active mods. Import a list or activate mods from the Inactive tab."}</div>{/if}
  {/if}
  {#if store.tab !== "active"}
    <div class="ph"><span class="dot"></span><span class="n">Inactive</span><span class="c num">{store.visibleInactive.length}{store.visibleInactive.length !== store.inactive.length ? ` of ${store.inactive.length}` : ""}</span><span class="note">Installed, not in ModsConfig.xml</span></div>
    {#each store.visibleInactive as m (m.uid)}
      {@const w = weightOf(m)}
      <div class="row off" class:sel={store.selected.includes(m.uid)} data-uid={m.uid} role="option" aria-selected={store.selected.includes(m.uid)} tabindex="0" draggable="true"
        onclick={(e) => click(e, m)} ondblclick={() => toggle(m)} onkeydown={(e) => key(e, m)} ondragstart={(e) => dragStart(e, m)}>
        <span class="idx"></span>
        <span class="grip">{@html I.grip}</span>
        <span class="src {m.source}" title={SOURCE_LABEL[m.source] ?? m.source}>{SOURCE_GLYPH[m.source] ?? "?"}</span>
        <span class="name"><b>{m.name ?? m.uid}</b><span>{m.invalid ?? (m.authors ?? []).join(", ")}</span></span>
        <span class="pkg">{m.packageId}</span>
        {#if store.showWeight}<span class="wt">{#if w && w.share != null}<span class="band {w.band}">{w.share.toFixed(1)}%</span>{/if}</span>{/if}
        <span class="vers">{#each versions as v}<span class:off={!(m.supportedVersions ?? []).includes(v)}>{v}</span>{/each}</span>
        <span class="g">{#if store.groupOf(m.uid)}<span class="dot c-{store.groupOf(m.uid)?.color}"></span>{/if}</span>
        <span class="flags">{#if m.invalid}<span class="flag error" title={m.invalid}>{@html I.error}</span>{/if}</span>
        <span class="delta"></span>
      </div>
    {/each}
  {/if}
  {#if dropEnd}<div class="drop-line"></div>{/if}
</div>

<style>
  .list { flex: 1; min-height: 0; overflow: auto; padding: 0 6px 10px; position: relative; }
  .ph { position: sticky; top: 0; z-index: 2; background: var(--surface); display: flex; align-items: center; gap: 10px; padding: 14px 10px 7px; box-shadow: 0 6px 8px -6px rgba(0, 0, 0, 0.5); }
  .ph .dot { width: 7px; height: 7px; }
  .ph .n { font-size: 11px; font-weight: 700; letter-spacing: 0.09em; text-transform: uppercase; color: var(--text-2); }
  .ph .c { font-size: 11px; color: var(--text-3); font-weight: 600; }
  .ph .note { margin-left: auto; font-size: 11.5px; color: var(--text-3); }
  .row { display: grid; grid-template-columns: 34px 18px 22px minmax(0, 1fr) 150px auto 64px 14px 58px 40px; align-items: center; gap: 10px; height: 40px; padding: 0 8px 0 4px; border-radius: var(--r-row); cursor: default; position: relative; }
  .row:hover { background: var(--surface-2); }
  .row.sel { background: var(--surface-3); }
  .row.off { opacity: 0.72; }
  .row.off.sel, .row.off:hover { opacity: 1; }
  .row.drop-before::before, .row.drop-after::after { content: ""; position: absolute; left: 8px; right: 8px; height: 2px; background: var(--amber); border-radius: 1px; }
  .row.drop-before::before { top: -1px; }
  .row.drop-after::after { bottom: -1px; }
  .drop-line { height: 2px; margin: 0 8px; background: var(--amber); border-radius: 1px; }
  .idx { font-family: var(--mono); font-size: 11.5px; color: var(--text-3); text-align: right; }
  .grip { color: var(--text-4); opacity: 0; display: grid; place-items: center; cursor: grab; }
  .row:hover .grip, .row.sel .grip { opacity: 1; }
  .grip :global(svg) { width: 12px; height: 12px; }
  .name { min-width: 0; display: flex; flex-direction: column; justify-content: center; line-height: 1.2; }
  .name b { font-weight: 600; font-size: 13.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .name span { font-size: 11.5px; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .pkg { font-family: var(--mono); font-size: 11px; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .wt { min-width: 52px; display: flex; justify-content: flex-end; }
  .vers { justify-content: flex-end; }
  .g { display: grid; place-items: center; }
  .flags { display: flex; gap: 4px; justify-content: flex-end; align-items: center; }
  .delta { font: 700 11.5px var(--mono); text-align: right; color: var(--text-3); }
  .delta.down { color: var(--amber); }
  .delta.up { color: var(--blue); }
  @media (max-width: 1240px) { .row { grid-template-columns: 34px 18px 22px minmax(0, 1fr) auto 64px 14px 58px 40px; } .pkg { display: none; } }
</style>
