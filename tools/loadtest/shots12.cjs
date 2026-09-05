// Title bar with the compass mark; unsaved state on the RimWorld card. node tools/loadtest/shots12.cjs <outdir>
const { chromium } = require('playwright');
(async () => {
  const out = process.argv[2] || '/tmp';
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1600, height: 992 }, deviceScaleFactor: 2 });
  const errors = []; p.on('pageerror', e => errors.push('PAGEERROR ' + e.message)); p.on('console', m => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 300)); });
  await p.goto('http://localhost:4173/', { waitUntil: 'load' });
  for (let i = 0; i < 40; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
  await p.waitForTimeout(500);
  await p.click('.row[data-uid$="brrainz.harmony"]');
  await p.keyboard.press('Alt+ArrowDown');
  await p.waitForTimeout(400);
  await p.screenshot({ path: `${out}/m12-title.png`, clip: { x: 0, y: 0, width: 700, height: 240 } });
  await p.click('[aria-label="Settings"]');
  await p.waitForTimeout(300);
  await p.click('.mark');
  await p.waitForTimeout(300);
  const view = await p.evaluate(() => !!document.querySelector('.toolbar'));
  console.log('back on load order after clicking the mark:', view);
  console.log(errors.length ? errors.join('\n') : 'no console errors');
  await b.close();
})();
