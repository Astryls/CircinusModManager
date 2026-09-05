// The Defs view: run it, check every section renders, open a contested value, search a def.
// node tools/loadtest/shots19.cjs <outdir>
const { chromium } = require('playwright');
(async () => {
  const out = process.argv[2] || '/tmp';
  const b = await chromium.launch();
  const errors = [];
  const open = async (width, height) => {
    const p = await b.newPage({ viewport: { width, height }, deviceScaleFactor: 2 });
    p.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));
    p.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 300)); });
    await p.goto('http://127.0.0.1:4174/', { waitUntil: 'load' });
    for (let i = 0; i < 40; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
    await p.waitForTimeout(400);
    return p;
  };
  // Cells of one row with their x ranges: nothing may overlap its neighbour or leave the row.
  const cells = (p, sel) => p.evaluate((s) => {
    const e = document.querySelector(s);
    if (!e) return null;
    const box = (n) => { const r = n.getBoundingClientRect(); return { t: (n.className || '?').split(' ')[0] || n.tagName.toLowerCase(), x: Math.round(r.x), r: Math.round(r.right), w: Math.round(r.width), clipped: n.scrollWidth > n.clientWidth + 1 }; };
    return { row: box(e), kids: [...e.children].map(box) };
  }, sel);
  const overlaps = (k) => { const bad = []; for (let i = 1; i < k.length; i++) if (k[i].x < k[i - 1].r - 0.5) bad.push(`${k[i - 1].t}→${k[i].t}`); return bad; };

  const p = await open(1700, 1000);

  // the nav entry exists and opens the view
  console.log('rail entries:', await p.evaluate(() => [...document.querySelectorAll('.nav button')].map((b) => b.textContent.trim().split('\n')[0]).join(' | ')));
  await p.evaluate(() => [...document.querySelectorAll('.nav button')].find((b) => b.textContent.trim().startsWith('Defs')).click());
  await p.waitForTimeout(300);
  console.log('start state:', await p.evaluate(() => document.querySelector('.run .btn')?.textContent.trim()));
  console.log('analyzer "Coming next" gone:', await p.evaluate(() => { const n = [...document.querySelectorAll('.nav button')].find((b) => b.textContent.trim().startsWith('Analyzer')); n.click(); return true; }));
  await p.waitForTimeout(250);
  console.log('  analyzer headings:', await p.evaluate(() => [...document.querySelectorAll('main.center h3')].map((h) => h.childNodes[0].textContent.trim()).join(' | ')));
  await p.evaluate(() => [...document.querySelectorAll('.nav button')].find((b) => b.textContent.trim().startsWith('Defs')).click());
  await p.waitForTimeout(200);

  // run it: the progress bar shows, then the report
  await p.click('.run .btn.primary');
  await p.waitForTimeout(450);
  console.log('running:', await p.evaluate(() => { const e = document.querySelector('.prog .pl b'); return e ? e.textContent.trim() : 'no progress block'; }), '· stop button:', await p.evaluate(() => !!document.querySelector('.prog .btn')));
  await p.waitForSelector('.contested', { timeout: 15000 });
  await p.waitForTimeout(300);
  console.log('summary:', await p.evaluate(() => document.querySelector('.run .report')?.textContent.replace(/\s+/g, ' ').trim()));
  console.log('sections:', await p.evaluate(() => [...document.querySelectorAll('main.center .card')].map((c) => c.querySelector('h3')?.childNodes[0].textContent.trim()).join(' | ')));
  console.log('contested rows:', await p.evaluate(() => document.querySelectorAll('.contested .row.ch').length), '· first chain:', await p.evaluate(() => document.querySelector('.contested .row.ch')?.textContent.replace(/\s+/g, ' ').trim()));
  console.log('problem groups:', await p.evaluate(() => [...document.querySelectorAll('.problems .pm summary')].map((s) => s.textContent.replace(/\s+/g, ' ').trim()).join(' / ')));
  console.log('tolerated section:', await p.evaluate(() => document.querySelector('.problems .quiet summary')?.textContent.trim() || 'none'));
  console.log('unknown ops:', await p.evaluate(() => document.querySelector('.problems .unknown h4')?.textContent.replace(/\s+/g, ' ').trim() || 'none'));
  console.log('duplicates:', await p.evaluate(() => [...document.querySelectorAll('.dups .row.dp')].map((r) => r.textContent.replace(/\s+/g, ' ').trim()).join(' / ')));
  console.log('missing parents:', await p.evaluate(() => document.querySelectorAll('.parents .pr').length));
  console.log('per-mod rows:', await p.evaluate(() => document.querySelectorAll('.permod .row.pm2').length));

  // geometry: header over its own column, nothing overlapping, no clipped header label
  for (const [name, hdr, row] of [['contested', '.contested .hdr.ch', '.contested .row.ch'], ['per mod', '.permod .hdr.pm2', '.permod .row.pm2']]) {
    const h = await cells(p, hdr), rr = await cells(p, row);
    const aligned = h.kids.length === rr.kids.length && h.kids.every((c, i) => Math.abs(c.x - rr.kids[i].x) < 2 && Math.abs(c.w - rr.kids[i].w) < 2);
    const inside = rr.kids.every((c) => c.x >= rr.row.x - 1 && c.r <= rr.row.r + 1);
    console.log(`${name}: cells ${h.kids.map((c) => `${c.t}@${c.x}+${c.w}`).join(' ')}`);
    console.log(`  aligned ${aligned} | header overlaps ${overlaps(h.kids).join(',') || 'none'} | row overlaps ${overlaps(rr.kids).join(',') || 'none'} | cells inside row ${inside} | clipped headers ${h.kids.filter((c) => c.clipped).map((c) => c.t).join(',') || 'none'}`);
  }
  console.log('page scrolls sideways:', await p.evaluate(() => { const m = document.querySelector('main.center'); return m.scrollWidth > m.clientWidth + 1; }));

  // sorting the per-mod table
  const order = () => p.evaluate(() => [...document.querySelectorAll('.permod .row.pm2 .nm b')].slice(0, 3).map((e) => e.textContent).join(', '));
  console.log('per mod by values:', await order());
  await p.evaluate(() => [...document.querySelectorAll('.permod .hdr .h')].find((h) => h.textContent.trim() === 'Failed').click());
  await p.waitForTimeout(200);
  console.log('per mod by failed:', await order(), '· header marked:', await p.evaluate(() => document.querySelector('.permod .hdr .h.on')?.textContent.trim()));

  // clicking a contested row opens that def, with the contested path pointed at
  await p.click('.contested .row.ch');
  await p.waitForTimeout(600);
  console.log('tree for:', await p.evaluate(() => document.querySelector('.look h3 .aside')?.textContent.trim()), '· nodes:', await p.evaluate(() => document.querySelectorAll('.look .tn').length));
  console.log('highlighted node:', await p.evaluate(() => document.querySelector('.look .tn.hit')?.textContent.replace(/\s+/g, ' ').trim() || 'none'));
  console.log('legend mods:', await p.evaluate(() => [...document.querySelectorAll('.look .lg')].map((e) => e.textContent.trim()).join(' · ')));
  console.log('inherited marks:', await p.evaluate(() => [...document.querySelectorAll('.look .tn .inh')].map((e) => e.textContent.trim()).slice(0, 3).join(' / ')));
  console.log('tree rows clipped:', await p.evaluate(() => [...document.querySelectorAll('.look .tn')].filter((e) => e.scrollWidth > e.clientWidth + 1).length));

  // the search box
  await p.fill('#defsearch', 'gun');
  await p.waitForTimeout(600);
  console.log('search chips:', await p.evaluate(() => [...document.querySelectorAll('.look .found .chip')].map((c) => c.textContent.trim()).join(' | ')));
  await p.evaluate(() => document.querySelector('.look .found .chip').click());
  await p.waitForTimeout(500);
  console.log('after picking one:', await p.evaluate(() => document.querySelector('.look h3 .aside')?.textContent.trim()), '· nodes:', await p.evaluate(() => document.querySelectorAll('.look .tn').length));

  // the advanced XPath box
  await p.evaluate(() => document.querySelector('.adv').open = true);
  await p.waitForTimeout(150);
  await p.click('.xq .btn');
  await p.waitForTimeout(500);
  console.log('xpath result:', await p.evaluate(() => document.querySelector('.qres .hint')?.textContent.trim()), '· rows:', await p.evaluate(() => document.querySelectorAll('.qres .row.qr').length));
  const q = await cells(p, '.qres .row.qr');
  if (q) console.log('  query row overlaps:', overlaps(q.kids).join(',') || 'none', '| inside row:', q.kids.every((c) => c.x >= q.row.x - 1 && c.r <= q.row.r + 1));

  await p.evaluate(() => document.querySelector('main.center').scrollTo(0, 0));
  await p.waitForTimeout(200);
  await p.screenshot({ path: `${out}/m19-defs.png`, fullPage: false });

  // a narrow window: the columns still line up
  await p.close();
  const n = await open(1180, 900);
  await n.evaluate(() => [...document.querySelectorAll('.nav button')].find((b) => b.textContent.trim().startsWith('Defs')).click());
  await n.waitForTimeout(200);
  await n.click('.run .btn.primary');
  await n.waitForSelector('.contested .row.ch', { timeout: 15000 });
  await n.waitForTimeout(300);
  for (const [name, hdr, row] of [['contested', '.contested .hdr.ch', '.contested .row.ch'], ['per mod', '.permod .hdr.pm2', '.permod .row.pm2']]) {
    const h = await cells(n, hdr), rr = await cells(n, row);
    const aligned = h.kids.every((c, i) => Math.abs(c.x - rr.kids[i].x) < 2);
    console.log(`@1180 ${name}: aligned ${aligned} | overlaps ${overlaps(rr.kids).join(',') || 'none'} | inside row ${rr.kids.every((c) => c.x >= rr.row.x - 1 && c.r <= rr.row.r + 1)}`);
  }
  console.log('@1180 scrolls sideways:', await n.evaluate(() => { const m = document.querySelector('main.center'); return m.scrollWidth > m.clientWidth + 1; }));
  await n.close();

  console.log(errors.length ? errors.join('\n') : 'no console errors');
  await b.close();
})();
