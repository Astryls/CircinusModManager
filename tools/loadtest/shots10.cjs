// Inspector with a selected mod, analyzer and textures with the new icons. node tools/loadtest/shots10.cjs <outdir>
const { chromium } = require('playwright');
(async () => {
  const out = process.argv[2] || '/tmp';
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1600, height: 1000 } });
  const errors = []; p.on('pageerror', e => errors.push('PAGEERROR ' + e.message)); p.on('console', m => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 300)); });
  await p.goto('http://localhost:4173/', { waitUntil: 'load' });
  for (let i = 0; i < 40; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
  await p.waitForTimeout(500);
  await p.click('.row[data-uid$="brrainz.harmony"]');
  await p.waitForTimeout(400);
  await p.screenshot({ path: `${out}/m10-inspector.png` });
  const insp = await p.evaluate(() => { const r = document.querySelector('.inspector'); return r ? [r.scrollWidth, r.clientWidth, r.scrollHeight, r.clientHeight] : null; });
  console.log('inspector scrollW/clientW/scrollH/clientH', insp);
  await p.click('nav >> text=Analyzer');
  await p.waitForTimeout(300);
  await p.click('text=Read Player.log');
  await p.waitForTimeout(500);
  await p.screenshot({ path: `${out}/m10-analyzer.png` });
  await p.click('nav >> text=Textures');
  await p.waitForTimeout(400);
  await p.screenshot({ path: `${out}/m10-textures.png` });
  console.log(errors.length ? errors.join('\n') : 'no console errors');
  await b.close();
})();
