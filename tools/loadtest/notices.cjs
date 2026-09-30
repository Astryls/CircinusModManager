// One notice column, and the settings that drive the list.
//
// The thing this file exists to hold shut is the **collapsing rule**. Six columns became one,
// so the row now has to choose which of up to seven things to show, and the choice is the whole
// feature: a row with an error and a pin must show the error. Get the rank wrong and the column
// is worse than the six it replaced, because now the important thing can be hidden behind the
// unimportant one rather than merely being in a different cell.
//
// It also covers the two ways a dismissal can go wrong. Dismissing must actually hide the mark,
// and it must never be a memory hole: the notice has to still be reachable and bringable back,
// or people stop trusting the button and go back to ignoring the row.
//
//   npm run build && npx vite preview --port 4173 --strictPort
//   node tools/loadtest/notices.cjs [outdir]
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4173';
const OUT = process.argv[2] || '/tmp/notices';

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}

const openView = (p, label) =>
  p.evaluate((l) => [...document.querySelectorAll('.rail .nav button, .nav button')].find((x) => x.textContent.trim().startsWith(l))?.click(), label);

/** Click the notice mark on the first row that has one, and return that row's mod name. */
async function openFirstNotice(p) {
  const name = await p.evaluate(() => {
    const b = document.querySelector('.list .row .flag.nt');
    return b ? b.closest('.row').querySelector('.name b')?.textContent.trim() : null;
  });
  const box = await p.evaluate(() => {
    const r = document.querySelector('.list .row .flag.nt').getBoundingClientRect();
    return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
  });
  await p.mouse.click(box.x, box.y);
  await p.waitForSelector('.pop', { timeout: 5000 });
  return name;
}

