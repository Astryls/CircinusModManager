// Flat, coloured icons in the style of an emoji set: one fill and a darker outline of the
// same hue, drawn on a 24×24 grid. Each is an inline SVG string used with {@html}. Colours are
// baked in (they do not follow the text colour); the few neutral glyphs use currentColor.

const S = 'stroke-width="1.5" stroke-linejoin="round" stroke-linecap="round"';
const svg = (body: string) => `<svg viewBox="0 0 24 24" ${S} aria-hidden="true">${body}</svg>`;

// Palette: fill / outline
const RED = ["#f26d5f", "#a83a30"];
const AMB = ["#f6c04a", "#b3801a"];
const GRN = ["#6cc46f", "#2f8a3a"];
const BLU = ["#5c9ded", "#2f5fa8"];
const SKY = ["#9fd0f5", "#3f7fc0"];
const GRY = ["#a3adbb", "#55606f"];
const INK = ["#2c3139", "#15181c"];
const PAP = ["#f4f6f8", "#8e98a6"];
const WOOD = ["#c08a55", "#7a4f26"];
const fo = ([f, o]: string[]) => `fill="${f}" stroke="${o}"`;

const GEAR = "M19.60 12.00 L22.18 12.60 L21.64 15.34 L19.02 14.91 L17.37 17.37 L18.78 19.62 L16.45 21.18 L14.91 19.02 L12.00 19.60 L11.40 22.18 L8.66 21.64 L9.09 19.02 L6.63 17.37 L4.38 18.78 L2.82 16.45 L4.98 14.91 L4.40 12.00 L1.82 11.40 L2.36 8.66 L4.98 9.09 L6.63 6.63 L5.22 4.38 L7.55 2.82 L9.09 4.98 L12.00 4.40 L12.60 1.82 L15.34 2.36 L14.91 4.98 L17.37 6.63 L19.62 5.22 L21.18 7.55 L19.02 9.09Z";
const SPARK = "M17.00 2.80 L18.13 5.87 L21.20 7.00 L18.13 8.13 L17.00 11.20 L15.87 8.13 L12.80 7.00 L15.87 5.87Z M20.50 12.50 L21.04 13.96 L22.50 14.50 L21.04 15.04 L20.50 16.50 L19.96 15.04 L18.50 14.50 L19.96 13.96Z M12.50 2.00 L12.98 3.32 L14.30 3.80 L12.98 4.28 L12.50 5.60 L12.02 4.28 L10.70 3.80 L12.02 3.32Z";

const circleBadge = (c: string[], inner: string) => svg(`<circle cx="12" cy="12" r="9.5" ${fo(c)}/>${inner}`);

