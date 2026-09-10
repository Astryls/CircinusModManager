// The Defs report, and the duplicate key that took it down.
//
// Svelte treats a repeated key in a keyed `{#each}` as fatal: the block throws, the boundary
// catches it, and the whole screen becomes `Error: .../e/each_key_duplicate` with a Try again
// button that tries the same thing again. The report keyed its overwrite chains on
// `(c.def + c.path)`, which identifies a chain right up until two mods overwrite the same field
// of the same def -- ordinary on a real load order, absent from a forty-mod mock. So the report
// was unreachable for anybody actually using it, and every check passed.
//
// The mock now carries that shape on purpose (two chains on ThingDef/Wall's graphicData/color),
// so this is a test rather than a hope.
//
//   npm run build && npx vite preview --port 4173 --strictPort
//   node tools/loadtest/defs.cjs [outdir]
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4173';
const OUT = process.argv[2] || '/tmp/defs';

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
  await page.waitForSelector('.list .row[data-uid]', { timeout: 15000 });
  await page.evaluate(() => [...document.querySelectorAll('.rail .nav button, .nav button')].find((x) => x.textContent.trim().startsWith('Defs'))?.click());
  await page.waitForTimeout(500);

  await page.evaluate(() => [...document.querySelectorAll('button')].find((b) => /Work out the merged defs|Work it out again/.test(b.textContent))?.click());
  await page.waitForSelector('.ch-row', { timeout: 10000 });
  await page.waitForTimeout(400);

  const shown = await page.evaluate(() => {
    const rows = [...document.querySelectorAll('.ch-row')];
    const keyOf = (r) => `${r.querySelector('.def b')?.textContent}|${r.querySelector('.path')?.textContent}`;
    const keys = rows.map(keyOf);
    return { rows: rows.length, keys, dupes: keys.filter((k, i) => keys.indexOf(k) !== i) };
  });
  ok('the report renders its chains', shown.rows > 5, `${shown.rows} rows`);

  // The point of the whole test: the mock contains rows that agree on def and path, and the
  // report has to draw both rather than throw.
  ok('two chains share a def and a path, as real data does', shown.dupes.length > 0, shown.dupes[0] ?? 'none — the mock lost the case this guards');
  ok('and the report survives it', !(await page.evaluate(() => document.body.textContent.includes('each_key_duplicate'))));
  ok('with no error boundary in sight', !(await page.evaluate(() => !!document.querySelector('.boundary, .crash') || document.body.textContent.includes('COULD NOT BE SHOWN'))));
  await page.screenshot({ path: `${OUT}/defs.png` });

  // Rows that agree on their contents still have to open independently.
  const opened = await page.evaluate(() => {
    const rows = [...document.querySelectorAll('.ch-row')];
    const keyOf = (r) => `${r.querySelector('.def b')?.textContent}|${r.querySelector('.path')?.textContent}`;
    const keys = rows.map(keyOf);
    const dupe = keys.find((k, i) => keys.indexOf(k) !== i);
    const pair = rows.filter((r) => keyOf(r) === dupe);
    pair[0].querySelector('button.row')?.click();
    return { pair: pair.length };
  });
  await page.waitForTimeout(300);
  const after = await page.evaluate(() => document.querySelectorAll('.ch-row.open').length);
  ok('two rows that read the same are still two rows', opened.pair === 2, `${opened.pair}`);
  ok('and opening one opens only one', after === 1, `${after} opened`);

  // Filtering must not leave a stale row open, since what is open is a position now.
  await page.evaluate(() => {
    const inp = [...document.querySelectorAll('input')].find((i) => /filter/i.test(i.placeholder || ''));
    if (inp) { inp.value = 'Autopistol'; inp.dispatchEvent(new Event('input', { bubbles: true })); }
  });
  await page.waitForTimeout(400);
  ok('filtering closes what was open', (await page.evaluate(() => document.querySelectorAll('.ch-row.open').length)) === 0);

  console.log('errors:', errors.length ? errors.join('\n') : 'none');
  if (errors.length) failures += errors.length;
  await browser.close();
  console.log(failures ? `\n${failures} failed.` : '\nall good');
  process.exit(failures ? 1 : 0);
})();
