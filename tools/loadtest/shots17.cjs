// Load column, single-line toolbar at any width, draggable columns, the HALO page.
// node tools/loadtest/shots17.cjs <outdir>
const { chromium } = require('playwright');
(async () => {
  const out = process.argv[2] || '/tmp';
  const b = await chromium.launch();
  const errors = [];
  const open = async (width, height) => {
    const p = await b.newPage({ viewport: { width, height }, deviceScaleFactor: 2 });
    p.on('pageerror', e => errors.push('PAGEERROR ' + e.message)); p.on('console', m => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 300)); });
    await p.goto('http://127.0.0.1:4173/', { waitUntil: 'load' });
    for (let i = 0; i < 40; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
    await p.waitForTimeout(500);
    return p;
  };
  const toolbar = (p) => p.evaluate(() => ({ h: Math.round(document.querySelector('.toolbar').getBoundingClientRect().height), items: [...document.querySelector('.toolbar').children].map((c) => c.textContent.trim().replace(/\s+/g, ' ')).filter(Boolean).join(' | ') }));

  // 1. the default window: 1440 × 900, then a preview with Apply/Discard, then narrower
  for (const w of [1440, 1280, 1100]) {
    const p = await open(w, 900);
    const before = await toolbar(p);
    await p.click('.toolbar .btn.primary');
    await p.waitForTimeout(600);
    const during = await toolbar(p);
    console.log(`toolbar @${w}: idle ${before.h}px [${before.items}] | preview ${during.h}px [${during.items}]`);
    if (w === 1440) {
      await p.screenshot({ path: `${out}/m17-preview-1440.png`, clip: { x: 250, y: 300, width: 1190, height: 400 } });
      // the Move column appears only during a preview
      console.log('move header during preview:', await p.evaluate(() => !!document.querySelector('.hdr .h.delta')));
    }
    await p.click('.pair .btn:not(.primary)');
    await p.waitForTimeout(300);
    console.log(`  after discard: move header ${await p.evaluate(() => !!document.querySelector('.hdr .h.delta'))}, toolbar ${(await toolbar(p)).h}px`);
    await p.close();
  }

  {
    const n = await open(1440, 900);
    console.log('@1440 package column hidden (names keep their minimum):', !(await n.$('.hdr .h.pkg')), '| name width', await n.evaluate(() => Math.round(document.querySelector('.hdr .h.name').getBoundingClientRect().width)));
    await n.close();
  }
  const p = await open(1700, 960);
  // 2. the Load column
  console.log('header:', await p.evaluate(() => [...document.querySelectorAll('.hdr .h')].map((e) => e.textContent.trim()).join(' | ')));
  const loads = await p.evaluate(() => [...document.querySelectorAll('.list .row')].slice(0, 14).map((r) => `${r.querySelector('.name b').textContent.slice(0, 18)}=${r.querySelector('.load')?.textContent.trim() ?? '-'}`).join(', '));
  console.log('load column:', loads);
  await p.fill('#search', 'combat extended');
  await p.waitForTimeout(400);
  console.log('CE load tip:', (await p.getAttribute('.list .row .load .band', 'title')).replace(/\n/g, ' / ').slice(0, 260));
  await p.fill('#search', '');
  await p.waitForTimeout(300);
  await p.click('.filter > .btn'); await p.waitForTimeout(200);
  console.log('show menu has:', await p.evaluate(() => [...document.querySelectorAll('.filter .opt')].map((e) => e.textContent.trim().replace(/\s+/g, ' ')).filter((t) => t.startsWith('Slow')).join(' | ')));
  await p.click('.filter .opt:has-text("Slow to load")'); await p.waitForTimeout(500);
  console.log('slow rows:', await p.evaluate(() => [...document.querySelectorAll('.list .row .name b')].map((e) => e.textContent).join(', ')));
  await p.screenshot({ path: `${out}/m17-slow.png`, clip: { x: 250, y: 300, width: 1450, height: 360 } });
  await p.click('.filter > .btn'); await p.waitForTimeout(200);
  await p.click('.filter .opt:has-text("All mods")'); await p.waitForTimeout(400);

  // 3. dragging the Mod column, then the package column; double-click resets
  const widths = () => p.evaluate(() => ({ name: Math.round(document.querySelector('.hdr .h.name').getBoundingClientRect().width), pkg: Math.round(document.querySelector('.hdr .h.pkg').getBoundingClientRect().width), fill: !!document.querySelector('.hdr .fill') }));
  const w0 = await widths();
  const grab = await p.$('.hdr .h.name .grab');
  const gb = await grab.boundingBox();
  await p.mouse.move(gb.x + gb.width / 2, gb.y + gb.height / 2);
  await p.mouse.down();
  await p.mouse.move(gb.x + gb.width / 2 + 90, gb.y + gb.height / 2, { steps: 6 });
  await p.mouse.up();
  await p.waitForTimeout(500);
  const w1 = await widths();
  console.log('drag Mod +90:', JSON.stringify(w0), '→', JSON.stringify(w1));
  const grab2 = await p.$('.hdr .h.pkg .grab');
  const gb2 = await grab2.boundingBox();
  await p.mouse.move(gb2.x + gb2.width / 2, gb2.y + gb2.height / 2);
  await p.mouse.down();
  await p.mouse.move(gb2.x + gb2.width / 2 - 40, gb2.y + gb2.height / 2, { steps: 6 });
  await p.mouse.up();
  await p.waitForTimeout(500);
  const w2 = await widths();
  console.log('drag Package −40:', JSON.stringify(w2));
  await p.screenshot({ path: `${out}/m17-columns.png`, clip: { x: 250, y: 300, width: 1450, height: 300 } });
  await grab.dblclick();
  await p.waitForTimeout(500);
  console.log('after double-click on Mod:', JSON.stringify(await widths()));

  // 4. the group editor reads HALO: …
  await p.locator('.groups .grow').nth(1).locator('.edit').click();
  await p.waitForTimeout(300);
  console.log('frameworks fills with:', await p.evaluate(() => { const s = document.querySelector('.ged select'); return s.options[s.selectedIndex].textContent; }), '| options:', await p.evaluate(() => [...document.querySelector('.ged select').options].map((o) => o.textContent).slice(1, 5).join(' / ')));
  await p.click('.ged .x');

  // 5. the HALO page
  await p.click('.nav button:has-text("HALO")');
  await p.waitForTimeout(700);
  console.log('halo sections:', await p.evaluate(() => [...document.querySelectorAll('.halo h3')].map((e) => e.textContent.trim().replace(/\s+/g, ' ').slice(0, 50)).join(' | ')));
  console.log('signals:', await p.evaluate(() => document.querySelectorAll('.sig').length), '| wires:', await p.evaluate(() => document.querySelectorAll('.wires path').length), '| phases:', await p.evaluate(() => [...document.querySelectorAll('.phase')].map((e) => e.querySelector('b').textContent + ' ' + e.querySelector('.n').textContent).join(', ')));
  await p.screenshot({ path: `${out}/m17-halo.png`, fullPage: false });
  // switch off the name-library rule, send patch-only to late loaders
  const sigs = await p.$$('.sig');
  await sigs[8].$('input[type=checkbox]').then((c) => c.click());
  await p.waitForTimeout(400);
  await sigs[10].$('select').then((s) => s.selectOption('late'));
  await p.waitForTimeout(400);
  console.log('after edits: off wires', await p.evaluate(() => document.querySelectorAll('.wires path.off').length), '| nav count', await p.evaluate(() => document.querySelector('.nav button:nth-child(6) .count')?.textContent), '| reset label', await p.evaluate(() => document.querySelector('.caution .btn')?.textContent.trim()));
  // a package mapping
  await p.fill('.yours .add input.mono', 'dubwise.dubsbadhygiene');
  await p.selectOption('.yours .add select', 'late');
  await p.click('.yours .add .btn.primary');
  await p.waitForTimeout(500);
  console.log('package rows:', await p.evaluate(() => [...document.querySelectorAll('.yours .row .nm b')].map((e) => e.textContent).join(', ')));
  const board = await p.$('.board');
  await board.screenshot({ path: `${out}/m17-board.png` });
  // back to the list: DBH now files under late loaders
  await p.click('.nav button:has-text("Load order")');
  await p.waitForTimeout(500);
  await p.fill('#search', 'bad hygiene');
  await p.waitForTimeout(400);
  console.log('DBH phase dot:', await p.getAttribute('.list .row .phz .dot', 'title'));
  await p.fill('#search', '');
  await p.click('.nav button:has-text("HALO")');
  await p.waitForTimeout(500);
  await p.click('.caution .btn');
  await p.waitForTimeout(200);
  await p.click('.caution .ask .btn.danger');
  await p.waitForTimeout(500);
  console.log('after reset: edits', await p.evaluate(() => document.querySelector('.caution .btn')?.textContent.trim()), '| rows', await p.evaluate(() => document.querySelectorAll('.yours .row').length));
  console.log(errors.length ? errors.join('\n') : 'no console errors');
  await b.close();
})();
