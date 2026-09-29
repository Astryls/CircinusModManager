// The summary strip, and the fifth card in it.
//
// Two things worth checking, and the geometry is the one that would otherwise be checked by
// squinting. The strip was `repeat(4, …)` with one breakpoint to `repeat(2, …)`; a fifth card
// against that leaves an orphan on its own row at wide widths and 2+2+1 at narrow. It is
// The strip is one row at every width, because a second row costs the mod list under it about
// 113 pixels for ever, and the list is what the window is for. The cards give way instead. So
// this sweeps real widths and asserts one row of equal boxes with nothing clipped -- and that the
// load card still says whether its number is measured or modelled at the narrowest of them,
// because that is the one thing it must not shed on the way down.
//
// The other is honesty, and the load-time card now has three kinds of number rather than two.
// A per-mod measurement from the Loading Progress mod, the game's own log with a total and no
// breakdown, and the model. Each is worth a different amount and the caption has to say which:
//   default            some of the list measured here -> an estimate, calibrated, and it says so
//   ?noimpact          the log only -> a measured total, attributed to whoever timed it
//   ?noload&noimpact   nothing measured -> the model, and it says "Estimated"
// The trap this guards is the middle one reading like the first. A figure built from thirty
// measurements and fourteen guesses is an estimate, and a card that calls it "Measured" is
// lying in 17px bold.
//
//   npm run build && npx vite preview --port 4173 --strictPort
//   node tools/loadtest/stats.cjs [outdir]
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4173';
const OUT = process.argv[2] || '/tmp/stats';

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}

