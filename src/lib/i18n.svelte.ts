// Every word a player reads, in one place.
//
// Circinus was written in English, in the components. That is fine until somebody who does not
// read English installs it, and then there is nothing to translate: the words are spread across
// twenty files, tangled with markup, and half of them are inside `title=` attributes nobody would
// think to look in. This is the lookup that makes a translation a data file rather than a
// refactor.
//
// The rules that keep it honest:
//
//   - Keys are dotted and describe *where* the string is, not what it says: `toolbar.sort.label`,
//     not `sort` or the English text itself. Using the English as the key is tempting because a
//     miss is impossible, but then fixing a typo in the copy silently orphans every translation
//     of it.
//   - One entry per string a person reads. A string built by pasting fragments together cannot
//     be translated: word order is not the same everywhere. Interpolate instead: `{n}`, `{name}`.
//   - Counts take a plural entry, `{ one, other }`, because English needs two forms and other
//     languages need more. `plural()` is the one place that decides, so adding CLDR's few/many
//     later is a change here rather than in two hundred call sites.
//   - A missing key returns the key. Loudly wrong beats quietly blank: an untranslated string
//     shows as `settings.updates.title` on screen, which somebody reports.
//
// `tools/i18n-check.mjs` fails the build on a user-facing literal that never came through here,
// and carries the list of files not yet converted. That list only ever shrinks.

import { EN } from "./locales/en";

/** Locales with a catalogue. English is the source, and the fallback for a key another one is
 *  missing -- a half-finished translation should leave English on screen, not a blank. */
export type Locale = "en";
export const LOCALES: { id: Locale; label: string; english: string }[] = [{ id: "en", label: "English", english: "English" }];

export type Entry = string | { one: string; other: string };
export type Catalogue = Record<string, Entry>;

const CATALOGUES: Record<Locale, Catalogue> = { en: EN };

/** Reactive: changing it re-renders every string on screen without a reload. */
let current = $state<Locale>("en");

export function locale(): Locale {
  return current;
}
export function setLocale(l: Locale) {
  current = l;
}

/** Which plural form a count takes. English has two; this is where a language with more gets
 *  its rule, rather than at the call sites. */
function plural(entry: { one: string; other: string }, n: number): string {
  return n === 1 ? entry.one : entry.other;
}

/** Fill `{name}` placeholders. A value of 0 or "" is still a value; only undefined is missing. */
function fill(text: string, params?: Record<string, string | number>): string {
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (whole, key) => (params[key] === undefined ? whole : String(params[key])));
}

/**
 * The string for `key`, in the current locale.
 *
 * `params.n` doubles as the count for a plural entry, since a string with a plural in it always
 * has the number in it too.
 */
export function t(key: string, params?: Record<string, string | number>): string {
  const entry = CATALOGUES[current][key] ?? (current !== "en" ? EN[key] : undefined);
  if (entry === undefined) return key;
  const text = typeof entry === "string" ? entry : plural(entry, Number(params?.n ?? 0));
  return fill(text, params);
}

/** Keys the current catalogue is missing, against English. For the check and for a translator. */
export function missingKeys(l: Locale): string[] {
  return Object.keys(EN).filter((k) => !(k in CATALOGUES[l]));
}
