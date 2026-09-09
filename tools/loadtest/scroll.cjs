// Coming back to where you were.
//
// A user reported that scrolling halfway down the load order, going to look at something else,
// and coming back put them at the top again. App.svelte picks a view with `{#if}`, so leaving one
// destroys the component and everything local to it -- including the list's own scrollTop -- and
// returning mounts a fresh one at zero. The same thing happened in the Defs and Patches reports.
//
// This scrolls, leaves, returns, and asks the only question that matters: is the row that was
// under the top of the window still under the top of the window.
//
//   npm run build && npx vite preview --port 4173 --strictPort
//   node tools/loadtest/scroll.cjs [outdir]
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4173';
const OUT = process.argv[2] || '/tmp/scroll';

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
  page.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 200)); });

  await page.goto(BASE, { waitUntil: 'load' });
  await page.waitForSelector('.list .row', { timeout: 15000 });
  await page.waitForTimeout(400);

  const scroller = '.list';
  const topRow = () =>
    page.evaluate((sel) => {
      const el = document.querySelector(sel);
      if (!el) return null;
      const edge = el.getBoundingClientRect().top + 40;
      const row = [...el.querySelectorAll('.row[data-uid]')]
        .map((r) => ({ uid: r.getAttribute('data-uid'), y: r.getBoundingClientRect().top }))
        .filter((r) => r.y >= edge - 1)
        .sort((a, b) => a.y - b.y)[0];
      return row ? { uid: row.uid, y: Math.round(row.y), scrollTop: Math.round(el.scrollTop) } : null;
    }, scroller);

  // Halfway down a list long enough for "halfway" to mean something.
  const height = await page.evaluate((sel) => document.querySelector(sel)?.scrollHeight ?? 0, scroller);
  ok('the list is long enough to lose your place in', height > 1200, `${height}px`);
  await page.evaluate((sel) => { document.querySelector(sel).scrollTop = Math.round(document.querySelector(sel).scrollHeight / 2); }, scroller);
  await page.waitForTimeout(250);

  const before = await topRow();
  ok('a row is under the top of the window', !!before, before ? `${before.uid} at scrollTop ${before.scrollTop}` : 'none');

  // Away and back, the way the report described it.
  for (const view of ['Settings', 'HALO', 'Textures']) {
    await page.evaluate((v) => {
      const b = [...document.querySelectorAll('.rail .nav button, .nav button')].find((x) => x.textContent.trim().startsWith(v));
      if (b) b.click();
    }, view);
    await page.waitForTimeout(300);
    // The logo is the way back from anywhere, Settings included (it has no rail of its own).
    await page.click('.mark');
    await page.waitForSelector('.list .row', { timeout: 5000 });
    await page.waitForTimeout(350);

    const after = await topRow();
    ok(`back from ${view}: the same row is under the top`, !!after && !!before && after.uid === before.uid, after ? `${after.uid} at scrollTop ${after.scrollTop}` : 'nothing on screen');
    ok(`back from ${view}: and at the same height`, !!after && !!before && Math.abs(after.y - before.y) <= 2, after && before ? `${after.y} vs ${before.y}` : '');
  }

  await page.screenshot({ path: `${OUT}/scroll-restored.png` });

  // The reports do the same thing, with the pixel rather than a row.
  for (const [view, sel] of [['Defs', 'main.center'], ['Patches', 'main.center']]) {
    await page.evaluate((v) => {
      const b = [...document.querySelectorAll('.rail .nav button, .nav button')].find((x) => x.textContent.trim().startsWith(v));
      if (b) b.click();
    }, view);
    await page.waitForTimeout(500);
    const scrolled = await page.evaluate((s) => {
      const el = document.querySelector(s);
      if (!el || el.scrollHeight <= el.clientHeight + 40) return null;
      el.scrollTop = 220;
      return Math.round(el.scrollTop);
    }, sel);
    if (scrolled === null) {
      console.log(`skip  ${view}: nothing long enough to scroll in the mock`);
      continue;
    }
    await page.waitForTimeout(250);
    // The logo is the way back from anywhere, Settings included (it has no rail of its own).
    await page.click('.mark');
    await page.waitForTimeout(300);
    await page.evaluate((v) => {
      const b = [...document.querySelectorAll('.rail .nav button, .nav button')].find((x) => x.textContent.trim().startsWith(v));
      if (b) b.click();
    }, view);
    await page.waitForTimeout(400);
    const back = await page.evaluate((s) => Math.round(document.querySelector(s)?.scrollTop ?? -1), sel);
    ok(`${view}: the report comes back where it was`, Math.abs(back - scrolled) <= 2, `${back} vs ${scrolled}`);
  }

  console.log('errors:', errors.length ? errors.join('\n') : 'none');
  await browser.close();
  console.log(failures ? `\n${failures} failed.` : '\nall good');
  process.exit(failures ? 1 : 0);
})();
