<script lang="ts">
  import { tick } from "svelte";
  import { store } from "$lib/store.svelte";
  import { I } from "$lib/icons";
  import { describe } from "$lib/describe";
  import { BAND_LABEL, describeChange, severityOf, type Issue, type ModInfo } from "$lib/types";

  // ---- virtualization: only the rows in view exist in the DOM ----
  const ROW = 40;
  const HEADER = 44;
  /** The column header at the top of the list; the rows scroll under it. */
  const TOP = 34;
  const OVERSCAN = 8;
  type Item = { kind: "header"; key: string; name: string; color: string; note: string; count: number } | { kind: "row"; key: string; mod: ModInfo; inactive: boolean };

  let scroller = $state<HTMLDivElement | null>(null);
  let scrollTop = $state(0);
  let viewport = $state(600);
  let dragUids = $state<string[]>([]);
  let dropAt = $state<{ uid: string; after: boolean } | null>(null);
  let dropEnd = $state(false);

  const items = $derived.by((): Item[] => {
    const out: Item[] = [];
    if (store.tab !== "inactive") {
      if (store.byPhase) {
        for (const sec of store.sections) {
          if (sec.group) out.push({ kind: "header", key: `h:${sec.phase.id}:${sec.group.id}`, name: sec.group.name, color: sec.group.color, note: `Your group, after ${sec.phase.name.toLowerCase()}`, count: sec.mods.length });
          else out.push({ kind: "header", key: `h:${sec.phase.id}`, name: sec.phase.name, color: sec.phase.color, note: sec.phase.note, count: sec.mods.length });
          for (const m of sec.mods) out.push({ kind: "row", key: m.uid, mod: m, inactive: false });
        }
      } else {
        // The plain load order, exactly as ModsConfig.xml has it.
        const va = store.visibleActive;
        if (store.tab === "all") out.push({ kind: "header", key: "h:active", name: "Active", color: "blue", note: "In load order, as ModsConfig.xml has it", count: va.length });
        for (const m of va) out.push({ kind: "row", key: m.uid, mod: m, inactive: false });
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
    // Rows live below the column header: offsets are TOP further down the scroller.
    const top = Math.max(0, scrollTop - TOP - OVERSCAN * ROW);
    const bottom = scrollTop - TOP + viewport + OVERSCAN * ROW;
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
  /** The section whose header has scrolled under the column header. */
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
    if (y < scrollTop + HEADER || y + ROW > scrollTop + viewport - TOP) {
      scroller.scrollTop = Math.max(0, y + TOP - viewport / 2);
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

  // ---- the badge columns ----
  /** Issues of one severity, and the text for the column's tooltip. */
  function bySeverity(issues: Issue[], sev: "error" | "warning" | "note", m: ModInfo): { n: number; text: string } {
    const list = issues.filter((i) => severityOf(i) === sev);
    return { n: list.length, text: list.map((i) => describe(i, store.byUid, m.uid)).join("\n") };
  }

  // ---- interaction ----
  function visibleList(): string[] {
    return items.filter((it) => it.kind === "row").map((it) => it.key);
  }
  function click(e: MouseEvent, m: ModInfo) {
    store.select(m.uid, { toggle: e.ctrlKey || e.metaKey, range: e.shiftKey, list: visibleList() });
  }
  /** Right click: the menu for the selection when the row is part of it, else for that row alone. */
  function contextMenu(e: MouseEvent, m: ModInfo) {
    e.preventDefault();
    e.stopPropagation();
    if (!store.selected.includes(m.uid)) store.select(m.uid);
    store.menu = { x: e.clientX, y: e.clientY, uids: store.selected.includes(m.uid) ? [...store.selected] : [m.uid] };
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
  // ---- drag to reorder, driven by pointer events so it behaves the same in a browser and in
  // the Tauri webview (HTML5 drag and drop is unreliable there on Windows) ----
  let press: { x: number; y: number; uid: string; pointerId: number } | null = null;
  let dragging = $state(false);
  let dragCount = $state(0);
  let ghost = $state({ x: 0, y: 0 });
  let suppressClick = false;
  let autoScroll = 0;

  function pointerDown(e: PointerEvent, m: ModInfo) {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button, a, input, select")) return;
    press = { x: e.clientX, y: e.clientY, uid: m.uid, pointerId: e.pointerId };
  }
  function pointerMove(e: PointerEvent) {
    if (!press) return;
    if (!dragging) {
      if (Math.hypot(e.clientX - press.x, e.clientY - press.y) < 6) return;
      // Drag whatever is selected when the pressed row is part of it, else just that row.
      if (!store.selected.includes(press.uid)) store.select(press.uid);
      dragUids = store.selected.filter((u) => store.byUid.has(u));
      dragCount = dragUids.length;
      dragging = true;
      suppressClick = true;
      document.body.style.cursor = "grabbing";
    }
    ghost = { x: e.clientX, y: e.clientY };
    updateDrop(e.clientX, e.clientY);
    edgeScroll(e.clientY);
  }
  function updateDrop(x: number, y: number) {
    if (!scroller) return;
    const el = document.elementFromPoint(x, y) as HTMLElement | null;
    const row = el?.closest(".row[data-uid]") as HTMLElement | null;
    if (row) {
      const uid = row.dataset.uid ?? "";
      if (!store.activeSet.has(uid)) { dropAt = null; dropEnd = false; return; }
      const r = row.getBoundingClientRect();
      dropAt = { uid, after: y > r.top + r.height / 2 };
      dropEnd = false;
      return;
    }
    const box = scroller.getBoundingClientRect();
    const inside = x >= box.left && x <= box.right && y >= box.top && y <= box.bottom;
    dropAt = null;
    dropEnd = inside && store.tab !== "inactive" && y > box.top + TOP + (offsets[items.length] ?? 0) - scrollTop;
  }
  /** Scroll the list while the pointer sits near its top or bottom edge. */
  function edgeScroll(y: number) {
    if (!scroller) return;
    const box = scroller.getBoundingClientRect();
    const zone = 48;
    autoScroll = y < box.top + zone ? -Math.ceil((box.top + zone - y) / 6) : y > box.bottom - zone ? Math.ceil((y - (box.bottom - zone)) / 6) : 0;
    if (autoScroll && !scrollTimer) scrollTimer = setInterval(() => {
      if (!scroller || !autoScroll) return;
      scroller.scrollTop += autoScroll;
      scrollTop = scroller.scrollTop;
      updateDrop(ghost.x, ghost.y);
    }, 16);
    if (!autoScroll && scrollTimer) { clearInterval(scrollTimer); scrollTimer = 0; }
  }
  let scrollTimer = 0;
  async function pointerUp(e: PointerEvent) {
    const wasDragging = dragging;
    press = null;
    if (scrollTimer) { clearInterval(scrollTimer); scrollTimer = 0; }
    autoScroll = 0;
    document.body.style.cursor = "";
    if (!wasDragging) return;
    dragging = false;
    updateDrop(e.clientX, e.clientY);
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
  function pointerCancel() {
    press = null;
    dragging = false;
    dragUids = []; dropAt = null; dropEnd = false;
    if (scrollTimer) { clearInterval(scrollTimer); scrollTimer = 0; }
    document.body.style.cursor = "";
  }
  function rowClick(e: MouseEvent, m: ModInfo) {
    if (suppressClick) { suppressClick = false; return; }
    click(e, m);
  }
</script>

<svelte:window onpointermove={pointerMove} onpointerup={pointerUp} onpointercancel={pointerCancel} onblur={pointerCancel} />

<div class="card list" class:dragging class:weights={store.showWeight} bind:this={scroller} bind:clientHeight={viewport} onscroll={onScroll} role="listbox" aria-label="Load order" aria-multiselectable="true" tabindex="-1">
  <div class="hdr" role="presentation">
    <span class="h idx">#</span>
    <span></span>
    <span class="h">Mod</span>
    <span class="h pkg">Package id</span>
    {#if store.showWeight}<span class="h wt" title="Share of frame time, from circinus.sh or your own runs">Cost</span>{/if}
    <span class="h vers" title="Game versions the mod says it supports">Versions</span>
    <span class="h phz" title="Where HALO files the mod: the game, a library, content, a patch, a texture pack, a late loader, a performance mod">Phase</span>
    <span class="h g" title="The group the mod is in">Group</span>
    <span class="badges">
      <span class="h b" title="Changed since you last opened Circinus: new, or updated on disk">{@html I.change}<i>Changed</i></span>
      <span class="h b" title="A newer version is on the Workshop">{@html I.up}<i>Update</i></span>
      <span class="h b" title="Errors: a missing dependency, two mods that do not work together, a rule loop, a mod above the game's own content">{@html I.error}<i>Errors</i></span>
      <span class="h b" title="Warnings: a load-order rule not met, a mod not made for this game version, a performance mod not at the end">{@html I.warn}<i>Warnings</i></span>
      <span class="h b" title="HALO notes: textures replaced by more than one mod, a rule HALO set aside">{@html I.note}<i>Notes</i></span>
      <span class="h b" title="Pinned: keeps its position when HALO sorts">{@html I.pin}<i>Pinned</i></span>
    </span>
    <span class="h delta" title="How far HALO would move the mod, once you preview a sort">Move</span>
  </div>
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
        {@const pl = it.inactive ? undefined : store.placement(m.uid)}
        {@const grp = store.groupOf(m.uid)}
        {@const err = bySeverity(issues, "error", m)}
        {@const warn = bySeverity(issues, "warning", m)}
        {@const note = bySeverity(issues, "note", m)}
        {@const invalid = m.invalid && it.inactive ? m.invalid : ""}
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
          onclick={(e) => rowClick(e, m)}
          oncontextmenu={(e) => contextMenu(e, m)}
          ondblclick={() => toggle(m)}
          onkeydown={(e) => key(e, m)}
          onpointerdown={(e) => pointerDown(e, m)}
        >
          <span class="idx num">{it.inactive ? "" : (store.indexOf.get(m.uid) ?? 0) + 1}</span>
          <span class="grip">{@html I.grip}</span>
          <span class="name"><b>{m.name ?? m.uid}</b><span>{m.invalid ?? (m.authors ?? []).join(", ")}</span></span>
          <span class="pkg">{m.packageId}</span>
          {#if store.showWeight}
            <span class="wt">{#if w && w.share != null}<span class="band {w.band}" title="Performance cost: {w.share.toFixed(2)} % of frame time, {BAND_LABEL[w.band].toLowerCase()}. {w.measured ?? '?'} runs measured, from {w.origin === 'local' ? 'your runs' : 'circinus.sh'}">{w.share.toFixed(1)} %</span>{:else if w}<span class="band {w.band}" title="Performance cost: {BAND_LABEL[w.band].toLowerCase()}">{w.band === "negligible" ? "<0.1 %" : "n/a"}</span>{/if}</span>
          {/if}
          <span class="vers">{#each versions as v}<span class:off={!(m.supportedVersions ?? []).includes(v)}>{v}</span>{/each}</span>
          <span class="phz">{#if pl}<span class="dot c-{store.phaseInfo(pl.phase).color}" title="{store.phaseInfo(pl.phase).name} · {pl.reason}"></span>{/if}</span>
          <span class="g">{#if grp}<span class="dot c-{grp.color}" title="{grp.name}{grp.auto && !store.snap?.user.modGroups[m.uid] ? ' (by the group’s own rule)' : ''}"></span>{/if}</span>
          <span class="badges">
            <span class="b">{#if chg}<span class="flag chg" title="Changed since you last opened Circinus: {describeChange(chg)}">{@html chg.kind === "added" ? I.plus : I.change}</span>{/if}</span>
            <span class="b">{#if upd}<span class="flag" title="A newer version is on the Workshop, updated {new Date(upd.remoteUpdated * 1000).toLocaleDateString()}">{@html I.up}</span>{/if}</span>
            <span class="b">{#if err.n || invalid}<span class="flag" title={[invalid, err.text].filter(Boolean).join("\n")}>{@html I.error}{#if err.n + (invalid ? 1 : 0) > 1}<em class="num">{err.n + (invalid ? 1 : 0)}</em>{/if}</span>{/if}</span>
            <span class="b">{#if warn.n}<span class="flag" title={warn.text}>{@html I.warn}{#if warn.n > 1}<em class="num">{warn.n}</em>{/if}</span>{/if}</span>
            <span class="b">{#if note.n}<span class="flag" title={note.text}>{@html I.note}{#if note.n > 1}<em class="num">{note.n}</em>{/if}</span>{/if}</span>
            <span class="b">{#if store.pinned.has(m.uid)}<span class="flag" title="Pinned: keeps this position when sorting">{@html I.pin}</span>{/if}</span>
          </span>
          <span class="delta num" class:down={delta && delta > 0} class:up={delta && delta < 0}>{#if delta}{delta > 0 ? "+" : ""}{delta}{/if}</span>
        </div>
      {/if}
    {/each}
    {#if dropEnd}<div class="drop-line" style="transform: translateY({total}px)"></div>{/if}
  </div>
  {#if dragging}
    <div class="ghost" style="left: {ghost.x + 14}px; top: {ghost.y + 10}px">{dragCount === 1 ? store.byUid.get(dragUids[0])?.name ?? "1 mod" : `${dragCount} mods`}</div>
  {/if}
  {#if !items.length}
    <div class="empty">{store.active.length || store.tab !== "active" ? "No mod matches the search or filters." : "No active mods. Import a list, or activate mods from the Inactive tab."}</div>
  {/if}
</div>

<style>
  .list { flex: 1; min-height: 0; overflow: hidden auto; padding: 0 6px 10px; position: relative; }
  .list.dragging { cursor: grabbing; }
  .list.dragging .row { cursor: grabbing; }
  .ghost { position: fixed; z-index: 30; pointer-events: none; background: var(--surface-4); color: var(--text); font-size: 12.5px; font-weight: 600; padding: 6px 10px; border-radius: 8px; box-shadow: var(--shadow-float); max-width: 260px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .spacer { position: relative; }
  /* One grid for the column header and every row, so the columns line up. */
  .hdr, .row { display: grid; grid-template-columns: 34px 18px minmax(0, 1fr) 150px 64px 44px 44px 238px 40px; align-items: center; gap: 10px; padding: 0 8px 0 4px; }
  .list.weights .hdr, .list.weights .row { grid-template-columns: 34px 18px minmax(0, 1fr) 150px 64px 64px 44px 44px 238px 40px; }
  .hdr { position: sticky; top: 0; z-index: 4; height: 34px; background: var(--surface); border-bottom: 1px solid var(--surface-3); margin: 0 -6px; padding-left: 10px; padding-right: 14px; }
  .hdr .h { font-size: 10.5px; font-weight: 700; letter-spacing: 0.06em; text-transform: uppercase; color: var(--text-3); white-space: nowrap; overflow: hidden; }
  .hdr .idx, .hdr .vers, .hdr .delta, .hdr .wt { text-align: right; }
  .hdr .phz, .hdr .g { text-align: center; }
  .hdr .b { display: grid; justify-items: center; gap: 1px; text-transform: none; letter-spacing: 0; font-size: 9px; font-weight: 600; }
  .hdr .b :global(svg) { width: 13px; height: 13px; }
  .hdr .b i { font-style: normal; }
  .ph { position: absolute; left: 0; right: 0; height: 44px; display: flex; align-items: center; gap: 10px; padding: 14px 10px 6px; background: var(--surface); }
  /* Sticky overlay of the current section, under the column header; the negative bottom margin keeps it out of the flow so row offsets stay exact. */
  .ph.floating { position: sticky; top: 34px; z-index: 3; box-shadow: 0 6px 8px -6px rgba(0, 0, 0, 0.5); margin: 0 -6px -44px; padding-left: 16px; padding-right: 16px; }
  .ph .dot { width: 7px; height: 7px; }
  .ph .n { font-size: 11px; font-weight: 700; letter-spacing: 0.09em; text-transform: uppercase; color: var(--text-2); }
  .ph .c { font-size: 11px; color: var(--text-3); font-weight: 600; }
  .ph .note { margin-left: auto; font-size: 11.5px; color: var(--text-3); }
  .row { position: absolute; left: 0; right: 0; height: 40px; border-radius: var(--r-row); cursor: default; }
  .row:hover { background: var(--surface-2); }
  .row.sel { background: var(--surface-3); }
  .row.off { opacity: 0.72; }
  .row.off.sel, .row.off:hover { opacity: 1; }
  .row.drop-before::before, .row.drop-after::after { content: ""; position: absolute; left: 8px; right: 8px; height: 2px; background: var(--amber); border-radius: 1px; }
  .row.drop-before::before { top: -1px; }
  .row.drop-after::after { bottom: -1px; }
  .drop-line { position: absolute; left: 8px; right: 8px; height: 2px; background: var(--amber); border-radius: 1px; }
  .idx { font-family: var(--mono); font-size: 11.5px; color: var(--text-3); text-align: right; }
  .grip { color: var(--text-4); opacity: 0; display: grid; place-items: center; cursor: grab; touch-action: none; }
  .row:hover .grip, .row.sel .grip { opacity: 1; }
  .grip :global(svg) { width: 12px; height: 12px; }
  .name { min-width: 0; display: flex; flex-direction: column; justify-content: center; line-height: 1.2; }
  .name b { font-weight: 600; font-size: 13.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .name span { font-size: 11.5px; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .pkg { font-family: var(--mono); font-size: 11px; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .row .wt { min-width: 60px; display: flex; justify-content: flex-end; }
  .vers { justify-content: flex-end; }
  .row .phz, .row .g { display: grid; place-items: center; }
  .row .phz .dot, .row .g .dot { width: 8px; height: 8px; }
  /* Six fixed slots, one per kind of badge, so nothing ever draws over anything else. */
  .badges { display: grid; grid-template-columns: repeat(6, 38px); gap: 2px; align-items: center; }
  .badges .b { display: grid; place-items: center; height: 24px; }
  .flag { display: inline-flex; align-items: center; gap: 1px; height: 20px; padding: 0 2px; border-radius: 6px; }
  .flag :global(svg) { width: 15px; height: 15px; flex: none; }
  .flag em { font-style: normal; font-size: 10.5px; font-weight: 700; color: var(--text-2); }
  .delta { font: 700 11.5px var(--mono); text-align: right; color: var(--text-3); }
  .delta.down { color: var(--amber); }
  .delta.up { color: var(--blue); }
  .empty { padding: 40px; text-align: center; color: var(--text-3); }
  @media (max-width: 1240px) {
    .hdr, .row { grid-template-columns: 34px 18px minmax(0, 1fr) 64px 44px 44px 238px 40px; }
    .list.weights .hdr, .list.weights .row { grid-template-columns: 34px 18px minmax(0, 1fr) 64px 64px 44px 44px 238px 40px; }
    .pkg { display: none; }
  }
</style>