const cards = (page) =>
  page.evaluate(() => {
    const strip = document.querySelector('.stats');
    return [...document.querySelectorAll('.stats .stat')].map((c) => {
      const b = c.getBoundingClientRect();
      const v = c.querySelector('.v');
      const cap = c.querySelector('.cap .t');
      return {
        title: c.querySelector('.l span')?.textContent.trim(),
        value: v?.textContent.trim(),
        cap: cap?.textContent.trim() ?? '',
        x: Math.round(b.x), y: Math.round(b.y), w: Math.round(b.width),
        // Anything sticking out of its own card is text a player cannot read. Both directions:
        // the caption used to be clamped with an ellipsis, which cut the half that says what the
        // number means.
        clipped: c.scrollWidth > c.clientWidth + 1 || c.scrollHeight > c.clientHeight + 1,
        // And nothing inside may be cut either, which is what a line clamp does.
        cut: [...c.querySelectorAll('.l span, .cap .t')].some((e) => e.scrollHeight > e.clientHeight + 1 || e.scrollWidth > e.clientWidth + 1),
        h: Math.round(b.height),
        stripW: Math.round(strip.getBoundingClientRect().width)
      };
    });
  });

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1600, height: 950 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));
  page.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 200)); });

  await page.goto(BASE, { waitUntil: 'load' });
  await page.waitForSelector('.stats .stat', { timeout: 15000 });
  await page.waitForTimeout(500);

  // ---- partly measured: the common case once the mod is installed ---------------------------
  let c = await cards(page);
  ok('there are five cards', c.length === 5, c.map((x) => x.title).join(' | '));
  const part = c.find((x) => x.title === 'Load time');
  ok('one of them is the load time', !!part, c.map((x) => x.title).join(' | '));
  ok('and it reads as a clock, not as raw seconds', /^\d+m \d+s$|^\d+s$/.test(part.value), part.value);
  // The whole point: thirty measurements and fourteen guesses is an estimate.
  ok('a partly measured list is called an estimate', /Estimated/i.test(part.cap), part.cap);
  ok('and it says how much of it was measured', /\d+ of \d+ mods timed/.test(part.cap), part.cap);
  // Whose measurement it is. The log path has always named its source; the two paths that use
  // more of that mod's work than the log does must not quietly stop naming it.
  ok('and credits the mod that measured it', /Loading Progress/.test(part.cap), part.cap);
  await page.locator('.strip').screenshot({ path: `${OUT}/stats-calibrated.png` });

  // ---- the log alone: a real total, and nobody pretending it was ours -------------------------
  await page.goto(`${BASE}/?noimpact`, { waitUntil: 'load' });
  await page.waitForSelector('.stats .stat', { timeout: 15000 });
  await page.waitForTimeout(500);
  const logged = (await cards(page)).find((x) => x.title === 'Load time');
  ok('with only the log there is still a figure', !!logged && !!logged.value, logged?.value ?? 'none');
  ok('the log case says it was measured', /Measured/i.test(logged.cap), logged.cap);
  ok('and names who measured it', /DefLoadCache/.test(logged.cap), logged.cap);
  await page.locator('.strip').screenshot({ path: `${OUT}/stats-measured.png` });

  // ---- nothing measured at all ---------------------------------------------------------------
  await page.goto(`${BASE}/?noload&noimpact`, { waitUntil: 'load' });
  await page.waitForSelector('.stats .stat', { timeout: 15000 });
  await page.waitForTimeout(500);
  const est = (await cards(page)).find((x) => x.title === 'Load time');
  ok('with no measurement there is still a figure', !!est && !!est.value, est?.value ?? 'none');
  ok('and the caption calls it an estimate', /Estimated/i.test(est.cap), est.cap);
  ok('which is a different number from the measured one', est.value !== logged.value, `${est.value} vs ${logged.value}`);
  // Mods alone would read absurdly low, so the game's own load is in it.
  ok('and it includes the game itself, not only the mods', /for the game/.test(est.cap), est.cap);
  await page.locator('.strip').screenshot({ path: `${OUT}/stats-estimated.png` });

  // ---- geometry, swept ---------------------------------------------------------------------------
  await page.goto(BASE, { waitUntil: 'load' });
  await page.waitForSelector('.stats .stat', { timeout: 15000 });
  for (const width of [2200, 1800, 1600, 1400, 1240, 1100, 980, 860, 760]) {
    await page.setViewportSize({ width, height: 950 });
    await page.waitForTimeout(250);
    c = await cards(page);
    if (!c.length) { ok(`${width}px: the strip is on screen`, false); continue; }

    // Cards on one row share a top edge and a width; that is what "a row of cards" means.
    const rows = new Map();
    for (const x of c) rows.set(x.y, [...(rows.get(x.y) ?? []), x]);
    const even = [...rows.values()].every((r) => new Set(r.map((x) => x.w)).size === 1);
    ok(`${width}px: every row is equal boxes`, even, [...rows.values()].map((r) => r.map((x) => x.w).join('/')).join('  |  '));
    ok(`${width}px: nothing is clipped inside its card`, c.every((x) => !x.clipped), c.filter((x) => x.clipped).map((x) => x.title).join(', '));
    // The caption wraps rather than being cut: the half that gets cut is the half that says what
    // the number means.
    ok(`${width}px: no label or caption is cut short`, c.every((x) => !x.cut), c.filter((x) => x.cut).map((x) => `${x.title}: "${x.cap}"`).join(' | '));
    // The failure the old grid would have had: one card alone on a row while others share.
    const counts = [...rows.values()].map((r) => r.length);
    const orphan = counts.length > 1 && counts[counts.length - 1] === 1 && counts[0] > 2;
    ok(`${width}px: no card orphaned on a row of its own`, !orphan, counts.join('+'));
    ok(`${width}px: cards are wide enough to read`, c.every((x) => x.w >= 125), `narrowest ${Math.min(...c.map((x) => x.w))}px`);
    // The point of the whole layout: a second row costs the mod list under it about 113 pixels,
    // so the strip never takes one and the cards give way instead.
    ok(`${width}px: one row, always`, rows.size === 1, `${rows.size} rows at ${c[0].stripW}px of strip`);
    // What a card must never shed on the way down: whether its number is measured or modelled.
    const lt = c.find((x) => x.title === 'Load time');
    ok(`${width}px: the load card still says which kind of number it is`, /Measured|Estimated/.test(lt.cap), `"${lt.cap}"`);
  }
  await page.setViewportSize({ width: 1100, height: 950 });
  await page.waitForTimeout(300);
  await page.locator('.strip').screenshot({ path: `${OUT}/stats-narrow.png` });

  console.log('errors:', errors.length ? errors.join('\n') : 'none');
  if (errors.length) failures += errors.length;
  await browser.close();
  console.log(failures ? `\n${failures} failed.` : '\nall good');
  process.exit(failures ? 1 : 0);
})();
