// The Patches view without the scanner (`?noscanner`): the two audiences each get their own
// answer, the list of places looked opens and fits, and nothing overflows the card.
// node tools/loadtest/shots23.cjs <outdir> [port]
const { chromium } = require('playwright');
(async () => {
  const out = process.argv[2] || '/tmp';
  const port = process.argv[3] || 4178;
  const b = await chromium.launch(); const p = await b.newPage({ viewport: { width: 1600, height: 992 }, deviceScaleFactor: 2 });
  const errors = []; p.on('pageerror', e => errors.push('PAGEERROR ' + e.message)); p.on('console', m => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 300)); });
  await p.goto(`http://127.0.0.1:${port}/?noscanner`, { waitUntil: 'load' });
  for (let i = 0; i < 40; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
  await p.waitForTimeout(400);

  // Patches sits in the left rail; open it and wait for the missing-scanner card.
  await p.click('.rail .nav button:has-text("Patches")');
  await p.waitForSelector('.missing', { timeout: 5000 });
  await p.waitForTimeout(300);

  console.log('heading:', await p.evaluate(() => document.querySelector('.missing h3')?.textContent.trim()));
  console.log('cases:', JSON.stringify(await p.evaluate(() => [...document.querySelectorAll('.missing .case')].map((c) => ({ h: c.querySelector('h4').textContent.trim(), words: c.querySelector('p').textContent.trim().split(/\s+/).length })))));
  console.log('summary:', await p.evaluate(() => document.querySelector('.looked summary')?.textContent.trim()));

  // the two cases sit side by side at this width, and neither is clipped
  console.log('cases geometry:', await p.evaluate(() => {
    const cs = [...document.querySelectorAll('.missing .case')].map((c) => c.getBoundingClientRect());
    const sameRow = cs.length === 2 && Math.abs(cs[0].top - cs[1].top) < 1;
    const clipped = [...document.querySelectorAll('.missing .case p')].some((el) => el.scrollWidth > el.clientWidth + 1);
    return `sideBySide=${sameRow} clipped=${clipped} widths=${cs.map((r) => Math.round(r.width)).join(',')}`;
  }));

  await p.click('.looked summary');
  await p.waitForTimeout(200);
  console.log('places:', await p.evaluate(() => [...document.querySelectorAll('.looked li')].map((li) => `${li.querySelector('.path').textContent} [${li.querySelector('.why')?.textContent ?? ''}]`).join('\n  ')));
  console.log('list geometry:', await p.evaluate(() => {
    const card = document.querySelector('.missing').getBoundingClientRect();
    const ol = document.querySelector('.looked ol');
    const r = ol.getBoundingClientRect();
    const inside = r.right <= card.right + 0.5 && r.left >= card.left - 0.5;
    const overflow = ol.scrollWidth > ol.clientWidth + 1;
    // a path and its reason share a line and do not overlap
    const overlaps = [...document.querySelectorAll('.looked li')].some((li) => { const a = li.querySelector('.path').getBoundingClientRect(); const w = li.querySelector('.why'); if (!w) return false; const bb = w.getBoundingClientRect(); return a.right > bb.left + 0.5; });
    return `insideCard=${inside} overflow=${overflow} pathReasonOverlap=${overlaps}`;
  }));

  const box = await p.evaluate(() => { const r = document.querySelector('.missing').getBoundingClientRect(); return { x: Math.max(0, Math.round(r.x) - 12), y: Math.max(0, Math.round(r.y) - 12), width: Math.round(r.width) + 24, height: Math.round(r.height) + 24 }; });
  await p.screenshot({ path: `${out}/m23-no-scanner.png`, clip: box });
  await p.screenshot({ path: `${out}/m23-no-scanner-full.png` });
  console.log('errors:', errors.length ? errors.join('\n') : 'none');
  await b.close();
})();
