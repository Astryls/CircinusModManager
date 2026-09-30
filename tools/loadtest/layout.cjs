// Layout, and the promise that it is only layout.
//
// The reason this file is careful is that an update moved people's load orders and their
// organization, and this change adds three more ways to redraw a list. So the first assertion
// is not about headings at all: it is that switching layout writes nothing. `update_settings`
// and `update_user` are the two calls that reach the file holding somebody's groups and folders
// -- the old `listByPhase` went through the first of them on every click -- and the mock counts
// them, so a layout control that quietly saves fails here.
//
// After that, the shape: phases outer, the user's groups inner, a band of the player's own
// sitting in the run of phases, and a header that distinguishes nothing never drawn at all.
//
//   npm run build && npx vite preview --port 4173 --strictPort
//   node tools/loadtest/layout.cjs [outdir]
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4173';
const OUT = process.argv[2] || '/tmp/layout';

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}

/* Seed ONCE. `addInitScript` runs on every navigation, so writing the layout in it
   unconditionally would restore the starting value on reload -- and the reload assertion is
   precisely that the choice survives one. */
const seed = (layout) => (page) =>
  page.addInitScript((l) => {
    if (!localStorage.getItem('circinus.layout')) localStorage.setItem('circinus.layout', JSON.stringify(l));
  }, layout);

/** Headers as a reader sees them: band names flush, group names indented.
 *
 *  The list is virtualised, so only a screenful exists in the DOM at a time. Every assertion
 *  about the shape has to walk the whole thing, or it is really an assertion about whatever
 *  happened to be at the top. */
async function headings(p) {
  const seen = new Map();
  const step = await p.evaluate(() => document.querySelector('.list').clientHeight - 60);
  const total = await p.evaluate(() => document.querySelector('.spacer').getBoundingClientRect().height);
  for (let y = 0; y < total + step; y += step) {
    await p.evaluate((top) => (document.querySelector('.list').scrollTop = top), y);
    await p.waitForTimeout(90);
    for (const h of await p.evaluate(() =>
      [...document.querySelectorAll('.list .ph')].map((x) => ({
        y: Math.round(x.getBoundingClientRect().top),
        depth: x.classList.contains('sub') ? 1 : 0,
        mine: x.classList.contains('mine'),
        text: x.querySelector('.n')?.textContent.trim() ?? '',
        count: x.querySelector('.c')?.textContent.trim() ?? ''
      }))
    ))
      seen.set(`${h.depth}:${h.text}:${h.count}`, h);
  }
  await p.evaluate(() => (document.querySelector('.list').scrollTop = 0));
  await p.waitForTimeout(120);
  return [...seen.values()];
}

/** Are any rows drawn one step in, anywhere in the list? */
async function anyIndented(p) {
  const step = await p.evaluate(() => document.querySelector('.list').clientHeight - 60);
  const total = await p.evaluate(() => document.querySelector('.spacer').getBoundingClientRect().height);
  for (let y = 0; y < total + step; y += step) {
    await p.evaluate((top) => (document.querySelector('.list').scrollTop = top), y);
    await p.waitForTimeout(90);
    if (await p.evaluate(() => document.querySelectorAll('.list .row.ind').length)) {
      await p.evaluate(() => (document.querySelector('.list').scrollTop = 0));
      return true;
    }
  }
  await p.evaluate(() => (document.querySelector('.list').scrollTop = 0));
  return false;
}

