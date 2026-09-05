// Subscribe and unsubscribe through the Steam client: the banner's two ways of getting a missing
// mod, the unsubscribe confirmation in the right-click menu and in the Inspector, and the
// dependency's own Subscribe action. node tools/loadtest/shots22.cjs <outdir> [port]
const { chromium } = require('playwright');
(async () => {
  const out = process.argv[2] || '/tmp';
  const port = process.argv[3] || 4177;
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1600, height: 992 }, deviceScaleFactor: 2 });
  const errors = []; p.on('pageerror', e => errors.push('PAGEERROR ' + e.message)); p.on('console', m => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 300)); });
  const toast = () => p.evaluate(() => document.querySelector('.toast')?.textContent?.trim() ?? '(none)');
  await p.goto(`http://127.0.0.1:${port}/`, { waitUntil: 'load' });
  for (let i = 0; i < 40; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
  await p.waitForTimeout(600);

  // ---- the missing-mods banner: two routes, one sentence on the difference
  const banner = await p.evaluate(() => {
    const el = [...document.querySelectorAll('.banner')].find((b) => b.textContent.includes("installed"));
    if (!el) return null;
    return { title: el.querySelector('.t')?.firstChild?.textContent?.trim(), detail: el.querySelector('.t small, .t .inline')?.textContent?.trim(), buttons: [...el.querySelectorAll('button')].map((x) => x.getAttribute('aria-label') || x.textContent.trim()) };
  });
  console.log('missing banner:', JSON.stringify(banner));
  // The sentence about the difference has to be readable, not cut off by the ellipsis.
  console.log('detail fits:', await p.evaluate(() => {
    const el = [...document.querySelectorAll('.banner')].find((b) => b.textContent.includes('installed'));
    const t = el.querySelector('.t');
    return t.scrollWidth <= t.clientWidth + 1;
  }));
  const bannerBox = await p.evaluate(() => {
    const el = [...document.querySelectorAll('.banner')].find((b) => b.textContent.includes('installed'));
    const r = el.getBoundingClientRect();
    return { x: Math.round(r.x) - 8, y: Math.round(r.y) - 8, width: Math.round(r.width) + 16, height: Math.round(r.height) + 16 };
  });
  await p.screenshot({ path: `${out}/m22-banner.png`, clip: bannerBox });

  // the two actions must not overlap and must sit inside the banner
  console.log('banner geometry:', await p.evaluate(() => {
    const el = [...document.querySelectorAll('.banner')].find((b) => b.textContent.includes('installed'));
    const bs = [...el.querySelectorAll('button')].map((x) => x.getBoundingClientRect());
    const box = el.getBoundingClientRect();
    const inside = bs.every((r) => r.left >= box.left - 0.5 && r.right <= box.right + 0.5 && r.top >= box.top - 0.5 && r.bottom <= box.bottom + 0.5);
    let overlap = false;
    for (let i = 0; i < bs.length; i++) for (let j = i + 1; j < bs.length; j++) if (bs[i].right > bs[j].left + 0.5 && bs[j].right > bs[i].left + 0.5) overlap = true;
    return `buttons inside=${inside} overlap=${overlap} widths=${bs.map((r) => Math.round(r.width)).join(',')}`;
  }));

  await p.click('.banner button:has-text("Subscribe in Steam")');
  await p.waitForTimeout(600);
  console.log('after Subscribe in Steam:', await toast());
  await p.screenshot({ path: `${out}/m22-subscribe-toast.png`, clip: { x: 400, y: 860, width: 800, height: 120 } });

  // ---- unsubscribe from the right-click menu, behind a confirmation
  // Let the toast go first: it sits at the bottom of the window and would cover the menu.
  await p.waitForFunction(() => !document.querySelector('.toast'), null, { timeout: 12000 });
  await p.click('.row[data-uid$="brrainz.harmony"]', { button: 'right' });
  await p.waitForTimeout(300);
  console.log('menu items:', await p.evaluate(() => [...document.querySelectorAll('.menu .it')].map((e) => e.textContent.trim()).join(' | ')));
  await p.click('.menu .it:has-text("Unsubscribe in Steam")');
  await p.waitForTimeout(250);
  console.log('confirm item:', await p.evaluate(() => document.querySelector('.menu .it.danger')?.textContent.trim()));
  console.log('confirm sentence:', await p.evaluate(() => document.querySelector('.menu .foot')?.textContent.trim()));
  // The menu grows when the confirmation appears; it must still fit on screen.
  console.log('menu on screen:', await p.evaluate(() => {
    const r = document.querySelector('.menu').getBoundingClientRect();
    return r.top >= 0 && r.left >= 0 && r.bottom <= innerHeight && r.right <= innerWidth;
  }));
  const menu = await p.$('.menu');
  await menu.screenshot({ path: `${out}/m22-unsubscribe-confirm.png` });
  await p.click('.menu .it.danger:has-text("Yes, unsubscribe")');
  await p.waitForTimeout(600);
  console.log('after Yes, unsubscribe:', await toast());
  console.log('menu closed:', await p.evaluate(() => !document.querySelector('.menu')));

  // ---- the same, in the Inspector
  await p.click('.row[data-uid$="unlimitedhugs.hugslib"]');
  await p.waitForTimeout(400);
  console.log('inspector actions:', await p.evaluate(() => [...document.querySelectorAll('.inspector .acts .btn')].map((e) => e.textContent.trim()).join(' | ')));
  await p.click('.inspector .acts .btn:has-text("Unsubscribe")');
  await p.waitForTimeout(250);
  console.log('inspector confirmation:', await p.evaluate(() => document.querySelector('.inspector .warnline')?.textContent.trim()));
  console.log('inspector confirm buttons:', await p.evaluate(() => [...document.querySelectorAll('.inspector .acts .btn')].map((e) => e.textContent.trim()).filter((t) => /unsubscribe|Keep it/i.test(t)).join(' | ')));
  const acts = p.locator('.inspector section.card', { hasText: 'Actions' }).last();
  await acts.scrollIntoViewIfNeeded();
  await p.waitForTimeout(200);
  await acts.screenshot({ path: `${out}/m22-inspector-confirm.png` });
  await p.click('.inspector .acts .btn:has-text("Keep it")');
  await p.waitForTimeout(200);
  console.log('confirmation closed:', await p.evaluate(() => !document.querySelector('.inspector .warnline')));

  // a missing dependency with a Workshop id offers Subscribe of its own
  await p.fill('#search', 'Combat Extended');
  await p.waitForTimeout(400);
  await p.click('.row[data-uid$="ceteam.combatextended"]');
  await p.waitForTimeout(400);
  console.log('dependency line:', await p.evaluate(() => document.querySelector('.inspector .issues .it')?.textContent.replace(/\s+/g, ' ').trim()));
  console.log('dependency action:', await p.evaluate(() => [...document.querySelectorAll('.inspector .issues .lnk.sub')].map((e) => e.textContent.trim()).join(' | ') || '(none)'));
  await p.click('.inspector .issues .lnk.sub');
  await p.waitForTimeout(600);
  console.log('after dependency Subscribe:', await toast());

  // ---- and the honest case: Steam installed but not running
  await p.goto(`http://127.0.0.1:${port}/?nosteam`, { waitUntil: 'load' });
  for (let i = 0; i < 40; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
  await p.waitForTimeout(600);
  console.log('alt button title with Steam closed:', await p.evaluate(() => [...document.querySelectorAll('.banner button.alt')].map((e) => e.title).join(' | ')));
  await p.click('.banner button:has-text("Subscribe in Steam")');
  await p.waitForTimeout(600);
  console.log('with Steam closed:', await toast());

  console.log(errors.length ? 'CONSOLE ERRORS:\n' + errors.join('\n') : 'no console errors');
  await b.close();
})();
