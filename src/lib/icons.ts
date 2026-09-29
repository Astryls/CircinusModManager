// Cut paper.
//
// One solid shape in `currentColor` and, behind it, a second sheet of the same colour at 38%.
// No strokes, no gradients, no baked-in hues -- which is the whole point: the previous set had
// its colours compiled in, so every glyph had to be redrawn for a second paper and none of them
// could take the ink of the thing they sat in. These inherit, so one drawing serves both papers.
//
// Rules the set keeps, because they are what make it survive at 15px in the directory column:
//   - a 24 grid, and nothing thinner than 3 units, so no limb disappears at small sizes
//   - shapes meet at the grid, never overlap by a fraction, or the seam shimmers when scaled
//   - only `error` and `warn` carry colour, and they take it from the caller, not from here
//
// Each value is an inline SVG string used with {@html}. Keys are unchanged from the old set, so
// every call site still resolves.

const svg = (body: string) => `<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">${body}</svg>`;
/** The sheet behind: same colour, stepped back. */
const back = (d: string) => `<path opacity=".38" d="${d}"/>`;
const front = (d: string) => `<path d="${d}"/>`;

export const I = {
  // ---- navigation ----------------------------------------------------------------------
  list: svg(back("M4 4h16v3H4z") + front("M4 10h16v3H4zM4 16h11v3H4z")),
  split: svg(back("M3 4h8v16H3z") + front("M13 4h8v16h-8z")),
  library: svg(back("M4 4h7v7H4z") + front("M13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z")),
  download: svg(back("M4 17h16v3H4z") + front("M10.5 3h3v7h4L12 15.5 6.5 10h4z")),
  cloud: svg(back("M3 9h18v5H3z") + front("M10.5 3h3v6h4L12 14.5 6.5 9h4zM3 17h18v3H3z")),
  image: svg(back("M3 3h11v11H3z") + front("M10 10h11v11H10z")),
  analyze: svg(back("M3 14h4v7H3zM9.5 9h4v12h-4z") + front("M16 3h5v18h-5z")),
  gear: svg(back("M3 5h18v3H3zM3 16h18v3H3z") + front("M14 3h3v7h-3zM7 14h3v7H7z")),
  defs: svg(back("M3 4h8v3H3zM3 10.5h8v3H3z") + front("M3 17h8v3H3zM13 4h3v16h-3zM16 10.5h5v3h-5z")),
  pack: svg(back("M3 7l9-4 9 4v10l-9 4-9-4z") + front("M3 7l9 4v10l-9-4z")),

  // ---- actions -------------------------------------------------------------------------
  play: svg(back("M4 3h4v18H4z") + front("M9 3l11 9-11 9z")),
  save: svg(back("M3 3h18v18H3z") + front("M7 3h10v7H7zM7 14h10v7H7z")),
  refresh: svg(back("M4 4h3v16H4z") + front("M9 3l7 4.5L9 12zM20 21l-7-4.5L20 12z")),
  halo: svg(back("M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zm0 4a6 6 0 1 1 0 12 6 6 0 0 1 0-12z") + front("M9.5 9.5h5v5h-5z")),
  search: svg(back("M3 3h13v13H3zm3 3v7h7V6z") + front("M14 14h3v7h-3z")),
  folder: svg(back("M3 5h7l2 3h9v3H3z") + front("M3 11h18v8H3z")),
  terminal: svg(back("M3 3h18v18H3z") + front("M6 7l5 5-5 5V13.5H6v-3h0zM12.5 15h5.5v3h-5.5z")),
  link: svg(back("M3 9h9v6H3z") + front("M12 9h9v6h-9zM9 10.5h6v3H9z")),
  gauge: svg(back("M3 18h18v3H3z") + front("M3 16a9 9 0 0 1 18 0zM11 8h2v8h-2z")),
  // A stopwatch: crown and body behind, the hand cut out of the front. Nothing finer than 3
  // units, so the hand is still a hand at the 15px the directory draws it at.
  timer: svg(back("M4 12a8 8 0 1 1 16 0 8 8 0 0 1-16 0z") + front("M9 1h6v3H9zM10.5 7h3v6h-3zM13.5 12h5v3h-5z")),
  bell: svg(back("M6 6h12v10H6z") + front("M4 16h16v3H4zM10 20h4v2h-4zM11 2h2v3h-2z")),
  pin: svg(back("M7 2h10v9H7z") + front("M4 11h16v3H4zM10.5 14h3v8h-3z")),

  // ---- status. The only two that ever carry a hue, and they take it from the caller. -----
  check: svg(back("M3 3h18v18H3z") + front("M6 12.5l4 4L18 8.5l-2.2-2.2L10 12.1 8.2 10.3z")),
  error: svg(back("M3 3h18v18H3z") + front("M10.5 4h3v10h-3zM10.5 17h3v3h-3z")),
  warn: svg(back("M12 2l10 19H2z") + front("M10.5 9h3v6h-3zM10.5 16.5h3v3h-3z")),
  note: svg(back("M3 3h18v18H3z") + front("M10.5 9h3v9h-3zM10.5 5h3v3h-3z")),
  plus: svg(back("M3 3h18v18H3z") + front("M10.5 6h3v12h-3zM6 10.5h12v3H6z")),
  minus: svg(back("M3 3h18v18H3z") + front("M6 10.5h12v3H6z")),
  up: svg(back("M4 19h16v3H4z") + front("M12 2l7 8h-4v7h-6v-7H5z")),
  down: svg(back("M4 2h16v3H4z") + front("M12 22l-7-8h4V7h6v7h4z")),
  change: svg(back("M3 5h13v3H3z") + front("M16 3l5 3.5L16 10zM8 14l-5 3.5L8 21zM8 16h13v3H8z")),

  // ---- the paper switch. Half the sheet in ink, half stepped back: the two papers, side by
  // side. It reads as what it does without borrowing a sun or a moon, neither of which is
  // what this setting is about.
  paper: svg(back("M12 3h9v18h-9z") + front("M3 3h9v18H3z")),

  // ---- neutral glyphs, unchanged in spirit: they already followed the text colour ---------
  grip: svg(front("M8 4h3v3H8zM13 4h3v3h-3zM8 10.5h3v3H8zM13 10.5h3v3h-3zM8 17h3v3H8zM13 17h3v3h-3z")),
  close: svg(front("M5.6 3.5l14.9 14.9-2.1 2.1L3.5 5.6zM18.4 3.5l2.1 2.1L5.6 20.5l-2.1-2.1z")),
  over: svg(back("M3 10.5h18v3H3z") + front("M7 2l4 5H3zM17 22l-4-5h8z")),
  right: svg(back("M3 10.5h12v3H3z") + front("M14 5l7 7-7 7z")),
  rise: svg(back("M10.5 8h3v14h-3z") + front("M12 2l6 7H6z")),
  fall: svg(back("M10.5 2h3v14h-3z") + front("M12 22l-6-7h12z"))
};

// Older names some components still use.
export const sevIcon: Record<string, string> = { warning: I.warn, error: I.error, note: I.note };
