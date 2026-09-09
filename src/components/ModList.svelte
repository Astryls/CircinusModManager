<script lang="ts">
  import { tick } from "svelte";
  import { store, type Pane, type SortKey } from "$lib/store.svelte";
  import { I } from "$lib/icons";
  import { describe, explainLoad } from "$lib/describe";
  import { BAND_LABEL, LOAD_BAND_LABEL, describeChange, severityOf, type Issue, type ModInfo } from "$lib/types";

  // One list, shown three ways. Without `pane` this is the whole list and the tabs decide what is
  // in it; with one it is half of the side-by-side library and shows only its own half.
  let { pane = null }: { pane?: Pane | null } = $props();

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
  let listW = $state(1000);

  // ---- columns: Mod and Package id can be dragged; the rest are fixed ----
  const NAME_MIN = 120;
  /** Half the width, so a pane holds a column only while the name stays worth reading. */
  const PANE_NAME_MIN = 180;
  const PKG_MIN = 60;
  const PKG_DEFAULT = 120;
  /** A width being dragged right now, ahead of the saved one. */
  let live = $state<{ key: "name" | "pkg"; px: number } | null>(null);
  const savedNameW = $derived(live?.key === "name" ? live.px : store.columns.name);
  const pkgW = $derived(live?.key === "pkg" ? live.px : (store.columns.pkg ?? PKG_DEFAULT));
  /** An inactive mod has no place in the order, so nothing HALO could move it by. */
  const showMove = $derived(!!store.preview && pane !== "inactive");
  /** The optional columns, in the order they appear. Cost follows its own setting (it needs
      figures Circinus only has once weights are loaded); the rest are the user's to choose. */
  const on = $derived(new Set(store.listColumns));
  const optional = $derived([
    { key: "time", w: 62, show: on.has("time") },
    { key: "cost", w: 64, show: store.showWeight },
    { key: "load", w: 58, show: on.has("load") },
    { key: "versions", w: 64, show: on.has("versions") },
    { key: "phase", w: 106, show: on.has("phase") },
    { key: "group", w: 106, show: on.has("group") }
  ]);
  const wanted = $derived(optional.filter((c) => c.show));
  /** A pane carries six badges in the space, and without their words: the strip above it says
      what the list is, so the row only has to be readable. */
  const BADGES = $derived(pane === null ? 232 : 138);
  const minName = $derived(pane === null ? NAME_MIN : PANE_NAME_MIN);
  /** What a row costs before any optional column: the number, the badges, the move, the gaps. */
  const baseW = $derived(34 + BADGES + (showMove ? 40 + 6 : 0) + 3 * 6);
  /** The one list shows every column the user asked for. A pane is half as wide, so it keeps them
      from the left and drops the rest rather than squeeze the names past reading. */
  const shown = $derived.by(() => {
    if (pane === null) return wanted;
    const budget = listW - 24 - baseW - minName;
    const keep: typeof wanted = [];
    let used = 0;
    for (const c of wanted) {
      if (used + c.w + 6 > budget) break;
      used += c.w + 6;
      keep.push(c);
    }
    return keep;
  });
  /** Width of everything that is neither the name nor the package id, gaps included. */
  const fixedW = $derived(baseW + shown.reduce((n, c) => n + c.w + 6, 0));
  /** Narrow lists drop the package id column rather than squeeze the names below their minimum. */
  const showPkg = $derived(listW - 24 - fixedW - minName >= (store.columns.pkg ?? PKG_DEFAULT) + 6);
  /** The optional columns this list is actually drawing. */
  const has = $derived(new Set(shown.map((c) => c.key)));
  /** A width dragged in the wide single list would not fit a pane; there the name takes what is
      left instead of pushing the row past its edge. */
  const nameW = $derived(savedNameW == null || pane === null ? savedNameW : Math.max(minName, Math.min(savedNameW, listW - 24 - fixedW - (showPkg ? pkgW + 6 : 0))));
  const template = $derived.by(() => {
    const cols = ["34px", nameW != null ? `${nameW}px` : `minmax(${minName}px, 1fr)`];
    if (showPkg) cols.push(`${pkgW}px`);
    if (nameW != null) cols.push("minmax(0, 1fr)");
    for (const c of shown) cols.push(`${c.w}px`);
    cols.push(`${BADGES}px`);
    if (showMove) cols.push("40px");
    return cols.join(" ");
  });
  let colDrag: { key: "name" | "pkg"; startX: number; startW: number; max: number } | null = null;
  function colDown(e: PointerEvent, key: "name" | "pkg") {
    if (e.button !== 0 || !scroller) return;
    e.preventDefault();
    e.stopPropagation();
    const cell = scroller.querySelector(`.hdr .h.${key}`) as HTMLElement | null;
    const other = scroller.querySelector(`.hdr .h.${key === "name" ? "pkg" : "name"}`) as HTMLElement | null;
    const fill = scroller.querySelector(".hdr .fill") as HTMLElement | null;
    if (!cell) return;
    const startW = cell.getBoundingClientRect().width;
    // Everything but the two adjustable columns and the filler keeps its width; the
    // adjustable one may grow until the other is at its minimum.
    const inner = scroller.clientWidth - 24;
    const fixed = inner - startW - (other?.getBoundingClientRect().width ?? 0) - (fill?.getBoundingClientRect().width ?? 0);
    const max = Math.max(key === "name" ? NAME_MIN : PKG_MIN, inner - fixed - (key === "name" ? (showPkg ? PKG_MIN : 0) : NAME_MIN) - 8);
    colDrag = { key, startX: e.clientX, startW, max };
    live = { key, px: startW };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    document.body.style.cursor = "col-resize";
  }
  function colMove(e: PointerEvent) {
    if (!colDrag) return;
    const min = colDrag.key === "name" ? NAME_MIN : PKG_MIN;
    live = { key: colDrag.key, px: Math.round(Math.min(colDrag.max, Math.max(min, colDrag.startW + e.clientX - colDrag.startX))) };
  }
  function colUp() {
    if (!colDrag) return;
    const d = colDrag;
    const px = live?.px;
    colDrag = null;
    document.body.style.cursor = "";
    if (px != null && Math.abs(px - d.startW) >= 1) Promise.resolve(store.setColumn(d.key, px)).finally(() => (live = null));
    else live = null;
  }
  /** Double-click a handle: back to the default width. */
  function colReset(key: "name" | "pkg") {
    live = null;
    store.setColumn(key, null);
  }

  const sections = $derived(store.sections);
  const ordered = $derived(store.visibleActive);
  const indexOf = $derived(store.indexOf);
  const placementOf = (uid: string) => store.placement(uid);
  const items = $derived.by((): Item[] => {
    const out: Item[] = [];
    if (pane === "inactive") {
      // Its own pane says what it is in the strip above it, so the list needs no header.
      for (const m of store.visibleInactive) out.push({ kind: "row", key: m.uid, mod: m, inactive: true });
      return out;
    }
    // The New tab is the new mods and nothing else: no phase sections, because what you want
    // from it is when each one arrived, not where it will load.
    if (pane === null && store.tab === "new") {
      for (const m of store.visibleNew) out.push({ kind: "row", key: m.uid, mod: m, inactive: !store.activeSet.has(m.uid) });
      return out;
    }
    if (pane !== null || store.tab !== "inactive") {
      if (store.byPhase) {
        for (const sec of sections) {
          if (sec.group) out.push({ kind: "header", key: `h:${sec.phase.id}:${sec.group.id}`, name: sec.group.name, color: sec.group.color, note: `Your group, after ${sec.phase.name.toLowerCase()}`, count: sec.mods.length });
          else out.push({ kind: "header", key: `h:${sec.phase.id}`, name: sec.phase.name, color: sec.phase.color, note: sec.phase.note, count: sec.mods.length });
          for (const m of sec.mods) out.push({ kind: "row", key: m.uid, mod: m, inactive: false });
        }
      } else {
        // The plain load order, exactly as ModsConfig.xml has it.
        if (pane === null && store.tab === "all") out.push({ kind: "header", key: "h:active", name: "Active", color: "blue", note: "In load order, as ModsConfig.xml has it", count: ordered.length });
        for (const m of ordered) out.push({ kind: "row", key: m.uid, mod: m, inactive: false });
      }
    }
    if (pane === null && store.tab !== "active") {
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

  // ---- keeping the place ----
  /** Which list this is. Side by side there are two, and they scroll independently. */
  const scope = $derived(`order:${pane ?? "main"}`);

  /** Remember the row under the top of the viewport, not the pixel.
   *
   *  A pixel is only right if the list is identical when you come back, and it often is not --
   *  a filter changed, a mod was activated, HALO moved things. The first row still on screen is
   *  the thing the user was actually looking at, so that is what is put back, at the same
   *  distance from the top. `top` is the fallback for when that row has gone. */
  function remember() {
    if (!scroller) return;
    const y = scroller.scrollTop;
    let i = Math.max(0, range.start);
    while (i < items.length && (items[i].kind !== "row" || offsets[i] + TOP < y)) i++;
    const hit = i < items.length && items[i].kind === "row" ? items[i] : null;
    store.scrollMemory.set(scope, { anchor: hit ? hit.key : null, delta: hit ? offsets[i] + TOP - y : 0, top: y });
  }

  /** Put it back, once, as soon as there are rows to measure against. */
  let restored = false;
  $effect(() => {
    if (restored || !scroller || !items.length) return;
    restored = true;
    const m = store.scrollMemory.get(scope);
    if (!m) return;
    const i = m.anchor ? items.findIndex((it) => it.kind === "row" && it.key === m.anchor) : -1;
    // The anchor is gone (deactivated, filtered out): the raw offset is the best guess left, and
    // the browser clamps it to the new length rather than scrolling into nothing.
    scroller.scrollTop = Math.max(0, i >= 0 ? offsets[i] + TOP - m.delta : m.top);
    scrollTop = scroller.scrollTop;
  });

  function onScroll() {
    if (scroller) scrollTop = scroller.scrollTop;
    remember();
  }
  function scrollToUid(uid: string, focus = false) {
    const i = items.findIndex((it) => it.kind === "row" && it.key === uid);
    if (i < 0 || !scroller) return;
    const y = offsets[i];
    if (y < scrollTop + HEADER || y + ROW > scrollTop + viewport - TOP) {
      scroller.scrollTop = Math.max(0, y + TOP - viewport / 2);
      scrollTop = scroller.scrollTop;
    }
    // Scoped to this list: side by side, the same mod has a row in the other pane too.
    if (focus) tick().then(() => (scroller?.querySelector(`[data-uid="${CSS.escape(uid)}"]`) as HTMLElement | null)?.focus());
  }
  $effect(() => {
    const req = store.scrollRequest;
    if (req && req.pane === pane) {
      tick().then(() => { scrollToUid(req.uid, req.focus); store.scrollRequest = null; });
    }
  });

  // ---- sorting ----
  /** The heading of a sortable column: what it says, whether it is the one in force, and which
   *  way round. Panes do not sort: they are two halves of one comparison and sorting one of
   *  them alone would compare two different orders. */
  const canSort = $derived(pane === null);
  const arrowFor = (key: SortKey) => (store.sortKey === key ? (store.sortDir === 1 ? "\u2191" : "\u2193") : "");

  // ---- the badge columns ----
  /** Issues of one severity, and the text for the column's tooltip. */
  function bySeverity(issues: Issue[], sev: "error" | "warning" | "note", m: ModInfo): { n: number; text: string } {
    const list = issues.filter((i) => severityOf(i) === sev);
    return { n: list.length, text: list.map((i) => describe(i, store.byUid, m.uid)).join("\n") };
  }

  /** An estimate in milliseconds, said in seconds at a precision the estimate can support.
   *  Three decimal places on a number that came from counting XML nodes would be a lie about
   *  how well it is known; "under 0.05 s" is the honest floor. */
  function secs(ms: number): string {
    if (ms >= 9950) return `${(ms / 1000).toFixed(0)} s`;
    if (ms >= 950) return `${(ms / 1000).toFixed(1)} s`;
    if (ms >= 50) return `${(ms / 1000).toFixed(2)} s`;
    return "<0.05 s";
  }

  /** When a mod arrived, in words, or undefined for one that was already here. Relative for the
   *  first day because "3 hours ago" is what you want to know about something that just landed,
   *  and a date after that because "9 days ago" is not how anyone remembers a Tuesday. */
  function whenItCame(uid: string): string | undefined {
    const at = store.firstSeenByUid.get(uid);
    if (!at) return undefined;
    const secs = Math.floor(Date.now() / 1000) - at;
    const ago = (n: number, unit: string) => `arrived ${n} ${unit}${n === 1 ? "" : "s"} ago`;
    if (secs < 90) return "arrived just now";
    if (secs < 3600) return ago(Math.round(secs / 60), "minute");
    if (secs < 86400) return ago(Math.round(secs / 3600), "hour");
    return `arrived ${new Date(at * 1000).toLocaleDateString()}`;
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
  /** When the last drag ended, so the click it leaves behind can be told from a real one. */
  let droppedAt = 0;
  let autoScroll = 0;
  /** The list the pointer is over, which is not always the one the drag started in. */
  let dropScroller: HTMLElement | null = null;
  let autoTarget: HTMLElement | null = null;

  function pointerDown(e: PointerEvent, m: ModInfo) {
    if (e.button !== 0 || (e.target as HTMLElement).closest("button, a, input, select")) return;
    // Not while the list is sorted by something other than the load order. Dropping a row
    // between two others writes a position in ModsConfig.xml, and between two others *of a list
    // sorted by name* is not a position anybody chose: what you saw above and below the gap is
    // not what would end up there. The Sorted chip in the toolbar is one click away.
    if (store.sorted) return;
    press = { x: e.clientX, y: e.clientY, uid: m.uid, pointerId: e.pointerId };
  }
  function pointerMove(e: PointerEvent) {
    if (!press) return;
    if (!dragging) {
      if (Math.hypot(e.clientX - press.x, e.clientY - press.y) < 6) return;
      // Drag whatever is selected when the pressed row is part of it, else just that row.
      if (!store.selected.includes(press.uid)) store.select(press.uid);
      const uids = store.selected.filter((u) => store.byUid.has(u));
      store.drag = { uids, from: pane };
      dragCount = uids.length;
      dragging = true;
      document.body.style.cursor = "grabbing";
    }
    ghost = { x: e.clientX, y: e.clientY };
    updateDrop(e.clientX, e.clientY);
    edgeScroll(e.clientY);
  }
  /** Where the pointer would drop, in whichever list it is over. */
  function updateDrop(x: number, y: number) {
    const el = document.elementFromPoint(x, y) as HTMLElement | null;
    const list = el?.closest(".list") as HTMLElement | null;
    dropScroller = list;
    const target = list?.dataset.pane ?? "";
    // HALO's proposal is read-only, so nothing can be dropped into it.
    if (!list) { store.drop = null; return; }
    const to: Pane | null = target === "single" || target === "" ? null : (target as Pane);
    // The inactive pane takes anything active: dropping there switches a mod off, and there is no
    // position to pick, so the whole pane lights up rather than a line between two rows.
    if (to === "inactive") {
      store.drop = store.drag?.uids.some((u) => store.activeSet.has(u)) ? { pane: "inactive" } : null;
      return;
    }
    const row = el?.closest(".row[data-uid]") as HTMLElement | null;
    if (row) {
      const uid = row.dataset.uid ?? "";
      if (!store.activeSet.has(uid)) { store.drop = null; return; }
      const r = row.getBoundingClientRect();
      store.drop = { pane: to, uid, after: y > r.top + r.height / 2 };
      return;
    }
    // Past the last row: the end of the order. Measured from the spacer, so it works in a pane the
    // drag did not start in, whose scroll position this component does not know.
    const spacer = list.querySelector(".spacer") as HTMLElement | null;
    const bottom = spacer?.getBoundingClientRect().bottom ?? 0;
    const box = list.getBoundingClientRect();
    const inside = x >= box.left && x <= box.right && y >= box.top && y <= box.bottom;
    store.drop = inside && (to === "active" || store.tab !== "inactive") && y > bottom ? { pane: to, end: true } : null;
  }
  /** Scroll the list under the pointer while it sits near that list's top or bottom edge. Off the
   *  lists entirely, the one the drag came from is the one that moves. */
  function edgeScroll(y: number) {
    const el = dropScroller ?? scroller;
    if (!el) { autoScroll = 0; return; }
    autoTarget = el;
    const box = el.getBoundingClientRect();
    const zone = 48;
    autoScroll = y < box.top + zone ? -Math.ceil((box.top + zone - y) / 6) : y > box.bottom - zone ? Math.ceil((y - (box.bottom - zone)) / 6) : 0;
    if (autoScroll && !scrollTimer) scrollTimer = setInterval(() => {
      if (!autoTarget || !autoScroll) return;
      autoTarget.scrollTop += autoScroll;
      if (autoTarget === scroller && scroller) scrollTop = scroller.scrollTop;
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
    droppedAt = performance.now();
    updateDrop(e.clientX, e.clientY);
    const uids = store.drag?.uids ?? [], target = store.drop;
    store.drag = null; store.drop = null; dropScroller = null;
    if (!uids.length || !target) return;
    // Dropped on the inactive pane: switch the active ones off, there being no place to put them.
    if (target.pane === "inactive") {
      const on = uids.filter((u) => store.activeSet.has(u));
      if (on.length) await store.deactivate(on);
      return;
    }
    let index = store.active.length;
    if (target.uid) {
      const i = store.indexOf.get(target.uid);
      if (i == null) return;
      index = i + (target.after ? 1 : 0);
    } else if (!target.end) return;
    const fresh = uids.filter((u) => !store.activeSet.has(u));
    const moving = uids.filter((u) => store.activeSet.has(u));
    if (moving.length) await store.moveTo(moving, index);
    if (fresh.length) await store.activate(fresh, moving.length ? undefined : index);
  }
  function pointerCancel() {
    if (!press && !dragging) return;
    press = null;
    dragging = false;
    store.drag = null; store.drop = null; dropScroller = null;
    if (scrollTimer) { clearInterval(scrollTimer); scrollTimer = 0; }
    document.body.style.cursor = "";
  }
  function rowClick(e: MouseEvent, m: ModInfo) {
    // The browser fires a click on the row a drag started from, and that one click is not a click.
    // Read as a moment rather than a flag, so a drag that ends in the other pane cannot leave a
    // later, real click in this one swallowed.
    if (performance.now() - droppedAt < 120) return;
    click(e, m);
  }
  const emptyText = $derived(
    pane === "inactive" ? "Every installed mod is active." :
    pane === "active" ? "No active mod matches the search or filters." :
    store.active.length || store.tab !== "active" ? "No mod matches the search or filters." : "No active mods. Import a list, or activate mods from the Inactive tab."
  );
</script>

<svelte:window onpointermove={pointerMove} onpointerup={pointerUp} onpointercancel={pointerCancel} onblur={pointerCancel} />

<div
  class="card list"
  class:dragging
  class:pane={pane !== null}
  class:take={pane === "inactive" && store.drop?.pane === "inactive"}
  data-pane={pane ?? "single"}
  bind:this={scroller}
  bind:clientHeight={viewport}
  bind:clientWidth={listW}
  onscroll={onScroll}
  role="listbox"
  aria-label={pane === "inactive" ? "Inactive mods" : "Load order"}
  aria-multiselectable="true"
  tabindex="-1"
  style="--cols: {template}"
>
  <div class="hdr" role="presentation" onpointermove={colMove} onpointerup={colUp} onpointercancel={colUp}>
    <!-- Nothing inactive has a place in the order, so its pane numbers nothing and says so. -->
    <span class="h idx">{pane === "inactive" ? "" : "#"}</span>
    <span class="h name" class:by={store.sortKey === "name"}>{#if canSort}<button class="sortbtn" onclick={() => store.sortBy("name")} title="Sort by name">Mod{#if store.sortKey === "name"}<i class="dir">{arrowFor("name")}</i>{/if}</button>{:else}Mod{/if}<span class="grab" role="separator" aria-orientation="vertical" title="Drag to change the width; double-click for the default" onpointerdown={(e) => colDown(e, "name")} ondblclick={() => colReset("name")}></span></span>
    {#if showPkg}<span class="h pkg" class:by={store.sortKey === "pkg"}>{#if canSort}<button class="sortbtn" onclick={() => store.sortBy("pkg")} title="Sort by package id">Package id{#if store.sortKey === "pkg"}<i class="dir">{arrowFor("pkg")}</i>{/if}</button>{:else}Package id{/if}<span class="grab" role="separator" aria-orientation="vertical" title="Drag to change the width; double-click for the default" onpointerdown={(e) => colDown(e, "pkg")} ondblclick={() => colReset("pkg")}></span></span>{/if}
    {#if nameW != null}<span class="fill"></span>{/if}
    {#if has.has("time")}<span class="h tm" class:by={store.sortKey === "time"} title="How much of the game's loading time this mod is expected to add, in seconds. Estimated from what the folder holds, not measured with a stopwatch: use it to rank mods against each other rather than to predict the clock.">{#if canSort}<button class="sortbtn" onclick={() => store.sortBy("time")}>Time{#if store.sortKey === "time"}<i class="dir">{arrowFor("time")}</i>{/if}</button>{:else}Time{/if}</span>{/if}
    {#if has.has("cost")}<span class="h wt" class:by={store.sortKey === "cost"} title="Share of frame time, from circinus.sh or your own runs">{#if canSort}<button class="sortbtn" onclick={() => store.sortBy("cost")}>Cost{#if store.sortKey === "cost"}<i class="dir">{arrowFor("cost")}</i>{/if}</button>{:else}Cost{/if}</span>{/if}
    {#if has.has("load")}<span class="h load" class:by={store.sortKey === "load"} title="Expected share of the list's loading time, estimated from what the folder holds: Defs XML, patch operations and how far they search, PNG textures without DDS, assemblies. A ranking, not a stopwatch.">{#if canSort}<button class="sortbtn" onclick={() => store.sortBy("load")}>Load{#if store.sortKey === "load"}<i class="dir">{arrowFor("load")}</i>{/if}</button>{:else}Load{/if}</span>{/if}
    {#if has.has("versions")}<span class="h vers" class:by={store.sortKey === "versions"} title="Game versions the mod says it supports">{#if canSort}<button class="sortbtn" onclick={() => store.sortBy("versions")}>Versions{#if store.sortKey === "versions"}<i class="dir">{arrowFor("versions")}</i>{/if}</button>{:else}Versions{/if}</span>{/if}
    {#if has.has("phase")}<span class="h phz" class:by={store.sortKey === "phase"} title="Where HALO files the mod: the game, a library, content, a patch, a texture pack, a late loader, a performance mod">{#if canSort}<button class="sortbtn" onclick={() => store.sortBy("phase")}>Phase{#if store.sortKey === "phase"}<i class="dir">{arrowFor("phase")}</i>{/if}</button>{:else}Phase{/if}</span>{/if}
    {#if has.has("group")}<span class="h g" class:by={store.sortKey === "group"} title="The group the mod is in">{#if canSort}<button class="sortbtn" onclick={() => store.sortBy("group")}>Group{#if store.sortKey === "group"}<i class="dir">{arrowFor("group")}</i>{/if}</button>{:else}Group{/if}</span>{/if}
    <span class="badges">
      <span class="h b" title="Changed since you last opened Circinus: new, or updated on disk">{@html I.change}<i>Changed</i></span>
      <span class="h b" title="A newer version is on the Workshop">{@html I.up}<i>Update</i></span>
      <span class="h b" title="Errors: a missing dependency, two mods that do not work together, a rule loop, a mod above the game's own content">{@html I.error}<i>Errors</i></span>
      <span class="h b" title="Warnings: a load-order rule not met, a mod not made for this game version, a performance mod not at the end">{@html I.warn}<i>Warning</i></span>
      <span class="h b" title="HALO notes: textures replaced by more than one mod, a rule HALO set aside">{@html I.note}<i>Notes</i></span>
      <span class="h b" title="Pinned: keeps its position when HALO sorts">{@html I.pin}<i>Pinned</i></span>
    </span>
    {#if showMove}<span class="h delta" title="How far HALO would move the mod">Move</span>{/if}
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
        {@const pl = it.inactive ? undefined : placementOf(m.uid)}
        {@const grp = store.groupOf(m.uid)}
        {@const err = bySeverity(issues, "error", m)}
        {@const warn = bySeverity(issues, "warning", m)}
        {@const note = bySeverity(issues, "note", m)}
        {@const invalid = m.invalid && it.inactive ? m.invalid : ""}
        {@const ld = it.inactive ? undefined : store.loadOf(m.uid)}
        {@const tm = m.contents.load ? store.loadOf(m.uid)?.ms : undefined}
        {@const isNew = store.isNew(m.uid)}
        {@const arrived = whenItCame(m.uid)}
        <div
          class="row"
          class:off={it.inactive}
          class:sel={store.selected.includes(m.uid)}
          class:moved={pane !== null && delta != null}
          class:fresh={isNew}
          class:drop-before={store.drop?.pane === pane && store.drop.uid === m.uid && !store.drop.after}
          class:drop-after={store.drop?.pane === pane && store.drop.uid === m.uid && store.drop.after}
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
          <span class="idx num"><span class="grip">{@html I.grip}</span>{it.inactive ? "" : (indexOf.get(m.uid) ?? 0) + 1}</span>
          <span class="name"><b>{m.name ?? m.uid}{#if isNew}<i class="newtag">New</i>{/if}</b><span>{arrived ?? m.invalid ?? (m.authors ?? []).join(", ")}</span></span>
          {#if showPkg}<span class="pkg">{m.packageId}</span>{/if}
          {#if nameW != null}<span class="fill"></span>{/if}
          {#if has.has("time")}<span class="tm num">{#if tm != null}<span title="About {secs(tm)} of the game's loading time, of an estimated {store.loadTotalSeconds >= 60 ? `${(store.loadTotalSeconds / 60).toFixed(1)} min` : `${store.loadTotalSeconds.toFixed(0)} s`} for the whole list.&#10;{explainLoad(m).join('\n')}&#10;&#10;Estimated from the folder; a ranking, not a stopwatch.">{secs(tm)}</span>{:else if m.contents.load == null}<span class="unread" title="Not read yet: the folder is still being inspected">…</span>{/if}</span>{/if}
          {#if has.has("cost")}
            <span class="wt">{#if w && w.share != null}<span class="band {w.band}" title="Performance cost: {w.share.toFixed(2)} % of frame time, {BAND_LABEL[w.band].toLowerCase()}. {w.measured ?? '?'} runs measured, from {w.origin === 'local' ? 'your runs' : 'circinus.sh'}">{w.share.toFixed(1)} %</span>{:else if w}<span class="band {w.band}" title="Performance cost: {BAND_LABEL[w.band].toLowerCase()}">{w.band === "negligible" ? "<0.1 %" : "n/a"}</span>{/if}</span>
          {/if}
          {#if has.has("load")}<span class="load">{#if ld && m.contents.load}<span class="band {ld.band}" title="Expected share of loading time: {ld.share >= 0.0005 ? (ld.share * 100).toFixed(ld.share < 0.01 ? 2 : 1) : 'under 0.05'} % ({LOAD_BAND_LABEL[ld.band].toLowerCase()}), about {ld.ms >= 1000 ? `${(ld.ms / 1000).toFixed(1)} s` : `${ld.ms} ms`} of an estimated {store.loadTotalSeconds >= 60 ? `${(store.loadTotalSeconds / 60).toFixed(1)} min` : `${store.loadTotalSeconds.toFixed(0)} s`} for the list.&#10;{explainLoad(m).join('\n')}&#10;&#10;Estimated from the folder; a ranking, not a stopwatch.">{ld.share >= 0.001 ? `${(ld.share * 100).toFixed(ld.share < 0.01 ? 2 : 1)} %` : "<0.1 %"}</span>{:else if !it.inactive && m.contents.load == null}<span class="band unknown" title="Not read yet: the folder is still being inspected">…</span>{/if}</span>{/if}
          {#if has.has("versions")}<span class="vers">{#each versions as v}<span class:off={!(m.supportedVersions ?? []).includes(v)}>{v}</span>{/each}</span>{/if}
          {#if has.has("phase")}<span class="phz">{#if pl}{@const ph = store.phaseInfo(pl.phase)}<em class="tag" title="{ph.name} · {pl.reason}">{ph.name}</em>{/if}</span>{/if}
          {#if has.has("group")}<span class="g">{#if grp}<em class="tag" title="{grp.name}{grp.auto && !store.snap?.user.modGroups[m.uid] ? ' (by the group’s own rule)' : ''}">{grp.name}</em>{/if}</span>{/if}
          <span class="badges">
            <span class="b">{#if chg}<span class="flag chg" title="Changed since you last opened Circinus: {describeChange(chg)}">{@html chg.kind === "added" ? I.plus : I.change}</span>{:else if isNew}<span class="newdot" title={arrived ? `New: first seen ${arrived}` : "New"}></span>{/if}</span>
            <span class="b">{#if upd}<span class="flag" title="A newer version is on the Workshop, updated {new Date(upd.remoteUpdated * 1000).toLocaleDateString()}">{@html I.up}</span>{/if}</span>
            <span class="b">{#if err.n || invalid}<span class="flag" title={[invalid, err.text].filter(Boolean).join("\n")}>{@html I.error}{#if err.n + (invalid ? 1 : 0) > 1}<em class="num">{err.n + (invalid ? 1 : 0)}</em>{/if}</span>{/if}</span>
            <span class="b">{#if warn.n}<span class="flag" title={warn.text}>{@html I.warn}{#if warn.n > 1}<em class="num">{warn.n}</em>{/if}</span>{/if}</span>
            <span class="b">{#if note.n}<span class="flag" title={note.text}>{@html I.note}{#if note.n > 1}<em class="num">{note.n}</em>{/if}</span>{/if}</span>
            <span class="b">{#if store.pinned.has(m.uid)}<span class="flag" title="Pinned: keeps this position when sorting">{@html I.pin}</span>{/if}</span>
          </span>
          {#if showMove}<span class="delta num" class:down={delta && delta > 0} class:up={delta && delta < 0}>{#if delta}{delta > 0 ? "+" : ""}{delta}{/if}</span>{/if}
        </div>
      {/if}
    {/each}
    {#if store.drop?.pane === pane && store.drop.end}<div class="drop-line" style="transform: translateY({total}px)"></div>{/if}
  </div>
  {#if dragging}
    <div class="ghost" style="left: {ghost.x + 14}px; top: {ghost.y + 10}px">{dragCount === 1 ? store.byUid.get(store.drag?.uids[0] ?? "")?.name ?? "1 mod" : `${dragCount} mods`}</div>
  {/if}
  {#if !items.length}
    <div class="empty">{emptyText}</div>
  {/if}
</div>

<style>
  .list { flex: 1; min-height: 0; overflow: hidden auto; padding: 0 6px 10px; position: relative; }
  .list.dragging { cursor: grabbing; }
  .list.dragging .row { cursor: grabbing; }
  /* Dropping into the inactive pane switches mods off; there is no position to choose, so the
     pane itself is the target rather than a line between two rows. */
  .list.take { box-shadow: var(--shadow-card), inset 0 0 0 2px var(--amber); }
  .ghost { position: fixed; z-index: 30; pointer-events: none; background: var(--surface-4); color: var(--text); font-size: 12.5px; font-weight: 600; padding: 6px 10px; border-radius: 8px; box-shadow: var(--shadow-float); max-width: 260px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .spacer { position: relative; }
  /* One grid for the column header and every row, so the columns line up; the template is
     built in the script (dragged widths, optional columns) and handed down as --cols. */
  .hdr, .row { display: grid; grid-template-columns: var(--cols); align-items: center; gap: 6px; padding: 0 8px 0 4px; }
  .hdr { position: sticky; top: 0; z-index: 4; height: 34px; background: var(--surface); border-bottom: 1px solid var(--surface-3); margin: 0 -6px; padding-left: 10px; padding-right: 14px; }
  .hdr .h { font-size: 10.5px; font-weight: 700; letter-spacing: 0.04em; text-transform: uppercase; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; position: relative; }
  .hdr .idx, .hdr .vers, .hdr .delta, .hdr .wt, .hdr .load, .hdr .tm { text-align: right; }
  .hdr .phz, .hdr .g { text-align: left; }
  .hdr .h.name, .hdr .h.pkg { overflow: visible; }
  .hdr .grab { position: absolute; top: -8px; bottom: -8px; right: -6px; width: 11px; cursor: col-resize; touch-action: none; z-index: 1; }
  .hdr .grab::after { content: ""; position: absolute; top: 8px; bottom: 8px; left: 5px; width: 1px; background: var(--surface-4); }
  .hdr .grab:hover::after { background: var(--amber); width: 2px; left: 4px; }
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
  /* Side by side, the mods HALO would move are the ones worth finding in the other pane. */
  .row.moved { box-shadow: inset 2px 0 0 var(--amber); }
  .row.drop-before::before, .row.drop-after::after { content: ""; position: absolute; left: 8px; right: 8px; height: 2px; background: var(--amber); border-radius: 1px; }
  .row.drop-before::before { top: -1px; }
  .row.drop-after::after { bottom: -1px; }
  .drop-line { position: absolute; left: 8px; right: 8px; height: 2px; background: var(--amber); border-radius: 1px; }
  .idx { font-family: var(--mono); font-size: 11.5px; color: var(--text-3); text-align: right; position: relative; }
  .grip { position: absolute; left: -4px; top: 50%; transform: translateY(-50%); color: var(--text-4); opacity: 0; display: grid; place-items: center; cursor: grab; touch-action: none; }
  .row:hover .grip, .row.sel .grip { opacity: 1; }
  .grip :global(svg) { width: 11px; height: 11px; }
  .name { min-width: 0; display: flex; flex-direction: column; justify-content: center; line-height: 1.2; }
  .name b { font-weight: 600; font-size: 13.5px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .name span { font-size: 11.5px; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .pkg { font-family: var(--mono); font-size: 11px; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .row .wt, .row .load { display: flex; justify-content: flex-end; }
  .row .load .band.unknown { color: var(--text-4); }
  .vers { justify-content: flex-end; }
  /* Phase and Group are words, not decoration: a coloured dot on every one of two thousand
     rows is noise, and the section headings already carry the colour. Both columns are off
     unless the user asks for them (Show → Columns). */
  .row .phz, .row .g { min-width: 0; display: flex; align-items: center; }
  .tag { font-style: normal; font-size: 11.5px; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .row:hover .tag, .row.sel .tag { color: var(--text-2); }
  /* Six fixed slots, one per kind of badge, so nothing ever draws over anything else — in the
     header as in the rows, from the same grid, so the labels sit over their own icons. */
  .badges { display: grid; grid-template-columns: repeat(6, 1fr); gap: 0; align-items: center; min-width: 0; }
  .hdr .b { font-size: 8.5px; }
  .hdr .b i { display: block; max-width: 100%; overflow: hidden; text-overflow: clip; }
  /* A pane is half as wide: the badge words would not fit, and the icons say the same thing. */
  .list.pane .hdr .b i { display: none; }
  .list.pane .ph .note { display: none; }
  .badges .b { display: grid; place-items: center; height: 24px; }
  .flag { display: inline-flex; align-items: center; gap: 1px; height: 20px; padding: 0 2px; border-radius: 6px; }
  .flag :global(svg) { width: 15px; height: 15px; flex: none; }
  .flag em { font-style: normal; font-size: 10.5px; font-weight: 700; color: var(--text-2); }
  .delta { font: 700 11.5px var(--mono); text-align: right; color: var(--text-3); }
  .delta.down { color: var(--amber); }
  .delta.up { color: var(--blue); }
  .empty { padding: 40px; text-align: center; color: var(--text-3); }
  /* A mod that arrived since you last looked. Green rather than amber, and a right-hand edge
     rather than a left one, because amber on the left already means "HALO would move this" and
     in a split pane both can be true of the same row. */
  .row.fresh { background: var(--green-soft); box-shadow: inset -2px 0 0 var(--green); }
  .row.fresh:hover { background: rgba(63, 196, 106, 0.2); }
  .row.fresh.sel { background: rgba(63, 196, 106, 0.26); }
  .newtag { font-style: normal; margin-left: 7px; font-size: 9px; font-weight: 800; letter-spacing: 0.08em; color: var(--green); background: var(--green-soft); padding: 1px 5px; border-radius: var(--r-pill); vertical-align: 1px; }
  /* In a pane the name is barely wide enough for the name; the edge and the dot say it instead. */
  .list.pane .newtag { display: none; }
  .newdot { width: 7px; height: 7px; border-radius: 50%; background: var(--green); }
  /* The heading of the column the list is sorted by, and which way. */
  /* The heading is still the grid cell; the button inside it is the thing you press, so the
     columns keep lining up and the resize handle keeps its corner. */
  .sortbtn { font: inherit; color: inherit; letter-spacing: inherit; text-transform: inherit; cursor: pointer; max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sortbtn:hover { color: var(--text-2); }
  .hdr .h.by, .hdr .h.by .sortbtn { color: var(--amber); }
  .dir { font-style: normal; margin-left: 3px; }
  /* Seconds, in the same monospace as the other figures so the decimal points line up down the
     column. No colour band: Load sits beside it saying the same thing as a share, and colouring
     both would be one fact drawn twice. */
  .row .tm { text-align: right; font-family: var(--mono); font-size: 11.5px; color: var(--text-2); }
  .row .tm .unread { color: var(--text-4); }
</style>
