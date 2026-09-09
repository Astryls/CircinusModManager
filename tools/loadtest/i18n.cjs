// The catalogue, on screen.
//
// `t()` does three things that are easy to get quietly wrong: it fills `{name}` placeholders, it
// picks between the one and other forms of a count, and it returns the key when a string is
// missing. The first two show up as wrong words rather than as errors, so they are worth reading
// off a real window; the third is the reason a missing key is loud instead of blank.
//
//   npm run build && npx vite preview --port 4173 --strictPort
//   node tools/loadtest/i18n.cjs [outdir]
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4173';
const OUT = process.argv[2] || '/tmp/i18n';

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

  // No key should ever reach the screen. A dotted lowercase token with no spaces is what a
  // missing entry looks like, and it is the one failure mode of the fallback.
  const leaked = await page.evaluate(() =>
    [...document.querySelectorAll('.toolbar *')]
      .flatMap((el) => [...el.childNodes])
      .filter((n) => n.nodeType === 3)
      .map((n) => n.nodeValue.trim())
      .filter((s) => /^[a-z][a-z0-9]*(\.[a-z][A-Za-z0-9]*){2,}$/.test(s))
  );
  ok('no untranslated key is on screen', leaked.length === 0, leaked.join(', ') || 'none');

  // Interpolation: "Show: All mods" and "Sort: Load order" are one string each with a {what}.
  const show = await page.textContent('.filter > .btn');
  ok('the Show button interpolates its current choice', /Show:\s*\S/.test(show.replace(/\s+/g, ' ')), show.replace(/\s+/g, ' ').trim());
  const sort = await page.textContent('.sortm > .btn');
  ok('the Sort button interpolates its current choice', /Sort:\s*Load order/.test(sort.replace(/\s+/g, ' ')), sort.replace(/\s+/g, ' ').trim());

  // Plurals: the Apply button counts moves, and English needs both forms.
  await page.evaluate(() => {
    const b = [...document.querySelectorAll('.toolbar button')].find((x) => x.textContent.includes('Sort with HALO'));
    b?.click();
  });
  await page.waitForTimeout(900);
  const apply = await page.evaluate(() => {
    const b = [...document.querySelectorAll('.toolbar button')].find((x) => x.textContent.trim().startsWith('Apply'));
    return b ? { text: b.textContent.replace(/\s+/g, ' ').trim(), title: b.title } : null;
  });
  if (!apply) {
    console.log('skip  the mock produced no moves to apply');
  } else {
    const n = Number((apply.text.match(/(\d+)\s+moves?/) ?? [])[1] ?? -1);
    ok('the Apply button counts the moves', n >= 0, apply.text);
    const wantPlural = n !== 1;
    ok(`and uses the ${wantPlural ? 'plural' : 'singular'} form`, wantPlural ? /\bmoves\b/.test(apply.text) : /\bmove\b/.test(apply.text) && !/\bmoves\b/.test(apply.text), apply.text);
    ok('its tooltip agrees with its label', apply.title.includes(String(n)) && (wantPlural ? /moves/.test(apply.title) : /move\b/.test(apply.title)), apply.title);
  }

  await page.screenshot({ path: `${OUT}/i18n-toolbar.png` });
  console.log('errors:', errors.length ? errors.join('\n') : 'none');
  await browser.close();
  console.log(failures ? `\n${failures} failed.` : '\nall good');
  process.exit(failures ? 1 : 0);
})();
