// The patch report, in four screens.
//
// Three complaints about it, all the same complaint: the tool answers four questions and shows
// all four answers stacked on one page, so the long ones were cut off at a hundred and twenty
// rows, the fourth was a thousand rows of scrolling from the first, and clicking a mod to look at
// it in the load order threw the scroll position away on the way back.
//
// So: a tab per question, every list complete, and each tab keeping its own place. This asks
// whether that is true — that the contested list holds every contested method rather than a
// hundred and twenty of them, that the method list draws rows where its scrollbar says they are,
// and that leaving for the load order and coming back lands on the same tab at the same pixel.
//
//   npm run build && npx vite preview --port 4173 --strictPort
//   node tools/loadtest/patches.cjs [outdir]
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4173';
const OUT = process.argv[2] || '/tmp/patches';

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}

const goto = (page, view) =>
  page.evaluate((v) => {
    const b = [...document.querySelectorAll('.rail .nav button, .nav button')].find((x) => x.textContent.trim().startsWith(v));
    if (b) b.click();
  }, view);

const tab = async (page, label) => {
  await page.evaluate((l) => {
    const b = [...document.querySelectorAll('.tabs button')].find((x) => x.textContent.trim().startsWith(l));
    if (b) b.click();
  }, label);
  await page.waitForTimeout(300);
};

