// Two lists side by side: Inactive | Active. (What HALO would change is a board of its own now:
// see shots27.cjs.)
// npm run build && npx vite preview --port 4180 --strictPort
// NODE_PATH=$(npm root -g) node tools/loadtest/shots26.cjs <outdir>
const { chromium } = require('playwright');

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}

/** Row and header geometry inside one list: cells side by side, inside the row, nothing cut off. */
const GEOMETRY = (sel) => {
  const list = document.querySelector(sel);
  if (!list) return { rows: 0, problems: ['no such list: ' + sel] };
  const problems = [];
  const lines = [document.querySelector(`${sel} .hdr`), ...list.querySelectorAll('.row')].filter(Boolean);
  for (const line of lines) {
    const rb = line.getBoundingClientRect();
    const what = line.classList.contains('hdr') ? 'the column header' : (line.querySelector('.name b')?.textContent ?? '?');
    const cells = [...line.children].map((c) => ({ el: c, cls: c.className.split(' ').filter((k) => k !== 'h')[0] || '?', b: c.getBoundingClientRect() }));
    for (let i = 0; i < cells.length; i++) {
      const c = cells[i];
      if (c.b.width <= 0) continue;
      if (c.b.left < rb.left - 0.5 || c.b.right > rb.right + 0.5) problems.push(`${what}: ${c.cls} outside the row`);
      // The mod name and the package id are meant to end in an ellipsis; nothing else may be cut.
      if (c.cls !== 'name' && c.cls !== 'pkg' && c.el.scrollWidth > c.el.clientWidth + 1) problems.push(`${what}: ${c.cls} clipped (${c.el.scrollWidth} > ${c.el.clientWidth})`);
      const n = cells[i + 1];
      if (n && n.b.width > 0 && c.b.right > n.b.left + 0.5) problems.push(`${what}: ${c.cls} overlaps ${n.cls}`);
    }
  }
  if (list.scrollWidth > list.clientWidth + 1) problems.push(`the list scrolls sideways (${list.scrollWidth} > ${list.clientWidth})`);
  return { rows: list.querySelectorAll('.row').length, width: list.clientWidth, problems };
};

/** The heading strips must stay on one line each and never run into one another. */
const HEADS = () => {
  const heads = [...document.querySelectorAll('.phead')];
  const problems = [];
  for (const h of heads) if (h.scrollWidth > h.clientWidth + 1) problems.push(`heading clipped: ${h.textContent.trim()}`);
  for (let i = 1; i < heads.length; i++) {
    const a = heads[i - 1].getBoundingClientRect(), b = heads[i].getBoundingClientRect();
    if (a.right > b.left + 0.5 && a.bottom > b.top + 0.5 && a.top < b.bottom - 0.5) problems.push('headings collide');
  }
  return { text: heads.map((h) => h.textContent.replace(/\s+/g, ' ').trim()), problems };
};

/** The toolbar is one row and stays one row: nothing on a second line, nothing pushed out of it. */
const TOOLBAR = () => {
  const bar = document.querySelector('.toolbar');
  const kids = [...bar.children].filter((e) => e.getBoundingClientRect().height > 0);
  const top = Math.min(...kids.map((e) => e.getBoundingClientRect().top));
  const problems = [];
  for (const e of kids) {
    const r = e.getBoundingClientRect();
    if (r.top > top + 8) problems.push(`wrapped: ${e.textContent.trim().slice(0, 20)}`);
    if (r.right > bar.getBoundingClientRect().right + 0.5) problems.push(`over the edge: ${e.textContent.trim().slice(0, 20)}`);
  }
  if (bar.scrollWidth > bar.clientWidth + 1) problems.push(`the toolbar overflows (${bar.scrollWidth} > ${bar.clientWidth})`);
  return { width: bar.clientWidth, buttons: kids.length, problems };
};

