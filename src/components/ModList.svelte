<script lang="ts">
  import { tick } from "svelte";
  import { store, type Pane, type Section, type SortKey } from "$lib/store.svelte";
  import { I } from "$lib/icons";
  import { describe, explainLoad } from "$lib/describe";
  import { BAND_LABEL, LOAD_BAND_LABEL, describeChange, severityOf, type Issue, type ModChange, type ModInfo, type UpdateInfo } from "$lib/types";
  import { NOTICE_ICON, noticesFor } from "$lib/notices";
  import { layouts, type Surface } from "$lib/layout.svelte";

  // One list, shown three ways. Without `pane` this is the whole list and the tabs decide what is
  // in it; with one it is half of the side-by-side library and shows only its own half.
  let { pane = null }: { pane?: Pane | null } = $props();

  // ---- virtualization: only the rows in view exist in the DOM ----
  const ROW = 40;
  const HEADER = 44;
  /** A group header inside a band is a line of small caps, not a section title. */
  const SUBHEADER = 26;
  const heightOf = (it: Item) => (it.kind !== "header" ? ROW : it.depth === 1 ? SUBHEADER : HEADER);
  /** The column header at the top of the list; the rows scroll under it. */
  const TOP = 34;
  const OVERSCAN = 8;
  /** `depth` 0 is a band (a HALO phase, or one of the user's own); 1 is a group inside it.
   *  `groupId` is set on any header that stands for a group, so the sidebar can scroll to it. */
  type Item =
    | { kind: "header"; key: string; name: string; color: string; note: string; count: number; depth: 0 | 1; groupId?: string; mine?: boolean }
    | { kind: "row"; key: string; mod: ModInfo; inactive: boolean; depth: 0 | 1 };

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
  /** The optional columns, in the order they appear.
   *
   *  Each one is now a single measurement with a single source, and the name says which. That
   *  was not true of the three they replace. **Time** mixed a Loading Progress reading with a
   *  folder model and told everybody, in its tooltip, that it was the model -- so a real
   *  stopwatch figure was presented as a guess, and the mod that took the measurement went
   *  uncredited. **Load** was not a second measurement at all: it was Time divided by the sum
   *  of Time, which is why sorting by it and sorting by Time ran the same comparison. And it
   *  sat beside **Cost**, in the same unit and the same typeface, so the modelled percentage
   *  and the measured one read as a pair when only one of them had ever been observed.
   *
   *  What is here instead:
   *    Start-up  seconds, measured by Loading Progress. No model, ever -- a mod it has not
   *              timed reads as a dash, because a guess in this column is indistinguishable
   *              from a reading and people quote it as one.
   *    Typical   share of frame time, pooled from circinus.sh. Everybody else's machines.
   *    Yours     share of frame time, from the Circinus profiler runs on *this* machine.
   *  Typical and Yours are the same quantity measured in two places, which is the whole reason
   *  to draw them next to each other: the gap between them is about this install.
   *
   *  Typical and Yours follow `showWeight` rather than the column picker, as Cost did: both
   *  need figures that only exist once weights are loaded. */
  const on = $derived(new Set(store.listColumns));
  const optional = $derived([
    { key: "startup", w: 66, show: on.has("startup") },
    { key: "typical", w: 68, show: store.showWeight },
    { key: "yours", w: 66, show: store.showWeight },
    { key: "loadmedian", w: 72, show: on.has("loadmedian") },
    { key: "versions", w: 64, show: on.has("versions") },
    { key: "phase", w: 106, show: on.has("phase") },
    { key: "group", w: 106, show: on.has("group") }
  ]);
  const wanted = $derived(optional.filter((c) => c.show));
  /** One notice mark, in a fixed gutter at the head of the row.
   *
   *  This used to be six columns and 232px of every row, for ever, and the reader still had to
   *  scan all six to answer one question. The 190px it gives back goes to the mod name, which
   *  is what the window is actually for.
   *
   *  It sits before the number rather than after the last column because a severity mark is
   *  the first thing anybody scans a list for, and on the right it was the one thing whose
   *  position moved: the middle of the row is elastic, so the marks landed in a different
   *  place at every width, and in a pane the strip was too narrow for its own heading, which
   *  was simply hidden. A fixed gutter is in the same place in both panes at every width, and
   *  the row reads mark, then position, then name. */
  const BADGES = 24;
  const minName = $derived(pane === null ? NAME_MIN : PANE_NAME_MIN);
  /** What a row costs before any optional column: the gutter, the number, the move, the gaps. */
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
    const cols = [`${BADGES}px`, "34px", nameW != null ? `${nameW}px` : `minmax(${minName}px, 1fr)`];
    if (showPkg) cols.push(`${pkgW}px`);
    if (nameW != null) cols.push("minmax(0, 1fr)");
    for (const c of shown) cols.push(`${c.w}px`);
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
  /** Which stored layout this list follows. The single list and the two panes are three
   *  surfaces because they answer different questions, and somebody can want different answers
   *  at once -- A to Z on the left to find a mod, phases on the right to check the order. */
  const surface = $derived<Surface>(pane === null ? "order" : pane === "active" ? "active" : "inactive");
  const byPhase = $derived(layouts.byPhase(surface));
  /* The inactive pane's classification is not in the snapshot -- hundreds of placements, each
     with a prose reason, for a pane most people never switch. Asked for when it is wanted. */
  $effect(() => {
    if (surface === "inactive" && byPhase) store.loadInactivePlacements();
  });

  /** Push a band and the groups inside it. Rows under a group header are indented one step. */
  function pushSection(out: Item[], sec: Section) {
    if (sec.group) out.push({ kind: "header", key: `h:${sec.phase.id}:${sec.group.id}`, name: sec.group.name, color: sec.group.color, note: `Your own band, after ${sec.phase.name.toLowerCase()}`, count: sec.mods.length, depth: 0, groupId: sec.group.id, mine: true });
    else out.push({ kind: "header", key: `h:${sec.phase.id}`, name: sec.phase.name, color: sec.phase.color, note: sec.phase.note, count: sec.mods.length, depth: 0 });
    // `subs` is empty when the second level would say nothing: a band that is one group would
    // print its own name twice, and a phase with one group and no ungrouped mods is the same.
    if (!sec.subs.length) {
      for (const m of sec.mods) out.push({ kind: "row", key: m.uid, mod: m, inactive: false, depth: 0 });
      return;
    }
    for (const sub of sec.subs) {
      out.push({
        kind: "header",
        key: `h:${sec.phase.id}:${sec.group?.id ?? ""}:g:${sub.group?.id ?? "none"}`,
        name: sub.group?.name ?? "Ungrouped",
        color: sub.group?.color ?? "slate",
        note: "",
        count: sub.mods.length,
        depth: 1,
        groupId: sub.group?.id
      });
      for (const m of sub.mods) out.push({ kind: "row", key: m.uid, mod: m, inactive: false, depth: 1 });
    }
  }

  const items = $derived.by((): Item[] => {
    const out: Item[] = [];
    if (pane === "inactive") {
      // Its own pane says what it is in the strip above it, so the list needs no band heading
      // unless the pane has been asked for phases.
      if (byPhase) for (const sec of store.inactiveSections) pushSection(out, sec);
      else for (const m of store.visibleInactive) out.push({ kind: "row", key: m.uid, mod: m, inactive: true, depth: 0 });
      // An inactive row is inactive whatever band it is drawn in.
      for (const it of out) if (it.kind === "row") it.inactive = true;
      return out;
    }
    // The New tab is the new mods and nothing else: no phase sections, because what you want
    // from it is when each one arrived, not where it will load.
    if (pane === null && store.tab === "new") {
      for (const m of store.visibleNew) out.push({ kind: "row", key: m.uid, mod: m, inactive: !store.activeSet.has(m.uid), depth: 0 });
      return out;
    }
    if (pane !== null || store.tab !== "inactive") {
      if (byPhase) {
        for (const sec of sections) pushSection(out, sec);
      } else {
        // The plain load order, exactly as ModsConfig.xml has it.
        if (pane === null && store.tab === "all") out.push({ kind: "header", key: "h:active", name: "Active", color: "blue", note: "In load order, as ModsConfig.xml has it", count: ordered.length, depth: 0 });
        for (const m of ordered) out.push({ kind: "row", key: m.uid, mod: m, inactive: false, depth: 0 });
      }
    }
    if (pane === null && store.tab !== "active") {
      const vi = store.visibleInactive;
      out.push({ kind: "header", key: "h:inactive", name: "Inactive", color: "", note: "Installed, not in ModsConfig.xml", count: vi.length, depth: 0 });
      for (const m of vi) out.push({ kind: "row", key: m.uid, mod: m, inactive: true, depth: 0 });
    }
    return out;
  });
  const offsets = $derived.by(() => {
    const o = new Array<number>(items.length + 1);
    let y = 0;
    for (let i = 0; i < items.length; i++) {
      o[i] = y;
      y += heightOf(items[i]);
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
  /** The one version worth printing: the newest the mod actually says it supports.
   *
   *  The column used to draw two fixed chips -- the installed game version and the one before
   *  it -- lit or greyed by whether the mod declared them. So it never showed what a mod
   *  actually supports, only whether it supports those two: a mod that stopped at 1.4 and a
   *  mod that declares nothing at all drew identically, two grey chips, on a 1.6 install. That
   *  also put the column out of step with its own sort, which reads the real declared maximum,
   *  so rows with identical chips reordered when you sorted by them.
   *
   *  Newest-declared answers the question the column is for -- is this mod keeping up -- and
   *  says something different in each of the three cases rather than the same thing in two. */
  const gameVersion = $derived(store.snap?.gameVersion.majorMinor ?? "1.6");
  function versionOf(m: ModInfo): { label: string; state: "on" | "behind" | "ahead"; title: string } | null {
    const declared = m.supportedVersions ?? [];
    if (!declared.length) return null;
    const newest = declared.reduce((a, b) => (store.versionRank(b) > store.versionRank(a) ? b : a));
    const d = store.versionRank(newest) - store.versionRank(gameVersion);
    const all = declared.length > 1 ? `\nSays it supports ${declared.join(", ")}.` : "";
    if (d === 0) return { label: newest, state: "on", title: `Supports ${gameVersion}, the version installed.${all}` };
    if (d < 0)
      return {
        label: newest,
        state: "behind",
        title: `The newest version this mod claims is ${newest}; the game installed is ${gameVersion}. It may still work, and it may not.${all}`
      };
    return { label: newest, state: "ahead", title: `Built for ${newest}, ahead of the ${gameVersion} installed here.${all}` };
  }

  /** A share of frame time, at the one decimal the column has room for.
   *
   *  Anything that would round to "0.0 %" reads as "this mod is free", which is a claim the
   *  figure is too coarse to make -- and on a real install most measured mods land there, so
   *  the column filled with zeros and looked broken even once the figures behind it were
   *  right. "<0.1 %" says the same thing without asserting a zero, and the exact figure is in
   *  the tooltip for anyone who wants it. */
  const pct = (share: number) => (share < 0.05 ? "<0.1 %" : `${share.toFixed(1)} %`);

  /** Why a Start-up cell is empty. Two sentences, because "no data" is not actionable and the
   *  mod's tracking settings are off out of the box -- absent is the normal case, not a fault. */
  const MEASURE_MISSING =
    "Not measured. The Loading Progress mod times each mod as the game starts; Circinus reads what it writes and never estimates this number. Enable it, start the game once, and this fills in.";
  const PROFILER_MISSING = "You have not profiled this mod. The Circinus profiler mod measures frame time on this machine; Typical is what everybody else measured.";

  function yoursTitle(w: { localShare: number | null; localBand: string | null; localRuns: number | null; share: number | null }): string {
    const mine = w.localShare ?? 0;
    const runs = w.localRuns ?? 0;
    const head = `${mine.toFixed(3)} % of frame time on this machine, over ${runs} of your own run${runs === 1 ? "" : "s"}.`;
    if (w.share == null) return `${head}\nNobody has pooled a figure for this mod, so there is nothing to compare it with.`;
    // The comparison is the reason both columns exist, so it is stated rather than left to be
    // worked out from two numbers in different columns.
    const ratio = w.share > 0 ? mine / w.share : null;
    if (ratio == null || (ratio > 0.75 && ratio < 1.33)) return `${head}\nAbout what it costs everybody else (${w.share.toFixed(3)} %).`;
    const word = ratio >= 1 ? `${ratio.toFixed(1)}× more` : `${(1 / ratio).toFixed(1)}× less`;
    return `${head}\n${word} than it costs everybody else (${w.share.toFixed(3)} %). That gap is about this install, not about the mod.`;
  }

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
  /** Everything one row has to say, most serious first.
   *
   *  The ranking and the dismissal keys live in `$lib/notices` rather than here, so they can be
   *  tested without rendering a row -- which matters because "which of these six things is the
   *  one to show" is the entire behaviour of the column. */
  function noticesOf(m: ModInfo, issues: Issue[], update: UpdateInfo | undefined, change: ModChange | undefined, inactive: boolean) {
    return noticesFor({
      mod: m,
      issues,
      describe: (i) => describe(i, store.byUid, m.uid),
      severityOf,
      update,
      change,
      describeChange,
      isNew: store.isNew(m.uid),
      arrived: whenItCame(m.uid),
      pinned: store.pinned.has(m.uid),
      inactive,
      dismissed: store.dismissedNotices
    });
  }

  /** Open the row's notices where the pointer is, clamped by the popover itself. */
  function openNotices(e: MouseEvent, uid: string) {
    store.noticePopover = { uid, x: e.clientX, y: e.clientY };
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
  // Anything that has to act on "the list on screen" without being this component -- select-all,
  // the search box handing over on Down -- reads it through here.
  $effect(() => {
    store.visibleListFn = visibleList;
    return () => {
      if (store.visibleListFn === visibleList) store.visibleListFn = null;
    };
  });

  /** The row the keyboard is on.
   *
   *  Rows are `tabindex="0"` only while they are the cursor -- a roving tabindex. Every row being
   *  tabbable meant Tab walked a thousand of them one at a time; worse, real DOM focus on a
   *  virtualised row is destroyed the moment it scrolls out, and with it the only record of where
   *  the keyboard was. Keeping the uid here outlives the element, and the effect below puts focus
   *  back on the row when it returns. */
  let cursor = $state<string | null>(null);
  const cursorUid = $derived(cursor && visibleList().includes(cursor) ? cursor : (store.selected.find((u) => visibleList().includes(u)) ?? visibleList()[0] ?? null));
  $effect(() => {
    // Only when this list already has the keyboard: stealing focus because a row came back on
    // screen would take it from whatever the user is actually typing in.
    if (!cursorUid || !scroller?.contains(document.activeElement)) return;
    const el = scroller.querySelector(`[data-uid="${CSS.escape(cursorUid)}"]`) as HTMLElement | null;
    if (el && el !== document.activeElement) el.focus();
  });
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
  /** Walking the list.
   *
   *  Two things were wrong with the version this replaces. It handled ArrowUp and ArrowDown
   *  without stopping them, so Alt+ArrowDown moved the selection down a row *and* moved the mods
   *  down a place -- one keystroke, two edits, one of them to the file. And it lived only on a
   *  focused row: the list is virtualised, so scrolling with the wheel destroys the row that had
   *  focus, and the next arrow press went to the window handler instead. Which is why arrow keys
   *  felt broken after any scrolling. `cursor` below is the fix for the second; bailing on Alt is
   *  the fix for the first.
   *
   *  Ctrl moves the cursor without taking the selection with it, which is how a list lets you
   *  reach a distant row and add it with Ctrl+Space rather than dragging a range over everything
   *  in between. */
  function key(e: KeyboardEvent) {
    // Alt+Arrow is "move the mods", and that belongs to one handler, not two.
    if (e.altKey) return;
    const list = visibleList();
    // From the cursor, not from an event target: focus lands on the container after a wheel
    // scroll destroys the focused row, and a list that only answers when a row is focused is a
    // list whose arrow keys stop working the moment you scroll.
    const from = list.indexOf(cursorUid ?? "");
    const page = Math.max(1, Math.floor(viewport / ROW) - 1);
    let to: number | null = null;
    switch (e.key) {
      case "Enter":
      case " ": {
        const m = cursorUid ? store.byUid.get(cursorUid) : null;
        if (!m) return;
        e.preventDefault();
        e.stopPropagation();
        toggle(m);
        return;
      }
      case "ArrowDown": to = from + 1; break;
      case "ArrowUp": to = from - 1; break;
      case "PageDown": to = from + page; break;
      case "PageUp": to = from - page; break;
      case "Home": to = 0; break;
      case "End": to = list.length - 1; break;
      default: return;
    }
    e.preventDefault();
    e.stopPropagation();
    // Home and End should land somewhere even from an unknown row; the arrows should not wrap.
    to = Math.max(0, Math.min(list.length - 1, to));
    if (to === from || !list.length) return;
    const uid = list[to];
    cursor = uid;
    if (!(e.ctrlKey || e.metaKey)) store.select(uid, { range: e.shiftKey, list });
    scrollToUid(uid, true);
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
    // A click is also where the keyboard now is: arrowing after clicking should carry on from
    // the row that was clicked, not from wherever the cursor happened to be left.
    cursor = m.uid;
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
  onkeydown={key}
  role="listbox"
  aria-label={pane === "inactive" ? "Inactive mods" : "Load order"}
  aria-multiselectable="true"
  aria-activedescendant={cursorUid ?? undefined}
  tabindex="-1"
  style="--cols: {template}"
>
  <div class="hdr" role="presentation" onpointermove={colMove} onpointerup={colUp} onpointercancel={colUp}>
    <!-- The notice gutter. An icon rather than the word: one heading over one column used to
         buy itself an exemption from the header's own type rules (9.5px, weight 600, mixed
         case, no tracking, against everything else's 10.5px/700/uppercase), which is what made
         it read as belonging to a different table. A glyph has no type to disagree with, and
         what the column holds is already spelled out at length in the tooltip. -->
    <span
      class="h b"
      title="Notices: errors first, then warnings, an available update, HALO notes, what changed since you last opened Circinus, and whether the mod is pinned. The row shows the most serious one; click it to read them all and put any of them down."
      >{@html I.bell}<span class="sr">Notices</span></span
    >
    <!-- Nothing inactive has a place in the order, so its pane numbers nothing and says so. -->
    <span class="h idx">{pane === "inactive" ? "" : "#"}</span>
    <span class="h name" class:by={store.sortKey === "name"}>{#if canSort}<button class="sortbtn" onclick={() => store.sortBy("name")} title="Sort by name">Mod{#if store.sortKey === "name"}<i class="dir">{arrowFor("name")}</i>{/if}</button>{:else}Mod{/if}<span class="grab" role="separator" aria-orientation="vertical" title="Drag to change the width; double-click for the default" onpointerdown={(e) => colDown(e, "name")} ondblclick={() => colReset("name")}></span></span>
    {#if showPkg}<span class="h pkg" class:by={store.sortKey === "pkg"}>{#if canSort}<button class="sortbtn" onclick={() => store.sortBy("pkg")} title="Sort by package id">Package id{#if store.sortKey === "pkg"}<i class="dir">{arrowFor("pkg")}</i>{/if}</button>{:else}Package id{/if}<span class="grab" role="separator" aria-orientation="vertical" title="Drag to change the width; double-click for the default" onpointerdown={(e) => colDown(e, "pkg")} ondblclick={() => colReset("pkg")}></span></span>{/if}
    {#if nameW != null}<span class="fill"></span>{/if}
    {#if has.has("startup")}<span class="h tm" class:by={store.sortKey === "startup"} title="Seconds this mod added to the last start-up, measured by the Loading Progress mod. Circinus times nothing itself and never estimates this: a mod Loading Progress has not timed reads as a dash rather than as a guess.">{#if canSort}<button class="sortbtn" onclick={() => store.sortBy("startup")}>Start-up{#if store.sortKey === "startup"}<i class="dir">{arrowFor("startup")}</i>{/if}</button>{:else}Start-up{/if}</span>{/if}
    {#if has.has("typical")}<span class="h wt" class:by={store.sortKey === "typical"} title="What this mod typically costs everybody else: the median share of frame time across the runs pooled by circinus.sh. Compare it with Yours.">{#if canSort}<button class="sortbtn" onclick={() => store.sortBy("typical")}>Typical{#if store.sortKey === "typical"}<i class="dir">{arrowFor("typical")}</i>{/if}</button>{:else}Typical{/if}</span>{/if}
    {#if has.has("yours")}<span class="h load" class:by={store.sortKey === "yours"} title="What this mod costs on this machine: the median share of frame time across the runs the Circinus profiler mod has written here. The same measurement as Typical, taken on one machine instead of the pool, so the gap between the two is about this install.">{#if canSort}<button class="sortbtn" onclick={() => store.sortBy("yours")}>Yours{#if store.sortKey === "yours"}<i class="dir">{arrowFor("yours")}</i>{/if}</button>{:else}Yours{/if}</span>{/if}
    {#if has.has("loadmedian")}<span class="h lmed" title="What this mod typically adds to a start-up on everyone else's machine: the median of the start-ups shared with circinus.sh, measured by the Loading Progress mod. Wall-clock milliseconds spent once, not a share of every frame, so it is never added to Typical or Yours. Blank means nobody has timed it yet.">Median</span>{/if}
    {#if has.has("versions")}<span class="h vers" class:by={store.sortKey === "versions"} title="Game versions the mod says it supports">{#if canSort}<button class="sortbtn" onclick={() => store.sortBy("versions")}>Version{#if store.sortKey === "versions"}<i class="dir">{arrowFor("versions")}</i>{/if}</button>{:else}Version{/if}</span>{/if}
    {#if has.has("phase")}<span class="h phz" class:by={store.sortKey === "phase"} title="Where HALO files the mod: the game, a library, content, a patch, a texture pack, a late loader, a performance mod">{#if canSort}<button class="sortbtn" onclick={() => store.sortBy("phase")}>Phase{#if store.sortKey === "phase"}<i class="dir">{arrowFor("phase")}</i>{/if}</button>{:else}Phase{/if}</span>{/if}
    {#if has.has("group")}<span class="h g" class:by={store.sortKey === "group"} title="The group the mod is in">{#if canSort}<button class="sortbtn" onclick={() => store.sortBy("group")}>Group{#if store.sortKey === "group"}<i class="dir">{arrowFor("group")}</i>{/if}</button>{:else}Group{/if}</span>{/if}
    {#if showMove}<span class="h delta" title="How far HALO would move the mod">Move</span>{/if}
  </div>
  {#if floating}
    <div class="ph floating"><span class="dot c-{floating.color}"></span><span class="n">{floating.name}</span><span class="c num">{floating.count}</span><span class="note">{floating.note}</span></div>
  {/if}
  <div class="spacer" style="height: {total}px">
    {#each visible as { it, y } (it.key)}
      {#if it.kind === "header"}
        <div class="ph" class:sub={it.depth === 1} class:mine={it.mine} class:lit={it.groupId != null && it.groupId === store.flashGroup} style="transform: translateY({y}px)" data-group={it.groupId ?? ""}><span class="dot c-{it.color}"></span><span class="n">{it.name}</span><span class="c num">{it.count}</span>{#if it.mine}<em class="yours">yours</em>{/if}{#if it.note}<span class="note">{it.note}</span>{/if}</div>
      {:else}
        {@const m = it.mod}
        {@const issues = store.issuesByUid.get(m.uid) ?? []}
        {@const delta = store.moveOf.get(m.uid)}
        {@const w = store.weightOf(m)}
        {@const upd = store.updateByUid.get(m.uid)}
        {@const chg = store.changeByUid.get(m.uid)}
        {@const pl = it.inactive ? undefined : placementOf(m.uid)}
        {@const grp = store.groupOf(m.uid)}
        {@const nt = noticesOf(m, issues, upd, chg, it.inactive)}
        <!-- Measured or nothing. `expectedMsOf` would answer for every mod, because it falls
             back to the folder model, and that fallback is exactly what this column must not
             have: a modelled figure and a stopwatch reading are indistinguishable once they
             are both set in the same mono digits. -->
        {@const tm = store.measuredMsOf(m.uid)}
        {@const isNew = store.isNew(m.uid)}
        {@const arrived = whenItCame(m.uid)}
        <div
          class="row"
          class:ind={it.depth === 1}
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
          tabindex={m.uid === cursorUid ? 0 : -1}
          onclick={(e) => rowClick(e, m)}
          onkeydown={key}
          oncontextmenu={(e) => contextMenu(e, m)}
          ondblclick={() => toggle(m)}
          onpointerdown={(e) => pointerDown(e, m)}
        >
          <span class="badges">
            {#if nt.shown.length || nt.hidden.length}
              {@const top = nt.shown[0]}
              <button
                class="flag nt"
                class:err={top?.kind === "error"}
                class:quiet={!top}
                title={[...nt.shown.map((x) => x.text), nt.hidden.length ? `${nt.hidden.length} dismissed` : ""].filter(Boolean).join("\n")}
                aria-label="{nt.shown.length} notice{nt.shown.length === 1 ? '' : 's'} for {m.name}"
                onclick={(e) => { e.stopPropagation(); openNotices(e, m.uid); }}
              >{@html top ? I[NOTICE_ICON[top.kind]] : I.check}{#if nt.shown.length > 1}<em class="num">{nt.shown.length}</em>{/if}</button>
            {/if}
          </span>
          <span class="idx num"><span class="grip">{@html I.grip}</span>{it.inactive ? "" : (indexOf.get(m.uid) ?? 0) + 1}</span>
          <span class="name"><b>{m.name ?? m.uid}{#if isNew}<i class="newtag">New</i>{/if}</b><span>{arrived ?? m.invalid ?? (m.authors ?? []).join(", ")}</span></span>
          {#if showPkg}<span class="pkg">{m.packageId}</span>{/if}
          {#if nameW != null}<span class="fill"></span>{/if}
          {#if has.has("startup")}<span class="tm num">{#if tm != null}<span title="Added about {secs(tm)} to the last start-up, measured by the Loading Progress mod. Of {store.measuredTotalSeconds >= 60 ? `${(store.measuredTotalSeconds / 60).toFixed(1)} min` : `${store.measuredTotalSeconds.toFixed(0)} s`} it measured across this list.">{secs(tm)}</span>{:else}<span class="unread" title={MEASURE_MISSING}>&mdash;</span>{/if}</span>{/if}
          {#if has.has("typical")}
            <span class="wt">{#if w && w.share != null}<span class="band {w.band}" title="Typically {w.share.toFixed(3)} % of frame time, {BAND_LABEL[w.band].toLowerCase()}. Pooled by circinus.sh over {w.measured ?? '?'} measured run{w.measured === 1 ? '' : 's'} on other people's machines.">{pct(w.share)}</span>{:else}<span class="dash" title="Nobody has pooled a measurement of this mod yet. Not the same as costing nothing.">&mdash;</span>{/if}</span>
          {/if}
          {#if has.has("yours")}<span class="load">{#if w?.localShare != null}<span class="band {w.localBand ?? 'unknown'}" title={yoursTitle(w)}>{pct(w.localShare)}</span>{:else}<span class="dash" title={PROFILER_MISSING}>&mdash;</span>{/if}</span>{/if}
          {#if has.has("loadmedian")}<span class="lmed num">{#if w?.loadMsMedian != null}<span title="Typically adds about {w.loadMsMedian >= 1000 ? `${(w.loadMsMedian / 1000).toFixed(1)} s` : `${Math.round(w.loadMsMedian)} ms`} to a start-up, over {w.loadRuns ?? 0} shared start-up{w.loadRuns === 1 ? '' : 's'} from {w.loadInstalls ?? 0} install{w.loadInstalls === 1 ? '' : 's'}. Measured by Loading Progress, pooled by circinus.sh. Start-up only: never added to Typical or Yours.">{w.loadMsMedian >= 1000 ? `${(w.loadMsMedian / 1000).toFixed(1)} s` : `${Math.round(w.loadMsMedian)} ms`}</span>{:else}<span class="unread" title="Nobody has shared a start-up with this mod loaded yet. Not the same as costing nothing.">&mdash;</span>{/if}</span>{/if}
          {#if has.has("versions")}{@const vr = versionOf(m)}<span class="vers">{#if vr}<span class={vr.state} title={vr.title}>{vr.label}</span>{/if}</span>{/if}
          {#if has.has("phase")}<span class="phz">{#if pl}{@const ph = store.phaseInfo(pl.phase)}<em class="tag" title="{ph.name} · {pl.reason}">{ph.name}</em>{/if}</span>{/if}
          {#if has.has("group")}<span class="g">{#if grp}<em class="tag" title="{grp.name}{grp.auto && !store.snap?.user.modGroups[m.uid] ? ' (by the group’s own rule)' : ''}">{grp.name}</em>{/if}</span>{/if}
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
  .ghost { position: fixed; z-index: 30; pointer-events: none; background: var(--surface-4); color: var(--text); font-size: 12.5px; font-weight: 600; padding: 6px 10px; border-radius: 0; box-shadow: var(--shadow-float); max-width: 260px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
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
  /* The heading is the glyph and nothing else, so there is no type here to disagree with the
     rest of the row. It used to be the word "Notices" under its own rules -- 9.5px, weight
     600, mixed case, no tracking, against every other heading's 10.5px/700/uppercase -- an
     exemption six icon columns once needed and one column never did. Retiring the exemption
     was the fix; overriding it back to match would have left the next person the same trap. */
  .hdr .b { display: grid; place-items: center; color: var(--text-3); }
  .hdr .b :global(svg) { width: 13px; height: 13px; }
  .ph { position: absolute; left: 0; right: 0; height: 44px; display: flex; align-items: center; gap: 10px; padding: 14px 10px 6px; background: var(--surface); }
  /* A group inside a band: a line of small caps rather than a section title, so the eye reads
     the band first and the group second. Same ground as the rows, not the header's. */
  .ph.sub { height: 26px; padding: 8px 10px 2px 26px; background: transparent; }
  .ph.sub .n { font-size: 9.5px; letter-spacing: 0.07em; color: var(--text-4); }
  .ph.sub .c { font-size: 9.5px; color: var(--text-4); }
  .ph.sub .dot { width: 6px; height: 6px; }
  /* A band of the user's own, so it is never taken for one of HALO's eight. */
  .ph .yours { font: 700 9px var(--mono); font-style: normal; letter-spacing: 0.04em; text-transform: uppercase; color: var(--amber); background: var(--amber-soft); padding: 1px 5px; }
  .ph.mine .n { color: var(--text); }
  /* Jumped to from the sidebar. One flash, then it goes: a header that stayed lit would look
     like a selection, and nothing here is selected. */
  .ph.lit { background: var(--amber-soft); animation: phflash 1.4s ease-out 1 forwards; }
  @keyframes phflash { 0%, 55% { background: var(--amber-soft); } 100% { background: var(--surface); } }
  .ph.sub.lit { animation-name: phflashsub; }
  @keyframes phflashsub { 0%, 55% { background: var(--amber-soft); } 100% { background: transparent; } }
  /* Sticky overlay of the current section, under the column header; the negative bottom margin keeps it out of the flow so row offsets stay exact. */
  .ph.floating { position: sticky; top: 34px; z-index: 3; box-shadow: 0 6px 8px -6px rgba(0, 0, 0, 0.5); margin: 0 -6px -44px; padding-left: 16px; padding-right: 16px; }
  .ph .dot { width: 7px; height: 7px; }
  .ph .n { font-size: 11px; font-weight: 700; letter-spacing: 0.09em; text-transform: uppercase; color: var(--text-2); }
  .ph .c { font-size: 11px; color: var(--text-3); font-weight: 600; }
  .ph .note { margin-left: auto; font-size: 11.5px; color: var(--text-3); }
  .row { position: absolute; left: 0; right: 0; height: 40px; border-radius: 0; cursor: default; }
  /* One step in under a group header, so a row's indent says which heading it belongs to
     without the heading having to be on screen. */
  .row.ind { padding-left: 20px; }
  .row:hover { background: var(--surface-2); }
  .row.sel { background: var(--surface-3); }
  .row.off { opacity: 0.72; }
  .row.off.sel, .row.off:hover { opacity: 1; }
  /* Side by side, the mods HALO would move are the ones worth finding in the other pane. */
  .row.moved { box-shadow: inset 2px 0 0 var(--amber); }
  .row.drop-before::before, .row.drop-after::after { content: ""; position: absolute; left: 8px; right: 8px; height: 2px; background: var(--amber); border-radius: 0; }
  .row.drop-before::before { top: -1px; }
  .row.drop-after::after { bottom: -1px; }
  .drop-line { position: absolute; left: 8px; right: 8px; height: 2px; background: var(--amber); border-radius: 0; }
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
  .row .lmed { display: flex; justify-content: flex-end; font-size: 11.5px; color: var(--text-2); }
  .row .lmed .unread { color: var(--text-4); }
  .hdr .lmed { text-align: right; }
  /* Phase and Group are words, not decoration: a coloured dot on every one of two thousand
     rows is noise, and the section headings already carry the colour. Both columns are off
     unless the user asks for them (Show → Columns). */
  .row .phz, .row .g { min-width: 0; display: flex; align-items: center; }
  .tag { font-style: normal; font-size: 11.5px; color: var(--text-3); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; min-width: 0; }
  .row:hover .tag, .row.sel .tag { color: var(--text-2); }
  /* One slot. The header label sits over the same grid cell as the mark, so the word and the
     icons under it line up whatever the list is showing. */
  .badges { display: grid; grid-template-columns: 1fr; gap: 0; align-items: center; min-width: 0; }
  /* The mark is a button, so it needs a hit area and a hover of its own without becoming a
     second thing to look at on a row that is only saying "nothing to report". */
  .flag.nt { cursor: pointer; justify-self: center; }
  .flag.nt:hover { background: var(--surface-3); color: var(--text); }
  .flag.nt.err { color: var(--red); }
  /* A row with nothing left to say after a dismissal still has a way back to what it put down,
     so it keeps a mark -- at the weight of punctuation rather than of a warning. */
  .flag.nt.quiet { color: var(--text-4); opacity: 0.55; }
  .flag.nt.quiet:hover { opacity: 1; }
  .list.pane .ph .note { display: none; }
  /* The gutter is the same 24px in the single list and in either pane. That is the point of
     moving it here: it used to be the last column, so its position moved with the elastic
     middle of the row and was different at every width, and in a pane it lost its heading
     entirely for want of room. A fixed gutter needs neither concession. */
  .badges { justify-self: center; }
  .flag { display: inline-flex; align-items: center; gap: 1px; height: 20px; padding: 0 2px; border-radius: 0; }
  .flag :global(svg) { width: 15px; height: 15px; flex: none; }
  .flag em { font-style: normal; font-size: 10.5px; font-weight: 700; color: var(--text-2); }
  .sr { position: absolute; width: 1px; height: 1px; margin: -1px; padding: 0; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
  /* A measurement nobody took. Punctuation weight, never a zero: a dash cannot be read as
     "this mod is free", and a 0 can. */
  .dash { font: 500 11.5px var(--mono); color: var(--text-4); }
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
  .newtag { font-style: normal; margin-left: 7px; font-size: 9px; font-weight: 800; letter-spacing: 0.08em; color: var(--green); background: var(--green-soft); padding: 1px 5px; border-radius: 0; vertical-align: 1px; }
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
