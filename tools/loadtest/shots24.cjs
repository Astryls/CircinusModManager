// Auto-update: the Updates section in Settings, the banner with `?update`, the title-bar mark,
// Install's progress, and the relaunch message. Asserts geometry (nothing clipped, nothing
// overlapping) rather than trusting the picture.
// NODE_PATH=$(npm root -g) node tools/loadtest/shots24.cjs <outdir>   (against `npx vite preview --port 4179`)
const { chromium } = require('playwright');
(async () => {
  const out = process.argv[2] || '/tmp';
  const b = await chromium.launch();
  const errors = [];
  const open = async (width, height, q = '') => {
    const p = await b.newPage({ viewport: { width, height }, deviceScaleFactor: 2 });
    p.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));
    p.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 300)); });
    await p.goto('http://127.0.0.1:4179/' + q, { waitUntil: 'load' });
    for (let i = 0; i < 40; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
    await p.waitForTimeout(500);
    return p;
  };
  // Text that does not fit its box, anywhere under `root`.
  const clipped = (p, root) => p.evaluate((root) => [...document.querySelectorAll(`${root} *`)].filter((e) => e.children.length === 0 && e.textContent.trim() && getComputedStyle(e).overflow !== 'visible' && (e.scrollWidth > e.clientWidth + 1 || e.scrollHeight > e.clientHeight + 1)).map((e) => e.textContent.trim().slice(0, 40)), root);
  // Children of a flex row must not overlap one another.
  const overlaps = (p, sel) => p.evaluate((sel) => {
    const row = document.querySelector(sel);
    if (!row) return ['(no such row)'];
    const kids = [...row.children].map((e) => { const r = e.getBoundingClientRect(); return { t: (e.className || e.tagName).toString().slice(0, 20), x: r.x, r: r.right }; }).filter((k) => k.r > k.x);
    const bad = [];
    for (let i = 1; i < kids.length; i++) if (kids[i].x < kids[i - 1].r - 0.5) bad.push(`${kids[i - 1].t}→${kids[i].t}`);
    return bad;
  }, sel);

  // 1. Settings, no update on offer: the section, the toggle, the version, Check now, the answer.
  let p = await open(1700, 960);
  await p.click('.ib[aria-label="Settings"]'); await p.waitForTimeout(400);
  const sec = await p.evaluate(() => {
    const h = [...document.querySelectorAll('.card h3')].find((e) => e.textContent.startsWith('Updates'));
    const card = h && h.closest('.card');
    if (!card) return null;
    return { head: h.textContent.trim().replace(/\s+/g, ' '), toggle: card.querySelector('input[type=checkbox]')?.checked, buttons: [...card.querySelectorAll('button')].map((b) => b.textContent.trim()), result: card.querySelector('.result')?.textContent.trim() };
  });
  console.log('updates section:', JSON.stringify(sec));
  await p.evaluate(() => [...document.querySelectorAll('.card button')].find((b) => b.textContent.trim() === 'Check now').click());
  await p.waitForTimeout(400);
  console.log('after Check now:', await p.evaluate(() => document.querySelector('.updates .result')?.textContent.trim()));
  console.log('settings clipped:', (await clipped(p, '.updates')).join(' | ') || 'none');
  console.log('about line:', await p.evaluate(() => [...document.querySelectorAll('.card h3')].find((e) => e.textContent.startsWith('About'))?.nextElementSibling?.textContent.trim().slice(0, 60)));
  const box = await p.evaluate(() => { const r = document.querySelector('.updates').getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height }; });
  await p.screenshot({ path: `${out}/m24-settings.png`, clip: { x: box.x - 8, y: box.y - 8, width: box.width + 16, height: box.height + 16 } });
  console.log('title-bar mark without an update:', await p.evaluate(() => !!document.querySelector('.chip.up')));
  await p.close();

  // 2. `?update`: the banner arrives on its own, the title-bar mark too.
  p = await open(1700, 960, '?update');
  await p.waitForTimeout(1800);
  const banner = await p.evaluate(() => { const b = document.querySelector('.banner.update'); return b ? { title: b.querySelector('.t').firstChild.textContent, detail: b.querySelector('small').textContent, buttons: [...b.querySelectorAll('button')].map((x) => x.textContent.trim()) } : null; });
  console.log('banner:', JSON.stringify(banner));
  console.log('banner overlaps:', (await overlaps(p, '.banner.update')).join(', ') || 'none');
  console.log('banner clipped:', (await clipped(p, '.banner.update')).join(' | ') || 'none');
  console.log('title-bar mark:', await p.evaluate(() => document.querySelector('.chip.up')?.textContent.trim() ?? 'none'));
  console.log('title-bar overlaps:', (await overlaps(p, '.title .actions')).join(', ') || 'none');
  await p.screenshot({ path: `${out}/m24-banner.png`, clip: { x: 0, y: 0, width: 1700, height: 400 } });

  // Not now hides the banner but not the mark; Check now in Settings brings it back.
  await p.evaluate(() => [...document.querySelectorAll('.banner.update button')].find((b) => b.textContent.trim() === 'Not now').click());
  await p.waitForTimeout(200);
  console.log('after Not now: banner', await p.evaluate(() => !!document.querySelector('.banner.update')), '· mark', await p.evaluate(() => !!document.querySelector('.chip.up')));
  await p.click('.chip.up'); await p.waitForTimeout(300);
  console.log('mark opens Settings:', await p.evaluate(() => !!document.querySelector('.updates')), '· buttons:', await p.evaluate(() => [...document.querySelectorAll('.updates button')].map((b) => b.textContent.trim()).join(' / ')));
  console.log('settings result:', await p.evaluate(() => document.querySelector('.updates .result')?.textContent.trim()));
  await p.evaluate(() => [...document.querySelectorAll('.updates button')].find((b) => b.textContent.trim() === 'Check now').click());
  await p.waitForTimeout(400);
  await p.click('.ib[aria-label="Settings"]'); await p.waitForTimeout(300);
  console.log('banner back after Check now:', await p.evaluate(() => !!document.querySelector('.banner.update')));

  // 3. Install: progress, then the relaunch message. Sample the bar while it moves.
  await p.evaluate(() => [...document.querySelectorAll('.banner.update button')].find((b) => b.textContent.trim() === 'Install and restart').click());
  const bannerBox = async () => { const r = await p.evaluate(() => { const r = document.querySelector('.banner.update').getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height }; }); return { x: r.x - 6, y: r.y - 6, width: r.width + 12, height: r.height + 12 }; };
  const samples = [];
  for (let i = 0; i < 22; i++) {
    await p.waitForTimeout(150);
    samples.push(await p.evaluate(() => { const b = document.querySelector('.banner.update'); if (!b) return 'gone'; const bar = b.querySelector('.bar i'); return `${b.querySelector('.t').firstChild.textContent} [${b.querySelector('small').textContent}]${bar ? ` bar=${Math.round(parseFloat(bar.style.width))}%` : ''}`; }));
    if (i === 4) {
      await p.screenshot({ path: `${out}/m24-progress.png`, clip: await bannerBox() });
      console.log('progress overlaps:', (await overlaps(p, '.banner.update')).join(', ') || 'none');
      console.log('progress clipped:', (await clipped(p, '.banner.update')).join(' | ') || 'none');
      console.log('buttons while installing:', await p.evaluate(() => [...document.querySelectorAll('.banner.update button')].map((b) => b.textContent.trim()).join(' / ') || 'none'));
    }
  }
  console.log('install samples:\n  ' + samples.filter((s, i) => i === 0 || s !== samples[i - 1]).join('\n  '));
  console.log('title-bar mark while installing/after:', await p.evaluate(() => !!document.querySelector('.chip.up')));
  console.log('relaunch clipped:', (await clipped(p, '.banner.update')).join(' | ') || 'none');
  await p.screenshot({ path: `${out}/m24-relaunch.png`, clip: await bannerBox() });
  await p.close();

  // 4. Narrow window: the banner still has room for both buttons and the label.
  p = await open(1180, 800, '?update');
  await p.waitForTimeout(1800);
  console.log('@1180 banner overlaps:', (await overlaps(p, '.banner.update')).join(', ') || 'none', '· clipped:', (await clipped(p, '.banner.update')).join(' | ') || 'none (title may ellipsize)');
  console.log('@1180 title ellipsized:', await p.evaluate(() => { const t = document.querySelector('.banner.update .t'); return t.scrollWidth > t.clientWidth + 1; }));
  await p.close();

  // 5. Offline: Check now says so in words.
  p = await open(1700, 960, '?offline');
  await p.click('.ib[aria-label="Settings"]'); await p.waitForTimeout(300);
  await p.evaluate(() => [...document.querySelectorAll('.updates button')].find((b) => b.textContent.trim() === 'Check now').click());
  await p.waitForTimeout(400);
  console.log('offline result:', await p.evaluate(() => document.querySelector('.updates .result')?.textContent.trim()), '· red:', await p.evaluate(() => document.querySelector('.updates .result').classList.contains('bad')));
  await p.close();

  console.log(errors.length ? errors.join('\n') : 'no console errors');
  await b.close();
})();
