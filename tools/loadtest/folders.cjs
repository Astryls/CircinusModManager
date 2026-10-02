// The four folders, and the difference between a path that is right and one that merely exists.
//
// All four have been user-selectable for a long time: Settings has Choose… and Auto for each,
// per instance, and `resolve_locations` applies an override over autodetection. What they could
// not do is tell you whether the folder you chose was the right one. The row printed the
// resolved path, or "not found", and nothing else -- so a game folder pointing at the Steam
// library root (which is exactly where a folder picker lands) read identically to one pointing
// at the game. The symptom was everything downstream: an empty mod list, a load order that
// would not save, a collection download that failed at the end.
//
// So each row now says what is in there, in a sentence somebody can check against what they
// believe is in there, and an unusable folder puts a banner at the top of the window rather
// than waiting to be found most of the way down Settings.
//
// What this asserts is that the two states are *distinguishable* and that the bad one is
// reachable without going looking. Geometry is not the point here; wording is. A check that
// only confirmed a path string was printed would have passed on every version of this card.
//
//   npm run build && npx vite preview --port 4173 --strictPort
//   node tools/loadtest/folders.cjs [outdir]
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4173';
const OUT = process.argv[2] || '/tmp/folders';

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}

/** The folder rows as the Settings card draws them. */
const rows = (page) =>
  page.evaluate(() => {
    const card = [...document.querySelectorAll('.settings .card')].find((c) => /Where RimWorld lives/.test(c.textContent));
    if (!card) return null;
    return [...card.querySelectorAll('.loc')].map((r) => {
      const chk = r.querySelector('.chk');
      const buttons = [...r.querySelectorAll('button')].map((b) => b.textContent.trim());
      return {
        label: r.querySelector('.lt b')?.textContent.trim() ?? '',
        path: r.querySelector('.path')?.textContent.trim() ?? '',
        says: chk?.textContent.replace(/\s+/g, ' ').trim() ?? '',
        mark: !!chk?.querySelector('.m'),
        bad: !!chk?.classList.contains('bad'),
        warn: !!chk?.classList.contains('warn'),
        // Both controls are the whole point of the card: choose one, or go back to automatic.
        canChoose: buttons.some((b) => /Choose/.test(b)),
        canAuto: buttons.some((b) => b === 'Auto'),
        // Nothing in a healthy row may carry colour: a tick in red is a tick people stop reading.
        colored: chk ? getComputedStyle(chk.querySelector('.m') ?? chk).color : ''
      };
    });
  });

