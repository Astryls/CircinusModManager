// Load-order arrangement, badge columns, self-filling groups, sectioned changes, background
// DDS removal. node tools/loadtest/shots16.cjs <outdir>
const { chromium } = require('playwright');
(async () => {
  const out = process.argv[2] || '/tmp';
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1600, height: 992 }, deviceScaleFactor: 2 });
  const errors = []; p.on('pageerror', e => errors.push('PAGEERROR ' + e.message)); p.on('console', m => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 300)); });
  await p.goto('http://127.0.0.1:4173/', { waitUntil: 'load' });
  for (let i = 0; i < 40; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
  await p.waitForTimeout(500);

  // 1. plain load order by default: index numbers run 1..n, no phase headers
  const idx = await p.evaluate(() => [...document.querySelectorAll('.row .idx')].map((e) => Number(e.textContent)).filter(Boolean));
  console.log('arrangement:', await p.evaluate(() => document.querySelector('.seg.arr .on')?.textContent.trim()), '| first indexes:', idx.slice(0, 12).join(','), '| monotonic:', idx.every((v, i) => i === 0 || v > idx[i - 1]), '| phase headers:', await p.evaluate(() => document.querySelectorAll('.spacer .ph').length));
  console.log('column header:', await p.evaluate(() => [...document.querySelectorAll('.hdr .h')].map((e) => e.textContent.trim()).join(' | ')));
  await p.screenshot({ path: `${out}/m16-order.png` });
  await p.screenshot({ path: `${out}/m16-header.png`, clip: { x: 250, y: 300, width: 1010, height: 220 } });

  // badges: one slot each. RocketMan: an update; RIMMSqol: an error; Retro Walls: four notes in one slot; Pawn Textures Redux: a warning.
  const slots = async (needle) => {
    await p.fill('#search', needle);
    await p.waitForTimeout(400);
    return p.evaluate(() => {
      const r = document.querySelector('.list .row');
      return [...r.querySelectorAll('.badges .b')].map((b) => (b.querySelector('.flag') ? ((b.querySelector('.flag em')?.textContent ?? '') + ' ' + (b.querySelector('.flag').getAttribute('title') || '')).trim().slice(0, 70) : '-'));
    });
  };
  console.log('rocketman slots:', JSON.stringify(await slots('rocketman')));
  console.log('rimmsqol slots:', JSON.stringify(await slots('rimmsqol')));
  console.log('retro walls slots:', JSON.stringify(await slots('retro wall')));
  console.log('pawn textures slots:', JSON.stringify(await slots('pawn textures')));
  const widths = await p.evaluate(() => [...document.querySelectorAll('.row .badges .b')].map((b) => Math.round(b.getBoundingClientRect().width)));
  console.log('slot widths:', widths.join(','));
  await p.fill('#search', '');
  await p.waitForTimeout(400);
  await p.click('.filter > .btn');
  await p.waitForTimeout(200);
  await p.click('.filter .opt:has-text("Needs attention")');
  await p.waitForTimeout(800);
  console.log('attention rows:', await p.evaluate(() => [...document.querySelectorAll('.list .row .name b')].map((e) => e.textContent).join(', ')), '| toolbar height:', await p.evaluate(() => document.querySelector('.toolbar').getBoundingClientRect().height));
  await p.screenshot({ path: `${out}/m16-badges.png`, clip: { x: 250, y: 300, width: 1010, height: 300 } });
  await p.click('.filter > .btn');
  await p.waitForTimeout(200);
  await p.click('.filter .opt:has-text("All mods")');
  await p.waitForTimeout(400);

  // 2. by phase
  await p.click('.seg.arr button:has-text("By phase")');
  await p.waitForTimeout(500);
  console.log('by phase headers:', await p.evaluate(() => [...document.querySelectorAll('.spacer .ph .n')].map((e) => e.textContent).join(' | ')));
  await p.screenshot({ path: `${out}/m16-byphase.png` });
  // scrolled: the section header floats just under the column header
  await p.evaluate(() => { const l = document.querySelector('.list'); l.scrollTop = 700; l.dispatchEvent(new Event('scroll')); });
  await p.waitForTimeout(400);
  console.log('floating section:', await p.evaluate(() => document.querySelector('.ph.floating .n')?.textContent), '| header top vs floating top:', await p.evaluate(() => [Math.round(document.querySelector('.hdr').getBoundingClientRect().top), Math.round(document.querySelector('.ph.floating')?.getBoundingClientRect().top ?? -1)].join(' / ')));
  await p.screenshot({ path: `${out}/m16-scrolled.png`, clip: { x: 250, y: 300, width: 1010, height: 300 } });
  await p.evaluate(() => { const l = document.querySelector('.list'); l.scrollTop = 0; l.dispatchEvent(new Event('scroll')); });
  await p.waitForTimeout(300);
  await p.click('.seg.arr button:has-text("Load order")');
  await p.waitForTimeout(400);

  // 3. self-filling groups: counts without hand assignments
  console.log('group counts:', await p.evaluate(() => [...document.querySelectorAll('.groups .grow .g')].map((e) => e.textContent.trim().replace(/\s+/g, ' ')).join(' | ')));
  await p.locator('.groups .grow').first().locator('.edit').click();
  await p.waitForTimeout(300);
  console.log('core editor fills with:', await p.evaluate(() => { const s = document.querySelector('.ged select'); return s.options[s.selectedIndex].textContent; }), '|', await p.evaluate(() => document.querySelector('.ged .hint')?.textContent));
  await p.screenshot({ path: `${out}/m16-groupeditor.png`, clip: { x: 0, y: 380, width: 270, height: 380 } });
  // filter by the Core group: the game and DLC only
  await p.locator('.groups .grow').first().locator('.g').click();
  await p.waitForTimeout(400);
  console.log('core group rows:', await p.evaluate(() => [...document.querySelectorAll('.row .name b')].map((e) => e.textContent).join(', ')));
  await p.click('.groups .clear');
  await p.waitForTimeout(300);
  // the Oskar group by author
  await p.locator('.groups .grow').nth(5).locator('.g').click();
  await p.waitForTimeout(400);
  console.log('author group rows:', await p.evaluate(() => [...document.querySelectorAll('.row .name b')].map((e) => e.textContent).join(', ')));
  await p.click('.groups .clear').catch(() => {});
  await p.waitForTimeout(300);

  // 4. changes dialog, sectioned
  await p.click('.banner .btn:has-text("What changed")').catch(async () => { await p.evaluate(() => (document.querySelector('.stats') && null)); });
  await p.waitForTimeout(300);
  if (!(await p.$('.dlg'))) { await p.click('button:has-text("changed")').catch(() => {}); await p.waitForTimeout(300); }
  console.log('changes sections:', await p.evaluate(() => [...document.querySelectorAll('.dlg h4, .dlg h5')].map((e) => e.textContent.trim().replace(/\s+/g, ' ')).join(' | ')));
  console.log('moves:', await p.evaluate(() => [...document.querySelectorAll('.dlg .row.moved')].map((e) => e.textContent.trim().replace(/\s+/g, ' ')).join(' | ')));
  const dlg = await p.$('.dlg');
  if (dlg) await dlg.screenshot({ path: `${out}/m16-changes.png` });
  await p.keyboard.press('Escape');
  await p.waitForTimeout(300);

  // 5. textures: removal runs in the background with progress
  await p.click('.nav button:has-text("Textures")');
  await p.waitForTimeout(600);
  await p.click('.acts .btn:has-text("Remove every DDS")');
  await p.waitForTimeout(300);
  console.log('busy label:', await p.evaluate(() => document.querySelector('.titlebar .busy')?.textContent ?? '(none)'), '| running:', await p.evaluate(() => !!document.querySelector('.prog')));
  console.log('progress line:', await p.evaluate(() => document.querySelector('.prog .pl')?.textContent.trim().replace(/\s+/g, ' ').slice(0, 160)));
  await p.screenshot({ path: `${out}/m16-revert.png`, clip: { x: 250, y: 260, width: 1100, height: 320 } });
  await p.waitForTimeout(1500);
  await p.click('.nav button:has-text("Load order")');
  await p.waitForTimeout(300);
  await p.click('.nav button:has-text("Textures")');
  await p.waitForTimeout(600);
  console.log('report:', await p.evaluate(() => document.querySelector('.report')?.textContent.trim().replace(/\s+/g, ' ').slice(0, 160)));

  console.log(errors.length ? errors.join('\n') : 'no console errors');
  await b.close();
})();
