// Textures view with the DDS audit: node tools/loadtest/shots8.cjs <outdir>
const { chromium } = require('playwright');
(async () => {
  const out = process.argv[2] || '/tmp';
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1440, height: 1000 } });
  const errors = []; p.on('pageerror', e => errors.push('PAGEERROR ' + e.message)); p.on('console', m => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 300)); });
  await p.goto('http://localhost:4173/', { waitUntil: 'load' });
  for (let i = 0; i < 40; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
  await p.waitForTimeout(500);
  await p.click('nav >> text=Textures').catch(() => p.click('text=Textures'));
  await p.waitForTimeout(400);
  await p.click('text=Check active mods');
  await p.waitForTimeout(600);
  await p.evaluate(() => document.querySelector('.auditc')?.scrollIntoView());
  await p.waitForTimeout(200);
  await p.screenshot({ path: `${out}/m8-audit.png` });
  await p.click('text=Fix all');
  await p.waitForTimeout(600);
  await p.screenshot({ path: `${out}/m8-fixed.png` });
  console.log(errors.length ? errors.join('\n') : 'no console errors');
  await b.close();
})();
