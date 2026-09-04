// First-run state (no SteamCMD): node tools/loadtest/shots2.cjs <outdir>
const { chromium } = require('playwright');
(async () => {
  const out = process.argv[2] || '/tmp';
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1440, height: 900 } });
  const errors = []; p.on('pageerror', e => errors.push('PAGEERROR ' + e.message));
  await p.goto('http://localhost:4173/?nosteamcmd', { waitUntil: 'load' });
  for (let i = 0; i < 40; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
  await p.waitForTimeout(700);
  await p.click('text=Dismiss', { timeout: 2000 }).catch(() => {});
  await p.screenshot({ path: `${out}/m3-firstrun.png` });
  await p.click('nav >> text=Downloads');
  await p.waitForTimeout(400);
  await p.screenshot({ path: `${out}/m3-firstrun-downloads.png` });
  console.log(errors.length ? errors.join('\n') : 'no console errors');
  await b.close();
})();