const toSettings = async (page) => {
  await page.evaluate(() => {
    const b = [...document.querySelectorAll('.rail button, .dir button')].find((x) => /Settings/.test(x.textContent));
    if (b) b.click();
  });
  await page.waitForSelector('.settings .card', { timeout: 15000 });
  await page.waitForTimeout(300);
};

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1500, height: 950 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));
  page.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 200)); });

  // ---- a healthy install -------------------------------------------------------------------
  await page.goto(BASE, { waitUntil: 'load' });
  await page.waitForSelector('.stats .stat', { timeout: 15000 });
  await toSettings(page);
  let r = await rows(page);
  ok('the card lists all four folders', r?.length === 4, (r ?? []).map((x) => x.label).join(' | '));
  ok('and every one can be chosen by hand', r.every((x) => x.canChoose), r.filter((x) => !x.canChoose).map((x) => x.label).join(', '));
  // The defaults are kept: "Auto" only appears on a folder somebody has overridden, because
  // offering to undo a choice nobody made is a button that does nothing.
  ok('and an untouched folder offers no Auto, having nothing to undo', r.every((x) => !x.canAuto), r.filter((x) => x.canAuto).map((x) => x.label).join(', '));
  ok('each one says what is in it', r.every((x) => x.says.length > 20), r.map((x) => x.says.slice(0, 30)).join(' | '));
  // The sentence has to be checkable. A count or a version is a fact the reader can compare
  // against what they believe; "found" on its own is not.
  ok('and says it with a figure, not just a word', r.every((x) => /\d/.test(x.says)), r.map((x) => x.says).join(' | '));
  ok('and says whether it was found or set', r.every((x) => /Found automatically|Set by you/.test(x.says)), r[0]?.says);
  ok('a healthy row carries no mark', r.every((x) => !x.mark && !x.bad && !x.warn));
  const game = r.find((x) => /RimWorld folder/.test(x.label));
  ok('the game folder names the version it found', /1\.6\.4530/.test(game?.says ?? ''), game?.says);
  const cfg = r.find((x) => /Config folder/.test(x.label));
  ok('the config folder says how many mods the file lists', /ModsConfig\.xml/.test(cfg?.says ?? ''), cfg?.says);
  ok('and nothing is shouting on a healthy install', await page.evaluate(() => !document.querySelector('.banner.error')));
  await page.locator('.settings .card').first().screenshot({ path: `${OUT}/folders-ok.png` });

  // ---- present and wrong -------------------------------------------------------------------
  //
  // The whole reason this exists. Both of these paths are set, both look like paths, and the
  // old card would have printed them in exactly the ink it printed a correct one in.
  await page.goto(`${BASE}/?badpaths`, { waitUntil: 'load' });
  await page.waitForSelector('.stats .stat', { timeout: 15000 });
  await page.waitForTimeout(400);

  // Found before it is gone looking for: the banner is at the top of the list view, not three
  // screens down in Settings, which is where this could only be read before.
  const banner = await page.evaluate(() => {
    const b = [...document.querySelectorAll('.banner')].find((x) => /folder/i.test(x.textContent));
    if (!b) return null;
    return {
      text: b.textContent.replace(/\s+/g, ' ').trim(),
      error: b.classList.contains('error'),
      first: document.querySelector('.banner') === b,
      action: [...b.querySelectorAll('button')].map((x) => x.textContent.trim()),
      dismissable: !!b.querySelector('button.x')
    };
  });
  ok('a wrong folder puts a banner in front of you', !!banner, 'no banner mentioning a folder');
  ok('and it is an error, not a note', banner?.error === true);
  ok('and it is the first thing in the stack', banner?.first === true, banner?.text?.slice(0, 60));
  ok('and it names which folders and why', /RimWorld folder/.test(banner?.text ?? '') && /Config folder/.test(banner?.text ?? ''), banner?.text?.slice(0, 120));
  ok('and offers the one place that fixes it', (banner?.action ?? []).some((a) => /Set the folders/.test(a)), (banner?.action ?? []).join(' | '));
  // Everything else in the window can be lived with and put down. This cannot: closing it
  // leaves an application that is not reading the game, with nothing on screen saying so.
  ok('and cannot be dismissed', banner?.dismissable === false);
  await page.locator('.banner').first().screenshot({ path: `${OUT}/folders-banner.png` });

  await page.evaluate(() => {
    const b = [...document.querySelectorAll('.banner button')].find((x) => /Set the folders/.test(x.textContent));
    if (b) b.click();
  });
  await page.waitForSelector('.settings .card', { timeout: 15000 });
  await page.waitForTimeout(300);
  ok('and pressing it lands on the folders', await page.evaluate(() => /Where RimWorld lives/.test(document.querySelector('.settings .card')?.textContent ?? '')));

  r = await rows(page);
  const badGame = r.find((x) => /RimWorld folder/.test(x.label));
  const badCfg = r.find((x) => /Config folder/.test(x.label));
  const noMods = r.find((x) => /Local mods/.test(x.label));
  const ws = r.find((x) => /Workshop/.test(x.label));
  ok('the wrong game folder is marked wrong', badGame?.bad === true && badGame?.mark === true, badGame?.says);
  ok('and says what is missing from it', /Version\.txt/.test(badGame?.says ?? ''), badGame?.says);
  ok('and says what should be there instead', /Data/.test(badGame?.says ?? ''), badGame?.says);
  ok('and the path itself is marked too', await page.evaluate(() => {
    const card = [...document.querySelectorAll('.settings .card')].find((c) => /Where RimWorld lives/.test(c.textContent));
    return !!card.querySelector('.loc .path.bad');
  }));
  ok('a folder that has gone away says so', badCfg?.bad === true && /not there/.test(badCfg?.says ?? ''), badCfg?.says);
  ok('and both offer Auto, having been set by hand', badGame?.canAuto === true && badCfg?.canAuto === true);
  ok('and say they were set by hand', /Set by you/.test(badGame?.says ?? ''), badGame?.says);

  /* A missing Mods folder is a warning rather than an error, and the warning names the thing it
   * actually breaks. This is report #3 written down: a collection downloaded for thirteen
   * minutes and then failed per item because there was nowhere to put it. */
  ok('no Mods folder is a warning, not an error', noMods?.warn === true && noMods?.bad === false, noMods?.says);
  ok('and it names what that costs', /downloads/i.test(noMods?.says ?? ''), noMods?.says);

  /* And the folder that is fine stays quiet while two beside it are not. A card that goes amber
   * all over the moment anything is wrong is a card nobody can read for the one bad row. */
  ok('a folder that is fine carries no mark even next to two that are not', ws?.mark === false && !ws?.bad && !ws?.warn, ws?.says);
  await page.locator('.settings .card').first().screenshot({ path: `${OUT}/folders-bad.png` });

  console.log('errors:', errors.length ? errors.join('\n') : 'none');
  if (errors.length) failures += errors.length;
  await browser.close();
  console.log(failures ? `\n${failures} failed.` : '\nall good');
  process.exit(failures ? 1 : 0);
})();