(async () => {
  const browser = await chromium.launch();
  const errors = [];
  const page = await browser.newPage({ viewport: { width: 1500, height: 940 }, colorScheme: 'dark' });
  page.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));
  // Linked, so both panes start on the same layout -- anything else is not a state the
  // controls can produce.
  await seed({ order: 'phase', active: 'phase', inactive: 'phase', linked: true, seeded: true })(page);
  await page.goto(BASE, { waitUntil: 'load' });
  await page.waitForSelector('.list .row[data-uid]', { timeout: 15000 });
  await page.waitForTimeout(800);

  // ---- the shape ------------------------------------------------------------------------------
  let hs = await headings(page);
  ok('the list is drawn in bands', hs.some((h) => h.depth === 0 && /Preloads/i.test(h.text)), hs.slice(0, 3).map((h) => h.text).join(' | '));
  ok('with the user groups inside them', hs.some((h) => h.depth === 1), hs.filter((h) => h.depth === 1).map((h) => h.text).join(' | '));
  // A heading that distinguishes nothing is furniture: "Game and DLC" followed by "Core" is the
  // same fact twice, and a phase whose members are all ungrouped gains nothing from saying so.
  hs.sort((a, b) => a.y - b.y);
  const pairs = hs.map((h, i) => [h, hs[i + 1]]).filter(([a, b]) => a?.depth === 0 && b?.depth === 1);
  const pointless = pairs.filter(([a, b]) => a.count === b.count && (b.text === 'Ungrouped' || b.text === a.text));
  ok('and no heading that distinguishes nothing', pointless.length === 0, pointless.map(([a, b]) => `${a.text} > ${b.text}`).join(', '));
  // Rows under a group heading are indented, so the indent says which heading a row belongs to
  // even when the heading has scrolled off.
  ok('rows under a group heading are indented', await anyIndented(page));
  await page.screenshot({ path: `${OUT}/layout-phase.png` });

  // ---- switching layout writes nothing ---------------------------------------------------------
  // The whole point of the change. `listByPhase` used to go through update_settings, so glancing
  // at the phases wrote the file that holds the groups and the folder paths.
  await page.evaluate(() => [...document.querySelectorAll('.toolbar .lay button')].find((b) => /Load order|Order/.test(b.textContent))?.click());
  await page.waitForTimeout(600);
  hs = await headings(page);
  ok('switching to the flat layout removes the bands', hs.filter((h) => h.depth === 0).length <= 1, hs.map((h) => h.text).join(' | '));
  const rowsFlat = await page.evaluate(() => [...document.querySelectorAll('.list .row[data-uid] .idx')].slice(0, 6).map((x) => x.textContent.trim()));
  ok('and the numbers run 1, 2, 3 down the side', rowsFlat.join(',') === '1,2,3,4,5,6', rowsFlat.join(','));
  ok('the choice is remembered outside user data', (await page.evaluate(() => JSON.parse(localStorage.getItem('circinus.layout')).order)) === 'flat');
  // It survives a reload, and it is still not in the snapshot the backend holds.
  await page.reload({ waitUntil: 'load' });
  await page.waitForSelector('.list .row[data-uid]', { timeout: 15000 });
  await page.waitForTimeout(700);
  ok('and outlives a reload', (await page.evaluate(() => document.querySelectorAll('.list .ph').length)) <= 1);
  await page.evaluate(() => [...document.querySelectorAll('.toolbar .lay button')].find((b) => /phase/i.test(b.textContent))?.click());
  await page.waitForTimeout(600);

  // ---- a band of the player's own -----------------------------------------------------------
  hs = await headings(page);
  const band = hs.find((h) => h.depth === 0 && h.mine);
  ok("a band of the player's own sits in the run of phases", !!band, hs.filter((h) => h.depth === 0).map((h) => h.text).join(' | '));
  if (band) {
    const i = hs.indexOf(band);
    // It is one of the user's groups, so a sub-heading repeating its name would be the same
    // word twice in a row.
    ok('and does not repeat its own name inside itself', hs[i + 1]?.depth !== 1 || hs[i + 1]?.text !== band.text, hs[i + 1]?.text ?? '');
  }

  // ---- the sidebar ------------------------------------------------------------------------------
  await page.evaluate(() => (document.querySelector('.list').scrollTop = 0));
  await page.waitForTimeout(200);
  const jumped = await page.evaluate(async () => {
    const before = document.querySelector('.list').scrollTop;
    [...document.querySelectorAll('.groups .g')].find((g) => /Oskar/.test(g.textContent))?.click();
    await new Promise((r) => setTimeout(r, 700));
    return { before, after: document.querySelector('.list').scrollTop, lit: document.querySelectorAll('.list .ph.lit').length, rows: document.querySelectorAll('.list .row[data-uid]').length };
  });
  ok('clicking a group scrolls the list to it', jumped.after > jumped.before, `${jumped.before} -> ${jumped.after}`);
  ok('and marks where it landed', jumped.lit >= 1, String(jumped.lit));
  // The old behaviour hid every other mod. Jumping must not.
  ok('and hides nothing', jumped.rows > 10, String(jumped.rows));

  await page.evaluate(() => {
    const g = [...document.querySelectorAll('.groups .g')].find((x) => /Performance/.test(x.textContent));
    g?.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
  });
  await page.waitForTimeout(500);
  ok('double-clicking one opens its editor', await page.evaluate(() => !!document.querySelector('.ged')));
  await page.evaluate(() => {
    const g = [...document.querySelectorAll('.groups .g')].find((x) => /Oskar/.test(x.textContent));
    g?.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
  });
  await page.waitForTimeout(700);
  const filtered = await page.evaluate(() => ({ rows: document.querySelectorAll('.list .row[data-uid]').length, clear: document.querySelector('.groups .clear')?.textContent.trim() ?? '' }));
  ok('right-clicking still narrows to one group', filtered.rows > 0 && filtered.rows < 20, String(filtered.rows));
  ok('and says which, with the way back', /Oskar/.test(filtered.clear), filtered.clear);
  await page.evaluate(() => document.querySelector('.groups .clear')?.click());
  await page.waitForTimeout(500);

  // ---- the split, and the link ------------------------------------------------------------------
  await page.evaluate(() => [...document.querySelectorAll('.toolbar .btn')].find((x) => /split/i.test(x.className))?.click());
  await page.waitForTimeout(1100);
  ok('each pane has its own layout control', (await page.evaluate(() => document.querySelectorAll('.phead .lay').length)) === 2);
  const geom = await page.evaluate(() => {
    const box = (s) => {
      const r = document.querySelector(s).getBoundingClientRect();
      return { l: Math.round(r.left), r: Math.round(r.right) };
    };
    return { left: box('.panes > *:nth-child(1) .lay'), link: box('.panes .link'), right: box('.panes > *:nth-child(2) .lay'), title: box('.panes > *:nth-child(2) .phead .t') };
  });
  // The link belongs to neither pane, so it must not touch either pane's furniture.
  ok('the link sits between the two, touching neither', geom.link.l > geom.left.r && geom.link.r < geom.right.l && geom.link.r <= geom.title.l, JSON.stringify(geom));
  ok('and is bare, with no box around it', (await page.evaluate(() => getComputedStyle(document.querySelector('.panes .link')).boxShadow)) === 'none');

  const onOf = () => page.evaluate(() => [...document.querySelectorAll('.panes .lay')].map((l) => [...l.querySelectorAll('button')].findIndex((b) => b.classList.contains('on'))));
  ok('both panes start on the same layout', JSON.stringify(await onOf()) === JSON.stringify([1, 1]), JSON.stringify(await onOf()));
  await page.evaluate(() => document.querySelector('.panes > *:nth-child(1) .lay button')?.click());
  await page.waitForTimeout(600);
  ok('while linked, setting one sets the other', JSON.stringify(await onOf()) === JSON.stringify([0, 0]), JSON.stringify(await onOf()));
  ok('the glyph reads as an equals', (await page.evaluate(() => document.querySelector('.panes .link').textContent.trim())) === '=');
  await page.evaluate(() => document.querySelector('.panes .link')?.click());
  await page.waitForTimeout(400);
  ok('unlinking says so', (await page.evaluate(() => document.querySelector('.panes .link').textContent.trim())) === '≠');
  // Unlinking on its own must change nothing: the button is never a surprise.
  ok('and changes nothing on screen by itself', JSON.stringify(await onOf()) === JSON.stringify([0, 0]), JSON.stringify(await onOf()));
  await page.evaluate(() => [...document.querySelectorAll('.panes > *:nth-child(2) .lay button')][1]?.click());
  await page.waitForTimeout(600);
  ok('and the panes can then differ', JSON.stringify(await onOf()) === JSON.stringify([0, 1]), JSON.stringify(await onOf()));
  await page.screenshot({ path: `${OUT}/layout-split.png` });

  console.log('errors:', errors.length ? errors.join('\n') : 'none');
  if (errors.length) failures += errors.length;
  await browser.close();
  console.log(failures ? `\n${failures} failed.` : '\nall good');
  process.exit(failures ? 1 : 0);
})();
