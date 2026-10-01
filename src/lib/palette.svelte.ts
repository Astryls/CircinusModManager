// The colours, when the shipped two are not the ones somebody wants.
//
// This sits on top of `theme.svelte.ts` rather than replacing it. The papers stay what they
// are -- circinus.sh's own tokens, solved together -- and this writes overrides for the ones a
// player has changed. Three properties make that safe, and they are the whole design:
//
//  1. **An override is an inline custom property on <html>, and nothing else.** Setting one is
//     `style.setProperty("--bg", v)`; clearing one is `removeProperty`. The stylesheet is never
//     edited and never consulted, so putting a colour back is *removing* a thing rather than
//     writing a remembered value -- there is no stored "default" that can be wrong, missing or
//     corrupted. That is what makes Reset unable to fail.
//  2. **Per paper.** A colour chosen against the dark ground is usually wrong on the light one,
//     so the two sets are kept apart and the toggle swaps between them. Switching paper
//     re-applies; it never carries a colour across.
//  3. **Legibility is checked before anything is applied**, on load as well as on edit. A
//     palette that would leave the text unreadable against the ground is refused outright, so
//     a bad save cannot produce a window nobody can read -- including a window where the
//     Settings page that holds the Reset button has become invisible. The guard runs at start
//     on whatever was stored, which is the case that would otherwise be unrecoverable.
//
// Derived tokens are kept in step here rather than being exposed. `--amber-soft` is `--amber`
// at 13% and exists in a dozen places; letting somebody set the accent and leaving the tint
// behind would be worse than not offering the accent at all.

import { theme } from "$lib/theme.svelte";

/** One editable colour. `key` is the CSS custom property, without the leading dashes. */
export type Slot = { key: string; label: string; hint: string };
export type Group = { title: string; note: string; slots: Slot[] };

/** What can be changed, grouped the way the tokens are reasoned about in `app.css`.
 *
 *  Deliberately not every token. The ones left out are either derived (the `-soft` tints, which
 *  follow their accent below), structural (`--shadow-*`, which are rules rather than colours),
 *  or legacy aliases that already point at something in this list. */
export const GROUPS: Group[] = [
  {
    title: "Ground",
    note: "Behind everything. The window's own background.",
    slots: [
      { key: "bg", label: "Background", hint: "The window behind every panel" },
      { key: "bg-2", label: "Background, deeper", hint: "Behind the directory column" },
      { key: "well", label: "Well", hint: "Inset areas: logs, samples, read-only boxes" }
    ]
  },
  {
    title: "Panels",
    note: "Cards, rows and the rules between them. These four carry most of the layout: every edge in the window is a 1px line in one of them rather than a shadow.",
    slots: [
      { key: "surface", label: "Card", hint: "Panels, the list, the column header" },
      { key: "surface-2", label: "Card, raised", hint: "Menus, chips, inputs" },
      { key: "surface-3", label: "Rule", hint: "The 1px lines separating everything" },
      { key: "surface-4", label: "Rule, stronger", hint: "Edges that need to be seen" }
    ]
  },
  {
    title: "Ink",
    note: "Four weights, brightest first. Anything below the fourth stops being readable.",
    slots: [
      { key: "text", label: "Text", hint: "Mod names, headings, figures" },
      { key: "text-2", label: "Text, secondary", hint: "Authors, values, labels" },
      { key: "text-3", label: "Text, quiet", hint: "Column headings, hints" },
      { key: "text-4", label: "Text, faintest", hint: "Dashes, placeholders, what is absent" }
    ]
  },
  {
    title: "Accents",
    note: "Two, and only two. Amber means look at this before you play; red means something is wrong. The soft tints behind them follow automatically.",
    slots: [
      { key: "amber", label: "Attention", hint: "Moves, warnings, the thing to check" },
      { key: "red", label: "Error", hint: "The only colour that means broken" },
      { key: "bar", label: "Meter", hint: "The filled part of a bar" }
    ]
  },
  {
    title: "Groups",
    note: "Identity, not severity -- the one place hue does real work. The shipped eight are at least 15 ΔE apart and 15 from both accents, so a group can never be mistaken for an error. Changing them gives that up; the group's name is always printed beside its dot, which is what makes it survivable.",
    slots: [
      { key: "g-blue", label: "Blue", hint: "Group slot" },
      { key: "g-teal", label: "Teal", hint: "Group slot" },
      { key: "g-green", label: "Green", hint: "Group slot" },
      { key: "g-pink", label: "Pink", hint: "Group slot" },
      { key: "g-amber", label: "Amber", hint: "Group slot (not the accent)" },
      { key: "g-coral", label: "Coral", hint: "Group slot (not red)" },
      { key: "g-violet", label: "Violet", hint: "Group slot" },
      { key: "g-slate", label: "Slate", hint: "The no-colour slot" }
    ]
  }
];

