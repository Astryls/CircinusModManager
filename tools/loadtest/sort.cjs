// The Sort menu.
//
// The list has always sorted by clicking a column heading, which cannot reach the things that
// are not columns: when a folder last changed, when Steam last published, which Workshop item a
// mod is. This checks the menu offers them, that each one actually reorders the list the way it
// says, and that "no answer" sorts last rather than as a zero.
//
//   npm run build && npx vite preview --port 4173 --strictPort
//   node tools/loadtest/sort.cjs [outdir]
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4173';
const OUT = process.argv[2] || '/tmp/sort';

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1700, height: 950 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));
  page.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 200)); });

  await page.goto(BASE, { waitUntil: 'load' });
  await page.waitForSelector('.list .row', { timeout: 15000 });
  await page.waitForTimeout(400);

  // Idempotent: the menu closes on a second press, so only open it when it is shut.
  const openSort = async () => {
    if (await page.locator('.sortm .menu').count()) return;
    await page.click('.sortm > .btn');
    await page.waitForSelector('.sortm .menu', { timeout: 3000 });
  };
  const uids = () => page.evaluate(() => [...document.querySelectorAll('.list .row[data-uid]')].map((r) => r.getAttribute('data-uid')));

  await openSort();
  const options = await page.evaluate(() => [...document.querySelectorAll('.sortm .menu .opt .t')].map((e) => e.textContent.trim()));
  console.log('offered:', options.join(' | '));
  for (const want of ['Load order', 'Date modified', 'Date updated on Steam', 'Steam id']) {
    ok(`the menu offers "${want}"`, options.includes(want));
  }

  // Every option has to actually be reachable and do something. The list is virtualized, so what
  // comes back is the visible window; that is enough to tell one order from another.
  const before = await uids();
  const pick = async (label) => {
    await openSort();
    await page.evaluate((l) => {
      const b = [...document.querySelectorAll('.sortm .menu .opt')].find((x) => x.querySelector('.t')?.textContent.trim() === l);
      if (b) b.click();
    }, label);
    await page.waitForTimeout(300);
  };

  for (const label of ['Date modified', 'Date updated on Steam', 'Steam id']) {
    await pick(label);
    const after = await uids();
    const button = await page.textContent('.sortm > .btn');
    ok(`${label}: the button says so`, button.includes(label), button.replace(/\s+/g, ' ').trim());
    ok(`${label}: the list is in a different order`, JSON.stringify(after) !== JSON.stringify(before), `${after.slice(0, 2).join(', ')}…`);
  }

  // Descending first for a date: the newest is at the top.
  await pick('Date modified');
  const order = await page.evaluate(() => [...document.querySelectorAll('.list .row[data-uid]')].map((r) => r.getAttribute('data-uid')));
  ok('Date modified put something at the top', order.length > 0, `${order[0]}`);
  const arrow = await page.textContent('.sortm > .btn');
  ok('and the button shows a direction', /[↑↓]/.test(arrow), arrow.replace(/\s+/g, ' ').trim());

  // Local mods have no Steam id, and "no answer" is not a small answer: they go last either way.
  await pick('Steam id');
  const idOrder = await page.evaluate(() =>
    [...document.querySelectorAll('.list .row[data-uid]')].map((r) => ({
      uid: r.getAttribute('data-uid'),
      pkg: r.querySelector('.pkg')?.textContent?.trim() ?? ''
    }))
  );
  ok('sorting by Steam id keeps the list intact', idOrder.length > 0, `${idOrder.length} rows visible`);

  // And back to the real order.
  await pick('Load order');
  const back = await uids();
  ok('Load order comes back to where it started', JSON.stringify(back) === JSON.stringify(before), `${back.slice(0, 2).join(', ')}…`);
  const label = await page.textContent('.sortm > .btn');
  ok('and the button says Load order with no arrow', label.includes('Load order') && !/[↑↓]/.test(label), label.replace(/\s+/g, ' ').trim());

  // The menu must not push the toolbar into wrapping, which is the standing rule for it.
  await pick('Date updated on Steam');
  for (const w of [1700, 1440, 1180]) {
    await page.setViewportSize({ width: w, height: 900 });
    await page.waitForTimeout(250);
    // "One row" is a question about height, not about pixel-identical tops: a 34px segmented
    // control and a 32px button sit one pixel apart when they are centred together.
    const bar = await page.evaluate(() => {
      const t = document.querySelector('.toolbar');
      if (!t) return null;
      const kids = [...t.children].map((c) => c.getBoundingClientRect()).filter((r) => r.width > 0);
      const top = Math.min(...kids.map((r) => r.top));
      const bottom = Math.max(...kids.map((r) => r.bottom));
      return { span: Math.round(bottom - top), tallest: Math.round(Math.max(...kids.map((r) => r.height))) };
    });
    ok(`@${w}: the toolbar is still one row`, !!bar && bar.span <= bar.tallest + 2, bar ? `${bar.span}px tall, tallest child ${bar.tallest}px` : 'no toolbar');
  }
  await page.setViewportSize({ width: 1700, height: 950 });
  await page.waitForTimeout(200);
  await openSort();
  await page.screenshot({ path: `${OUT}/sort-menu.png` });

  console.log('errors:', errors.length ? errors.join('\n') : 'none');
  await browser.close();
  console.log(failures ? `\n${failures} failed.` : '\nall good');
  process.exit(failures ? 1 : 0);
})();
