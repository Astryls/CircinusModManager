// Instances: the title-bar switcher, switching, and the dialog (create, rename, folders).
// node tools/loadtest/shots21.cjs <outdir>   (against npx vite preview --port 4176 --strictPort)
const { chromium } = require('playwright');
(async () => {
  const out = process.argv[2] || '/tmp';
  const base = process.env.BASE || 'http://127.0.0.1:4176/';
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1600, height: 992 }, deviceScaleFactor: 2 });
  const errors = []; p.on('pageerror', e => errors.push('PAGEERROR ' + e.message)); p.on('console', m => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 300)); });
  await p.goto(base, { waitUntil: 'load' });
  for (let i = 0; i < 40; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
  await p.waitForTimeout(500);

  // nothing may overflow the window sideways, before or after any of this
  const noSideways = async (where) => {
    const w = await p.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
    console.log(`${where}: horizontal overflow ${w}px`);
  };
  await noSideways('start');

  // ---- the switcher in the title bar ----
  console.log('current instance:', await p.evaluate(() => document.querySelector('.ibtn .n')?.textContent));
  await p.click('.ibtn');
  await p.waitForTimeout(250);
  console.log('menu:', await p.evaluate(() => [...document.querySelectorAll('.iswitch .menu .opt')].map((e) => e.textContent.trim().replace(/\s+/g, ' ')).join(' | ')));
  console.log('menu shows the open one:', await p.evaluate(() => document.querySelector('.iswitch .cur b')?.textContent));
  const clipped = await p.evaluate(() => {
    const m = document.querySelector('.iswitch .menu'), r = m.getBoundingClientRect();
    return { offRight: Math.round(r.right - innerWidth), offBottom: Math.round(r.bottom - innerHeight), h: Math.round(r.height), overflowing: m.scrollHeight - m.clientHeight };
  });
  console.log('menu geometry:', JSON.stringify(clipped));
  await p.screenshot({ path: `${out}/m21-switcher.png`, clip: { x: 0, y: 0, width: 560, height: 420 } });

  // switch to the second instance: the folders in the rail and Settings follow
  await p.click('.iswitch .menu .opt:has-text("CE playthrough")');
  await p.waitForTimeout(700);
  console.log('after switching:', await p.evaluate(() => document.querySelector('.ibtn .n')?.textContent));
  console.log('list being worked on:', await p.evaluate(() => document.querySelector('.lbtn .n')?.textContent));
  console.log('its lists:', await p.evaluate(async () => { document.querySelector('.lbtn').click(); await new Promise(r => setTimeout(r, 250)); return [...document.querySelectorAll('.inst .menu .opt')].map(e => e.textContent.trim()).join(' | '); }));
  await p.keyboard.press('Escape');
  await p.waitForTimeout(200);

  // ---- the dialog ----
  await p.click('.ibtn');
  await p.waitForTimeout(200);
  await p.click('.iswitch .menu .opt:has-text("Instances…")');
  await p.waitForTimeout(400);
  console.log('dialog:', await p.evaluate(() => document.querySelector('#inst-title')?.textContent), '·', await p.evaluate(() => [...document.querySelectorAll('.dlg .side .row .nm b')].map((e) => e.textContent).join(' | ')));
  console.log('selected instance:', await p.evaluate(() => document.querySelector('.dlg h4')?.textContent.trim()));
  console.log('folder rows:', await p.evaluate(() => [...document.querySelectorAll('.dlg .loc .lt b')].map((e) => e.textContent).join(' | ')));
  console.log('savedatafolder note:', await p.evaluate(() => [...document.querySelectorAll('.dlg .hint')].map((e) => e.textContent.trim()).find((t) => t.includes('savedatafolder'))?.slice(0, 160)));
  await p.screenshot({ path: `${out}/m21-dialog.png` });
  const dlg = await p.$('.dlg');
  await dlg.screenshot({ path: `${out}/m21-dialog-only.png` });

  // create one
  await p.click('.dlg .add');
  await p.fill('.dlg .side .newg input', 'Anomaly run');
  await p.keyboard.press('Enter');
  await p.waitForTimeout(600);
  console.log('after creating:', await p.evaluate(() => [...document.querySelectorAll('.dlg .side .row .nm b')].map((e) => e.textContent).join(' | ')));
  console.log('now editing:', await p.evaluate(() => document.querySelector('.dlg h4')?.textContent.trim()));

  // rename it
  await p.click('.dlg .acts .btn:has-text("Rename")');
  await p.fill('.dlg .dh .newg input', 'Anomaly 1.6');
  await p.keyboard.press('Enter');
  await p.waitForTimeout(600);
  console.log('after renaming:', await p.evaluate(() => document.querySelector('.dlg h4')?.textContent.trim()), '· in the list:', await p.evaluate(() => [...document.querySelectorAll('.dlg .side .row .nm b')].map((e) => e.textContent).join(' | ')));

  // the delete question says plainly what is and is not deleted
  await p.click('.dlg .acts .btn:has-text("Delete")');
  await p.waitForTimeout(250);
  console.log('delete asks:', await p.evaluate(() => document.querySelector('.dlg .ask')?.textContent.trim().slice(0, 190)));
  await p.screenshot({ path: `${out}/m21-delete.png`, clip: { x: 350, y: 60, width: 900, height: 500 } });
  await p.click('.dlg .ask .btn:has-text("Keep it")');
  await p.waitForTimeout(200);

  // nothing in the dialog is clipped: every row's text fits its box
  const overflow = await p.evaluate(() => {
    const bad = [];
    for (const el of document.querySelectorAll('.dlg .side .row, .dlg .loc, .dlg .acts .btn, .dlg .ft, .dlg h4')) {
      if (el.scrollWidth > el.clientWidth + 1) bad.push(`${el.className}: ${el.scrollWidth} > ${el.clientWidth}`);
    }
    const d = document.querySelector('.dlg').getBoundingClientRect();
    return { bad, offRight: Math.round(d.right - innerWidth), offBottom: Math.round(d.bottom - innerHeight) };
  });
  console.log('dialog clipping:', JSON.stringify(overflow));
  await noSideways('dialog open');

  // switching from inside the dialog
  await p.click('.dlg .side .row:has-text("1.6 vanilla-ish")');
  await p.waitForTimeout(200);
  await p.click('.dlg .acts .btn:has-text("Switch to this one")');
  await p.waitForTimeout(700);
  console.log('back to:', await p.evaluate(() => document.querySelector('.ibtn .n')?.textContent), '· dialog closed:', await p.evaluate(() => !document.querySelector('.dlg')));
  console.log('settings says which instance:', await p.evaluate(async () => { document.querySelector('.ib[aria-label=Settings]').click(); await new Promise(r => setTimeout(r, 400)); return document.querySelector('.card h3 .aside')?.textContent; }));
  await p.waitForTimeout(300);
  await noSideways('settings');
  await p.screenshot({ path: `${out}/m21-settings.png`, clip: { x: 0, y: 40, width: 900, height: 460 } });

  console.log(errors.length ? errors.join('\n') : 'no console errors');
  await b.close();
})();
