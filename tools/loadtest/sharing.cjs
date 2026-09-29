// Consent, and the two things it must never get wrong.
//
// First: off unless answered. There is no build, no locale and no upgrade path where a load
// run leaves a machine whose owner has not been asked, so the card appears when the stored
// answer is absent and the switch behind it reads off.
//
// Second: an answer about an older set of fields is not an answer about this one. The privacy
// page promises "if what gets collected ever changes, you'll be asked again", and the only
// thing that makes that true is refusing to read a stale consentVersion as consent. That half
// is a Rust test (`an_old_yes_does_not_authorise_a_new_payload`); this file covers the window.
//
//   npm run build && npx vite preview --port 4173 --strictPort
//   node tools/loadtest/sharing.cjs [outdir]
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4173';
const OUT = process.argv[2] || '/tmp/sharing';

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}
const sharingSection = () => {
  const sec = [...document.querySelectorAll('.card')].find((c) => /^\s*Sharing/.test(c.querySelector('h3')?.textContent ?? ''));
  return sec;
};
const openSettings = async (p) => {
  await p.evaluate(() => [...document.querySelectorAll('.rail .nav button, .nav button')].find((x) => x.textContent.trim().startsWith('Settings'))?.click());
  await p.waitForTimeout(500);
};

(async () => {
  const browser = await chromium.launch();
  const errors = [];

  // ---- never asked: the card appears, and nothing is on ---------------------------------------
  let page = await browser.newPage({ viewport: { width: 1500, height: 940 } });
  page.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));
  await page.goto(`${BASE}/?consent`, { waitUntil: 'load' });
  await page.waitForSelector('[aria-labelledby="sh-title"]', { timeout: 15000 });
  await page.waitForTimeout(400);
  const card = await page.evaluate(() => document.querySelector('[aria-labelledby="sh-title"]')?.closest('.dlg')?.textContent ?? '');
  ok('an install that was never asked is asked', card.length > 0);
  // A card that says "help us improve Circinus" collects consent without informing anybody.
  ok('the card says what is sent', /package ids/i.test(card), card.slice(0, 70));
  ok('and what never is', /File paths/.test(card));
  // The correction that matters. Listing "mod names: never sent" beside "your username: never
  // sent" reads as "the site cannot tell what I run", which is false -- anyone can read
  // brrainz.harmony, and resolving the real title from the Workshop id is deliberate. The card
  // has to say which of the two kinds of thing is actually protected.
  ok('it does not pretend the site cannot tell which mods you run', /can tell which mods you run/i.test(card), card.slice(0, 70));
  ok('and it is the load order that is withheld, not the mod identity', /order your mods load in/i.test(card));
  ok('and that reading is not sending', /only about sending/i.test(card));
  await page.screenshot({ path: `${OUT}/sharing-card.png` });
  await page.close();

  // ---- keeping it local --------------------------------------------------------------------
  page = await browser.newPage({ viewport: { width: 1500, height: 940 } });
  page.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));
  await page.goto(`${BASE}/?consent`, { waitUntil: 'load' });
  await page.waitForSelector('[aria-labelledby="sh-title"]', { timeout: 15000 });
  await page.evaluate(() => [...document.querySelectorAll('.dlg .ft .btn')].find((b) => /Keep it local/.test(b.textContent))?.click());
  await page.waitForTimeout(700);
  ok('answering no closes the card', !(await page.evaluate(() => !!document.querySelector('[aria-labelledby="sh-title"]'))));
  await openSettings(page);
  const off = await page.evaluate(() => {
    const sec = [...document.querySelectorAll('.card')].find((c) => /^\s*Sharing/.test(c.querySelector('h3')?.textContent ?? ''));
    return { on: sec?.querySelector('input[type=checkbox]')?.checked, text: sec?.textContent ?? '' };
  });
  ok('and Settings shows it off', off.on === false, String(off.on));
  ok('with the load card working regardless', /still read and still shown/.test(off.text));
  await page.close();

  // ---- already answered: not asked again, and the id is still reachable -----------------------
  page = await browser.newPage({ viewport: { width: 1500, height: 940 } });
  page.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));
  await page.goto(BASE, { waitUntil: 'load' });
  await page.waitForSelector('.list .row[data-uid]', { timeout: 15000 });
  await page.waitForTimeout(800);
  ok('an install that already answered is not asked again', !(await page.evaluate(() => !!document.querySelector('[aria-labelledby="sh-title"]'))));
  await openSettings(page);
  const shown = await page.evaluate(() => {
    const el = document.querySelector('#installid');
    const sec = [...document.querySelectorAll('.card')].find((c) => /^\s*Sharing/.test(c.querySelector('h3')?.textContent ?? ''));
    return { id: el ? el.value : '', readonly: el ? el.readOnly : null, text: sec?.textContent ?? '' };
  });
  // The id is what a player needs in order to ask for a deletion, and needing it does not
  // depend on still sharing.
  ok('the install id is shown even with sharing off', shown.id.length > 0, shown.id);
  ok('and cannot be edited by hand', shown.readonly === true);
  ok('and says it is separate from the analyzer', /cannot be tied to the same machine/.test(shown.text));
  await page.screenshot({ path: `${OUT}/sharing-settings.png` });

  // ---- turning it on from Settings -------------------------------------------------------------
  await page.evaluate(() => {
    const sec = [...document.querySelectorAll('.card')].find((c) => /^\s*Sharing/.test(c.querySelector('h3')?.textContent ?? ''));
    sec?.querySelector('input[type=checkbox]')?.click();
  });
  await page.waitForTimeout(700);
  const nowOn = await page.evaluate(() => {
    const sec = [...document.querySelectorAll('.card')].find((c) => /^\s*Sharing/.test(c.querySelector('h3')?.textContent ?? ''));
    return sec?.querySelector('input[type=checkbox]')?.checked;
  });
  ok('the switch turns it on', nowOn === true);

  console.log('errors:', errors.length ? errors.join('\n') : 'none');
  if (errors.length) failures += errors.length;
  await browser.close();
  console.log(failures ? `\n${failures} failed.` : '\nall good');
  process.exit(failures ? 1 : 0);
})();
