// Every keystroke the app answers to, in one table.
//
// It used to be an `if/else if` chain in `App.svelte` against string literals, which had the
// three problems a chain always has. Nothing could enumerate it, so the Shortcuts nobody could
// find were the ones nobody had written down -- F8 walked the review list and was advertised in
// no place at all. Nothing could label it, so five components carried the string "Ctrl S"
// hardcoded and a Mac was told to press Ctrl. And the same action was written twice, once on a
// button and once on a key, which is how the Downloads toggle came to exist character for
// character in two files.
//
// So: an action has an id, a name, an optional chord, and a `run`. Buttons, the key handler, the
// palette and the labels all read the same row.

import { store } from "./store.svelte";
import { t } from "./i18n.svelte";
export { chord, isMac, keyLabel, matches, type Chord } from "./chord";

/** Where a chord is allowed to fire.
 *
 *  The old guard returned early for any INPUT, which is why Escape did nothing in the search box
 *  and Ctrl+S did not save while you were typing -- the one moment you most want to save. The
 *  distinction that actually matters is not "is an input focused" but "would this keystroke
 *  otherwise be a character": a chord is safe in a text field, a bare key is not. */
export type Scope = "always" | "app";

export type Action = {
  id: string;
  /** Which part of the shortcut list it belongs under. */
  section: string;
  name: string;
  keys?: string;
  scope?: Scope;
  /** Absent means always available. A disabled action is still listed, greyed, so the palette
   *  says the thing exists rather than pretending it does not. */
  enabled?: () => boolean;
  run: () => void;
  /** Kept out of the palette: bound, but not a thing you would go looking for by name. */
  hidden?: boolean;
};

/** The views, numbered the way the sidebar lists them, so the number is on screen to be counted. */
const VIEWS = ["order", "library", "downloads", "textures", "patches", "analyzer", "defs", "halo", "settings"] as const;

/** Built on call, never as a module constant: a table of `t()` results made at import time would
 *  freeze whichever locale happened to be current when this file first loaded. */
export function actions(): Action[] {
  const sel = () => store.selected.filter((u) => store.activeSet.has(u));
  const out: Action[] = [
    // ---- the list ----
    { id: "save", section: t("keys.section.list"), name: t("keys.save"), keys: "Mod+s", scope: "always", run: () => store.save() },
    { id: "search", section: t("keys.section.list"), name: t("keys.search"), keys: "Mod+k", scope: "always", run: focusSearch },
    { id: "import", section: t("keys.section.list"), name: t("keys.import"), keys: "Mod+i", scope: "always", run: () => (store.showImport = true) },
    { id: "refresh", section: t("keys.section.list"), name: t("keys.refresh"), keys: "Mod+r", scope: "always", run: () => store.rescan() },
    { id: "play", section: t("keys.section.list"), name: t("keys.play"), keys: "Mod+Enter", scope: "always", run: () => store.launch() },
    { id: "halo", section: t("keys.section.list"), name: t("keys.halo"), keys: "Mod+h", scope: "always", run: () => store.haloPreview() },
    { id: "halo.apply", section: t("keys.section.list"), name: t("keys.halo.apply"), enabled: () => !!store.preview, run: () => store.haloApply() },
    { id: "halo.discard", section: t("keys.section.list"), name: t("keys.halo.discard"), enabled: () => !!store.preview, run: () => store.clearPreview() },
    { id: "selectAll", section: t("keys.section.list"), name: t("keys.selectAll"), keys: "Mod+a", run: selectAllVisible },
    { id: "deactivate", section: t("keys.section.list"), name: t("keys.deactivate"), keys: "Delete", enabled: () => sel().length > 0, run: () => store.deactivate(sel()) },
    { id: "moveUp", section: t("keys.section.list"), name: t("keys.moveUp"), keys: "Alt+ArrowUp", enabled: () => store.selected.length > 0, run: () => store.moveSelected(-1) },
    { id: "moveDown", section: t("keys.section.list"), name: t("keys.moveDown"), keys: "Alt+ArrowDown", enabled: () => store.selected.length > 0, run: () => store.moveSelected(1) },

    // ---- problems ----
    { id: "reviewNext", section: t("keys.section.review"), name: t("keys.reviewNext"), keys: "F8", enabled: () => store.reviewList.length > 0, run: () => store.reviewNext() },
    { id: "reviewPrev", section: t("keys.section.review"), name: t("keys.reviewPrev"), keys: "Shift+F8", enabled: () => store.reviewList.length > 0, run: () => store.reviewPrev() },

    // ---- getting around ----
    { id: "palette", section: t("keys.section.go"), name: t("keys.palette"), keys: "Mod+Shift+p", scope: "always", run: () => (store.showPalette = true) },
    { id: "shortcuts", section: t("keys.section.go"), name: t("keys.shortcuts"), keys: "?", run: () => (store.showKeys = true) }
  ];
  for (let i = 0; i < VIEWS.length; i++) {
    const v = VIEWS[i];
    out.push({ id: `view.${v}`, section: t("keys.section.go"), name: t("keys.view", { name: t(`rail.nav.${v}`) }), keys: `Mod+${i + 1}`, scope: "always", run: () => (store.view = v) });
  }
  return out;
}

export function focusSearch() {
  const el = document.getElementById("search") as HTMLInputElement | null;
  if (!el) return;
  el.focus();
  // Select rather than only focus: pressing the shortcut again with a query already typed means
  // "search for something else", and parking a cursor at the end makes that a backspace exercise.
  el.select();
}

function selectAllVisible() {
  const list = store.visibleList();
  if (list.length) store.selected = list;
}
