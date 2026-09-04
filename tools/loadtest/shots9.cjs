// UI pass: main page at Sean's window size, settings, inspector. node tools/loadtest/shots9.cjs <outdir>
const { chromium } = require('playwright');
(async () => {
  const out = process.argv[2] || '/tmp';
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 2000, height: 1180 } });
  const errors = []; p.on('pageerror', e => errors.push('PAGEERROR ' + e.message)); p.on('console', m => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 300)); });
  await p.goto('http://localhost:4173/?abovecore', { waitUntil: 'load' });
  for (let i = 0; i < 40; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
  await p.waitForTimeout(500);
  await p.click('text=Harmony');
  await p.waitForTimeout(400);
  await p.screenshot({ path: `${out}/m9-main.png` });
  const scroll = await p.evaluate(() => ({ rail: (() => { const r = document.querySelector('.rail'); return r ? [r.scrollHeight, r.clientHeight] : null; })(), insp: (() => { const r = document.querySelector('.inspector'); return r ? [r.scrollWidth, r.clientWidth] : null; })() }));
  console.log('rail scrollHeight/clientHeight', scroll.rail, 'inspector scrollWidth/clientWidth', scroll.insp);
  await p.click('[aria-label="Settings"]');
  await p.waitForTimeout(400);
  await p.screenshot({ path: `${out}/m9-settings.png` });
  const sets = await p.evaluate(() => { const r = document.querySelector('.settings'); return r ? [r.scrollHeight, r.clientHeight] : null; });
  console.log('settings scrollHeight/clientHeight', sets);
  console.log(errors.length ? errors.join('\n') : 'no console errors');
  await b.close();
})();
