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

  /* ---- a measurement exists, and the list has moved on since ------------------------------
   *
   * The common case, and the one this card used to get wrong. It fell back to the calibrated
   * model the moment one mod was added or removed, so somebody who had measured a real 7m 43s
   * was shown a number nobody had ever observed -- thirty measurements and fourteen guesses
   * blended together -- because the list was off by one.
   *
   * A measurement beats an estimate even when the list has moved on. Loading Progress recorded
   * what the start-up took; the card shows that and the caption says which list it was. */
  let c = await cards(page);
  ok('there are five cards', c.length === 5, c.map((x) => x.title).join(' | '));
  const part = c.find((x) => x.title === 'Load time');
  ok('one of them is the load time', !!part, c.map((x) => x.title).join(' | '));
  ok('and it reads as a clock, not as raw seconds', /^\d+m \d+s$|^\d+s$/.test(part.value), part.value);
  ok('a measured start-up is shown as measured, not blended into an estimate', !/Estimated/i.test(part.cap), part.cap);
  ok('and it is the total Loading Progress reported', part.value === (await page.evaluate(() => {
    const ms = window.__CX_IMPACT_TOTAL_MS;
    if (!ms) return null;
    const s = ms / 1000;
    return s >= 60 ? `${Math.floor(s / 60)}m ${Math.round(s % 60)}s` : `${Math.round(s)}s`;
  })), part.value);
  // Which list it was. Without this the figure reads as a claim about the list on screen.
  ok('and says the list has changed since', /list has changed/i.test(part.cap), part.cap);
  // Whose measurement it is. The log path has always named its source; the paths that use more
  // of that mod's work than the log does must not quietly stop naming it.
  ok('and credits the mod that measured it', /Loading Progress/.test(part.cap), part.cap);
  // None of these numbers is anybody else's machine. Pooled medians belong on a mod's page,
  // not in a card headed with this computer's start-up time.
  ok('and never cites circinus.sh', !/circinus\.sh/i.test(part.cap), part.cap);
  await page.locator('.strip').screenshot({ path: `${OUT}/stats-lastrun.png` });

  // ---- nothing measured: the card says so and shows no number --------------------------------
  //
  // This card used to print an estimate here -- `VANILLA_SECS` plus the folder model, summed
  // over the list and set in 17px bold with "Estimated:" under it. On a real 1,078-mod install
  // that read "6m 39s", a figure nobody had ever observed, and the caption under a number that
  // large does not stop anybody quoting it. The card now holds one kind of number, from one
  // source, and when that source has said nothing the card says nothing.
  await page.goto(`${BASE}/?noload&noimpact`, { waitUntil: 'load' });
  await page.waitForSelector('.stats .stat', { timeout: 15000 });
  await page.waitForTimeout(500);
  const none = (await cards(page)).find((x) => x.title === 'Load time');
  ok('with nothing measured the card shows a dash', none?.value === '\u2014', JSON.stringify(none?.value));
  ok('and no digit anywhere in it', !/\d/.test(`${none?.value} ${none?.cap}`), `${none?.value} / ${none?.cap}`);
  ok('and never says "estimated"', !/estimat/i.test(none?.cap ?? ''), none?.cap);
  ok('it says plainly that nothing measured it', /Not measured/i.test(none?.cap ?? ''), none?.cap);
  // The point of saying so is that there is something to do about it.
  const noneTip = await page.evaluate(() => [...document.querySelectorAll('.stats .stat')].find((c) => /Load time/.test(c.textContent))?.getAttribute('title') ?? '');
  ok('and the tooltip names the mod and both of its settings', /Loading Progress/.test(noneTip) && /Track startup loading impact/.test(noneTip) && /Auto-save startup impact report/.test(noneTip), noneTip.slice(0, 90));
  ok('and promises no guess', !/estimat/i.test(noneTip) || /will not put a guess/.test(noneTip), noneTip.slice(0, 90));
  await page.locator('.strip').screenshot({ path: `${OUT}/stats-nothing.png` });

  /* ---- why there is no figure, and the mark that reads it again --------------------------
   *
   * "Loading Progress has not written a report" was one sentence covering four machines: one
   * where the mod is not installed, one where it is installed and inactive, one where it is
   * active and not tracking, and one where everything is on and the game simply has not been
   * started since. Only the last of those needs nothing done, and the other three each need a
   * different thing done, so a card that cannot tell them apart sends people looking.
   *
   * `?noimpact` is installed-and-idle; `?nolp` on top of it is not installed at all. The two
   * captions must differ, and neither may be the other's advice. */
  const capOf = async () => (await cards(page)).find((x) => x.title === 'Load time')?.cap ?? '';
  const tipOf = () => page.evaluate(() => [...document.querySelectorAll('.stats .stat')].find((c) => /Load time/.test(c.textContent))?.getAttribute('title') ?? '');

  const idleCap = none.cap;
  ok('installed-and-idle says it is installed and not tracking', /installed/i.test(idleCap) && /not tracking/i.test(idleCap), idleCap);

  await page.goto(`${BASE}/?noload&noimpact&nolp`, { waitUntil: 'load' });
  await page.waitForSelector('.stats .stat', { timeout: 15000 });
  await page.waitForTimeout(500);
  const missingCap = await capOf();
  ok('with the mod absent the caption says a different thing', missingCap !== idleCap, `${missingCap}`);
  ok('and does not claim it is installed', !/is installed/i.test(missingCap), missingCap);
  ok('and still names who would measure it', /Loading Progress/.test(missingCap), missingCap);
  const missingTip = await tipOf();
  ok('its tooltip still names both settings', /Track startup loading impact/.test(missingTip) && /Auto-save startup impact report/.test(missingTip), missingTip.slice(0, 80));

  /* The refresh mark. It is a second button inside the card's cell rather than inside the card,
   * because an interactive element nested in another is invalid markup -- and because a mark
   * that fired the card's own click would narrow the list to the slowest mods every time
   * somebody pressed it, which is not what a refresh mark means. */
  await page.goto(`${BASE}/?noload&noimpact`, { waitUntil: 'load' });
  await page.waitForSelector('.stats .stat', { timeout: 15000 });
  await page.waitForTimeout(500);
  const rf = await page.evaluate(() => {
    const mark = document.querySelector('.cell .rf');
    if (!mark) return null;
    const cell = mark.closest('.cell');
    const card = cell.querySelector('.stat');
    const m = mark.getBoundingClientRect(), b = card.getBoundingClientRect();
    // Nothing the card says may end up underneath the mark: not the figure, not the label, and
    // above all not the caption, which is the one thing this card may never lose.
    const over = (el) => {
      if (!el) return false;
      const t = el.getBoundingClientRect();
      return m.left < t.right - 1 && m.right > t.left + 1 && m.top < t.bottom - 1 && m.bottom > t.top + 1;
    };
    return {
      label: mark.getAttribute('aria-label') ?? '',
      title: mark.getAttribute('title') ?? '',
      inside: m.left >= b.left - 1 && m.right <= b.right + 1 && m.top >= b.top - 1 && m.bottom <= b.bottom + 1,
      overlaps: [...card.querySelectorAll('.l span, .cap .t')].some(over),
      nested: !!mark.closest('button.stat'),
      onLoadCard: /Load time/.test(card.textContent)
    };
  });
  ok('the load card carries a refresh mark', !!rf, 'no .cell .rf');
  ok('and it is on the load card', rf?.onLoadCard === true);
  ok('and it is not a button inside a button', rf?.nested === false);
  ok('and it sits inside the card', rf?.inside === true);
  ok('and never over anything the card says', rf?.overlaps === false);
  ok('and says what it does without a mouse', /report|read/i.test(rf?.label ?? ''), rf?.label);
  ok('and its tooltip says the figure only changes when the game starts', /RimWorld starts/.test(rf?.title ?? ''), (rf?.title ?? '').slice(0, 60));
  await page.click('.cell .rf');
  await page.waitForTimeout(400);
  ok('pressing it does not narrow the list', await page.evaluate(() => !document.querySelector('.stats .stat.on')));

  /* ---- switching tracking on, from the page that needs it --------------------------------
   * The two checkboxes live three clicks deep in another mod's settings window and the second
   * only appears once the first is on. Circinus writes them, and the state it reports back has
   * to change -- a switch that reports the state it was given is not a switch. */
  await page.goto(`${BASE}/?noload&noimpact#loadtimes`, { waitUntil: 'load' });
  await page.waitForSelector('.stats .stat', { timeout: 15000 });
  await page.waitForTimeout(400);
  await page.evaluate(() => { const b = [...document.querySelectorAll('.rail button, .dir button')].find((x) => /Load times/.test(x.textContent)); if (b) b.click(); });
  await page.waitForTimeout(400);
  const trackBtn = page.locator('button', { hasText: 'Switch on start-up tracking' }).first();
  ok('the Load times page offers to switch tracking on', await trackBtn.count() > 0);
  if (await trackBtn.count()) {
    await trackBtn.click();
    await page.waitForTimeout(600);
    ok('and once on it offers to stop instead', await page.locator('button', { hasText: 'Stop measuring start-up times' }).count() > 0);
    // Back to the list, because the strip is not drawn on the Load times page.
    await page.evaluate(() => { const b = [...document.querySelectorAll('.rail button, .dir button')].find((x) => /Load order/.test(x.textContent)); if (b) b.click(); });
    await page.waitForSelector('.stats .stat', { timeout: 15000 });
    await page.waitForTimeout(300);
    const after = await capOf();
    ok('and the strip is back on screen', after.length > 0, after);
    ok('and the card stops saying it is not tracking', !/not tracking/i.test(after), after);
    ok('and says a launch is what is missing now', /Start the game once/i.test(after), after);
  }

  // ---- the log alone is no longer enough -----------------------------------------------------
  // A Player.log total is a real measurement of a real start-up, but it is not per mod and it
  // is not what this card is for. It stays on the Load times page; here it would be a second
  // kind of number in a card whose whole rule is that it holds one.
  await page.goto(`${BASE}/?noimpact`, { waitUntil: 'load' });
  await page.waitForSelector('.stats .stat', { timeout: 15000 });
  await page.waitForTimeout(500);
  const logged = (await cards(page)).find((x) => x.title === 'Load time');
  ok('a log figure alone leaves the card empty', logged?.value === '\u2014', JSON.stringify(logged?.value));
  await page.locator('.strip').screenshot({ path: `${OUT}/stats-measured.png` });
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
    // Case-insensitive: the word can open the sentence ("Measured today...") or sit inside it
    // ("Your last start-up, today, measured by..."). What must never happen is a caption that
    // sheds the word altogether and leaves a bare number to be read as fact.
    ok(`${width}px: the load card still says which kind of number it is`, /measured|estimated/i.test(lt.cap), `"${lt.cap}"`);
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
