// The toolbar, at every width and in every state it has. It is one row and it must stay one row:
// when it overflowed, Discard was drawn under the panel beside it and could not be pressed, and
// nothing caught it because the checks only looked at three widths in one state.
//
// Also the two places a player is told where to ask for help.
//
// npm run build && npx vite preview --port 4180 --strictPort
// NODE_PATH=$(npm root -g) node tools/loadtest/shots28.cjs <outdir>
const { chromium } = require('playwright');

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}

/** One row, inside its own box, with every control reachable. */
const TOOLBAR = () => {
  const t = document.querySelector('.toolbar');
  if (!t) return { problems: ['no toolbar'] };
  const tb = t.getBoundingClientRect();
  const kids = [...t.children].filter((e) => e.getBoundingClientRect().height > 0);
  const top = Math.min(...kids.map((e) => e.getBoundingClientRect().top));
  const problems = [];
  for (const e of kids) {
    const r = e.getBoundingClientRect();
    const what = e.textContent.replace(/\s+/g, ' ').trim().slice(0, 18) || e.className;
    if (r.top > top + 8) problems.push(`wrapped: ${what}`);
    if (r.right > tb.right + 0.5) problems.push(`over the edge: ${what}`);
    if (r.left < tb.left - 0.5) problems.push(`off the left: ${what}`);
  }
  if (t.scrollWidth > t.clientWidth + 1) problems.push(`overflows (${t.scrollWidth} > ${t.clientWidth})`);
  // A short label that is still shown must be readable. The filter's own label is the exception:
  // it carries whatever the current choice is called and is meant to end in an ellipsis.
  for (const s of t.querySelectorAll('.opt-lbl, .cnt-lbl, .split-lbl, .seg button span')) {
    if (s.getBoundingClientRect().width > 0 && s.scrollWidth > s.clientWidth + 1) problems.push(`clipped: ${s.textContent.trim()}`);
  }
  return { problems, width: Math.round(t.clientWidth), controls: kids.length };
};

/** Every control the toolbar carries can actually be clicked where it is drawn. */
const REACHABLE = () => {
  const t = document.querySelector('.toolbar');
  const bad = [];
  for (const btn of t.querySelectorAll('button')) {
    const r = btn.getBoundingClientRect();
    if (r.width <= 0 || r.height <= 0) continue;
    const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
    if (!hit || !btn.contains(hit)) bad.push(btn.getAttribute('aria-label') || btn.textContent.replace(/\s+/g, ' ').trim().slice(0, 18));
  }
  return bad;
};

async function open(p, url) {
  await p.goto(url, { waitUntil: 'load' });
  for (let i = 0; i < 80; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
  await p.waitForTimeout(600);
}

/** Every width from wide to narrow, in steps small enough to land between two breakpoints. */
async function sweep(p, label) {
  let bad = 0;
  let widest = 0;
  for (let w = 2200; w >= 640; w -= 40) {
    await p.setViewportSize({ width: w, height: 1100 });
    await p.waitForTimeout(180);
    const t = await p.evaluate(TOOLBAR);
    widest = Math.max(widest, t.width);
    if (t.problems.length) {
      bad++;
      if (bad <= 3) console.log(`      ${label} @${w} (toolbar ${t.width}px): ${[...new Set(t.problems)].slice(0, 3).join('; ')}`);
    }
  }
  ok(`${label}: one row at every width from 2200 down to 640`, bad === 0, bad ? `${bad} widths overflow` : `widest toolbar ${widest}px`);
}

(async () => {
  const out = process.argv[2] || '/tmp';
  const base = 'http://127.0.0.1:4180/';
  const b = await chromium.launch();
  const p = await b.newPage({ viewport: { width: 2200, height: 1100 }, deviceScaleFactor: 2 });
  const errors = [];
  p.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));
  p.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 300)); });

  // A thousand-mod list: the counts are four digits wide, which is the toolbar at its fullest.
  await open(p, `${base}?big=1100&jumble=30`);

  await sweep(p, 'the list alone');

  await p.setViewportSize({ width: 2200, height: 1100 });
  await p.waitForTimeout(400);
  await p.click('.btn.primary:has-text("Sort with HALO")');
  await p.waitForTimeout(1200);
  // The state this was reported in: a preview up, the tabs still there, a wide window.
  await sweep(p, 'with a preview');
  await p.setViewportSize({ width: 1960, height: 1100 });
  await p.waitForTimeout(400);
  ok('every control is where it can be pressed', (await p.evaluate(REACHABLE)).length === 0, (await p.evaluate(REACHABLE)).join(', '));
  ok('Discard is inside the toolbar, not under the panel beside it', await p.evaluate(() => {
    const t = document.querySelector('.toolbar').getBoundingClientRect();
    const d = [...document.querySelectorAll('.toolbar button')].find((e) => /Discard/.test(e.getAttribute('aria-label') || ''));
    return !!d && d.getBoundingClientRect().right <= t.right + 0.5;
  }));
  await p.screenshot({ path: `${out}/m28-toolbar.png` });

  await p.click('.btn.split:has-text("What changes")');
  await p.waitForTimeout(800);
  await sweep(p, 'the board open');
  await p.click('.btn.split:has-text("Inactive | Active")');
  await p.waitForTimeout(800);
  await sweep(p, 'two lists open');

  // ---- where a player is told to go for help ----
  await open(p, base);
  const link = 'https://discord.gg/JvsdeBw897';
  const rail = await p.$('.rail .help');
  ok('the sidebar carries a way to ask for help', !!rail && /discord/i.test((await rail.textContent()) ?? ''), (await rail?.textContent())?.trim());
  await p.evaluate(() => [...document.querySelectorAll('.rail .nav button')].find((e) => /Settings/.test(e.textContent))?.click());
  await p.waitForTimeout(800);
  const help = await p.evaluate(() => {
    const card = [...document.querySelectorAll('.settings .card')].find((c) => /Help and about/.test(c.querySelector('h3')?.textContent ?? ''));
    if (!card) return null;
    return {
      title: card.querySelector('h3').textContent.trim(),
      buttons: [...card.querySelectorAll('button')].map((e) => e.textContent.replace(/\s+/g, ' ').trim()),
      clipped: [...card.querySelectorAll('button')].some((e) => e.scrollWidth > e.clientWidth + 1)
    };
  });
  ok('Settings has a help card with the Discord in it', !!help && help.buttons.some((t) => /Discord/.test(t)), JSON.stringify(help));
  ok('and its buttons are not squeezed', !!help && !help.clipped);
  await p.screenshot({ path: `${out}/m28-help.png` });
  console.log(`      the link both places open: ${link}`);

  ok('no errors in the console', errors.length === 0, errors.slice(0, 3).join(' | '));
  await b.close();
  console.log(failures ? `\n${failures} failed` : '\nall good');
  process.exit(failures ? 1 : 0);
})();
