// Drag to reorder with pointer events, the Show menu, and the narrow-window title bar. node tools/loadtest/shots11.cjs <outdir>
const { chromium } = require('playwright');
(async () => {
  const out = process.argv[2] || '/tmp';
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1600, height: 992 } });
  const errors = []; p.on('pageerror', e => errors.push('PAGEERROR ' + e.message)); p.on('console', m => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 300)); });
  await p.goto('http://localhost:4173/', { waitUntil: 'load' });
  for (let i = 0; i < 40; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
  await p.waitForTimeout(500);
  // Drag HugsLib (row 10) above Harmony with the mouse.
  const from = await p.locator('.row[data-uid$="unlimitedhugs.hugslib"]').boundingBox();
  const to = await p.locator('.row[data-uid$="brrainz.harmony"]').boundingBox();
  const idx = (uid) => p.evaluate((u) => document.querySelector(`.row[data-uid$="${u}"] .idx`)?.textContent, uid);
  const before = [await idx('unlimitedhugs.hugslib'), await idx('brrainz.harmony')];
  await p.mouse.move(from.x + 200, from.y + from.height / 2);
  await p.mouse.down();
  await p.mouse.move(from.x + 200, from.y + 10, { steps: 4 });
  await p.mouse.move(to.x + 200, to.y + 6, { steps: 12 });
  await p.waitForTimeout(150);
  await p.screenshot({ path: `${out}/m11-dragging.png` });
  await p.mouse.up();
  await p.waitForTimeout(500);
  const after = [await idx('unlimitedhugs.hugslib'), await idx('brrainz.harmony')];
  console.log('HugsLib/Harmony index before', before.join('/'), 'after', after.join('/'));
  await p.click('text=Show: All mods');
  await p.waitForTimeout(300);
  await p.screenshot({ path: `${out}/m11-menu.png` });
  await p.click('text=With warnings');
  await p.waitForTimeout(300);
  await p.screenshot({ path: `${out}/m11-filtered.png` });
  const rail = await p.evaluate(() => { const r = document.querySelector('.rail'); return [r.scrollHeight, r.clientHeight]; });
  const bodyW = await p.evaluate(() => [document.documentElement.scrollWidth, document.documentElement.clientWidth]);
  console.log('rail', rail, 'document scrollWidth/clientWidth', bodyW);
  console.log(errors.length ? errors.join('\n') : 'no console errors');
  await b.close();
})();
