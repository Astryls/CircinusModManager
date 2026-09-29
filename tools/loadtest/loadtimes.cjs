// The load times page, and the three papers.
//
// Two things this covers that nothing else can.
//
// First: **a missing figure must never render as a zero.** "Nobody has timed this mod" and
// "this mod costs nothing at start-up" are opposite claims, and a blank cell filled with 0
// quietly makes the second one. A third of the mock corpus has no pooled median on purpose, so
// this file fails if any of those rows shows a number.
//
// Second: **the machine must be divided out.** The mock makes this computer a uniform 1.3x the
// pooled median and then pushes exactly two mods off that line. A page that compares raw
// milliseconds "finds" forty-four slow mods; a page that does the arithmetic right finds those
// two. So the assertion is not that the column exists, it is that sorting by it puts Combat
// Extended and RimMSQoL at the top and everything else at about 1.0x.
//
//   npm run build && npx vite preview --port 4173 --strictPort
//   node tools/loadtest/loadtimes.cjs [outdir]
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4173';
const OUT = process.argv[2] || '/tmp/loadtimes';

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}

const openView = (p, label) =>
  p.evaluate((l) => {
    const b = [...document.querySelectorAll('.rail .nav button, .nav button')].find((x) => x.textContent.trim().startsWith(l));
    b?.click();
  }, label);

const readRows = (p) =>
  p.evaluate(() =>
    [...document.querySelectorAll('.tbl .rw')].map((r) => {
      const c = [...r.querySelectorAll('[role=cell]')].map((x) => x.textContent.trim());
      return { name: c[0], mine: c[1], theirs: c[2], rel: c[3] };
    })
  );