/** The count printed on a tab button, which is what the tab claims to hold. */
const tabCount = (page, label) =>
  page.evaluate((l) => {
    const b = [...document.querySelectorAll('.tabs button')].find((x) => x.textContent.trim().startsWith(l));
    const n = b?.querySelector('.num')?.textContent ?? '';
    return Number(n.replace(/[^\d]/g, '')) || 0;
  }, label);

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1600, height: 950 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));
  page.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 200)); });

  await page.goto(BASE, { waitUntil: 'load' });
  await page.waitForSelector('.list .row', { timeout: 15000 });
  await goto(page, 'Patches');
  await page.waitForSelector('.tabs button', { timeout: 10000 });
  await page.waitForTimeout(500);

  // ---- the frame stays, the tab moves --------------------------------------------------------
  const frame = await page.evaluate(() => {
    const c = document.querySelector('main.center');
    const p = document.querySelector('.pane');
    return { centreScrolls: c ? c.scrollHeight > c.clientHeight + 2 : null, pane: !!p, tabs: document.querySelectorAll('.tabs button').length };
  });
  ok('there are five tabs', frame.tabs === 5, `${frame.tabs}`);
  ok('the frame itself does not scroll', frame.centreScrolls === false);
  ok('the tab underneath it does', frame.pane);

  // ---- every contested method, not the first hundred and twenty ------------------------------
  await tab(page, 'Two mods over one method');
  const claimed = await tabCount(page, 'Two mods over one method');
  const fights = await page.evaluate(() => document.querySelectorAll('.fights .fight').length);
  ok('the contested list is long enough to have been cut before', claimed > 120, `${claimed} contested`);
  ok('and every one of them is in the DOM', fights === claimed, `${fights} drawn of ${claimed}`);
  const apology = await page.evaluate(() => [...document.querySelectorAll('.hint')].some((h) => /more contested|narrow the filter/.test(h.textContent)));
  ok('with nothing apologising for a cut', !apology);
  await page.screenshot({ path: `${OUT}/patches-contested.png` });

  // ---- every patched method: virtualised, so the rows must line up with the scrollbar ---------
  await tab(page, 'Every patched method');
  const claimedM = await tabCount(page, 'Every patched method');
  const geom = await page.evaluate(() => {
    const v = document.querySelector('.vrows');
    return { h: v ? Math.round(v.getBoundingClientRect().height) : 0, drawn: document.querySelectorAll('.vrows .row').length };
  });
  ok('the method list is long enough to need a window', claimedM > 400, `${claimedM} methods`);
  ok('its height is the whole list, not the screenful', Math.abs(geom.h - claimedM * 33) <= 33, `${geom.h}px for ${claimedM} rows`);
  ok('and only a screenful is drawn', geom.drawn > 5 && geom.drawn < 120, `${geom.drawn} rows in the DOM`);

  // Scroll deep and ask whether the row drawn at the top is the row the offset says it is.
  const deep = await page.evaluate(() => {
    const p = document.querySelector('.pane');
    p.scrollTop = Math.round(p.scrollHeight * 0.6);
    return Math.round(p.scrollTop);
  });
  await page.waitForTimeout(300);
  const aligned = await page.evaluate(() => {
    const p = document.querySelector('.pane');
    const v = document.querySelector('.vrows');
    const rows = [...v.querySelectorAll('.row')];
    const edge = p.getBoundingClientRect().top;
    const onScreen = rows.filter((r) => { const b = r.getBoundingClientRect(); return b.bottom > edge && b.top < p.getBoundingClientRect().bottom; });
    // Every drawn row sits exactly where its inline offset puts it inside the full-height box.
    const vt = v.getBoundingClientRect().top;
    const off = rows.every((r) => Math.abs(r.getBoundingClientRect().top - vt - parseFloat(r.style.top)) <= 1);
    return { onScreen: onScreen.length, off, first: onScreen[0]?.textContent.trim().slice(0, 40) ?? '' };
  });
  ok('scrolled deep, rows are still on screen', aligned.onScreen > 5, `${aligned.onScreen} visible at ${deep}px, first "${aligned.first}"`);
  ok('and each is drawn at the offset it claims', aligned.off);
  // The column names follow the list down, so they have to still name the right column.
  const heads = await page.evaluate(() => {
    const p = document.querySelector('.pane');
    const h = [...document.querySelectorAll('.list .hdr > *')];
    const r = [...document.querySelectorAll('.vrows .row')][0].children;
    return { stuck: Math.round(document.querySelector('.list .hdr').getBoundingClientRect().top - p.getBoundingClientRect().top), off: h.map((c, i) => Math.round(c.getBoundingClientRect().left - r[i].getBoundingClientRect().left)) };
  });
  ok('the column names are still on screen at the top', heads.stuck >= 0 && heads.stuck < 80, `${heads.stuck}px into the pane`);
  ok('and each still sits over its column', heads.off.every((d) => Math.abs(d) <= 1), heads.off.join(', '));
  await page.screenshot({ path: `${OUT}/patches-methods.png` });

  // ---- per mod: an open row is not capped ----------------------------------------------------
  await tab(page, 'Per mod');
  const opened = await page.evaluate(() => {
    const rows = [...document.querySelectorAll('.mrow')];
    // The busiest mod, which is the one a height cap would have cut.
    rows[0]?.click();
    return rows.length;
  });
  await page.waitForTimeout(600);
  const detail = await page.evaluate(() => {
    const d = document.querySelector('.detail');
    if (!d) return null;
    const s = getComputedStyle(d);
    return { rows: d.querySelectorAll('.drow').length, maxH: s.maxHeight, overflow: s.overflowY, h: Math.round(d.getBoundingClientRect().height) };
  });
  ok('the per-mod table has rows', opened > 10, `${opened} mods`);
  // A sticky heading strip is a second element with its own padding: the columns have to line up
  // with the rows under them, or the strip is worse than no strip.
  const cols = await page.evaluate(() => {
    const cell = (sel, i) => Math.round([...document.querySelectorAll(sel)][i].getBoundingClientRect().left);
    return [0, 1, 2, 3, 4, 5].map((i) => cell('.mhdr > *', i) - cell('.mrow:first-of-type > *', i));
  });
  ok('its column names line up with the rows', cols.every((d) => Math.abs(d) <= 1), cols.join(', '));

  ok('opening one shows its methods', !!detail && detail.rows > 0, detail ? `${detail.rows} methods` : 'nothing opened');
  ok('and the panel is not capped or scrolled', !!detail && detail.maxH === 'none' && detail.overflow === 'visible', detail ? `max-height ${detail.maxH}, overflow-y ${detail.overflow}` : '');
  await page.screenshot({ path: `${OUT}/patches-permod.png` });

  // ---- coming back ---------------------------------------------------------------------------
  // The complaint exactly: click a mod, land in the load order, come back.
  await tab(page, 'Every patched method');
  await page.evaluate(() => { const p = document.querySelector('.pane'); p.scrollTop = 4000; });
  await page.waitForTimeout(300);
  const left = await page.evaluate(() => Math.round(document.querySelector('.pane').scrollTop));
  ok('deep in the method list', left > 3000, `${left}px`);

  await page.evaluate(() => { document.querySelector('.vrows .row .lnk')?.click(); });
  await page.waitForTimeout(500);
  const wentToOrder = await page.evaluate(() => !!document.querySelector('.list .row[data-uid]'));
  ok('clicking a mod goes to the load order', wentToOrder);

  await goto(page, 'Patches');
  await page.waitForSelector('.pane', { timeout: 10000 });
  await page.waitForTimeout(600);
  const back = await page.evaluate(() => ({
    tab: [...document.querySelectorAll('.tabs button.on')].map((b) => b.textContent.trim().split(/\s{2,}|\n/)[0])[0] ?? '',
    top: Math.round(document.querySelector('.pane')?.scrollTop ?? -1)
  }));
  ok('back on the tab that was left', back.tab.startsWith('Every patched method'), back.tab);
  ok('and at the same place in it', Math.abs(back.top - left) <= 2, `${back.top} vs ${left}`);

  // Each tab keeps its own place rather than sharing one number.
  await tab(page, 'Two mods over one method');
  const contestedTop = await page.evaluate(() => Math.round(document.querySelector('.pane').scrollTop));
  ok('switching tab does not carry the other tab\'s position over', contestedTop === 0, `${contestedTop}px`);
  const parked = await page.evaluate(() => {
    const p = document.querySelector('.pane');
    p.scrollTop = Math.min(900, p.scrollHeight - p.clientHeight);
    return Math.round(p.scrollTop);
  });
  await page.waitForTimeout(250);
  await tab(page, 'Every patched method');
  const methodsTop = await page.evaluate(() => Math.round(document.querySelector('.pane').scrollTop));
  ok('and the method list is still where it was', Math.abs(methodsTop - left) <= 2, `${methodsTop} vs ${left}`);
  await tab(page, 'Two mods over one method');
  const backContested = await page.evaluate(() => Math.round(document.querySelector('.pane').scrollTop));
  ok('as is the contested list', Math.abs(backContested - parked) <= 2, `${backContested} vs ${parked}`);

  // ---- the last tab --------------------------------------------------------------------------
  await tab(page, 'Cannot be followed');
  const manual = await page.evaluate(() => ({ mans: document.querySelectorAll('.man').length, errs: document.querySelectorAll('.errlist div').length }));
  ok('the runtime-target tab has something in it', manual.mans > 0, `${manual.mans} mods`);
  await page.screenshot({ path: `${OUT}/patches-manual.png` });

  console.log('errors:', errors.length ? errors.join('\n') : 'none');
  await browser.close();
  console.log(failures ? `\n${failures} failed.` : '\nall good');
  process.exit(failures ? 1 : 0);
})();
