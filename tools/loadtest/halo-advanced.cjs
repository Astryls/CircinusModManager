// Bands of your own, on the HALO page.
//
// A group with its own place in the load order is, to the sort, a phase the user invented: it
// holds a band between two built-in phases, and several after the same phase keep the order they
// are listed in. There is no ninth `Phase` behind it — `section_rank` does the work — so this
// checks the thing that matters: a band made here shows up as its own heading in the list, in the
// right place, and moving it moves the mods in it.
//
//   npm run build && npx vite preview --port 4173 --strictPort
//   node tools/loadtest/halo-advanced.cjs [outdir]
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4173';
const OUT = process.argv[2] || '/tmp/halo';

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1600, height: 980 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));
  page.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 250)); });

  await page.goto(BASE, { waitUntil: 'load' });
  await page.waitForSelector('.list .row', { timeout: 15000 });
  await page.waitForTimeout(400);

  const toHalo = async () => {
    await page.evaluate(() => {
      const b = [...document.querySelectorAll('.rail .nav button, .nav button')].find((x) => x.textContent.trim().startsWith('HALO'));
      b?.click();
    });
    await page.waitForSelector('.halo .adv', { timeout: 5000 });
    await page.waitForTimeout(250);
  };
  const toList = async () => {
    await page.click('.mark');
    await page.waitForSelector('.list .row', { timeout: 5000 });
    await page.waitForTimeout(350);
  };

  await toHalo();

  // Off by default: the eight phases are the answer for most lists.
  const openAtFirst = await page.locator('.adv .add').count();
  ok('the advanced controls are off to begin with', openAtFirst === 0, `${openAtFirst} forms visible`);

  await page.click('.adv .switch input');
  await page.waitForSelector('.adv .add', { timeout: 3000 });
  ok('the toggle reveals them', (await page.locator('.adv .add').count()) === 1);

  // Make a band that loads after Libraries, which is well before Content in the built-in order.
  await page.fill('.adv .add .input', 'My overhauls');
  await page.selectOption('.adv .add .sel', { label: 'Libraries' });
  await page.click('.adv .add button[type="submit"]');
  await page.waitForTimeout(400);
  const rows = await page.evaluate(() => [...document.querySelectorAll('.adv .row.band .nm')].map((i) => i.value));
  ok('the band is listed', rows.includes('My overhauls'), rows.join(', ') || 'none');

  // It survives leaving the page, which is the whole point of it living in the user data.
  await toList();
  await toHalo();
  const still = await page.evaluate(() => [...document.querySelectorAll('.adv .row.band .nm')].map((i) => i.value));
  ok('and it is still there after leaving the page', still.includes('My overhauls'), still.join(', ') || 'none');
  ok('and the toggle stayed on', (await page.locator('.adv .add').count()) === 1);

  // An empty band has nothing to head, on purpose, so put a mod in it the way a user would.
  await toList();
  const empty = await page.evaluate(() => [...document.querySelectorAll('.list .ph')].some((h) => h.textContent.includes('My overhauls')));
  ok('an empty band is not a heading of its own', !empty, empty ? 'it drew one anyway' : 'nothing drawn, as intended');

  const victim = await page.evaluate(() => {
    const r = [...document.querySelectorAll('.list .row[data-uid]')].find((x) => !/ludeon|harmony|prepatcher|fishery/i.test(x.getAttribute('data-uid') ?? ''));
    if (!r) return null;
    r.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 500, clientY: 400 }));
    return r.getAttribute('data-uid');
  });
  await page.waitForTimeout(250);
  await page.evaluate(() => {
    const b = [...document.querySelectorAll('[role="menuitem"]')].find((x) => x.textContent.trim().startsWith('Group'));
    b?.click();
  });
  await page.waitForTimeout(250);
  await page.evaluate(() => {
    const b = [...document.querySelectorAll('[role="menuitemradio"]')].find((x) => x.textContent.includes('My overhauls'));
    b?.click();
  });
  await page.waitForTimeout(450);
  ok('a mod could be put in the band', !!victim, victim ?? 'no row to use');

  const headings = await page.evaluate(() =>
    [...document.querySelectorAll('.list .ph')].map((h) => h.textContent.replace(/\s+/g, ' ').trim())
  );
  const named = headings.some((h) => h.includes('My overhauls'));
  ok('the band is a heading in the list once it holds something', named, headings.slice(0, 12).join(' | ') || '(none)');

  if (named) {
    const order = await page.evaluate(() => {
      const out = [];
      for (const h of document.querySelectorAll('.list .ph')) out.push(h.textContent.replace(/\s+/g, ' ').trim());
      return out;
    });
    const iBand = order.findIndex((h) => h.includes('My overhauls'));
    // A heading reads "<phase><count><note>", so anchor on the start: a band called "Adult
    // content" matches a loose /content/i and made this assert against the wrong row.
    const iLib = order.findIndex((h) => /^Libraries\d/.test(h));
    const iContent = order.findIndex((h) => /^Content\d/.test(h));
    ok('it sits after Libraries', iLib >= 0 && iBand > iLib, `Libraries@${iLib}, band@${iBand}`);
    ok('and before Content', iContent < 0 || iBand < iContent, `band@${iBand}, Content@${iContent}`);
  }

  await toHalo();
  await page.screenshot({ path: `${OUT}/halo-advanced.png` });

  // Removing it takes the label away and nothing else.
  const before = await page.evaluate(() => document.querySelectorAll('.adv .row.band').length);
  await page.click('.adv .row.band button[aria-label="Remove"]');
  await page.waitForTimeout(350);
  const after = await page.evaluate(() => document.querySelectorAll('.adv .row.band').length);
  ok('removing a band removes it', after === before - 1, `${before} → ${after}`);

  console.log('errors:', errors.length ? errors.join('\n') : 'none');
  await browser.close();
  console.log(failures ? `\n${failures} failed.` : '\nall good');
  process.exit(failures ? 1 : 0);
})();