/** uids are Windows paths, so the backslashes have to survive being put in a CSS selector. */
const esc = (uid) => uid.replace(/\\/g, '\\\\');
const panes = (p) => p.evaluate(() => [...document.querySelectorAll('.list')].map((e) => e.dataset.pane));
const heads = (p) => p.evaluate(HEADS).then((h) => h.text);
const names = (p, sel, n) => p.evaluate(([s, n]) => [...document.querySelectorAll(`${s} .row`)].slice(0, n).map((r) => `${r.querySelector('.idx').textContent.trim()}:${r.querySelector('.name b').textContent}`), [sel, n]);
const footActive = (p) => p.evaluate(() => (document.querySelector('.foot')?.textContent.match(/(\d+) active/) ?? [])[1]);

/** A handle on one mod's row in one pane, scrolled fully into that pane first: the list is
 *  virtualized, so a row off screen has no element and a half-clipped one cannot be pressed. */
async function rowIn(p, pane, uid) {
  const sel = `.list[data-pane=${pane}]`;
  for (let i = 0; i < 30; i++) {
    const state = await p.evaluate(([s, u]) => {
      const list = document.querySelector(s), row = list.querySelector(`.row[data-uid="${CSS.escape(u)}"]`);
      if (!row) { list.scrollTop += 240; return 'seeking'; }
      const a = row.getBoundingClientRect(), lb = list.getBoundingClientRect();
      if (a.top < lb.top + 50 || a.bottom > lb.bottom - 10) { list.scrollTop += a.top - lb.top - 120; return 'scrolled'; }
      return 'ok';
    }, [sel, uid]);
    if (state === 'ok') return p.$(`${sel} .row[data-uid="${esc(uid)}"]`);
    await p.waitForTimeout(120);
  }
  return p.$(`${sel} .row[data-uid="${esc(uid)}"]`);
}

async function drag(p, from, to, edge = 0.5) {
  const a = await from.boundingBox(), b = await to.boundingBox();
  await p.mouse.move(a.x + 60, a.y + a.height / 2);
  await p.mouse.down();
  await p.mouse.move(a.x + 70, a.y + a.height / 2 + 8, { steps: 4 });
  await p.mouse.move(b.x + 60, b.y + b.height * edge, { steps: 12 });
  await p.waitForTimeout(150);
  return { drop: async () => { await p.mouse.up(); await p.waitForTimeout(500); } };
}

async function checkGeometry(p, label, widths) {
  for (const w of widths) {
    await p.setViewportSize({ width: w, height: 1000 });
    await p.waitForTimeout(400);
    const list = await panes(p);
    const h = await p.evaluate(HEADS);
    const out = [];
    for (const pane of list) {
      const g = await p.evaluate(GEOMETRY, `.list[data-pane="${pane}"]`);
      out.push(`${pane} ${g.width}px/${g.rows} rows`);
      ok(`${label} @${w}: the ${pane} pane lays out cleanly`, g.problems.length === 0, g.problems.slice(0, 4).join('; '));
    }
    ok(`${label} @${w}: headings clear`, h.problems.length === 0, h.problems.join('; '));
    const t = await p.evaluate(TOOLBAR);
    ok(`${label} @${w}: the toolbar stays on one row`, t.problems.length === 0, t.problems.join('; '));
    const centre = await p.evaluate(() => document.querySelector('.center').clientWidth);
    const narrow = await p.evaluate(() => !!document.querySelector('.narrow'));
    ok(`${label} @${w}: one pane comes with the line that says why`, (list.length === 1) === narrow, `${list.length} pane(s), fallback line ${narrow ? 'shown' : 'absent'}`);
    console.log(`      ${label} @${w}: centre ${centre}px · ${out.join(' | ')} · ${h.text.join(' | ')} · toolbar ${t.width}px, ${t.buttons} controls`);
  }
}