const ALL_KEYS = GROUPS.flatMap((g) => g.slots.map((s) => s.key));

/** Accents whose soft tint is the same colour at low alpha. Kept in step automatically. */
const SOFT: Record<string, { token: string; alpha: number }> = {
  amber: { token: "--amber-soft", alpha: 0.13 },
  red: { token: "--red-soft", alpha: 0.13 }
};

const KEY = "circinus.palette";

/** `#rgb`, `#rrggbb` or `#rrggbbaa` to 0-255 components. Null for anything else. */
function rgb(hex: string): [number, number, number] | null {
  const h = hex.trim().replace(/^#/, "");
  const full = h.length === 3 ? [...h].map((c) => c + c).join("") : h;
  if (!/^[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$/.test(full)) return null;
  return [parseInt(full.slice(0, 2), 16), parseInt(full.slice(2, 4), 16), parseInt(full.slice(4, 6), 16)];
}

function luminance([r, g, b]: [number, number, number]): number {
  const f = (v: number) => {
    const x = v / 255;
    return x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
}

/** WCAG contrast ratio, 1 to 21. */
export function contrast(a: string, b: string): number | null {
  const x = rgb(a);
  const y = rgb(b);
  if (!x || !y) return null;
  const [l1, l2] = [luminance(x), luminance(y)].sort((p, q) => q - p);
  return (l1 + 0.05) / (l2 + 0.05);
}

/** Is this something CSS will accept as a colour? */
export function valid(v: string): boolean {
  if (!v.trim()) return false;
  try {
    return CSS.supports("color", v.trim());
  } catch {
    return rgb(v) != null;
  }
}

/** The floor a custom palette has to clear before it is applied.
 *
 *  Not a style rule -- a way back. Text at 3:1 on its ground is poor and still readable, which
 *  is all this needs to guarantee: that somebody who has made a mess can still find Settings
 *  and press Reset. Below it the whole custom set is dropped and the shipped paper comes back. */
const MIN_CONTRAST = 3;

type Saved = Partial<Record<"dark" | "oled" | "light", Record<string, string>>>;

function read(): Saved {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return {};
    const v = JSON.parse(raw);
    return v && typeof v === "object" ? (v as Saved) : {};
  } catch {
    // Unparseable is the same as none. A broken file must not be able to stop the window.
    return {};
  }
}

class Palette {
  /** The overrides for the paper on screen now, by token key without the dashes. */
  current = $state<Record<string, string>>({});
  /** Set when a stored palette was refused for being unreadable, so the page can say so. */
  refused = $state(false);

  /** Which stored set applies: the three papers get three sets, because a colour chosen
   *  against #131210 is usually wrong against #e4e1d9. */
  private get slot(): "dark" | "oled" | "light" {
    if (theme.paper === "light") return "light";
    return theme.darkVariant === "oled" ? "oled" : "dark";
  }

  /** Read the stylesheet's own value for a token, with every override cleared first.
   *
   *  Used only to show what a slot looks like when it has not been changed. It asks the
   *  browser rather than holding a table of defaults, so it cannot drift from `app.css`. */
  shipped(key: string): string {
    const el = document.documentElement;
    const had = el.style.getPropertyValue(`--${key}`);
    if (had) el.style.removeProperty(`--${key}`);
    const v = getComputedStyle(el).getPropertyValue(`--${key}`).trim();
    if (had) el.style.setProperty(`--${key}`, had);
    return v;
  }

  start() {
    this.load();
  }

  /** Re-read and re-apply for the paper now showing. Called by the theme toggle. */
  load() {
    const saved = read()[this.slot] ?? {};
    const keep: Record<string, string> = {};
    for (const [k, v] of Object.entries(saved)) {
      if (ALL_KEYS.includes(k) && typeof v === "string" && valid(v)) keep[k] = v;
    }
    this.clearDom();
    this.current = keep;
    this.applyAll();
    // The guard, run on load and not only on edit. A palette saved before this check existed,
    // or saved on another build, or hand-edited in localStorage, has to be survivable: without
    // this, an unreadable set would come back on every launch and the Reset button would be on
    // a page nobody can see.
    if (!this.readable()) {
      this.refused = true;
      this.clearDom();
      this.current = {};
    } else {
      this.refused = false;
    }
  }

  /** Is the text legible against the ground, as things stand right now? */
  readable(): boolean {
    const cs = getComputedStyle(document.documentElement);
    const text = cs.getPropertyValue("--text").trim();
    const bg = cs.getPropertyValue("--bg").trim();
    const surface = cs.getPropertyValue("--surface").trim();
    const a = contrast(text, bg);
    const b = contrast(text, surface);
    // A colour CSS understands but this cannot parse (a named colour, say) returns null. That
    // is "cannot tell", and refusing on it would reject valid palettes, so it passes.
    return (a == null || a >= MIN_CONTRAST) && (b == null || b >= MIN_CONTRAST);
  }

  private clearDom() {
    const el = document.documentElement;
    for (const k of ALL_KEYS) el.style.removeProperty(`--${k}`);
    for (const s of Object.values(SOFT)) el.style.removeProperty(s.token);
  }

  private applyOne(key: string, value: string) {
    const el = document.documentElement;
    el.style.setProperty(`--${key}`, value);
    const soft = SOFT[key];
    if (soft) {
      const c = rgb(value);
      // A tint can only be computed from a hex. For anything else the shipped tint is left
      // alone, which is wrong-ish and visible, rather than cleared, which would make a dozen
      // backgrounds vanish.
      if (c) el.style.setProperty(soft.token, `rgba(${c[0]}, ${c[1]}, ${c[2]}, ${soft.alpha})`);
    }
  }

  private applyAll() {
    for (const [k, v] of Object.entries(this.current)) this.applyOne(k, v);
  }

  private save() {
    try {
      const all = read();
      all[this.slot] = this.current;
      localStorage.setItem(KEY, JSON.stringify(all));
    } catch {
      // Not being able to remember it is not a reason to refuse to show it.
    }
  }

  /** Change one colour. Refused, with nothing written, if the value is not a colour or if the
   *  result would be unreadable -- the window is left exactly as it was either way. */
  set(key: string, value: string): boolean {
    if (!ALL_KEYS.includes(key) || !valid(value)) return false;
    const before = this.current[key];
    this.applyOne(key, value);
    if (!this.readable()) {
      if (before) this.applyOne(key, before);
      else {
        document.documentElement.style.removeProperty(`--${key}`);
        const soft = SOFT[key];
        if (soft) document.documentElement.style.removeProperty(soft.token);
      }
      return false;
    }
    this.current = { ...this.current, [key]: value };
    this.save();
    return true;
  }

  /** Put one colour back to the shipped one. */
  clear(key: string) {
    const next = { ...this.current };
    delete next[key];
    this.current = next;
    document.documentElement.style.removeProperty(`--${key}`);
    const soft = SOFT[key];
    if (soft) document.documentElement.style.removeProperty(soft.token);
    this.save();
  }

  /** Put everything back.
   *
   *  **This cannot fail, and that is deliberate.** It writes no colour: it removes every
   *  override, which hands each token back to the stylesheet that defined it. There is no
   *  stored default to be missing, no parse to go wrong, and nothing here reads the palette it
   *  is undoing -- so it works just as well on a set that is unreadable, half-saved, or
   *  written by a build that had different tokens. */
  reset() {
    this.clearDom();
    this.current = {};
    this.refused = false;
    try {
      const all = read();
      delete all[this.slot];
      localStorage.setItem(KEY, JSON.stringify(all));
    } catch {
      // The screen is already right. Failing to forget it is a smaller problem than the mess
      // that prompted the button, and it will be refused on load anyway if unreadable.
    }
  }

  /** Put every paper's palette back, not only the one showing. The way out for somebody who
   *  has made two messes. */
  resetAll() {
    this.clearDom();
    this.current = {};
    this.refused = false;
    try {
      localStorage.removeItem(KEY);
    } catch {
      /* as above */
    }
  }

  get changed(): number {
    return Object.keys(this.current).length;
  }
}

export const palette = new Palette();
