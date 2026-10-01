// What one row has to say, reduced to one mark.
//
// There used to be six columns of icons: Changed, Update, Errors, Warning, Notes, Pinned. Six
// columns of mostly-empty cells cost 232px of every row for ever, and the reader still had to
// scan across all six to find out whether anything was wrong -- which is one question, not six.
// So there is one column, it shows the most serious thing the row carries, and the rest is in
// the tooltip and the popover.
//
// **Order is severity, not category.** The rank below is the whole of the collapsing rule: an
// error outranks a warning outranks an update, and "pinned" comes last because it is a state
// the user chose rather than something the row is telling them. A mod with an error and a pin
// shows the error; that is the point.
//
// This file is deliberately free of runes, of the store and of Svelte, so the ranking and the
// dismissal keys can be tested directly rather than through a rendered row.

import type { Issue, ModChange, ModInfo, UpdateInfo } from "./types";

export type NoticeKind = "error" | "warning" | "update" | "note" | "changed" | "new" | "pinned";

/** Most serious first. Index in this array is the rank; nothing else defines it. */
export const NOTICE_ORDER: NoticeKind[] = ["error", "warning", "update", "note", "changed", "new", "pinned"];

/** The icon key each kind draws, as a key of `$lib/icons`. Typed against that object so a
 *  rename there fails the build here rather than rendering an empty cell. */
export const NOTICE_ICON: Record<NoticeKind, keyof typeof import("./icons").I> = {
  error: "error",
  warning: "warn",
  update: "up",
  note: "note",
  changed: "change",
  new: "plus",
  pinned: "pin"
};

export interface Notice {
  kind: NoticeKind;
  /** One line for the tooltip and the popover. Already resolved to real mod names. */
  text: string;
  /** The stored key, or null for a notice that cannot be waved away. */
  dismissKey: string | null;
}

/**
 * The key a dismissal is stored under: `<packageId>|<kind>|<token>`.
 *
 * On the **package id** rather than the uid, like `muted`, so unsubscribing and resubscribing
 * does not undo the decision.
 *
 * The **token** is what stops a dismissal outliving the thing it was about. "I have seen this
 * update" means *this* update, so the token is what the author published and the next release
 * raises the mark again. A standing condition -- an unmet rule, a mod above the game -- has no
 * token, because there is nothing for it to expire against: it stays down until brought back.
 */
export function dismissKey(packageId: string, kind: NoticeKind, token: string | number = ""): string {
  return `${packageId.toLowerCase().replace(/_steam$/, "")}|${kind}|${token}`;
}

export interface NoticeInput {
  mod: ModInfo;
  issues: Issue[];
  /** `describe(issue, byUid, uid)` from `$lib/describe`, passed in so this file stays pure. */
  describe: (i: Issue) => string;
  severityOf: (i: Issue) => "error" | "warning" | "note";
  update?: UpdateInfo;
  change?: ModChange;
  describeChange: (c: ModChange) => string;
  isNew: boolean;
  arrived?: string;
  pinned: boolean;
  /** Set to true for a row in the inactive pane, where `mod.invalid` is worth reporting. */
  inactive: boolean;
  dismissed: ReadonlySet<string>;
}

/**
 * Every notice this row carries, most serious first, with the dismissed ones marked rather than
 * dropped -- the popover has to be able to offer them back, and a count of what is hidden is
 * the only thing that keeps dismissal from being a memory hole.
 */
export function noticesFor(a: NoticeInput): { shown: Notice[]; hidden: Notice[] } {
  const pkg = a.mod.packageId ?? a.mod.uid;
  const out: Notice[] = [];

  const bySev = (sev: "error" | "warning" | "note") => a.issues.filter((i) => a.severityOf(i) === sev);

  // An unreadable folder is an error about the mod itself rather than about the order, and it
  // only means anything on a row that is not in the list.
  const invalid = a.inactive && a.mod.invalid ? a.mod.invalid : "";
  const errors = bySev("error").map((i) => a.describe(i));
  if (invalid) errors.unshift(invalid);
  if (errors.length) out.push({ kind: "error", text: errors.join("\n"), dismissKey: dismissKey(pkg, "error") });

  const warnings = bySev("warning").map((i) => a.describe(i));
  if (warnings.length) out.push({ kind: "warning", text: warnings.join("\n"), dismissKey: dismissKey(pkg, "warning") });

  if (a.update) {
    const when = new Date(a.update.remoteUpdated * 1000).toLocaleDateString();
    out.push({
      kind: "update",
      text: `A newer version is on the Workshop, updated ${when}`,
      // The publish time is the token: dismissing this one must not silence the next one.
      dismissKey: dismissKey(pkg, "update", a.update.remoteUpdated)
    });
  }

  const notes = bySev("note").map((i) => a.describe(i));
  if (notes.length) out.push({ kind: "note", text: notes.join("\n"), dismissKey: dismissKey(pkg, "note") });

  if (a.change) {
    out.push({
      kind: "changed",
      text: `Changed since you last opened Circinus: ${a.describeChange(a.change)}`,
      // Keyed on *what* changed, not on when.
      //
      // `change.when` is `ModInfo.modified`, and that figure grows during start-up: the quick
      // pass sets it from the folder and About.xml mtimes, then the folder walk raises it to
      // the newest mtime anywhere in the tree a few seconds later. A dismissal taken in that
      // window was stored against the first value and came undone when the second landed --
      // the mark reappearing on its own, for no reason the player could see.
      //
      // The reasons are settled by the time there is anything to dismiss and say the thing
      // worth keying on anyway: "I have seen that this mod's files changed" survives the
      // second pass, and a later, different change still raises a new mark because the
      // baseline moves and the notice is rebuilt from it.
      dismissKey: dismissKey(pkg, "changed", [...(a.change.reasons ?? [])].sort().join(",") || "changed")
    });
  } else if (a.isNew) {
    out.push({ kind: "new", text: a.arrived ? `New: first seen ${a.arrived}` : "New", dismissKey: dismissKey(pkg, "new") });
  }

  if (a.pinned) {
    // Not dismissable, and the only one in the list that is not. A pin is a thing the user did,
    // and the way to stop seeing it is to unpin the mod; a dismissal would leave the list
    // behaving in a way nothing on screen explained.
    out.push({ kind: "pinned", text: "Pinned: keeps this position when sorting", dismissKey: null });
  }

  out.sort((x, y) => NOTICE_ORDER.indexOf(x.kind) - NOTICE_ORDER.indexOf(y.kind));
  const shown: Notice[] = [];
  const hidden: Notice[] = [];
  for (const n of out) (n.dismissKey && a.dismissed.has(n.dismissKey) ? hidden : shown).push(n);
  return { shown, hidden };
}
