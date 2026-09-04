const { chromium } = require('playwright'); const fs = require('fs');
(async () => {
  const fixture = fs.readFileSync(process.argv[2] || 'tools/loadtest/snapshot-js.json', 'utf8');
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1440, height: 900 } });
  const errors = []; p.on('pageerror', e => errors.push('PAGEERROR ' + e.message)); p.on('console', m => { if (m.type() === 'error' || m.type() === 'warning') errors.push(m.type() + ': ' + m.text().slice(0, 300)); });
  await p.addInitScript(`window.__CIRCINUS_FIXTURE__ = ${fixture};`);
  const t = Date.now();
  await p.goto('http://localhost:4173/', { waitUntil: 'load' });
  // wait for overlay to disappear or 60s
  let ok = false; for (let i = 0; i < 120; i++) { const loading = await p.evaluate(() => !!document.querySelector('.loading')); if (!loading) { ok = true; break; } await p.waitForTimeout(500); }
  const rows = await p.evaluate(() => document.querySelectorAll('.row').length);
  console.log(`overlay cleared: ${ok} after ${Date.now() - t} ms, rows rendered: ${rows}`);
  console.log(errors.length ? errors.slice(0, 15).join('\n') : 'no console errors');
  await p.screenshot({ path: 'tools/loadtest/loadtest.png' });
  await b.close();
})();
