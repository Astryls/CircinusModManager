// Textures view: node tools/loadtest/shots3.cjs <outdir>
const { chromium } = require('playwright');
(async () => {
  const out = process.argv[2] || '/tmp';
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1440, height: 900 } });
  const errors = []; p.on('pageerror', e => errors.push('PAGEERROR ' + e.message)); p.on('console', m => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 300)); });
  await p.goto('http://localhost:4173/', { waitUntil: 'load' });
  for (let i = 0; i < 40; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
  await p.waitForTimeout(500);
  await p.click('nav >> text=Textures');
  await p.waitForTimeout(500);
  await p.screenshot({ path: `${out}/m4-textures.png` });
  await p.click('text=Optimise active mods');
  await p.waitForTimeout(400);
  await p.screenshot({ path: `${out}/m4-textures-running.png` });
  await p.waitForTimeout(1800);
  await p.screenshot({ path: `${out}/m4-textures-done.png` });
  console.log(errors.length ? errors.join('\n') : 'no console errors');
  await b.close();
})();