(async () => {
  const out = process.argv[2] || '/tmp';
  const b = await chromium.launch();
  const p = await b.newPage({ viewport: { width: 1700, height: 1000 }, deviceScaleFactor: 2 });
  const errors = [];
  p.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));
  p.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 300)); });
  await p.goto('http://127.0.0.1:4180/', { waitUntil: 'load' });
  for (let i = 0; i < 40; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
  await p.waitForTimeout(500);

  const activeAtStart = await footActive(p);
  ok('starts as one list with the tabs in charge', (await panes(p)).join() === 'single' && (await p.evaluate(() => !!document.querySelector('.seg[role=tablist]'))));
  // The tightest the toolbar ever is: the tabs are still there and two new buttons sit beside them.
  for (const w of [1700, 1440, 1180]) {
    await p.setViewportSize({ width: w, height: 1000 });
    await p.waitForTimeout(300);
    const t = await p.evaluate(TOOLBAR);
    ok(`one list @${w}: the toolbar stays on one row`, t.problems.length === 0, t.problems.join('; '));
    const g = await p.evaluate(GEOMETRY, '.list[data-pane="single"]');
    ok(`one list @${w}: the single list lays out cleanly`, g.problems.length === 0, g.problems.slice(0, 4).join('; '));
    console.log(`      one list @${w}: list ${g.width}px · toolbar ${t.width}px, ${t.buttons} controls`);
  }

  // ---- the one list is what it always was ----
  await p.setViewportSize({ width: 1700, height: 1000 });
  await p.waitForTimeout(300);
  ok('the single list keeps its package-id column at every width it had it before', await p.evaluate(() => !!document.querySelector('.list .row .pkg')));
  await p.click('.seg[role=tablist] [role=tab]:has-text("All")');
  await p.waitForTimeout(400);
  ok('the All tab still puts an Inactive header in the one list', await p.evaluate(() => {
    const l = document.querySelector('.list');
    l.scrollTop = l.scrollHeight;
    return true;
  }) && (await p.waitForTimeout(400), await p.evaluate(() => [...document.querySelectorAll('.list .ph .n')].some((e) => e.textContent === 'Inactive'))));
  await p.evaluate(() => { document.querySelector('.list').scrollTop = 0; });
  await p.click('.seg[role=tablist] [role=tab]:has-text("Active")');
  await p.waitForTimeout(400);
  const before1 = await names(p, '.list', 3);
  const rows0 = await p.$$('.list .row');
  const d0 = await drag(p, rows0[0], rows0[2], 0.75);
  await d0.drop();
  ok('dragging still reorders inside the one list', (await names(p, '.list', 3)).join() !== before1.join(), `${before1.join(' | ')} → ${(await names(p, '.list', 3)).join(' | ')}`);

  // ---- Inactive | Active ----
  await p.click('.btn.split');
  await p.waitForTimeout(500);
  ok('Inactive | Active opens two panes', (await panes(p)).join() === 'inactive,active');
  ok('the tabs are gone while the panes decide', await p.evaluate(() => !document.querySelector('.seg[role=tablist]')));
  const libHeads = await heads(p);
  const counts = await p.evaluate(() => ({
    inactive: document.querySelectorAll('.list[data-pane=inactive] .row').length,
    off: document.querySelectorAll('.list[data-pane=inactive] .row.off').length,
    actOff: document.querySelectorAll('.list[data-pane=active] .row.off').length,
    idx: [...document.querySelectorAll('.list[data-pane=inactive] .row .idx')].every((e) => !e.textContent.trim()),
    heads: document.querySelectorAll('.list[data-pane=inactive] .ph').length
  }));
  ok('the inactive pane holds only inactive mods', counts.inactive > 0 && counts.off === counts.inactive && counts.idx, JSON.stringify(counts));
  ok('the inactive pane has no section headers', counts.heads === 0);
  ok('the active pane holds only active mods', counts.actOff === 0);
  ok('the headings count what each pane holds', libHeads[0] === `Inactive·${counts.inactive}` && libHeads[1] === `Active·${activeAtStart}`, libHeads.join(' | '));
  ok('a pane is too narrow for the package id, so it drops out', await p.evaluate(() => !document.querySelector('.list.pane .row .pkg')));
  await checkGeometry(p, 'library', [1700, 1440, 1180]);
  await p.setViewportSize({ width: 1700, height: 1000 });
  await p.waitForTimeout(400);
  await p.screenshot({ path: `${out}/m26-library.png` });

  // drag a mod from the inactive pane into the middle of the active one
  const moving = await p.evaluate(() => document.querySelector('.list[data-pane=inactive] .row').dataset.uid);
  const target = await p.evaluate(() => [...document.querySelectorAll('.list[data-pane=active] .row')][3].dataset.uid);
  const wantIndex = await p.evaluate((u) => Number([...document.querySelectorAll('.list[data-pane=active] .row')].find((r) => r.dataset.uid === u).querySelector('.idx').textContent), target);
  const d1 = await drag(p, await rowIn(p, 'inactive', moving), await rowIn(p, 'active', target), 0.25);
  ok('a drop line appears in the active pane', await p.evaluate(() => !!document.querySelector('.list[data-pane=active] .row.drop-before, .list[data-pane=active] .row.drop-after')));
  await d1.drop();
  const landed = await p.evaluate((u) => { const r = document.querySelector(`.list[data-pane=active] .row[data-uid="${CSS.escape(u)}"]`); return r ? Number(r.querySelector('.idx').textContent) : 0; }, moving);
  ok('dragging into the active pane activates it where it was dropped', landed === wantIndex, `landed at ${landed}, dropped at ${wantIndex}`);
  ok('one more mod is active', (await footActive(p)) === String(Number(activeAtStart) + 1), `${activeAtStart} → ${await footActive(p)}`);

  // and back out again
  const d2 = await drag(p, await rowIn(p, 'active', moving), await p.$('.list[data-pane=inactive]'), 0.75);
  ok('the inactive pane lights up as a whole', await p.evaluate(() => !!document.querySelector('.list[data-pane=inactive].take')));
  await d2.drop();
  ok('dragging into the inactive pane deactivates it', (await footActive(p)) === activeAtStart && (await p.evaluate((u) => !!document.querySelector(`.list[data-pane=inactive] .row[data-uid="${CSS.escape(u)}"]`), moving)));

  // double click still toggles, in both panes
  await (await rowIn(p, 'inactive', moving)).dblclick();
  await p.waitForTimeout(500);
  ok('double click in the inactive pane activates', (await footActive(p)) === String(Number(activeAtStart) + 1));
  await (await rowIn(p, 'active', moving)).dblclick();
  await p.waitForTimeout(500);
  ok('double click in the active pane deactivates', (await footActive(p)) === activeAtStart);

  // reordering inside the active pane is untouched by any of that
  await p.evaluate(() => { document.querySelector('.list[data-pane=active]').scrollTop = 0; });
  await p.waitForTimeout(300);
  const before2 = await names(p, '.list[data-pane=active]', 4);
  const uids = await p.evaluate(() => [...document.querySelectorAll('.list[data-pane=active] .row')].map((r) => r.dataset.uid));
  const d5 = await drag(p, await rowIn(p, 'active', uids[0]), await rowIn(p, 'active', uids[3]), 0.75);
  await d5.drop();
  await p.evaluate(() => { document.querySelector('.list[data-pane=active]').scrollTop = 0; });
  await p.waitForTimeout(300);
  ok('dragging within the active pane still reorders', (await names(p, '.list[data-pane=active]', 4)).join() !== before2.join(), `${before2.join(' | ')} → ${(await names(p, '.list[data-pane=active]', 4)).join(' | ')}`);

  await p.click('.btn.split');
  await p.waitForTimeout(400);
  ok('Inactive | Active closes again', (await panes(p)).join() === 'single' && (await p.evaluate(() => !!document.querySelector('.seg[role=tablist]'))));

  // The order you have beside the one HALO proposes used to be a second pair of panes here. It
  // is a board of its own now — two bars and a line per mod actually lifted out of its place —
  // and shots27.cjs is what checks it.

  console.log(errors.length ? errors.join('\n') : 'no console errors');
  ok('no console errors', errors.length === 0);
  console.log(failures ? `${failures} FAILURES` : 'all checks passed');
  await b.close();
  process.exit(failures ? 1 : 0);
})();
