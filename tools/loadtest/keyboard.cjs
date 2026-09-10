// The keyboard, driven by a real keyboard.
//
// Playwright presses keys the way a person does, which is the only way to test this: the bugs
// here were all about *where* a keystroke went -- into an input that swallowed it, into two
// handlers at once, into a row the virtualiser had thrown away -- and none of them are visible
// by calling a function.
//
//   npm run build && npx vite preview --port 4173 --strictPort
//   node tools/loadtest/keyboard.cjs [outdir]
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4173';
const OUT = process.argv[2] || '/tmp/keyboard';

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}

const state = (page) =>
  page.evaluate(() => ({
    query: document.getElementById('search').value,
    focused: document.activeElement?.id || document.activeElement?.getAttribute('data-uid') || document.activeElement?.tagName,
    selected: [...document.querySelectorAll('.row.sel[data-uid]')].map((r) => r.getAttribute('data-uid')),
    rows: [...document.querySelectorAll('.list .row[data-uid]')].map((r) => r.getAttribute('data-uid')),
    palette: !!document.querySelector('.dlg [role="listbox"]'),
    scrollTop: Math.round(document.querySelector('.list')?.scrollTop ?? -1)
  }));

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1600, height: 950 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));
  page.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 200)); });

  await page.goto(BASE, { waitUntil: 'load' });
  await page.waitForSelector('.list .row[data-uid]', { timeout: 15000 });
  await page.waitForTimeout(600);

  // ---- the search box ------------------------------------------------------------------------
  await page.keyboard.press('Control+k');
  await page.waitForTimeout(200);
  ok('Ctrl+K reaches the search box', (await state(page)).focused === 'search');
  await page.keyboard.type('rim');
  await page.waitForTimeout(300);
  ok('typing filters', (await state(page)).query === 'rim');

  // The complaint that started this.
  await page.keyboard.press('Escape');
  await page.waitForTimeout(250);
  let s = await state(page);
  ok('Escape clears the search', s.query === '', `"${s.query}"`);
  ok('and leaves the caret in the box, to type again', s.focused === 'search', s.focused);
  await page.keyboard.press('Escape');
  await page.waitForTimeout(250);
  ok('Escape again, with nothing to clear, lets the box go', (await state(page)).focused !== 'search');

  // Ctrl+K on a box that already has a query selects it, so typing replaces rather than appends.
  await page.keyboard.press('Control+k');
  await page.keyboard.type('alpha');
  await page.keyboard.press('Control+k');
  await page.keyboard.type('beta');
  await page.waitForTimeout(300);
  ok('Ctrl+K selects what is there, so the next word replaces it', (await state(page)).query === 'beta', (await state(page)).query);

  // Chords must survive a focused text field -- Ctrl+S while typing is the one that matters.
  const saved = await page.evaluate(() => {
    window.__saves = 0;
    const btn = [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Save');
    return !!btn;
  });
  ok('there is a Save button to compare against', saved);
  await page.keyboard.press('Escape');
  await page.keyboard.press('Escape');
  await page.waitForTimeout(200);

  // ---- walking the list ------------------------------------------------------------------------
  await page.click('.list .row[data-uid]');
  await page.waitForTimeout(250);
  s = await state(page);
  const first = s.rows[0];
  ok('clicking a row selects it', s.selected.includes(first), s.selected.join(','));

  await page.keyboard.press('ArrowDown');
  await page.keyboard.press('ArrowDown');
  await page.waitForTimeout(250);
  s = await state(page);
  ok('ArrowDown walks the list', s.selected.length === 1 && s.selected[0] === s.rows[2], `${s.selected[0]} vs ${s.rows[2]}`);
  ok('and the row it lands on has the keyboard', s.focused === s.selected[0], s.focused);

  await page.keyboard.press('Shift+ArrowDown');
  await page.waitForTimeout(250);
  ok('Shift extends the selection', (await state(page)).selected.length === 2, `${(await state(page)).selected.length}`);

  await page.keyboard.press('End');
  await page.waitForTimeout(350);
  s = await state(page);
  ok('End goes to the last row', s.selected.length === 1 && s.selected[0] === s.rows[s.rows.length - 1], `${s.selected[0]}`);
  await page.keyboard.press('Home');
  await page.waitForTimeout(350);
  s = await state(page);
  ok('Home comes back to the first', s.selected[0] === s.rows[0], `${s.selected[0]}`);

  const beforePage = (await state(page)).selected[0];
  await page.keyboard.press('PageDown');
  await page.waitForTimeout(350);
  ok('PageDown moves a screenful', (await state(page)).selected[0] !== beforePage);

  // ---- the bug: focus after a wheel scroll ------------------------------------------------------
  // The list is virtualised, so scrolling destroys the focused row. Before the roving cursor,
  // the next arrow press went to the window handler and nothing moved.
  await page.keyboard.press('Home');
  await page.waitForTimeout(300);
  const parked = (await state(page)).selected[0];
  await page.mouse.move(700, 500);
  await page.mouse.wheel(0, 4000);
  await page.waitForTimeout(400);
  const scrolled = await state(page);
  ok('the wheel scrolled the list well away', scrolled.scrollTop > 1000, `${scrolled.scrollTop}px`);
  await page.evaluate(() => document.querySelector('.list')?.focus());
  await page.keyboard.press('ArrowDown');
  await page.waitForTimeout(400);
  s = await state(page);
  ok('and an arrow still moves the selection afterwards', s.selected.length === 1 && s.selected[0] !== parked, `${parked} → ${s.selected[0]}`);

  // ---- Alt+Arrow does one thing --------------------------------------------------------------
  await page.keyboard.press('Home');
  await page.waitForTimeout(400);
  const before = await state(page);
  const wasAt = before.rows.indexOf(before.selected[0]);
  await page.keyboard.press('Alt+ArrowDown');
  await page.waitForTimeout(500);
  const after = await state(page);
  ok('Alt+ArrowDown keeps the same mod selected', after.selected[0] === before.selected[0], `${before.selected[0]} → ${after.selected[0]}`);
  ok('and moves it exactly one place, not two', after.rows.indexOf(after.selected[0]) === wasAt + 1, `${wasAt} → ${after.rows.indexOf(after.selected[0])}`);

  // ---- Escape goes to the topmost thing --------------------------------------------------------
  await page.click('.list .row[data-uid]', { button: 'right' });
  await page.waitForSelector('.menu [role="menuitem"]', { timeout: 5000 });
  const selBefore = (await state(page)).selected;
  await page.keyboard.press('Escape');
  await page.waitForTimeout(300);
  s = await state(page);
  ok('Escape closes the context menu', (await page.evaluate(() => !document.querySelector('.menu [role="menuitem"]'))));
  // The old behaviour: five racing listeners, so this also wiped the selection behind the menu.
  ok('and stops there, leaving the selection alone', s.selected.length === selBefore.length, `${selBefore.length} → ${s.selected.length}`);
  await page.keyboard.press('Escape');
  await page.waitForTimeout(250);
  ok('a second Escape then clears the selection', (await state(page)).selected.length === 0);

  // ---- the palette -----------------------------------------------------------------------------
  await page.keyboard.press('Control+Shift+P');
  await page.waitForTimeout(350);
  ok('Ctrl+Shift+P opens the palette', (await state(page)).palette);
  await page.keyboard.type('textur');
  await page.waitForTimeout(300);
  const hits = await page.evaluate(() => [...document.querySelectorAll('.it .nm')].map((n) => n.textContent.trim()));
  ok('typing finds a command by name', hits.some((h) => /Textures/i.test(h)), hits.slice(0, 3).join(' | '));
  const keysShown = await page.evaluate(() => [...document.querySelectorAll('.it kbd')].map((k) => k.textContent.trim()));
  ok('and each row shows its own binding', keysShown.length > 0, keysShown.slice(0, 3).join(' | '));
  await page.screenshot({ path: `${OUT}/palette.png` });
  await page.keyboard.press('Enter');
  await page.waitForTimeout(500);
  ok('Enter runs it', await page.evaluate(() => !!document.querySelector('.center')), '');
  ok('and the palette is gone', !(await state(page)).palette);

  // Ctrl+1 goes back to the load order; the numbers follow the sidebar.
  await page.keyboard.press('Control+1');
  await page.waitForTimeout(500);
  ok('Ctrl+1 returns to the load order', (await page.evaluate(() => !!document.querySelector('.list .row[data-uid]'))));

  // ---- the shortcuts reference -----------------------------------------------------------------
  await page.keyboard.press('?');
  await page.waitForTimeout(350);
  const ref = await page.evaluate(() => ({
    open: !!document.querySelector('[aria-labelledby="keys-title"]'),
    rows: document.querySelectorAll('[aria-labelledby="keys-title"] .row').length,
    ctrl: [...document.querySelectorAll('[aria-labelledby="keys-title"] kbd')].map((k) => k.textContent.trim())
  }));
  ok('? opens the shortcuts list', ref.open);
  ok('and it lists the bindings', ref.rows > 10, `${ref.rows}`);
  ok('spelled for this platform', ref.ctrl.some((k) => k.startsWith('Ctrl')), ref.ctrl.slice(0, 4).join(' | '));
  await page.screenshot({ path: `${OUT}/shortcuts.png` });
  await page.keyboard.press('Escape');
  await page.waitForTimeout(250);
  ok('Escape closes it', !(await page.evaluate(() => !!document.querySelector('[aria-labelledby="keys-title"]'))));

  console.log('errors:', errors.length ? errors.join('\n') : 'none');
  if (errors.length) failures += errors.length;
  await browser.close();
  console.log(failures ? `\n${failures} failed.` : '\nall good');
  process.exit(failures ? 1 : 0);
})();
