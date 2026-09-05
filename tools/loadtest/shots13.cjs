// Linked mod folders: the Folder row in the inspector and the left-out entries in Settings.
// node tools/loadtest/shots13.cjs <outdir>
const { chromium } = require('playwright');
(async () => {
  const out = process.argv[2] || '/tmp';
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1600, height: 992 }, deviceScaleFactor: 2 });
  const errors = []; p.on('pageerror', e => errors.push('PAGEERROR ' + e.message)); p.on('console', m => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 300)); });
  await p.goto('http://127.0.0.1:4173/', { waitUntil: 'load' });
  for (let i = 0; i < 40; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
  await p.waitForTimeout(500);
  await p.keyboard.press('Control+K');
  await p.keyboard.type('Casino');
  await p.waitForTimeout(400);
  await p.click('.row[data-uid$="community.hospitality.casino"]');
  await p.waitForTimeout(400);
  const insp = await p.$('.inspector');
  await insp.screenshot({ path: `${out}/m13-inspector.png` });
  const folder = await p.evaluate(() => document.querySelector('.inspector dd.fld')?.textContent);
  console.log('folder row:', folder);
  await p.click('[aria-label="Settings"]');
  await p.waitForTimeout(400);
  const card = await p.$('.settings .card');
  await card.screenshot({ path: `${out}/m13-settings.png` });
  console.log('unreadable box:', await p.evaluate(() => document.querySelector('.unread')?.textContent?.slice(0, 200)));
  console.log(errors.length ? errors.join('\n') : 'no console errors');
  await b.close();
})();
