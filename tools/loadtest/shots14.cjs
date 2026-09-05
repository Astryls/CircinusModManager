// Review cycles through the things to look at; groups can be edited and given their own
// section in the load order. node tools/loadtest/shots14.cjs <outdir>
const { chromium } = require('playwright');
(async () => {
  const out = process.argv[2] || '/tmp';
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1600, height: 992 }, deviceScaleFactor: 2 });
  const errors = []; p.on('pageerror', e => errors.push('PAGEERROR ' + e.message)); p.on('console', m => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 300)); });
  await p.goto('http://127.0.0.1:4173/?abovecore', { waitUntil: 'load' });
  for (let i = 0; i < 40; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
  await p.waitForTimeout(500);

  // Review: each press selects the next mod with something to look at
  const label = async () => p.evaluate(() => document.querySelector('.banner button')?.textContent?.trim());
  const sel = async () => p.evaluate(() => document.querySelector('.row.sel .name b')?.textContent);
  const seen = [];
  console.log('button before:', await label());
  for (let i = 0; i < 4; i++) {
    await p.click('.banner button');
    await p.waitForTimeout(350);
    seen.push(`${await sel()} [${await label()}]`);
  }
  console.log('review walk:', seen.join(' -> '));
  await p.screenshot({ path: `${out}/m14-review.png`, clip: { x: 270, y: 190, width: 1250, height: 420 } });
  // F8 does the same from the keyboard
  await p.keyboard.press('F8');
  await p.waitForTimeout(300);
  console.log('after F8:', await sel());

  // A group with its own section shows as a section in the list
  const headers = await p.evaluate(() => [...document.querySelectorAll('.ph .n')].map((e) => e.textContent));
  console.log('section headers:', headers.join(' | '));

  // Group editor in the rail
  await p.hover('.grow:last-of-type');
  await p.click('.grow:last-of-type .edit');
  await p.waitForTimeout(300);
  console.log('editor sort as:', await p.evaluate(() => (document.querySelector('.ged .sel'))?.value));
  const rail = await p.$('.rail .groups');
  await rail.screenshot({ path: `${out}/m14-groups.png` });
  // change where it sits: after Patches
  await p.selectOption('.ged .fld:nth-of-type(2) .sel', 'patch');
  await p.waitForTimeout(400);
  const headers2 = await p.evaluate(() => [...document.querySelectorAll('.ph .n')].map((e) => e.textContent));
  console.log('after moving the section:', headers2.join(' | '));

  // Sort it as, in the inspector, lists the group's section
  await p.keyboard.press('Control+K');
  await p.keyboard.type('Hospitality');
  await p.waitForTimeout(300);
  await p.click('.row[data-uid$="orion.hospitality"]');
  await p.waitForTimeout(300);
  const opts = await p.evaluate(() => [...document.querySelectorAll('.halo select option')].map((o) => o.textContent));
  const val = await p.evaluate(() => (document.querySelector('.halo select')).value);
  console.log('sort-as options:', opts.join(' | '));
  console.log('sort-as value for Hospitality:', val);
  const insp = await p.$('.inspector');
  await insp.screenshot({ path: `${out}/m14-inspector.png` });
  console.log(errors.length ? errors.join('\n') : 'no console errors');
  await b.close();
})();
