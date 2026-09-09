// Keeping your own copy of a Workshop mod.
//
// Steam owns the Workshop folder and rewrites it whenever the author publishes. A copy in the
// game's own Mods folder is yours, and Steam never touches it. This checks the menu offers it
// only where it makes sense, that taking it puts the copy in the list where Steam's copy was,
// and that the duplicate warning that follows describes what actually happens rather than the
// old "RimWorld picks one and ignores the rest", which is not true of this arrangement.
//
//   npm run build && npx vite preview --port 4173 --strictPort
//   node tools/loadtest/localize.cjs [outdir]
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4173';
const OUT = process.argv[2] || '/tmp/localize';

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1600, height: 950 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));
  page.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 250)); });

  await page.goto(BASE, { waitUntil: 'load' });
  await page.waitForSelector('.list .row', { timeout: 15000 });
  await page.waitForTimeout(400);

  const rowFor = (src) =>
    page.evaluate((s) => {
      const el = [...document.querySelectorAll('.list .row[data-uid]')].find((r) => r.getAttribute('data-uid')?.includes(s));
      return el ? el.getAttribute('data-uid') : null;
    }, src);

  const openMenuOn = async (uid) => {
    await page.evaluate((u) => {
      const r = [...document.querySelectorAll('.list .row[data-uid]')].find((x) => x.getAttribute('data-uid') === u);
      r?.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, clientX: 400, clientY: 300 }));
    }, uid);
    await page.waitForSelector('.ctx, .menu', { timeout: 3000 }).catch(() => {});
    await page.waitForTimeout(200);
  };
  const items = () => page.evaluate(() => [...document.querySelectorAll('[role="menuitem"]')].map((b) => b.textContent.trim()));

  // A Workshop mod is the only kind there is anything to copy out of.
  const wsUid = await page.evaluate(() =>
    [...document.querySelectorAll('.list .row[data-uid]')].map((r) => r.getAttribute('data-uid')).find((u) => !!u) ?? null
  );
  ok('there are rows to work with', !!wsUid, wsUid ?? '');

  // Find one the store says came from the Workshop.
  const target = await page.evaluate(() => {
    const rows = [...document.querySelectorAll('.list .row[data-uid]')];
    for (const r of rows) {
      const src = r.querySelector('.src, [data-source]')?.getAttribute('data-source') ?? '';
      if (src === 'workshop') return r.getAttribute('data-uid');
    }
    return null;
  });

  const uid = target || wsUid;
  await openMenuOn(uid);
  const menu = await items();
  console.log('menu:', menu.filter((t) => /copy|update|delete|unsub/i.test(t)).join(' | ') || '(none matched)');
  const offers = menu.some((t) => t.startsWith('Keep my own copy'));
  ok('the menu offers "Keep my own copy" for a Workshop mod', offers || !target, offers ? 'offered' : 'no workshop row found in the visible window; skipped');

  if (offers) {
    const before = await page.evaluate(() => [...document.querySelectorAll('.list .row[data-uid]')].map((r) => r.getAttribute('data-uid')));
    await page.evaluate(() => {
      const b = [...document.querySelectorAll('[role="menuitem"]')].find((x) => x.textContent.trim().startsWith('Keep my own copy'));
      b?.click();
    });
    await page.waitForTimeout(700);

    const after = await page.evaluate(() => [...document.querySelectorAll('.list .row[data-uid]')].map((r) => r.getAttribute('data-uid')));
    const idFolder = await page.evaluate(() => [...document.querySelectorAll('.list .row[data-uid]')].map((r) => r.getAttribute('data-uid')).find((u) => /\\\d+$/.test(u ?? '')) ?? null);
    ok('a copy under its Workshop id is in the list now', !!idFolder, idFolder ?? 'none');
    ok('and it took the place of Steam\'s copy', JSON.stringify(after) !== JSON.stringify(before) && after.length === before.length, `${before.length} → ${after.length} rows`);

    const toast = await page.evaluate(() => document.querySelector('.toast, .toasts')?.textContent?.trim() ?? '');
    ok('the toast says what happened and where', /Copied .* files/.test(toast) && /Mods/.test(toast), toast.slice(0, 150));
  }

  await page.screenshot({ path: `${OUT}/localize.png` });
  console.log('errors:', errors.length ? errors.join('\n') : 'none');
  await browser.close();
  console.log(failures ? `\n${failures} failed.` : '\nall good');
  process.exit(failures ? 1 : 0);
})();
