// Right-click menu, named lists, followed collections, stat tiles. node tools/loadtest/shots15.cjs <outdir>
const { chromium } = require('playwright');
(async () => {
  const out = process.argv[2] || '/tmp';
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1600, height: 992 }, deviceScaleFactor: 2 });
  const errors = []; p.on('pageerror', e => errors.push('PAGEERROR ' + e.message)); p.on('console', m => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 300)); });
  await p.goto('http://127.0.0.1:4173/', { waitUntil: 'load' });
  for (let i = 0; i < 40; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
  await p.waitForTimeout(500);

  // right-click a row
  await p.click('.row[data-uid$="brrainz.harmony"]', { button: 'right' });
  await p.waitForTimeout(300);
  console.log('menu items:', await p.evaluate(() => [...document.querySelectorAll('.menu .it')].map((e) => e.textContent.trim()).join(' | ')));
  await p.hover('.menu .it:has-text("Sort it as")');
  await p.waitForTimeout(250);
  console.log('sort flyout:', await p.evaluate(() => [...document.querySelectorAll('.flyout .it')].map((e) => e.textContent.trim()).join(' | ')));
  await p.screenshot({ path: `${out}/m15-menu.png`, clip: { x: 280, y: 300, width: 700, height: 560 } });
  await p.keyboard.press('Escape');
  await p.waitForTimeout(200);
  console.log('menu closed:', await p.evaluate(() => !document.querySelector('.menu')));

  // group flyout: assign to a group
  await p.click('.row[data-uid$="brrainz.harmony"]', { button: 'right' });
  await p.waitForTimeout(200);
  await p.hover('.menu .it:has-text("Group")');
  await p.waitForTimeout(250);
  await p.click('.flyout .it:has-text("Visual")');
  await p.waitForTimeout(400);
  console.log('harmony group now:', await p.evaluate(() => document.querySelector('.inspector .kv select')?.value));

  // list switcher
  await p.click('.lbtn');
  await p.waitForTimeout(250);
  console.log('lists menu:', await p.evaluate(() => [...document.querySelectorAll('.inst .menu .opt')].map((e) => e.textContent.trim()).join(' | ')));
  await p.screenshot({ path: `${out}/m15-lists.png`, clip: { x: 0, y: 40, width: 420, height: 420 } });
  await p.click('.inst .menu .opt:has-text("Save as a new list")');
  await p.fill('.inst .menu .newg input', 'RJW night');
  await p.keyboard.press('Enter');
  await p.waitForTimeout(500);
  console.log('current list:', await p.evaluate(() => document.querySelector('.lbtn .n')?.textContent));
  await p.click('.lbtn');
  await p.waitForTimeout(200);
  await p.click('.inst .menu .opt:has-text("Vanilla plus")');
  await p.waitForTimeout(500);
  console.log('after loading Vanilla plus:', await p.evaluate(() => document.querySelector('.lbtn .n')?.textContent), 'active:', await p.evaluate(() => document.querySelector('.seg [role=tab] .num')?.textContent));

  // collections
  console.log('collections:', await p.evaluate(() => [...document.querySelectorAll('.cols .col')].map((e) => e.textContent.trim()).join(' | ')));
  await p.click('.cols .col');
  await p.waitForTimeout(400);
  console.log('dialog title:', await p.evaluate(() => document.querySelector('#col-title')?.textContent), '|', await p.evaluate(() => document.querySelector('.hd .sub')?.textContent));
  const dlg = await p.$('.dlg');
  await dlg.screenshot({ path: `${out}/m15-collection.png` });
  await p.click('.ft .btn:has-text("Got it")');
  await p.waitForTimeout(300);
  console.log('changes after Got it:', await p.evaluate(() => !!document.querySelector('.changes')));
  await p.keyboard.press('Escape');
  await p.click('.scrim', { position: { x: 5, y: 5 } }).catch(() => {});
  await p.waitForTimeout(300);

  // back to a list with texture collisions
  await p.click('.lbtn');
  await p.waitForTimeout(200);
  await p.click('.inst .menu .opt:has-text("Full run")');
  await p.waitForTimeout(300);
  await p.click('.inst .menu .ask .btn:has-text("Load")').catch(() => {});
  await p.waitForTimeout(500);
  // stat tiles: hover text and click
  console.log('textures tile tip:', (await p.getAttribute('.stats .stat:nth-child(3)', 'title')).slice(0, 220).replace(/\n/g, ' / '));
  await p.click('.stats .stat:nth-child(3)');
  await p.waitForTimeout(400);
  console.log('after clicking textures tile, show:', await p.evaluate(() => document.querySelector('.filter > .btn')?.textContent.trim()));
  await p.click('.stats .stat:nth-child(3)');
  await p.waitForTimeout(300);
  console.log('clicked again, show:', await p.evaluate(() => document.querySelector('.filter > .btn')?.textContent.trim()));
  await p.screenshot({ path: `${out}/m15-main.png` });
  console.log(errors.length ? errors.join('\n') : 'no console errors');
  await b.close();
})();