(async () => {
  const browser = await chromium.launch();
  const errors = [];
  // Headless Chromium reports prefers-color-scheme: light, so without this the window starts
  // on the paper set and every assertion below about the dark ones is testing the wrong one.
  const page = await browser.newPage({ viewport: { width: 1500, height: 940 }, colorScheme: 'dark' });
  page.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));

  await page.goto(BASE, { waitUntil: 'load' });
  await page.waitForSelector('.list .row[data-uid]', { timeout: 15000 });
  await page.waitForTimeout(600);

  // ---- the page is reachable and says whose measurement it is ---------------------------------
  await openView(page, 'Load times');
  await page.waitForSelector('.tbl .rw', { timeout: 10000 });
  const head = await page.evaluate(() => document.querySelector('main.center')?.textContent ?? '');
  ok('the page names the mod that measured it', /Loading Progress/.test(head));
  ok('and its author', /ilyvion/.test(head));
  // Circinus times nothing. A page that implies otherwise is taking credit for somebody's work.
  ok('and says Circinus did none of the timing', /does no timing of its own/.test(head), head.slice(0, 80));

  // ---- the machine is divided out ------------------------------------------------------------
  ok('it reports the machine factor rather than hiding it', /loads mods at about/.test(head));
  const rows = await readRows(page);
  ok('there are rows', rows.length > 5, String(rows.length));
  const top = rows.slice(0, 2).map((r) => r.name);
  ok('the two planted outliers sort to the top', top.some((n) => /Alpha Animals/i.test(n)) && top.some((n) => /RIMMSqol/i.test(n)), top.join(' | '));
  // The whole point: everything that is only the computer must land at about 1.0x, or the page
  // is reporting the disk forty-four times over and calling each one a finding.
  const withRel = rows.filter((r) => /\d/.test(r.rel));
  const middling = withRel.filter((r) => !/Alpha Animals|RIMMSqol/i.test(r.name));
  const offOne = middling.filter((r) => Math.abs(parseFloat(r.rel) - 1) > 0.1);
  ok('and the rest sit at about 1.0x, because that part was the machine', offOne.length === 0, offOne.slice(0, 3).map((r) => `${r.name} ${r.rel}`).join(', '));

  // ---- a blank is a blank --------------------------------------------------------------------
  const untimed = rows.filter((r) => /not timed yet/.test(r.theirs));
  ok('some mods have no pooled median, as in life', untimed.length > 0, String(untimed.length));
  ok('and none of them reads as zero', untimed.every((r) => !/(^|\s)0(\.0)?\s*(ms|s)/.test(r.theirs)), untimed.slice(0, 2).map((r) => r.theirs).join(' | '));
  ok('nor do they claim to be perfectly in line', untimed.every((r) => !/\d/.test(r.rel)), untimed.slice(0, 2).map((r) => r.rel).join(' | '));

  // ---- both refresh buttons are there and neither throws ---------------------------------------
  const btns = await page.evaluate(() => [...document.querySelectorAll('main.center .btn')].map((b) => b.textContent.trim()));
  ok('there is a button to re-read Loading Progress', btns.some((b) => /Re-read Loading Progress/.test(b)), btns.join(' | '));
  ok('and one to refresh from circinus.sh', btns.some((b) => /Refresh from circinus\.sh/.test(b)));
  await page.evaluate(() => [...document.querySelectorAll('main.center .btn')].find((b) => /Re-read/.test(b.textContent))?.click());
  await page.waitForTimeout(500);
  await page.evaluate(() => [...document.querySelectorAll('main.center .btn')].find((b) => /Refresh from/.test(b.textContent))?.click());
  await page.waitForTimeout(700);
  ok('pressing both leaves the page standing', (await page.evaluate(() => document.querySelectorAll('.tbl .rw').length)) > 5);
  await page.screenshot({ path: `${OUT}/loadtimes-dark.png` });

  // ---- nothing is round ------------------------------------------------------------------------
  // Buttons were square in one panel and rounded in the next. The exceptions are deliberate and
  // named: a dot is round because it is a circle, and a spinner has to be.
  const round = await page.evaluate(() => {
    const out = [];
    for (const el of document.querySelectorAll('button, .btn, .chip, .pill, .seg, .input, .card, .switch input')) {
      const r = getComputedStyle(el).borderRadius;
      if (r && r !== '0px' && !/(^|\s)(dot|newdot|spin)(\s|$)/.test(el.className)) out.push(`${el.className || el.tagName}:${r}`);
    }
    return [...new Set(out)];
  });
  ok('every control is square', round.length === 0, round.slice(0, 5).join(', '));

  // ---- the three papers ------------------------------------------------------------------------
  const ground = () => page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  ok('dark is the plain one, on no attribute', (await page.evaluate(() => document.documentElement.getAttribute('data-theme'))) === null);
  const darkBg = await ground();

  await openView(page, 'Settings');
  await page.waitForTimeout(500);
  const setOled = (on) =>
    page.evaluate((want) => {
      const sec = [...document.querySelectorAll('.card')].find((c) => /^\s*Appearance/.test(c.querySelector('h3')?.textContent ?? ''));
      const box = [...(sec?.querySelectorAll('.switch') ?? [])].find((l) => /OLED/.test(l.textContent))?.querySelector('input');
      if (box && box.checked !== want) box.click();
      return !!box;
    }, on);
  ok('Settings offers the OLED switch', await setOled(true));
  await page.waitForTimeout(400);
  ok('turning it on writes the oled paper', (await page.evaluate(() => document.documentElement.getAttribute('data-theme'))) === 'oled');
  const oledBg = await ground();
  ok('and the ground is actually black, not merely darker', /rgba?\(0, ?0, ?0/.test(oledBg), oledBg);
  ok('which is a different ground from the normal dark', oledBg !== darkBg, `${darkBg} -> ${oledBg}`);
  // The ink must not go white with it: white on black smears when an OLED panel scrolls, and
  // the bone already clears 17:1 here.
  const ink = await page.evaluate(() => getComputedStyle(document.body).color);
  ok('and the ink stays bone rather than going white', !/rgba?\(255, ?255, ?255/.test(ink), ink);
  await openView(page, 'Load times');
  await page.waitForTimeout(400);
  await page.screenshot({ path: `${OUT}/loadtimes-oled.png` });

  // It is a variant of dark, not a third stop on the toggle: flipping to paper and back must
  // land on the black one, or the setting did not mean anything.
  await page.evaluate(() => [...document.querySelectorAll('.rail .ib')].find((b) => /paper/i.test(b.getAttribute('aria-label') || ''))?.click());
  await page.waitForTimeout(300);
  const afterToggle = await page.evaluate(() => document.documentElement.getAttribute('data-theme'));
  ok('the toggle still reaches the light paper', afterToggle === 'light', String(afterToggle));
  await page.evaluate(() => [...document.querySelectorAll('.rail .ib')].find((b) => /paper/i.test(b.getAttribute('aria-label') || ''))?.click());
  await page.waitForTimeout(300);
  ok('and coming back lands on the black one, not the grey one', (await page.evaluate(() => document.documentElement.getAttribute('data-theme'))) === 'oled');

  // ---- it survives a reload, before first paint --------------------------------------------------
  await page.reload({ waitUntil: 'load' });
  await page.waitForTimeout(500);
  ok('the choice outlives a reload', (await page.evaluate(() => document.documentElement.getAttribute('data-theme'))) === 'oled');

  console.log('errors:', errors.length ? errors.join('\n') : 'none');
  if (errors.length) failures += errors.length;
  await browser.close();
  console.log(failures ? `\n${failures} failed.` : '\nall good');
  process.exit(failures ? 1 : 0);
})();
