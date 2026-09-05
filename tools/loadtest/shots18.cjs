// Header alignment, the default columns, the column chooser, "By phase" by default.
// node tools/loadtest/shots18.cjs <outdir>
const { chromium } = require('playwright');
(async () => {
  const out = process.argv[2] || '/tmp';
  const b = await chromium.launch();
  const errors = [];
  const open = async (width, height, q = '') => {
    const p = await b.newPage({ viewport: { width, height }, deviceScaleFactor: 2 });
    p.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));
    p.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 300)); });
    await p.goto('http://127.0.0.1:4173/' + q, { waitUntil: 'load' });
    for (let i = 0; i < 40; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
    await p.waitForTimeout(500);
    return p;
  };
  // Every header cell and every cell of the first row, with their x ranges: nothing may overlap
  // and the header must sit over its own column.
  const geometry = (p) => p.evaluate(() => {
    const box = (e) => { const r = e.getBoundingClientRect(); return { t: e.className.split(' ').filter((c) => c !== 'h').join('.') || '?', x: Math.round(r.x), r: Math.round(r.right), w: Math.round(r.width) }; };
    const hdr = [...document.querySelector('.hdr').children].map(box);
    const row = [...document.querySelector('.list .row').children].map(box);
    return { hdr, row };
  });
  const overlaps = (cells) => { const bad = []; for (let i = 1; i < cells.length; i++) if (cells[i].x < cells[i - 1].r - 0.5) bad.push(`${cells[i - 1].t}→${cells[i].t}`); return bad; };

  const p = await open(1700, 960);
  console.log('arrangement:', await p.evaluate(() => [...document.querySelectorAll('.seg.arr button')].map((b) => b.textContent.trim() + (b.classList.contains('on') ? ' [on]' : '')).join(' | ')));
  console.log('sections shown:', await p.evaluate(() => [...document.querySelectorAll('.ph .n')].map((e) => e.textContent).slice(0, 4).join(' / ')));
  const g = await geometry(p);
  console.log('header cells:', g.hdr.map((c) => `${c.t}@${c.x}+${c.w}`).join(' '));
  console.log('row cells:   ', g.row.map((c) => `${c.t}@${c.x}+${c.w}`).join(' '));
  console.log('header overlaps:', overlaps(g.hdr).join(', ') || 'none');
  console.log('row overlaps:', overlaps(g.row).join(', ') || 'none');
  // each header cell sits over the row cell of the same index
  console.log('columns aligned:', g.hdr.length === g.row.length && g.hdr.every((c, i) => Math.abs(c.x - g.row[i].x) < 2 && Math.abs(c.w - g.row[i].w) < 2));
  // no header label is clipped (the text fits its column)
  console.log('clipped labels:', await p.evaluate(() => [...document.querySelectorAll('.hdr .h')].filter((e) => e.scrollWidth > e.clientWidth + 1).map((e) => e.textContent.trim()).join(', ') || 'none'));
  console.log('badge labels:', await p.evaluate(() => [...document.querySelectorAll('.hdr .b')].map((e) => { const i = e.querySelector('i'); return `${i.textContent}${i.scrollWidth > i.clientWidth + 1 ? '(CLIPPED)' : ''}`; }).join(' ')));
  await p.screenshot({ path: `${out}/m18-header.png`, clip: { x: 250, y: 292, width: 1450, height: 220 } });

  // the column chooser
  await p.click('.filter > .btn'); await p.waitForTimeout(250);
  console.log('column chips:', await p.evaluate(() => { const ls = [...document.querySelectorAll('.menu .label')]; const c = ls.find((l) => l.textContent === 'Columns').nextElementSibling; return [...c.querySelectorAll('.chip')].map((b) => b.textContent.trim() + (b.classList.contains('on') ? '[on]' : '')).join(' '); }));
  await p.evaluate(() => { const ls = [...document.querySelectorAll('.menu .label')]; const c = ls.find((l) => l.textContent === 'Columns').nextElementSibling; [...c.querySelectorAll('.chip')].find((b) => b.textContent.trim() === 'Phase').click(); });
  await p.waitForTimeout(600);
  console.log('after turning Phase on:', await p.evaluate(() => [...document.querySelectorAll('.hdr .h')].map((e) => e.textContent.trim()).join(' | ')));
  const g2 = await geometry(p);
  console.log('  still aligned:', g2.hdr.length === g2.row.length && g2.hdr.every((c, i) => Math.abs(c.x - g2.row[i].x) < 2));
  console.log('  overlaps:', overlaps(g2.hdr).join(', ') || 'none');
  await p.evaluate(() => { const ls = [...document.querySelectorAll('.menu .label')]; const c = ls.find((l) => l.textContent === 'Columns').nextElementSibling; [...c.querySelectorAll('.chip')].find((b) => b.textContent.trim() === 'Phase').click(); });
  await p.waitForTimeout(500);

  // narrow windows: the package id gives way, the rest still lines up
  await p.close();
  for (const w of [1440, 1180]) {
    const n = await open(w, 900);
    const gg = await geometry(n);
    const clipped = await n.evaluate(() => [...document.querySelectorAll('.hdr .h')].filter((e) => e.scrollWidth > e.clientWidth + 1).map((e) => e.textContent.trim()).join(',') || 'none');
    const aligned = gg.hdr.length === gg.row.length && gg.hdr.every((c, i) => Math.abs(c.x - gg.row[i].x) < 2);
    console.log(`@${w}: cells ${gg.hdr.map((c) => c.t).join(',')} | aligned ${aligned} | overlaps ${overlaps(gg.hdr).join(',') || 'none'} | clipped ${clipped}`);
    if (w === 1180) await n.screenshot({ path: `${out}/m18-narrow.png`, clip: { x: 240, y: 292, width: 930, height: 200 } });
    await n.close();
  }
  console.log(errors.length ? errors.join('\n') : 'no console errors');
  await b.close();
})();
