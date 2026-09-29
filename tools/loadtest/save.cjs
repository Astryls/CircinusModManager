// The save confirmation, and the two things it has to get right.
//
// First, that it describes the change rather than remembering it. `dirty` is a sticky flag --
// it goes true on the first edit and never goes back -- so the Save button is lit after you
// activate a mod and deactivate it again. The dialog compares what would be written against
// what is on disk, so it is the one thing in the app that can say "nothing to write", and this
// asserts it actually does.
//
// Second, that the kinds stay apart. Activated and deactivated are opposite risks and a flat
// list of "12 changes" hides which is which; the removals are the ones that carry the colour.
//
//   npm run build && npx vite preview --port 4173 --strictPort
//   node tools/loadtest/save.cjs [outdir]
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4173';
const OUT = process.argv[2] || '/tmp/save';

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}
const openSave = async (page) => {
  await page.evaluate(() => [...document.querySelectorAll('.acts .btn, .btn')].find((b) => /^\s*Save\s*$/.test(b.textContent))?.click());
  await page.waitForSelector('[aria-labelledby="save-title"]', { timeout: 8000 });
  await page.waitForTimeout(250);
};
const close = async (page) => {
  await page.keyboard.press('Escape');
  await page.waitForTimeout(250);
};

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1500, height: 940 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));
  page.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 200)); });

  await page.goto(BASE, { waitUntil: 'load' });
  await page.waitForSelector('.list .row[data-uid]', { timeout: 15000 });
  await page.waitForTimeout(500);

  // ---- an untouched list has nothing to write ---------------------------------------------
  // The Save button may be disabled here, which is itself correct; force the dialog open the
  // way the keyboard would so the empty state is still exercised.
  await page.evaluate(() => (window.__c ??= {}));
  await page.keyboard.press('Control+s');
  await page.waitForTimeout(400);
  let seen = await page.evaluate(() => !!document.querySelector('[aria-labelledby="save-title"]'));
  if (seen) {
    const txt = await page.evaluate(() => document.querySelector('[aria-labelledby="save-title"]')?.closest('.dlg')?.textContent ?? '');
    ok('an unedited list says there is nothing to write', /Nothing to write/.test(txt), txt.slice(0, 120));
    await page.screenshot({ path: `${OUT}/save-nothing.png` });
    await close(page);
  } else {
    ok('the shortcut opens the confirmation', false, 'no dialog appeared');
  }

  // ---- deactivate one, and it has to be named ----------------------------------------------
  const removedName = await page.evaluate(() => {
    const row = [...document.querySelectorAll('.list .row[data-uid]')].find((r) => r.querySelector('.name'));
    row.click();
    return row.querySelector('.name')?.textContent.trim();
  });
  await page.waitForTimeout(200);
  await page.keyboard.press('Delete');
  await page.waitForTimeout(400);
  await page.keyboard.press('Control+s');
  await page.waitForSelector('[aria-labelledby="save-title"]', { timeout: 8000 });
  await page.waitForTimeout(300);

  const shown = await page.evaluate(() => {
    const d = document.querySelector('[aria-labelledby="save-title"]')?.closest('.dlg');
    return {
      text: d?.textContent ?? '',
      removedChips: [...d.querySelectorAll('.chip.removed')].map((c) => c.textContent.trim()),
      headings: [...d.querySelectorAll('h4')].map((h) => h.textContent.trim())
    };
  });
  ok('a deactivation is counted under its own heading', shown.headings.some((h) => /deactivated/i.test(h)), shown.headings.join(' | '));
  ok('and the mod is named, not just counted', shown.removedChips.length > 0, shown.removedChips.join(', '));
  ok('it no longer says there is nothing to write', !/Nothing to write/.test(shown.text));
  await page.screenshot({ path: `${OUT}/save-removed.png` });

  // The removal is the change that can lose you something, so it is the one carrying colour.
  const coloured = await page.evaluate(() => {
    const c = document.querySelector('.chip.removed');
    if (!c) return null;
    const s = getComputedStyle(c);
    const plain = document.querySelector('.chip:not(.removed)');
    return { removed: s.color, plain: plain ? getComputedStyle(plain).color : null };
  });
  ok('the removals are the ones that carry colour', !!coloured && coloured.removed !== coloured.plain, JSON.stringify(coloured));

  // ---- keep editing leaves the list alone ---------------------------------------------------
  await page.evaluate(() => [...document.querySelectorAll('.dlg .ft .btn')].find((b) => /Keep editing/.test(b.textContent))?.click());
  await page.waitForTimeout(300);
  ok('Keep editing closes without saving', !(await page.evaluate(() => !!document.querySelector('[aria-labelledby="save-title"]'))));

  console.log('errors:', errors.length ? errors.join('\n') : 'none');
  if (errors.length) failures += errors.length;
  await browser.close();
  console.log(failures ? `\n${failures} failed.` : '\nall good');
  process.exit(failures ? 1 : 0);
})();
