// Reset banner + import dialog history: node tools/loadtest/shots5.cjs <outdir>
const { chromium } = require('playwright');
(async () => {
  const out = process.argv[2] || '/tmp';
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1440, height: 900 } });
  const errors = []; p.on('pageerror', e => errors.push('PAGEERROR ' + e.message)); p.on('console', m => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 300)); });
  await p.goto('http://localhost:4173/?reset', { waitUntil: 'load' });
  for (let i = 0; i < 40; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
  await p.waitForTimeout(600);
  await p.screenshot({ path: `${out}/m5-reset.png` });
  await p.click('text=Import');
  await p.waitForTimeout(400);
  await p.screenshot({ path: `${out}/m5-import.png` });
  console.log(errors.length ? errors.join('\n') : 'no console errors');
  await b.close();
})();
