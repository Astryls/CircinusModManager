<script lang="ts">
  import { tick } from "svelte";
  import { store } from "$lib/store.svelte";
  import { I, sevIcon } from "$lib/icons";
  import { describe } from "$lib/describe";
  import { BAND_LABEL, SOURCE_GLYPH, SOURCE_LABEL, describeChange, severityOf, type ModInfo } from "$lib/types";

  // ---- virtualization: only the rows in view exist in the DOM ----
  const ROW = 40;
  const HEADER = 44;
  const OVERSCAN = 8;
  type Item = { kind: "header"; key: string; name: string; color: string; note: string; count: number } | { kind: "row"; key: string; mod: ModInfo; inactive: boolean };

  let scroller = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let viewport = $state(600);
  let dragUids: string[] = [];
  let dropAt = $state<{ uid: string; after: boolean } | null>(null);
  let dropEnd = $state(false);

  const items = $derived.by((): Item[] => {
    const out: Item[] = [];
    if (store.tab !== "inactive") {
      for (const sec of store.sections) {
        out.push({ kind: "header", key: `h:${sec.phase.id}`, name: sec.phase.name, color: sec.phase.color, note: sec.phase.note, count: sec.mods.length });
        for (const m of sec.mods) out.push({ kind: "row", key: m.uid, mod: m, inactive: false });
      }
    }
    if (store.tab !== "active") {
      const vi = store.visibleInactive;
      out.push({ kind: "header", key: "h:inactive", name: "Inactive", color: "", note: "Installed, not in ModsConfig.xml", count: vi.length });
      for (const m of vi) out.push({ kind: "row", key: m.uid, mod: m, inactive: true });
    }
    return out;
  });
  const offsets = $derived.by(() => {
    const o = new Array<number>(items.length + 1);
    let y = 0;
    for (let i = 0; i < items.length; i++) {
      o[i] = y;
      y += items[i].kind === "header" ? HEADER : ROW;
    }
    o[items.length] = y;
    return o;
  });
  const total = $derived(offsets[items.length] ?? 0);
  const range = $derived.by(() => {
    if (!items.length) return { start: 0, end: 0 };
    const top = Math.max(0, scrollTop - OVERSCAN * ROW);
    const bottom = scrollTop + viewport + OVERSCAN * ROW;
    let lo = 0, hi = items.length - 1;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (offsets[mid + 1] <= top) lo = mid + 1;
      else hi = mid;
    }
    let end = lo;
    while (end < items.length && offsets[end] < bottom) end++;
    return { start: lo, end };
  });
  const visible = $derived(items.slice(range.start, range.end).map((it, i) => ({ it, y: offsets[range.start + i] })));
  /** The section whose header has scrolled above the top edge. */
  const floating = $derived.by(() => {
    let cur: Item | null = null;
    for (let i = 0; i < items.length && offsets[i] <= scrollTop; i++) if (items[i].kind === "header") cur = items[i];
    return cur && cur.kind === "header" && offsets[items.indexOf(cur)] < scrollTop ? cur : null;
  });
  const versions = $derived.by(() => {
    const cur = store.snap?.gameVersion.majorMinor ?? "1.6";
    const [ma, mi] = cur.split(".").map(Number);
    return [`${ma}.${mi - 1}`, cur];
  });

  function onScroll() {
    if (scroller) scrollTop = scroller.scrollTop;
  }
  function scrollToUid(uid: string, focus = false) {
    const i = items.findIndex((it) => it.kind === "row" && it.key === uid);
    if (i < 0 || !scroller) return;
    const y = offsets[i];
    if (y < scrollTop + HEADER || y + ROW > scrollTop + viewport) {
      scroller.scrollTop = Math.max(0, y - viewport / 2);
      scrollTop = scroller.scrollTop;
    }
    if (focus) tick().then(() => (document.querySelector(`[data-uid="${CSS.escape(uid)}"]`) as HTMLElement | null)?.focus());
  }
  $effect(() => {
    const uid = store.scrollRequest;
    if (uid) {
      tick().then(() => { scrollToUid(uid, true); store.scrollRequest = null; });
    }
  });

  // ---- interaction ----
  function visibleList(): string[] {
    return items.filter((it) => it.kind === "row").map((it) => it.key);
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
        scrollToUid(list[i], true);
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
    if (!dropAt) dropEnd = true;
  }
  async function drop(e: DragEvent) {
    e.preventDefault();
    const uids = dragUids, target = dropAt, end = dropEnd;
    dragUids = []; dropAt = null; dropEnd = false;
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
  function dragEnd() { dragUids = []; dropAt = null; dropEnd = false; }
  function dragLeaveRow() { dropAt = null; }
</script>

<div class="card list" bind:this={scroller} bind:clientHeight={viewport} onscroll={onScroll} ondragover={dragOverEnd} ondrop={drop} ondragend={dragEnd} role="listbox" aria-label="Load order" aria-multiselectable="true" tabindex="-1">
  {#if floating}
    <div class="ph floating"><span class="dot c-{floating.color}"></span><span class="n">{floating.name}</span><span class="c num">{floating.count}</span><span class="note">{floating.note}</span></div>
  {/if}
  <div class="spacer" style="height: {total}px">
    {#each visible as { it, y } (it.key)}
      {#if it.kind === "header"}
        <div class="ph" style="transform: translateY({y}px)"><span class="dot c-{it.color}"></span><span class="n">{it.name}</span><span class="c num">{it.count}</span><span class="note">{it.note}</span></div>
      {:else}
        {@const m = it.mod}
        {@const issues = store.issuesByUid.get(m.uid) ?? []}
        {@const delta = store.moveOf.get(m.uid)}
        {@const w = store.weightOf(m)}
        {@const upd = store.updateByUid.get(m.uid)}
        {@const chg = store.changeByUid.get(m.uid)}
        <div
          class="row"
          class:off={it.inactive}
          class:sel={store.selected.includes(m.uid)}
          class:drop-before={dropAt?.uid === m.uid && !dropAt.after}
          class:drop-after={dropAt?.uid === m.uid && dropAt.after}
          style="transform: translateY({y}px)"
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
          ondragleave={dragLeaveRow}
        >
          <span class="idx num">{it.inactive ? "" : (store.indexOf.get(m.uid) ?? 0) + 1}</span>
          <span class="grip">{@html I.grip}</span>
          <span class="src {m.source}" title={SOURCE_LABEL[m.source] ?? m.source}>{SOURCE_GLYPH[m.source] ?? "?"}</span>
          <span class="name"><b>{m.name ?? m.uid}</b><span>{m.invalid ?? (m.authors ?? []).join(", ")}</span></span>
          <span class="pkg">{m.packageId}</span>
          {#if store.showWeight}
            <span class="wt">{#if w && w.share != null}<span class="band {w.band}" title="Performance cost: {w.share.toFixed(2)} % of frame time, {BAND_LABEL[w.band].toLowerCase()}. {w.measured ?? '?'} runs measured, from {w.origin === 'local' ? 'your runs' : 'circinus.sh'}">{w.share.toFixed(1)} %</span>{:else if w}<span class="band {w.band}" title="Performance cost: {BAND_LABEL[w.band].toLowerCase()}">{w.band === "negligible" ? "<0.1 %" : "n/a"}</span>{/if}</span>
          {/if}
          <span class="vers">{#each versions as v}<span class:off={!(m.supportedVersions ?? []).includes(v)}>{v}</span>{/each}</span>
          <span class="g">{#if store.groupOf(m.uid)}<span class="dot c-{store.groupOf(m.uid)?.color}" title={store.groupOf(m.uid)?.name}></span>{/if}</span>
          <span class="flags">
            {#if chg}<span class="flag chg" title="Changed since you last opened Circinus: {describeChange(chg)}">{@html chg.kind === "added" ? I.plus : I.change}</span>{/if}
            {#if upd}<span class="flag note" title="A newer version is on the Workshop, updated {new Date(upd.remoteUpdated * 1000).toLocaleDateString()}">{@html I.up}</span>{/if}
            {#each issues.slice(0, 3) as i}<span class="flag {severityOf(i)}" title={describe(i, store.byUid, m.uid)}>{@html sevIcon[severityOf(i)] ?? sevIcon.warning}</span>{/each}
            {#if m.invalid && it.inactive}<span class="flag error" title={m.invalid}>{@html I.error}</span>{/if}
            {#if store.pinned.has(m.uid)}<span class="flag pin" title="Pinned: keeps this position when sorting">{@html I.pin}</span>{/if}
          </span>
          <span class="delta num" class:down={delta && delta > 0} class:up={delta && delta < 0}>{#if delta}{delta > 0 ? "+" : ""}{delta}{/if}</span>
        </div>
      {/if}
    {/each}
    {#if dropEnd}<div class="drop-line" style="transform: translateY({total}px)"></div>{/if}
  </div>
  {#if !items.length}
    <div class="empty">{store.active.length || store.tab !== "active" ? "No mod matches the search or filters." : "No active mods. Import a list, or activate mods from the Inactive tab."}</div>
  {/if}
</div>

<style>
  .list { flex: 1; min-height: 0; overflow: auto; padding: 0 6px 10px; position: relative; }
  .spacer { position: relative; }
  .ph { position: absolute; left: 0; right: 0; height: 44px; display: flex; align-items: center; gap: 10px; padding: 14px 10px 6px; background: var(--surface); }
  /* Sticky overlay of the current section; the negative bottom margin keeps it out of the flow so row offsets stay exact. */
  .ph.floating { position: sticky; top: 0; z-index: 3; box-shadow: 0 6px 8px -6px rgba(0, 0, 0, 0.5); margin: 0 -6px -44px; padding-left: 16px; padding-right: 16px; }
  .ph .dot { width: 7px; height: 7px; }
  .ph .n { font-size: 11px; font-weight: 700; letter-spacing: 0.09em; text-transform: uppercase; color: var(--text-2); }
  .ph .c { font-size: 11px; color: var(--text-3); font-weight: 600; }
  .ph .note { margin-left: auto; font-size: 11.5px; color: var(--text-3); }
  .row { position: absolute; left: 0; right: 0; height: 40px; display: grid; grid-template-columns: 34px 18px 22px minmax(0, 1fr) 150px auto 64px 14px 82px 40px; align-items: center; gap: 10px; padding: 0 8px 0 4px; border-radius: var(--r-row); cursor: default; }
  .row:hover { background: var(--surface-2); }
  .row.sel { background: var(--surface-3); }
  .row.off { opacity: 0.72; }
  .row.off.sel, .row.off:hover { opacity: 1; }
  .row.drop-before::before, .row.drop-after::after { content: ""; position: absolute; left: 8px; right: 8px; height: 2px; background: var(--amber); border-radius: 1px; }
  .row.drop-before::before { top: -1px; }
  .row.drop-after::after { bottom: -1px; }
  .drop-line { position: absolute; left: 8px; right: 8px; height: 2px; background: var(--amber); border-radius: 1px; }
  .idx { font-family: var(--mono); font-size: 11.5px; color: var(--text-3); text-align: right; }
  .grip { color: var(--text-4); opacity: 0; display: grid; place-items: center; cursor: grab; }
  .row:hover .grip, .row.sel .grip { opacity: 1; }
  .grip :global(svg) { width: 12px; height: 12px; }
  .name { min-width: 0; display: flex; flex-direction: column; justify-content: center; line-height: 1.2; }
  .name b { font-weight: 600; font-size: 13.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .name span { font-size: 11.5px; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .pkg { font-family: var(--mono); font-size: 11px; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .wt { min-width: 60px; display: flex; justify-content: flex-end; }
  .vers { justify-content: flex-end; }
  .g { display: grid; place-items: center; }
  .flags { display: flex; gap: 4px; justify-content: flex-end; align-items: center; }
  .flags .flag { width: 17px; height: 17px; }
  .flags .flag :global(svg) { width: 15px; height: 15px; }
  .delta { font: 700 11.5px var(--mono); text-align: right; color: var(--text-3); }
  .delta.down { color: var(--amber); }
  .delta.up { color: var(--blue); }
  .empty { padding: 40px; text-align: center; color: var(--text-3); }
  @media (max-width: 1240px) { .row { grid-template-columns: 34px 18px 22px minmax(0, 1fr) auto 64px 14px 78px 40px; } .pkg { display: none; } }
</style>
