// The Patches view: every section renders, the tables line up, and the missing-scanner page
// explains itself instead of throwing.
// node tools/loadtest/shots20.cjs <outdir>
const { chromium } = require('playwright');
const PORT = process.env.PORT || 4175;
(async () => {
  const out = process.argv[2] || '/tmp';
  const b = await chromium.launch();
  const errors = [];
  const open = async (width, height, q = '') => {
    const p = await b.newPage({ viewport: { width, height }, deviceScaleFactor: 2 });
    p.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));
    p.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 300)); });
    await p.goto(`http://127.0.0.1:${PORT}/` + q, { waitUntil: 'load' });
    for (let i = 0; i < 40; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
    await p.waitForTimeout(400);
    return p;
  };
  const gotoPatches = async (p) => {
    await p.evaluate(() => [...document.querySelectorAll('.nav button')].find((b) => b.textContent.includes('Patches')).click());
    await p.waitForTimeout(500);
  };
  // Cells of a grid row, left to right, with their x ranges.
  const cells = (p, sel) => p.evaluate((s) => {
    const row = document.querySelector(s);
    if (!row) return null;
    // A cell hidden at this width has no box; it is not a column any more.
    return [...row.children].map((e) => { const r = e.getBoundingClientRect(); return { t: [...e.classList].filter((c) => !c.startsWith('svelte-') && c !== 'h' && c !== 'r').join('.') || e.textContent.trim().slice(0, 9) || e.tagName, x: Math.round(r.x), r: Math.round(r.right), w: Math.round(r.width) }; }).filter((c) => c.w > 0);
  }, sel);
  const overlaps = (cs) => { const bad = []; for (let i = 1; i < cs.length; i++) if (cs[i].x < cs[i - 1].r - 0.5) bad.push(`${cs[i - 1].t}→${cs[i].t}`); return bad; };
  const clipped = (p, sel) => p.evaluate((s) => [...document.querySelectorAll(s)].filter((e) => e.scrollWidth > e.clientWidth + 1).map((e) => e.textContent.trim().slice(0, 24)).join(', ') || 'none', sel);

  const p = await open(1700, 1000);
  console.log('rail count:', await p.evaluate(() => { const b = [...document.querySelectorAll('.nav button')].find((x) => x.textContent.includes('Patches')); return b.textContent.trim().replace(/\s+/g, ' '); }));
  await gotoPatches(p);

  // every section is present
  console.log('sections:', await p.evaluate(() => [...document.querySelectorAll('main.center > .card h3')].map((h) => h.childNodes[0].textContent.trim()).join(' | ')));
  console.log('tiles:', await p.evaluate(() => [...document.querySelectorAll('.tile')].map((t) => `${t.querySelector('.k').textContent}=${t.querySelector('.v').textContent}`).join(' ')));

  // contested methods: each fight names a method and two or more mods
  const fights = await p.evaluate(() => [...document.querySelectorAll('.fight')].map((f) => ({ m: f.querySelector('.mth b').textContent, who: [...f.querySelectorAll('.pat')].map((x) => `${x.querySelector('.kd').textContent}:${x.querySelector('.nm').textContent}`) })));
  console.log('contested methods:', fights.length);
  for (const f of fights.slice(0, 4)) console.log(`  ${f.m} — ${f.who.join(' / ')}`);
  console.log('every fight has 2+ patchers:', fights.every((f) => f.who.length >= 2));
  console.log('every fight has 2+ mods that run before or rewrite:', fights.every((f) => new Set(f.who.filter((w) => w.startsWith('runs before') || w.startsWith('rewrites')).map((w) => w.split(':')[1])).size >= 2));
  console.log('fight text clipped:', await clipped(p, '.fight .mth b, .pat .nm'));

  // the searchable list of everything patched
  const hdr = await cells(p, '.list .hdr');
  const row = await cells(p, '.list .row');
  console.log('list header:', hdr.map((c) => `${c.t}@${c.x}+${c.w}`).join(' '));
  console.log('list row:   ', row.map((c) => `${c.t}@${c.x}+${c.w}`).join(' '));
  console.log('list aligned:', hdr.length === row.length && hdr.every((c, i) => Math.abs(c.x - row[i].x) < 2 && Math.abs(c.w - row[i].w) < 2));
  console.log('list overlaps:', [...overlaps(hdr), ...overlaps(row)].join(', ') || 'none');
  const before = await p.evaluate(() => document.querySelectorAll('.list .row').length);
  await p.fill('.list .input', 'SpawnSetup');
  await p.waitForTimeout(300);
  const after = await p.evaluate(() => ({ n: document.querySelectorAll('.list .row').length, first: document.querySelector('.list .row .mth2')?.textContent }));
  console.log(`filter "SpawnSetup": ${before} rows → ${after.n} ("${after.first}")`);
  await p.fill('.list .input', 'RocketMan');
  await p.waitForTimeout(300);
  console.log('filter "RocketMan":', await p.evaluate(() => document.querySelectorAll('.list .row').length), 'rows');
  await p.fill('.list .input', '');
  await p.waitForTimeout(300);

  // the per-mod table: sortable, and a row expands
  const mh = await cells(p, '.permod .mhdr');
  const mr = await cells(p, '.permod .mrow');
  console.log('per-mod header:', mh.map((c) => `${c.t}@${c.x}+${c.w}`).join(' '));
  console.log('per-mod aligned:', mh.length === mr.length && mh.every((c, i) => Math.abs(c.x - mr[i].x) < 2 && Math.abs(c.w - mr[i].w) < 2));
  console.log('per-mod overlaps:', [...overlaps(mh), ...overlaps(mr)].join(', ') || 'none');
  const firstNames = () => p.evaluate(() => [...document.querySelectorAll('.permod .mrow .nm b')].slice(0, 3).map((e) => e.textContent).join(', '));
  console.log('sorted by patches:', await firstNames());
  await p.evaluate(() => [...document.querySelectorAll('.permod .mhdr .h')].find((h) => h.textContent === 'Mod').click());
  await p.waitForTimeout(250);
  console.log('sorted by mod:    ', await firstNames());
  await p.evaluate(() => [...document.querySelectorAll('.permod .mhdr .h')].find((h) => h.textContent === 'Rewrites').click());
  await p.waitForTimeout(250);
  console.log('sorted by rewrites:', await firstNames());
  await p.evaluate(() => document.querySelector('.permod .mrow').click());
  await p.waitForTimeout(400);
  const detail = await p.evaluate(() => { const d = document.querySelector('.permod .detail'); return d ? { rows: d.querySelectorAll('.drow').length, first: d.querySelector('.drow')?.textContent.replace(/\s+/g, ' ').trim() } : null; });
  console.log('expanded row:', detail ? `${detail.rows} methods, first "${detail.first}"` : 'DID NOT EXPAND');
  const drow = await cells(p, '.permod .detail .drow');
  console.log('detail overlaps:', drow ? overlaps(drow).join(', ') || 'none' : 'n/a');
  await p.evaluate(() => document.querySelector('.permod').scrollIntoView({ block: 'start' }));
  await p.waitForTimeout(300);
  await p.screenshot({ path: `${out}/m20-expanded.png`, clip: { x: 300, y: 60, width: 1390, height: 620 } });

  // the manual section
  console.log('manual entries:', await p.evaluate(() => [...document.querySelectorAll('.manual .man')].map((m) => `${m.querySelector('.b').textContent}: ${[...m.querySelectorAll('.mline .mono')].map((x) => x.textContent).join(' + ')}`).join(' | ')));
  console.log('manual explains itself:', await p.evaluate(() => /computed|at runtime|works out/.test(document.querySelector('.manual .lead').textContent)));

  // nothing runs off the side of the page
  console.log('page scrolls sideways:', await p.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1));
  await p.evaluate(() => document.querySelector('main.center').scrollTo(0, 0));
  await p.waitForTimeout(200);
  await p.screenshot({ path: `${out}/m20-patches.png`, fullPage: false });
  await p.evaluate(() => { const c = document.querySelector('main.center'); c.scrollTo(0, c.scrollHeight); });
  await p.waitForTimeout(300);
  await p.screenshot({ path: `${out}/m20-permod.png`, fullPage: false });

  // the Inspector's compact section
  await p.evaluate(() => [...document.querySelectorAll('.nav button')].find((b) => b.textContent.includes('Load order')).click());
  await p.waitForTimeout(300);
  const picked = await p.evaluate(() => {
    const want = ['Combat Extended', 'Vanilla Expanded Framework', 'RocketMan', 'HugsLib'];
    const rows = [...document.querySelectorAll('.row[data-uid]')];
    const row = rows.find((r) => want.some((w) => r.textContent.includes(w))) ?? rows[0];
    row.click();
    return row.querySelector('.name b')?.textContent.trim();
  });
  await p.waitForTimeout(400);
  console.log('inspector shows:', picked, '→', await p.evaluate(() => { const s = document.querySelector('.inspector .patches'); return s ? s.textContent.replace(/\s+/g, ' ').trim().slice(0, 240) : 'NO PATCHES SECTION'; }));
  await p.evaluate(() => document.querySelector('.inspector .patches')?.scrollIntoView({ block: 'center' }));
  await p.waitForTimeout(300);
  await p.screenshot({ path: `${out}/m20-inspector.png`, clip: { x: 1345, y: 60, width: 355, height: 930 } });
  await p.close();

  // a machine with no scanner: an explanation and a command, no error dump
  const n = await open(1500, 950, '?noscanner');
  await gotoPatches(n);
  console.log('no scanner heading:', await n.evaluate(() => document.querySelector('main.center .card h3')?.textContent.trim()));
  console.log('no scanner says it does not run mods:', await n.evaluate(() => /never loads or runs/.test(document.querySelector('.missing').textContent)));
  console.log('no scanner command:', await n.evaluate(() => document.querySelector('.missing .cmd')?.textContent.trim()));
  console.log('no scanner sections:', await n.evaluate(() => document.querySelectorAll('main.center > .card').length), '(1 = nothing else is shown)');
  await n.screenshot({ path: `${out}/m20-noscanner.png`, clip: { x: 250, y: 60, width: 1230, height: 400 } });
  await n.close();

  // narrow windows still line up
  for (const w of [1440, 1180]) {
    const q = await open(w, 900);
    await gotoPatches(q);
    const h = await cells(q, '.permod .mhdr');
    const r = await cells(q, '.permod .mrow');
    const ok = h.length === r.length && h.every((c, i) => Math.abs(c.x - r[i].x) < 2);
    console.log(`@${w}: per-mod cells ${h.map((c) => c.t).join(',')} | aligned ${ok} | overlaps ${overlaps(h).join(',') || 'none'} | sideways ${await q.evaluate(() => document.documentElement.scrollWidth > innerWidth + 1)}`);
    await q.close();
  }

  console.log(errors.length ? errors.join('\n') : 'no console errors');
  await b.close();
})();
