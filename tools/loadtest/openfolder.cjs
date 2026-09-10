// Open folder means that folder.
//
// Every "Open folder" in the app called the opener plugin's `revealItemInDir`, which opens the
// folder *containing* a thing and selects it. That is the right gesture for a file — you asked
// about that file, and you want to see it among its neighbours — and the wrong one for a folder:
// asked for a mod's folder it opened the Mods folder with the mod highlighted, one level above
// what was asked for, which in a folder of five hundred mods is not a small difference.
//
// Outside Tauri both calls are no-ops, so they say what they would have done on the console.
// That is what this reads: press the button, and check the app asked to *open* the mod's own
// path rather than to reveal it.
//
// Worth knowing what this does NOT prove, because a version of it passed while the feature was
// broken for every user: the mock short-circuits before the call, so this checks the call site
// and never the call. `tools/commands-check.mjs` is what checks the other half.
//
//   npm run build && npx vite preview --port 4173 --strictPort
//   node tools/loadtest/openfolder.cjs
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4173';

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1600, height: 950 } });
  const said = [];
  page.on('console', (m) => { const t = m.text(); if (t.startsWith('[circinus] open ') || t.startsWith('[circinus] reveal ')) said.push(t.replace('[circinus] ', '')); });

  await page.goto(BASE, { waitUntil: 'load' });
  await page.waitForSelector('.list .row[data-uid]', { timeout: 15000 });
  await page.waitForTimeout(400);

  // The row the menu will be opened on, and the folder it should lead to.
  const row = await page.evaluate(() => {
    const r = document.querySelector('.list .row[data-uid]');
    return { uid: r.getAttribute('data-uid'), name: r.textContent.trim().slice(0, 30) };
  });
  ok('there is a mod to right-click', !!row.uid, `${row.name} at ${row.uid}`);

  await page.click('.list .row[data-uid]', { button: 'right' });
  await page.waitForSelector('.menu [role="menuitem"]', { timeout: 5000 });
  const item = await page.evaluate(() => {
    const b = [...document.querySelectorAll('.menu [role="menuitem"]')].find((x) => x.textContent.trim() === 'Open folder');
    if (!b) return null;
    b.click();
    return true;
  });
  ok('the menu offers Open folder', item === true);
  await page.waitForTimeout(400);

  ok('pressing it asked the file manager for something', said.length === 1, said.join(' / ') || 'nothing');
  ok('and that something is the mod\'s own folder, opened', said[0] === `open ${row.uid}`, said[0] ?? '');
  ok('rather than its parent with the mod selected', !said.some((s) => s.startsWith('reveal ')), said.join(' / '));

  // A file is the other case, and still reveals: the log is one file in a folder of many.
  said.length = 0;
  await page.evaluate(() => {
    const b = [...document.querySelectorAll('.rail .nav button, .nav button')].find((x) => x.textContent.trim().startsWith('Downloads'));
    if (b) b.click();
  });
  await page.waitForTimeout(500);
  const both = await page.evaluate(() => {
    const folder = [...document.querySelectorAll('button')].find((b) => b.textContent.trim() === 'Open folder');
    const file = [...document.querySelectorAll('button.lnk.mono')].find((b) => b.getAttribute('title') === 'Show in your file manager');
    folder?.click();
    file?.click();
    return { folder: !!folder, file: !!file };
  });
  await page.waitForTimeout(400);
  if (!both.folder || !both.file) {
    console.log('skip  Downloads: SteamCMD is not set up in the mock, so its two buttons are not there');
  } else {
    ok('the Downloads folder button opens the folder', said.some((s) => s.startsWith('open ')), said.join(' / '));
    ok('and the tool beside it, being a file, is still revealed', said.some((s) => s.startsWith('reveal ')), said.join(' / '));
  }

  console.log(said.length ? `\nasked for: ${said.join(', ')}` : '');
  await browser.close();
  console.log(failures ? `\n${failures} failed.` : '\nall good');
  process.exit(failures ? 1 : 0);
})();