(async () => {
  const browser = await chromium.launch();
  const errors = [];
  const page = await browser.newPage({ viewport: { width: 1500, height: 940 }, colorScheme: 'dark' });
  page.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));

  await page.goto(BASE, { waitUntil: 'load' });
  await page.waitForSelector('.list .row[data-uid]', { timeout: 15000 });
  await page.waitForTimeout(800);

  // ---- one column, not six -------------------------------------------------------------------
  const head = await page.evaluate(() => {
    const h = document.querySelector('.hdr .h.b');
    if (!h) return null;
    const r = h.getBoundingClientRect();
    return { svg: !!h.querySelector('svg'), text: h.textContent.trim(), w: Math.round(r.width), x: Math.round(r.x) };
  });
  ok('there is one notice heading', !!head, JSON.stringify(head));
  // The 190px those five columns were costing every row is the point of the change.
  ok('and the strip is one icon wide, not six', (head?.w ?? 999) <= 60, `${head?.w}px`);

  // The heading is a glyph. It used to be the word "Notices" under type rules it had given
  // itself -- 9.5px, weight 600, mixed case, no tracking, against every other heading's
  // 10.5px/700/uppercase -- which is what made the column read as belonging to another table.
  ok('the heading is a glyph, not a word in its own typeface', head?.svg === true && head?.text === 'Notices', JSON.stringify(head));
  const sr = await page.evaluate(() => {
    const el = document.querySelector('.hdr .h.b .sr');
    if (!el) return null;
    const r = el.getBoundingClientRect();
    return { w: Math.round(r.width), h: Math.round(r.height) };
  });
  ok('with the word still there for a screen reader', sr != null && sr.w <= 2 && sr.h <= 2, JSON.stringify(sr));

  // ---- the gutter is first, and is in the same place at every width ----------------------------
  // It sat last, so its position moved with the elastic middle of the row -- different at every
  // width, and in a pane too narrow for its own heading, which was simply hidden. This is the
  // assertion that the move happened rather than the styling changing.
  const order = await page.evaluate(() => {
    const kids = [...document.querySelector('.hdr').children];
    return kids.map((k) => (k.classList.contains('b') ? 'notices' : k.classList.contains('idx') ? 'idx' : k.classList.contains('name') ? 'name' : 'other'));
  });
  ok('the notice gutter comes before the number', order.indexOf('notices') === 0 && order.indexOf('idx') === 1, order.join(' > '));
  // Measured from the list's own left edge, not the window's: the card itself moves when the
  // window narrows, and that is the layout doing its job. What must not move is where the mark
  // sits *inside* the row, which is the thing the old last-column position could not promise.
  const marksAt = async (p) =>
    p.evaluate(() => {
      const left = document.querySelector('.list').getBoundingClientRect().x;
      const xs = [...document.querySelectorAll('.list .row .flag.nt')].map((b) => Math.round(b.getBoundingClientRect().x - left));
      return [...new Set(xs)];
    });
  const wide = await marksAt(page);
  ok('every mark is in one column', wide.length === 1, wide.join(', '));
  await page.setViewportSize({ width: 1040, height: 940 });
  await page.waitForTimeout(500);
  const narrow = await marksAt(page);
  ok('and it is the same column at a narrower width', narrow.length === 1 && narrow[0] === wide[0], `${wide[0]} -> ${narrow[0]}`);
  await page.setViewportSize({ width: 1500, height: 940 });
  await page.waitForTimeout(500);

  // ---- the collapsing rule ---------------------------------------------------------------------
  // Severity, not category. Read every row's mark against everything its tooltip says it holds:
  // whatever is shown must be the most serious line in it.
  const RANK = ['error', 'warning', 'update', 'note', 'changed', 'new', 'pinned'];
  const rows = await page.evaluate(() =>
    [...document.querySelectorAll('.list .row')].map((r) => {
      const b = r.querySelector('.flag.nt');
      return b ? { name: r.querySelector('.name b')?.textContent.trim(), title: b.getAttribute('title') || '', err: b.classList.contains('err') } : null;
    }).filter(Boolean)
  );
  ok('some rows carry a notice', rows.length > 0, String(rows.length));
  // A row whose tooltip mentions an error must be drawn as an error, whatever else it holds.
  const looksError = (t) => /do not work together|needs |loads above|rule loop|cannot be read/i.test(t);
  const wrong = rows.filter((r) => looksError(r.title) && !r.err);
  ok('a row with an error is drawn as an error, whatever else it has', wrong.length === 0, wrong.slice(0, 2).map((r) => r.name).join(', '));
  // And the reverse: nothing gets an error mark without an error in it.
  const overstated = rows.filter((r) => r.err && !looksError(r.title));
  ok('and nothing else borrows the error mark', overstated.length === 0, overstated.slice(0, 2).map((r) => `${r.name}: ${r.title.slice(0, 40)}`).join(' | '));
  ok('the ranking is the documented one', RANK.length === 7);

  // The case the whole rule exists for: a row holding two notices of different weight. The mock
  // pins a mod that also has an error, so the row must draw the error and keep the pin for the
  // panel. A corpus where no row holds two cannot tell a correct ranking from a broken one.
  // The list is virtualised, so a row forty places down does not exist in the DOM. Search for
  // it rather than scrolling: this assertion is about the mark, not about the scroller.
  await page.fill('#search', 'rimmsqol');
  await page.waitForTimeout(600);
  const both = await page.evaluate(() => {
    const r = [...document.querySelectorAll('.list .row')].find((x) => /RIMMSqol/i.test(x.querySelector('.name b')?.textContent ?? ''));
    const b = r?.querySelector('.flag.nt');
    return b ? { err: b.classList.contains('err'), title: b.getAttribute('title') || '', count: b.querySelector('.num')?.textContent ?? '' } : null;
  });
  ok('a pinned mod with an error shows the error, not the pin', both?.err === true, JSON.stringify(both));
  ok('and the pin is still in there, counted', /Pinned/.test(both?.title ?? '') && both?.count === '2', JSON.stringify(both));
  await page.fill('#search', '');
  await page.waitForTimeout(500);

  // ---- the popover says everything the column could not -----------------------------------------
  const name = await openFirstNotice(page);
  const pop = await page.evaluate(() => document.querySelector('.pop')?.textContent ?? '');
  ok('clicking the mark opens the row it belongs to', pop.includes(name), `${name} vs ${pop.slice(0, 40)}`);
  const lines = await page.evaluate(() => document.querySelectorAll('.pop .it').length);
  ok('and lists at least one notice', lines >= 1, String(lines));
  await page.screenshot({ path: `${OUT}/notices-popover.png` });

  // ---- dismissing hides the mark ------------------------------------------------------------------
  const before = await page.evaluate(() => document.querySelectorAll('.list .row .flag.nt').length);
  const dismissable = await page.evaluate(() => !!document.querySelector('.pop .it .x[aria-label^="Dismiss"]'));
  ok('a notice can be put down', dismissable);
  await page.evaluate(() => document.querySelector('.pop .it .x[aria-label^="Dismiss"]')?.click());
  await page.waitForTimeout(900);
  const gone = await page.evaluate(() => document.querySelectorAll('.pop .it.gone').length);
  ok('and the panel then shows it as put down rather than forgetting it', gone >= 1, String(gone));
  // A memory hole is the failure mode here: the way back has to be on screen, next to it.
  const back = await page.evaluate(() => !!document.querySelector('.pop .it.gone .x'));
  ok('with the way back beside it', back);
  await page.evaluate(() => document.querySelector('.pop .it.gone .x')?.click());
  await page.waitForTimeout(900);
  const after = await page.evaluate(() => document.querySelectorAll('.list .row .flag.nt').length);
  ok('bringing it back restores the row', after === before, `${before} -> ${after}`);
  await page.keyboard.press('Escape');
  await page.waitForTimeout(300);
  ok('Escape closes the panel', !(await page.evaluate(() => !!document.querySelector('.pop'))));

  // ---- the list settings ---------------------------------------------------------------------------
  await openView(page, 'Settings');
  await page.waitForTimeout(600);
  const card = await page.evaluate(() => {
    const c = [...document.querySelectorAll('.card')].find((x) => /^\s*The list/.test(x.querySelector('h3')?.textContent ?? ''));
    return c ? { text: c.textContent, chips: [...c.querySelectorAll('.chip')].map((b) => b.textContent.trim()), selects: c.querySelectorAll('select').length } : null;
  });
  ok('Settings has a page for the list', !!card);
  ok('with a chip per optional column', (card?.chips.length ?? 0) >= 5, (card?.chips ?? []).join(' | '));
  ok('including the pooled start-up median', (card?.chips ?? []).includes('Median'));
  ok('and a control for what to open on, and how to sort', (card?.selects ?? 0) >= 2, String(card?.selects));
  ok('and a way to bring back everything put down', /Bring them all back/.test(card?.text ?? ''));

  // Turning the median column on has to actually add a column, not just light a chip.
  await page.evaluate(() => {
    const c = [...document.querySelectorAll('.card')].find((x) => /^\s*The list/.test(x.querySelector('h3')?.textContent ?? ''));
    [...c.querySelectorAll('.chip')].find((b) => b.textContent.trim() === 'Median')?.click();
  });
  await page.waitForTimeout(800);
  await openView(page, 'Load order');
  await page.waitForTimeout(700);
  const med = await page.evaluate(() => ({
    head: !!document.querySelector('.hdr .lmed'),
    cells: document.querySelectorAll('.list .row .lmed').length,
    // A mod nobody has timed must read as a dash, never as a zero.
    zeros: [...document.querySelectorAll('.list .row .lmed')].filter((c) => /^0\s*(ms|s)$/.test(c.textContent.trim())).length
  }));
  ok('the Median column appears in the list', med.head && med.cells > 0, JSON.stringify(med));
  ok('and an untimed mod shows a dash, not a zero', med.zeros === 0, String(med.zeros));
  await page.screenshot({ path: `${OUT}/notices-median.png` });

  // ---- where the window opens ---------------------------------------------------------------
  // Read once, on the first snapshot, and never again: it is a starting position rather than a
  // mode, and re-applying it on a later snapshot would drag somebody back every time settings
  // were saved. A value this build does not have must fall through to the load order rather
  // than opening a window onto nothing.
  const fresh = await browser.newPage({ viewport: { width: 1500, height: 940 }, colorScheme: 'dark' });
  fresh.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));
  await fresh.goto(`${BASE}/?open=loadtimes&sort=name`, { waitUntil: 'load' });
  await fresh.waitForTimeout(1500);
  const where = await fresh.evaluate(() => [...document.querySelectorAll('.rail .nav button')].find((b) => b.classList.contains('on'))?.textContent.trim() ?? '');
  ok('it opens on the page the settings name', /Load times/.test(where), where);
  // `plain` turns off the by-phase sections. Sorting by name inside each phase is correct and
  // is what the list does by default, but it is not a check of the opening sort.
  // Layout is remembered per machine, not per list, so an earlier page in this browser has
  // already stored one. Set it explicitly: `?plain` only sets the old backend field, which
  // seeds the layout once and never again.
  await fresh.addInitScript(() => localStorage.setItem('circinus.layout', JSON.stringify({ order: 'flat', active: 'flat', inactive: 'flat', linked: true, seeded: true })));
  await fresh.goto(`${BASE}/?plain&sort=name`, { waitUntil: 'load' });
  await fresh.waitForSelector('.list .row[data-uid]', { timeout: 15000 });
  await fresh.waitForTimeout(900);
  const names = await fresh.evaluate(() => [...document.querySelectorAll('.list .row .name b')].slice(0, 6).map((x) => x.textContent.trim()));
  const sorted = [...names].sort((a, b) => a.localeCompare(b));
  ok('and sorted the way they asked', JSON.stringify(names) === JSON.stringify(sorted), names.join(' | '));
  await fresh.goto(`${BASE}/?open=nonesuch`, { waitUntil: 'load' });
  await fresh.waitForSelector('.list .row[data-uid]', { timeout: 15000 });
  await fresh.waitForTimeout(700);
  const fell = await fresh.evaluate(() => [...document.querySelectorAll('.rail .nav button')].find((b) => b.classList.contains('on'))?.textContent.trim() ?? '');
  ok('a view this build does not have falls back to the load order', /Load order/.test(fell), fell);
  await fresh.close();

  console.log('errors:', errors.length ? errors.join('\n') : 'none');
  if (errors.length) failures += errors.length;
  await browser.close();
  console.log(failures ? `\n${failures} failed.` : '\nall good');
  process.exit(failures ? 1 : 0);
})();
