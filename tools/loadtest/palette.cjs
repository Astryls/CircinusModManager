// The colour picker, and the one thing it absolutely has to get right.
//
// A feature that lets somebody repaint every token can put the window in a state where nothing
// is readable -- including the page holding the button that undoes it. So the assertions here
// are mostly about the way out, and the hardest of them loads a deliberately unreadable
// palette out of localStorage, the way a bad save would come back on the next launch.
//
//   npm run build && npx vite preview --port 4173 --strictPort
//   node tools/loadtest/palette.cjs [outdir]
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4173';
const OUT = process.argv[2] || '/tmp/palette';

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}

const openSettings = async (p) => {
  await p.evaluate(() => [...document.querySelectorAll('.rail .nav button')].find((b) => b.textContent.trim().startsWith('Settings'))?.click());
  await p.waitForTimeout(600);
};

/** The computed value of a token on <html>, as the window is actually drawing it. */
const token = (p, name) => p.evaluate((n) => getComputedStyle(document.documentElement).getPropertyValue(n).trim(), name);

(async () => {
  const browser = await chromium.launch();
  const errors = [];
  const page = await browser.newPage({ viewport: { width: 1500, height: 1000 }, colorScheme: 'dark' });
  page.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));

  await page.goto(BASE, { waitUntil: 'load' });
  await page.waitForSelector('.list .row[data-uid]', { timeout: 15000 });
  await openSettings(page);

  // ---- the card is there and says what it is ------------------------------------------------
  const card = await page.evaluate(() => {
    const c = [...document.querySelectorAll('.card')].find((x) => /^\s*Colours/.test(x.querySelector('h3')?.textContent ?? ''));
    return c ? { aside: c.querySelector('.aside')?.textContent.trim(), swatches: c.querySelectorAll('.sw input[type="color"]').length, groups: c.querySelectorAll('.pal').length } : null;
  });
  ok('Settings has a colour card', !!card, JSON.stringify(card));
  ok('with a swatch per editable token', (card?.swatches ?? 0) >= 18, String(card?.swatches));
  ok('grouped rather than one long wall', (card?.groups ?? 0) >= 4, String(card?.groups));
  ok('and it starts out saying nothing is changed', card?.aside === 'as shipped', card?.aside);

  // ---- changing one colour changes the window ------------------------------------------------
  const shippedBg = await token(page, '--bg');
  await page.evaluate(() => {
    const el = [...document.querySelectorAll('.card')].find((x) => /^\s*Colours/.test(x.querySelector('h3')?.textContent ?? ''));
    const input = el.querySelector('.sw input[type="color"]');
    input.value = '#201a2e';
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
  await page.waitForTimeout(400);
  const changedBg = await token(page, '--bg');
  ok('picking a colour applies it at once', changedBg !== shippedBg && /#201a2e/i.test(changedBg), `${shippedBg} -> ${changedBg}`);
  const aside = await page.evaluate(() => [...document.querySelectorAll('.card')].find((x) => /^\s*Colours/.test(x.querySelector('h3')?.textContent ?? ''))?.querySelector('.aside')?.textContent.trim());
  ok('and the card says how many are changed', /1 changed/.test(aside ?? ''), aside);
  await page.screenshot({ path: `${OUT}/palette-card.png` });

  // It has to survive a reload, or it is not a setting.
  await page.reload({ waitUntil: 'load' });
  await page.waitForSelector('.list .row[data-uid]', { timeout: 15000 });
  await page.waitForTimeout(400);
  ok('a chosen colour survives a reload', /#201a2e/i.test(await token(page, '--bg')), await token(page, '--bg'));

  // ---- an unreadable colour is refused, and says so ------------------------------------------
  // The floor is not a style opinion: it is what keeps the Reset button findable.
  await openSettings(page);
  const beforeText = await token(page, '--text');
  await page.evaluate(() => {
    const el = [...document.querySelectorAll('.card')].find((x) => /^\s*Colours/.test(x.querySelector('h3')?.textContent ?? ''));
    // The "Text" swatch, by its label rather than by position.
    const lab = [...el.querySelectorAll('.sw')].find((s) => s.querySelector('.nm')?.textContent.trim() === 'Text');
    const input = lab.querySelector('input[type="color"]');
    input.value = '#1f1b16';
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
  await page.waitForTimeout(400);
  ok('text that would vanish into the ground is refused', (await token(page, '--text')) === beforeText, `${beforeText} -> ${await token(page, '--text')}`);
  const warned = await page.evaluate(() => [...document.querySelectorAll('.card .warnline')].map((x) => x.textContent.trim()).join(' | '));
  ok('and the card says why, rather than silently snapping back', /too hard to read/i.test(warned), warned.slice(0, 80));

  // ---- the derived tint follows its accent ----------------------------------------------------
  await page.evaluate(() => {
    const el = [...document.querySelectorAll('.card')].find((x) => /^\s*Colours/.test(x.querySelector('h3')?.textContent ?? ''));
    const lab = [...el.querySelectorAll('.sw')].find((s) => s.querySelector('.nm')?.textContent.trim() === 'Attention');
    const input = lab.querySelector('input[type="color"]');
    input.value = '#3b7dd8';
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
  await page.waitForTimeout(400);
  const soft = await token(page, '--amber-soft');
  ok('changing an accent carries its soft tint with it', /59,\s*125,\s*216/.test(soft), soft);

  // ---- reset ------------------------------------------------------------------------------
  await page.evaluate(() => [...document.querySelectorAll('.hardreset')].find((b) => /Reset these colours/.test(b.textContent))?.click());
  await page.waitForTimeout(400);
  ok('Reset puts the shipped colour back', (await token(page, '--bg')) === shippedBg, `${await token(page, '--bg')} vs ${shippedBg}`);
  ok('and the tint with it', (await token(page, '--amber-soft')) !== soft, await token(page, '--amber-soft'));
  const afterAside = await page.evaluate(() => [...document.querySelectorAll('.card')].find((x) => /^\s*Colours/.test(x.querySelector('h3')?.textContent ?? ''))?.querySelector('.aside')?.textContent.trim());
  ok('and the card says so', afterAside === 'as shipped', afterAside);

  // The reset button must not be drawn in the colours it resets, or it is invisible exactly
  // when it is wanted. This is the assertion that stops somebody "tidying" it into tokens.
  const btn = await page.evaluate(() => {
    const b = [...document.querySelectorAll('.hardreset')][0];
    const c = getComputedStyle(b);
    return { bg: c.backgroundColor, fg: c.color, border: c.borderTopColor };
  });
  const tokens = await page.evaluate(() => {
    const cs = getComputedStyle(document.documentElement);
    return ['--bg', '--surface', '--surface-2', '--text', '--text-2', '--amber', '--red'].map((n) => cs.getPropertyValue(n).trim().toLowerCase());
  });
  const asRgb = (hex) => {
    const m = hex.replace('#', '');
    if (!/^[0-9a-f]{6}$/i.test(m)) return hex;
    return `rgb(${parseInt(m.slice(0, 2), 16)}, ${parseInt(m.slice(2, 4), 16)}, ${parseInt(m.slice(4, 6), 16)})`;
  };
  const tokenRgb = tokens.map(asRgb);
  ok('the Reset button borrows no palette colour', !tokenRgb.includes(btn.bg) && !tokenRgb.includes(btn.fg), JSON.stringify(btn));

  // ---- the case that matters: an unreadable palette already saved -----------------------------
  // What a bad save looks like on the next launch. If this came back applied, the window --
  // Settings included -- would be unreadable and the button above unreachable.
  const fresh = await browser.newPage({ viewport: { width: 1500, height: 1000 }, colorScheme: 'dark' });
  fresh.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));
  await fresh.addInitScript(() => {
    localStorage.setItem('circinus.palette', JSON.stringify({ dark: { bg: '#111111', text: '#131313', surface: '#121212', 'text-2': '#141414' } }));
  });
  await fresh.goto(BASE, { waitUntil: 'load' });
  await fresh.waitForSelector('.list .row[data-uid]', { timeout: 15000 });
  await fresh.waitForTimeout(600);
  const loadedBg = await token(fresh, '--bg');
  const loadedText = await token(fresh, '--text');
  ok('an unreadable saved palette is not applied', loadedBg !== '#111111' && loadedText !== '#131313', `${loadedBg} / ${loadedText}`);
  const ratio = await fresh.evaluate(() => {
    const cs = getComputedStyle(document.documentElement);
    const hex = (v) => { const m = v.trim().replace('#', ''); return [0, 2, 4].map((i) => parseInt(m.slice(i, i + 2), 16)); };
    const lum = (c) => { const f = (v) => { const x = v / 255; return x <= 0.03928 ? x / 12.92 : ((x + 0.055) / 1.055) ** 2.4; }; return 0.2126 * f(c[0]) + 0.7152 * f(c[1]) + 0.0722 * f(c[2]); };
    const [a, b] = [lum(hex(cs.getPropertyValue('--text'))), lum(hex(cs.getPropertyValue('--bg')))].sort((x, y) => y - x);
    return (a + 0.05) / (b + 0.05);
  });
  ok('so the window is still readable', ratio >= 3, `${ratio.toFixed(1)}:1`);
  await openSettings(fresh);
  const told = await fresh.evaluate(() => [...document.querySelectorAll('.card .warnline')].map((x) => x.textContent.trim()).join(' | '));
  ok('and the card explains what happened rather than pretending', /unreadable/i.test(told), told.slice(0, 90));
  await fresh.screenshot({ path: `${OUT}/palette-refused.png` });
  await fresh.close();

  console.log('errors:', errors.length ? errors.join('\n') : 'none');
  if (errors.length) failures += errors.length;
  await browser.close();
  console.log(failures ? `\n${failures} failed.` : '\nall good');
  process.exit(failures ? 1 : 0);
})();
