// Reading a keystroke, and writing one down.
//
// No runes and no store on purpose: this is the part with the rules in it -- which modifiers
// must be down, which must be up, and what a Mac prints instead of the word Ctrl -- and keeping
// it free of both means it can be bundled and called directly from a test. `keys.svelte.ts` is
// the table of actions that uses it.

/** Cmd on a Mac, Ctrl everywhere else. Written `Mod` in a chord so one table serves both. */
export const isMac = typeof navigator !== "undefined" && /mac|iphone|ipad/i.test(navigator.platform || navigator.userAgent);

export type Chord = { key: string; mod?: boolean; shift?: boolean; alt?: boolean };

/** `"Mod+Shift+p"` → the chord. Key names are the DOM's own: `ArrowUp`, `Escape`, `F8`, `Enter`. */
export function chord(spec: string): Chord {
  const parts = spec.split("+");
  const key = parts.pop() ?? "";
  return { key, mod: parts.includes("Mod"), shift: parts.includes("Shift"), alt: parts.includes("Alt") };
}

/** The modifier state of an event, as a chord would describe it. */
type Pressed = { mod: boolean; other: boolean; shift: boolean; alt: boolean };
function pressed(e: Pick<KeyboardEvent, "ctrlKey" | "metaKey" | "shiftKey" | "altKey">, mac: boolean): Pressed {
  // On a Mac, Ctrl is not Mod and must never stand in for it -- Ctrl+S there is a different
  // keystroke that happens to be near the one people mean. Elsewhere, Meta is the Windows key.
  return { mod: mac ? e.metaKey : e.ctrlKey, other: mac ? e.ctrlKey : e.metaKey, shift: e.shiftKey, alt: e.altKey };
}

/** Does this event *exactly* match?
 *
 *  Exactly is the point. The handler this replaces tested `ctrlKey || metaKey` and said nothing
 *  about the other modifiers, so Ctrl+Shift+I -- devtools, on every browser there is -- opened
 *  the Import dialog, and Ctrl+Alt+D toggled the Downloads view. A chord that does not name a
 *  modifier requires that modifier to be up. */
export function matches(e: Pick<KeyboardEvent, "key" | "ctrlKey" | "metaKey" | "shiftKey" | "altKey">, c: Chord, mac = isMac): boolean {
  const p = pressed(e, mac);
  if (p.other) return false;
  if (!!c.mod !== p.mod) return false;
  if (!!c.alt !== p.alt) return false;
  // Shift is not like the other modifiers: it changes the character, so a key that cannot be
  // typed without it already says so. `?` *is* Shift and something; demanding Shift be up for it
  // would bind nothing at all, and demanding Shift be down as well would be saying it twice.
  if (!shiftIsInTheKey(c.key) && !!c.shift !== p.shift) return false;
  return e.key.toLowerCase() === c.key.toLowerCase();
}

/** A single character that is not a lowercase letter carries Shift in itself: `?`, `!`, `:`. */
function shiftIsInTheKey(key: string): boolean {
  return key.length === 1 && !/[a-z0-9]/.test(key);
}

/** What to print on a button.
 *
 *  `⌘⇧P` on a Mac and `Ctrl Shift P` elsewhere: the shapes people already read on each platform,
 *  rather than one spelling forced on both. Five components used to carry `Ctrl S` as a literal,
 *  which is how a Mac came to be told to press a key it does not use for saving. */
export function keyLabel(spec: string, mac = isMac): string {
  const c = chord(spec);
  const named: Record<string, string> = { enter: mac ? "↩" : "Enter", escape: "Esc", arrowup: "↑", arrowdown: "↓", arrowleft: "←", arrowright: "→", delete: mac ? "⌫" : "Del", pageup: "PgUp", pagedown: "PgDn" };
  const key = named[c.key.toLowerCase()] ?? (c.key.length === 1 ? c.key.toUpperCase() : c.key);
  // A key that needs Shift to exist at all does not also get told to hold Shift.
  const shift = c.shift && !shiftIsInTheKey(c.key);
  if (mac) return `${c.mod ? "⌘" : ""}${shift ? "⇧" : ""}${c.alt ? "⌥" : ""}${key}`;
  return [c.mod ? "Ctrl" : "", shift ? "Shift" : "", c.alt ? "Alt" : "", key].filter(Boolean).join(" ");
}
