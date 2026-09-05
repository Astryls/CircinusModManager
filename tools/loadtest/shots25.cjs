// The contested-values rows of the Defs view: five columns that cannot collide, a count for
// the mods in between, and the whole history one click away.
// node tools/loadtest/shots25.cjs <outdir>   (against `npx vite preview --port 4174`)
const { chromium } = require('playwright');
(async () => {
  const out = process.argv[2] || '/tmp';
  const b = await chromium.launch();
  const p = await b.newPage({ viewport: { width: 1700, height: 1000 }, deviceScaleFactor: 2 });
  const errs = [];
  p.on('pageerror', (e) => errs.push(e.message));
  p.on('console', (m) => { if (m.type() === 'error') errs.push(m.text().slice(0, 200)); });
  await p.goto('http://127.0.0.1:4174/', { waitUntil: 'load' });
  for (let i = 0; i < 40; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
  await p.click('.nav button:has-text("Defs")');
  await p.waitForTimeout(500);
  await p.click('.run .btn.primary');
  await p.waitForSelector('.contested .row.ch', { timeout: 15000 });
  await p.waitForTimeout(300);
  const rows = await p.evaluate(() => [...document.querySelectorAll('.row.ch')].map((r) => [...r.children].map((c) => c.textContent.trim().replace(/\s+/g, ' ')).join(' | ')));
  console.log('rows:');
  rows.slice(0, 6).forEach((r) => console.log('  ', r));
  // no cell may overlap its neighbour, and none may hold more than it shows
  const geo = await p.evaluate(() => [...document.querySelectorAll('.row.ch')].slice(0, 12).map((r) => {
    const cells = [...r.children].map((c) => { const b = c.getBoundingClientRect(); return { x: Math.round(b.x), r: Math.round(b.right), sw: c.scrollWidth, cw: c.clientWidth }; });
    const bad = []; for (let i = 1; i < cells.length; i++) if (cells[i].x < cells[i - 1].r - 0.5) bad.push(i);
    return { bad, spill: cells.filter((c) => c.sw > c.cw + 1).length };
  }));
  console.log('overlapping cells in the first 12 rows:', geo.reduce((n, g) => n + g.bad.length, 0), '| cells wider than their track:', geo.reduce((n, g) => n + g.spill, 0));
  console.log('in-between pills:', await p.evaluate(() => [...document.querySelectorAll('.via em')].map((e) => e.textContent).join(' ')));
  await p.click('.row.ch');
  await p.waitForTimeout(400);
  const hist = await p.evaluate(() => [...document.querySelectorAll('.history .hs')].map((h) => [...h.children].map((c) => c.textContent.trim().replace(/\s+/g, ' ')).join(' | ')));
  console.log('history of the first row:');
  hist.forEach((h) => console.log('  ', h));
  console.log('history cells clipped:', await p.evaluate(() => [...document.querySelectorAll('.history .hs > *')].filter((e) => e.scrollWidth > e.clientWidth + 1).length));
  await p.screenshot({ path: `${out}/defs-contested.png`, clip: { x: 270, y: 280, width: 1420, height: 560 } });
  console.log('folded removal row:', rows.find((r) => r.includes('items')) || 'none');
  console.log(errs.length ? errs.join('\n') : 'no console errors');
  await b.close();
})();
