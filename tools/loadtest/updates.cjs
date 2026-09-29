// Force update: who may be offered one, and where the fresh copy goes.
//
// Two reported bugs, both of which the window is the visible half of.
//
// **A dev build is not a download.** Mods track `About/PublishedFileId.txt` in their own
// repositories, so a developer's build of one carried the Workshop id and was classified as a
// SteamCMD download: listed as permanently out of date, and offered a Force update that would
// have written `Mods/<id>` beside it. Two folders, one packageId, and RimWorld picks one. So
// the offer has to be absent on a local folder, and the reason it is absent has to be the
// source rather than the id -- which is why this file checks a local mod that HAS an id.
//
// **Update all must not queue what Steam already installed.** What changed lists updates Steam
// has already applied; its own hint says so. The button used to queue every changed Workshop
// mod, which on one report sent 32 mods through SteamCMD when 31 were already current, and the
// only result was a set of copies in Mods that the game loaded instead of Steam's. The mock
// makes four Workshop mods change and puts two of them on the update list, so a button offering
// four is the bug and a button offering two is the fix.
//
//   npm run build && npx vite preview --port 4173 --strictPort
//   node tools/loadtest/updates.cjs [outdir]
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4173';
const OUT = process.argv[2] || '/tmp/updates';

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}

/** Select a mod by searching for it, then reading the inspector. */
async function select(p, needle) {
  await p.fill('#search', needle);
  await p.waitForTimeout(500);
  await p.evaluate(() => document.querySelector('.list .row[data-uid]')?.click());
  await p.waitForTimeout(500);
  return p.evaluate(() => {
    const insp = document.querySelector('aside.inspector');
    if (!insp) throw new Error('the inspector is not on screen');
    return [...insp.querySelectorAll('.btn')].map((b) => ({ text: b.textContent.trim(), title: b.getAttribute('title') || '' }));
  });
}

(async () => {
  const browser = await chromium.launch();
  const errors = [];
  const page = await browser.newPage({ viewport: { width: 1500, height: 940 }, colorScheme: 'dark' });
  page.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));

  await page.goto(BASE, { waitUntil: 'load' });
  await page.waitForSelector('.list .row[data-uid]', { timeout: 15000 });
  await page.waitForTimeout(800);

  // ---- Update all offers only what the check found ---------------------------------------------
  await page.evaluate(() => [...document.querySelectorAll('button')].find((x) => /What changed/.test(x.textContent))?.click());
  await page.waitForSelector('.dlg', { timeout: 8000 });
  const foot = await page.evaluate(() => document.querySelector('.dlg .ft')?.textContent.trim() ?? '');
  // Four changed Workshop/SteamCMD mods in the mock; two of them on the update list.
  const changed = await page.evaluate(() => [...document.querySelectorAll('.dlg .row .ib[title*="Re-download"]')].length);
  ok('What changed still offers each mod its own re-download', changed >= 1, String(changed));
  ok('but Update all counts only the ones the check found behind', /Update all 2\b/.test(foot), foot);
  ok('and does not offer every changed Workshop mod', !/Update all [4-9]/.test(foot) && !/Re-download all/.test(foot), foot);
  const tip = await page.evaluate(() => [...document.querySelectorAll('.dlg .ft .btn')].find((b) => /Update all/.test(b.textContent))?.getAttribute('title') ?? '');
  ok('and says why the number is smaller than the list', /already/i.test(tip), tip.slice(0, 60));
  await page.screenshot({ path: `${OUT}/updates-changed.png` });
  await page.keyboard.press('Escape');
  await page.waitForTimeout(400);

  // ---- a local folder with a Workshop id is not offered one --------------------------------------
  // Character Editor is `local` in the mock and carries publishedFileId 1554146052. Under the
  // old rule that combination was treated as a SteamCMD download.
  await page.evaluate(() => [...document.querySelectorAll('.tabs button, .seg button')].find((b) => /^All\b/.test(b.textContent.trim()))?.click());
  await page.waitForTimeout(400);
  const local = await select(page, 'charactereditor');
  ok('a local folder is not offered a Force update', !local.some((b) => /Force update|Re-download/i.test(b.text)), local.map((b) => b.text).join(' | '));

  // ---- a subscribed mod is, and is told where the copy lands -------------------------------------
  const ws = await select(page, 'rocketman');
  const force = ws.find((b) => /Force update/i.test(b.text));
  ok('a subscribed mod is offered one', !!force, ws.map((b) => b.text).join(' | '));
  ok("and is told it replaces Steam's own copy", /Steam's own copy/.test(force?.title ?? ''), force?.title ?? '');

  // ---- a mod Circinus downloaded is replaced in Mods ----------------------------------------------
  const cmd = await select(page, 'combatextended');
  const force2 = cmd.find((b) => /Force update/i.test(b.text));
  ok('so is one Circinus downloaded', !!force2, cmd.map((b) => b.text).join(' | '));
  ok('and that one says Mods, not Steam', /Mods folder/.test(force2?.title ?? ''), force2?.title ?? '');
  await page.fill('#search', '');
  await page.waitForTimeout(400);

  // ---- the Downloads view says the same thing -----------------------------------------------------
  await page.evaluate(() => [...document.querySelectorAll('.rail .nav button, .nav button')].find((x) => x.textContent.trim().startsWith('Downloads'))?.click());
  await page.waitForTimeout(700);
  const dl = await page.evaluate(() => document.querySelector('main.center')?.textContent ?? '');
  ok('the update list says where a subscribed mod would be replaced', /replace Steam's copy in place/.test(dl), dl.slice(0, 80));
  ok('and where a downloaded one would be', /replace it in Mods/.test(dl));
  await page.screenshot({ path: `${OUT}/updates-downloads.png` });

  console.log('errors:', errors.length ? errors.join('\n') : 'none');
  if (errors.length) failures += errors.length;
  await browser.close();
  console.log(failures ? `\n${failures} failed.` : '\nall good');
  process.exit(failures ? 1 : 0);
})();