export const I = {
  // navigation
  list: svg(`<rect x="4.5" y="3.5" width="15" height="18" rx="2" ${fo(PAP)}/><rect x="9" y="1.8" width="6" height="3.4" rx="1.2" ${fo(GRY)}/><path d="M8 10h8M8 13.5h8M8 17h4.5" fill="none" stroke="#5c9ded" stroke-width="1.8"/>`),
  split: svg(`<rect x="2.5" y="4" width="8.5" height="16" rx="1.6" ${fo(GRY)}/><rect x="13" y="4" width="8.5" height="16" rx="1.6" ${fo(BLU)}/>`),
  library: svg(`<rect x="2.5" y="5" width="5.5" height="16" rx="1.2" ${fo(BLU)}/><rect x="9.25" y="3" width="5.5" height="18" rx="1.2" ${fo(GRN)}/><rect x="16" y="6.5" width="5.5" height="14.5" rx="1.2" ${fo(AMB)}/>`),
  download: svg(`<path d="M2.5 13.5h5l1.6 3h5.8l1.6-3h5V19a2 2 0 0 1-2 2h-15a2 2 0 0 1-2-2z" ${fo(GRY)}/><path d="M9.8 2.5h4.4v6.2H17L12 14 7 8.7h2.8z" ${fo(BLU)}/>`),
  cloud: svg(`<path d="M7 18.5a4.2 4.2 0 0 1-.4-8.4A5.6 5.6 0 0 1 17.4 8.6 4 4 0 0 1 17.5 18.5z" ${fo(SKY)}/><path d="M10.6 11h2.8v3.6h2.2L12 18.4 8.4 14.6h2.2z" ${fo(BLU)}/>`),
  image: svg(`<rect x="2.5" y="4" width="19" height="16" rx="2.5" ${fo(SKY)}/><path d="M3.5 18.8 9 12.3l4 4.2 2.6-2.8 4.9 5.1a1.5 1.5 0 0 1-1 .4h-15a1.5 1.5 0 0 1-1-.4z" ${fo(GRN)}/><circle cx="8.3" cy="9" r="2" ${fo(AMB)}/>`),
  analyze: svg(`<rect x="3" y="2.5" width="12.5" height="17" rx="2" ${fo(PAP)}/><path d="M6.5 7.5h5.5M6.5 11h5.5M6.5 14.5h3" fill="none" stroke="#a3adbb" stroke-width="1.6"/><circle cx="15.5" cy="15" r="4.6" ${fo(SKY)}/><path d="m18.9 18.4 2.8 2.8" fill="none" stroke="#15181c" stroke-width="2.8"/>`),
  gear: svg(`<path d="${GEAR}" ${fo(GRY)}/><circle cx="12" cy="12" r="3" ${fo(INK)}/>`),
  // actions
  play: circleBadge(GRN, `<path d="M9.5 7.6v8.8l7.2-4.4z" fill="#fff" stroke="none"/>`),
  save: svg(`<path d="M3.5 5.5A2 2 0 0 1 5.5 3.5h11.2l3.8 3.8v11.2a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z" ${fo(BLU)}/><rect x="7.5" y="3.8" width="8" height="4.8" rx="0.8" ${fo(PAP)}/><rect x="7" y="13" width="10" height="7" rx="1" ${fo(PAP)}/>`),
  refresh: svg(`<path d="M19.3 12.8a7.3 7.3 0 1 1-2.6-6.4" fill="none" stroke="#2f5fa8" stroke-width="4"/><path d="M19.3 12.8a7.3 7.3 0 1 1-2.6-6.4" fill="none" stroke="#5c9ded" stroke-width="1.8"/><path d="M20.5 2.5v6.2h-6.2z" ${fo(BLU)}/>`),
  halo: svg(`<path d="m3.2 20.8 10.6-10.6" fill="none" stroke="#7a4f26" stroke-width="4"/><path d="m3.2 20.8 10.6-10.6" fill="none" stroke="#c08a55" stroke-width="1.8"/><path d="${SPARK}" ${fo(AMB)}/>`),
  search: svg(`<circle cx="10" cy="10" r="6.5" ${fo(SKY)}/><circle cx="10" cy="10" r="3.2" fill="#e8f4fd" stroke="none"/><path d="m15 15 5.5 5.5" fill="none" stroke="#15181c" stroke-width="3.2"/>`),
  folder: svg(`<path d="M2.5 6.5A1.5 1.5 0 0 1 4 5h4.8l2 2.3h8.7A1.5 1.5 0 0 1 21 8.8V18a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 18z" ${fo(AMB)}/><path d="M2.5 10.3h18.5" fill="none" stroke="#b3801a" stroke-width="1.2"/>`),
  terminal: svg(`<rect x="2.5" y="3.5" width="19" height="17" rx="2.5" ${fo(INK)}/><path d="m6.5 8.5 3.2 3.2-3.2 3.2" fill="none" stroke="#6cc46f" stroke-width="2"/><path d="M12 15h5" fill="none" stroke="#6cc46f" stroke-width="2"/>`),
  link: svg(`<path d="M10.2 13.8a3.6 3.6 0 0 0 5.1 0l3.2-3.2a3.6 3.6 0 0 0-5.1-5.1l-1.2 1.2" fill="none" stroke="#2f5fa8" stroke-width="4"/><path d="M13.8 10.2a3.6 3.6 0 0 0-5.1 0l-3.2 3.2a3.6 3.6 0 0 0 5.1 5.1l1.2-1.2" fill="none" stroke="#2f5fa8" stroke-width="4"/><path d="M10.2 13.8a3.6 3.6 0 0 0 5.1 0l3.2-3.2a3.6 3.6 0 0 0-5.1-5.1l-1.2 1.2" fill="none" stroke="#5c9ded" stroke-width="1.8"/><path d="M13.8 10.2a3.6 3.6 0 0 0-5.1 0l-3.2 3.2a3.6 3.6 0 0 0 5.1 5.1l1.2-1.2" fill="none" stroke="#5c9ded" stroke-width="1.8"/>`),
  gauge: svg(`<path d="M3 17a9 9 0 1 1 18 0z" ${fo(AMB)}/><path d="m12 16 4.2-6.2" fill="none" stroke="#15181c" stroke-width="2.4"/><circle cx="12" cy="16" r="1.8" ${fo(INK)}/>`),
  bell: svg(`<path d="M6 16.5V11a6 6 0 0 1 12 0v5.5l1.8 2.3H4.2z" ${fo(AMB)}/><path d="M9.6 19.8a2.4 2.4 0 0 0 4.8 0" ${fo(WOOD)}/><path d="M12 2.5v2" fill="none" stroke="#b3801a" stroke-width="2"/>`),
  pin: svg(`<circle cx="14.5" cy="9.5" r="5" ${fo(RED)}/><path d="m11 13-6.5 6.5" fill="none" stroke="#55606f" stroke-width="2.6"/><path d="m9.8 15.6-1.4-1.4 4.3-4.3 1.4 1.4z" ${fo(RED)}/>`),
  // status
  check: circleBadge(GRN, `<path d="m7.3 12.4 3.2 3.2 6.4-6.8" fill="none" stroke="#fff" stroke-width="2.4"/>`),
  error: circleBadge(RED, `<path d="m8.4 8.4 7.2 7.2m0-7.2-7.2 7.2" fill="none" stroke="#fff" stroke-width="2.4"/>`),
  warn: svg(`<path d="M12 3 2.2 20.5h19.6z" ${fo(AMB)}/><path d="M12 9.2v5" fill="none" stroke="#3b2a05" stroke-width="2.4"/><circle cx="12" cy="17.2" r="1.35" fill="#3b2a05" stroke="none"/>`),
  note: circleBadge(BLU, `<circle cx="12" cy="7.8" r="1.45" fill="#fff" stroke="none"/><path d="M12 11v6.2" fill="none" stroke="#fff" stroke-width="2.4"/>`),
  plus: circleBadge(GRN, `<path d="M12 7.5v9M7.5 12h9" fill="none" stroke="#fff" stroke-width="2.4"/>`),
  minus: circleBadge(RED, `<path d="M7.5 12h9" fill="none" stroke="#fff" stroke-width="2.4"/>`),
  up: svg(`<path d="M12 3 4.5 11h4.8v9.5h5.4V11h4.8z" ${fo(GRN)}/>`),
  down: svg(`<path d="m12 21 7.5-8h-4.8V3.5H9.3V13H4.5z" ${fo(BLU)}/>`),
  change: svg(`<path d="M2.8 8.3 8 3.5v2.9h9.5v3.8H8v2.9z" ${fo(AMB)}/><path d="M21.2 15.7 16 20.5v-2.9H6.5v-3.8H16v-2.9z" ${fo(BLU)}/>`),
  // neutral glyphs that follow the text colour
  grip: '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><circle cx="9" cy="6" r="1.8"/><circle cx="15" cy="6" r="1.8"/><circle cx="9" cy="12" r="1.8"/><circle cx="15" cy="12" r="1.8"/><circle cx="9" cy="18" r="1.8"/><circle cx="15" cy="18" r="1.8"/></svg>',
  close: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" aria-hidden="true"><path d="m6 6 12 12M18 6 6 18"/></svg>',
  over: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M7 4v16m0 0-3-3m3 3 3-3M17 20V4m0 0-3 3m3-3 3 3"/></svg>',
  right: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4 12h15m0 0-5.5-5.5M19 12l-5.5 5.5"/></svg>',
  rise: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 20V5m0 0-5.5 5.5M12 5l5.5 5.5"/></svg>',
  fall: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 4v15m0 0-5.5-5.5M12 19l5.5-5.5"/></svg>'
};

// Older names some components still use.
export const sevIcon: Record<string, string> = { warning: I.warn, error: I.error, note: I.note };
