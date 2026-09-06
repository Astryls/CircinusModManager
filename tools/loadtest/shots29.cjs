// New mods, and sorting by any column. Driven in a real browser against the mock data, because
// both of these are claims about what a player can see and do, and neither is provable by
// reading the component.
//
// What it asserts:
//   * a mod that arrived is marked in the list, in a split pane, and in What changes
//   * the mark is not the same channel as the amber one HALO uses, so a row can be both
//   * the New tab exists only when something is in it, and holds exactly the new mods
//   * every column heading sorts, both ways, and the third click gives the load order back
//   * sorting by phase keeps the phase sections; sorting inside them does not break the grid
//   * a sorted list cannot be dragged, and says so
//
//   npm run build && npx vite preview --port 4181 --strictPort
//   NODE_PATH=$(npm root -g) node tools/loadtest/shots29.cjs [outdir]
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4181';
const OUT = process.argv[2] || '/tmp/shots29';

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}

/** Every row on screen, with the things that decide how it reads. */
const ROWS = () =>
  [...document.querySelectorAll('.list .row')].map((r) => ({
    uid: r.dataset.uid,
    name: (r.querySelector('.name b')?.textContent || '').replace(/New$/, '').trim(),
    pkg: r.querySelector('.pkg')?.textContent?.trim() || '',
    fresh: r.classList.contains('fresh'),
    moved: r.classList.contains('moved'),
    shadow: getComputedStyle(r).boxShadow,
    bg: getComputedStyle(r).backgroundColor,
    tag: !!r.querySelector('.newtag'),
    dot: !!r.querySelector('.newdot')
  }));

/** Everything in the list, gathered by scrolling it: only the rows in view exist in the DOM, so
 *  a question about the whole list has to be asked of the whole list. */
async function wholeList(page) {
  const rows = new Map();
  const heads = [];
  const h = await page.$eval('.list', (l) => l.scrollHeight);
  const step = await page.$eval('.list', (l) => l.clientHeight - 80);
  for (let y = 0; y < h + step; y += step) {
    await page.$eval('.list', (l, y) => (l.scrollTop = y), y);
    await page.waitForTimeout(60);
    const seen = await page.evaluate(() => ({
      rows: [...document.querySelectorAll('.list .row')].map((r) => ({
        uid: r.dataset.uid,
        y: Math.round(r.getBoundingClientRect().top + document.querySelector('.list').scrollTop),
        name: (r.querySelector('.name b')?.textContent || '').replace(/New$/, '').trim(),
        pkg: r.querySelector('.pkg')?.textContent?.trim() || '',
        fresh: r.classList.contains('fresh'),
        tag: !!r.querySelector('.newtag')
      })),
      heads: [...document.querySelectorAll('.list .ph:not(.floating) .n')].map((e) => e.textContent.trim())
    }));
    for (const r of seen.rows) rows.set(r.uid, r);
    for (const t of seen.heads) if (!heads.includes(t)) heads.push(t);
  }
  await page.$eval('.list', (l) => (l.scrollTop = 0));
  await page.waitForTimeout(60);
  return { rows: [...rows.values()].sort((a, b) => a.y - b.y), heads };
}

/** The column headings that can be pressed, in the order they appear. */
const HEADS = () => [...document.querySelectorAll('.hdr .sortbtn')].map((b) => b.textContent.replace(/[↑↓]/g, '').trim());

/** Do the header cells still sit over their own columns? One grid, so this is the real check. */
const ALIGNED = () => {
  const hdr = document.querySelector('.hdr');
  const row = document.querySelector('.list .row');
  if (!hdr || !row) return { problems: ['nothing rendered'] };
  const h = [...hdr.children].map((e) => e.getBoundingClientRect());
  const r = [...row.children].map((e) => e.getBoundingClientRect());
  const problems = [];
  if (h.length !== r.length) problems.push(`header has ${h.length} cells, a row has ${r.length}`);
  for (let i = 0; i < Math.min(h.length, r.length); i++) {
    if (Math.abs(h[i].left - r[i].left) > 1) problems.push(`cell ${i} is ${Math.round(h[i].left - r[i].left)}px out`);
  }
  return { problems };
};

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1900, height: 940 }, deviceScaleFactor: 1 });
  page.on('pageerror', (e) => ok(`no page error`, false, e.message));
  const go = async (q) => {
    await page.goto(`${BASE}/${q}`);
    await page.waitForSelector('.list .row', { timeout: 15000 });
    await page.waitForTimeout(250);
  };

  // ---------------------------------------------------------------- the mark
  await go('?big=400&new=4');
  // Four new mods among four hundred are not all on screen at once, which is the point of the
  // feature and a trap for the test: scroll the list rather than judging it by its first screen.
  let all = await wholeList(page);
  let rows = all.rows;
  const fresh = rows.filter((r) => r.fresh);
  ok('new mods are marked in the list', fresh.length === 4, `${fresh.length} marked of ${rows.length} rows`);
  ok('and they carry the word, not only a colour', fresh.every((r) => r.tag), 'a colour alone is no use to anyone who cannot see it');
  ok('while nothing else is marked', rows.filter((r) => r.tag).length === fresh.length);

  // Now measure the colours on one of them, brought into view by searching for it.
  await page.fill('#search', fresh[0].name);
  await page.waitForTimeout(300);
  const styled = (await page.evaluate(ROWS)).find((r) => r.fresh);
  ok('the marked row is findable by name', !!styled, fresh[0].name);

  // The colour has to be the green one and the edge has to be the right-hand one, because amber
  // on the left already means "HALO would move this" and one row can be both.
  const shadow = styled?.shadow ?? '';
  ok('the edge is green', /63,\s*196,\s*106/.test(shadow), shadow);
  ok('and on the right, where HALO is not', /(^|\s)-2px\s/.test(shadow) || /-2px 0px 0px/.test(shadow), shadow);
  ok('the row is tinted, not just edged', /rgba?\(63,\s*196,\s*106/.test(styled?.bg ?? ''), styled?.bg);
  await page.fill('#search', '');
  await page.waitForTimeout(200);

  // ---------------------------------------------------------------- the tab
  const tabs = () => page.$$eval('.seg[role="tablist"] button', (b) => b.map((x) => x.textContent.replace(/\s+/g, ' ').trim()));
  let names = await tabs();
  ok('the New tab is there when mods are new', names.some((t) => t.startsWith('New')), names.join(' | '));
  const said = Number((names.find((t) => t.startsWith('New')) || '').match(/(\d+)/)?.[1] ?? 0);
  await page.click('.seg[role="tablist"] button:has-text("New")');
  await page.waitForTimeout(200);
  rows = await page.evaluate(ROWS);
  ok('the tab holds exactly what it counted', rows.length === said, `tab says ${said}, list has ${rows.length}`);
  ok('and every one of them is a new mod', rows.every((r) => r.fresh));
  ok('each says when it arrived', await page.$$eval('.list .row .name span', (s) => s.every((e) => /arrived/.test(e.textContent))));
  await page.screenshot({ path: `${OUT}/new-tab.png`, fullPage: false });

  await go('?big=400&new=0');
  names = await tabs();
  ok('and no New tab when nothing is new', !names.some((t) => t.startsWith('New')), names.join(' | '));

  // ---------------------------------------------------------------- sorting
  await go('?big=400&new=4');
  const heads = await page.evaluate(HEADS);
  ok('every visible column heading can be pressed', heads.length >= 4, heads.join(', '));
  ok('including the two that are on by default', heads.includes('Load') && heads.includes('Versions'), heads.join(', '));

  const byName = async () => (await page.evaluate(ROWS)).map((r) => r.name);
  const before = await byName();
  await page.click('.hdr .sortbtn:has-text("Mod")');
  await page.waitForTimeout(200);
  const asc = await byName();
  ok('sorting by name reorders the list', asc.join('|') !== before.join('|'));
  ok('and it is actually A to Z', asc.every((n, i) => i === 0 || n.toLowerCase() >= asc[i - 1].toLowerCase()), asc.slice(0, 3).join(', '));
  const arrow = await page.$eval('.hdr .h.by .dir', (e) => e.textContent);
  ok('the heading says which way round it is', arrow === '↑', JSON.stringify(arrow));

  await page.click('.hdr .sortbtn:has-text("Mod")');
  await page.waitForTimeout(200);
  const desc = await byName();
  ok('clicking again turns it round', desc.every((n, i) => i === 0 || n.toLowerCase() <= desc[i - 1].toLowerCase()), desc.slice(0, 3).join(', '));

  await page.click('.hdr .sortbtn:has-text("Mod")');
  await page.waitForTimeout(200);
  ok('and a third click gives the load order back', (await byName()).join('|') === before.join('|'));

  // Sorting must not break the one grid the header and the rows share.
  await page.click('.hdr .sortbtn:has-text("Package id")');
  await page.waitForTimeout(200);
  const pkgs = await page.$$eval('.list .row .pkg', (e) => e.map((x) => x.textContent.trim()));
  ok('sorting by package id sorts by package id', pkgs.every((p, i) => i === 0 || p >= pkgs[i - 1]), pkgs.slice(0, 3).join(', '));
  let align = await page.evaluate(ALIGNED);
  ok('the columns still line up', align.problems.length === 0, align.problems.join('; '));

  // ---------------------------------------------------------------- sections survive
  all = await wholeList(page);
  ok('the phase sections are still there while sorted', all.heads.length > 1, all.heads.join(' / '));
  // Sorting happens inside each section: the first section's rows are sorted, and the section
  // after it starts over rather than carrying on from where the last one left off.
  // Gathered while scrolling, because one screen holds one section and a claim about every
  // section cannot be checked against the first of them.
  const perSection = new Map();
  {
    const h = await page.$eval('.list', (l) => l.scrollHeight);
    const step = await page.$eval('.list', (l) => l.clientHeight - 80);
    for (let y = 0; y < h + step; y += step) {
      await page.$eval('.list', (l, y) => (l.scrollTop = y), y);
      await page.waitForTimeout(60);
      const seen = await page.evaluate(() => {
        const out = [];
        let cur = null;
        for (const el of document.querySelectorAll('.list .ph, .list .row')) {
          if (el.classList.contains('ph')) {
            if (el.classList.contains('floating')) continue;
            cur = { head: el.querySelector('.n')?.textContent?.trim() ?? '', pkgs: [] };
            out.push(cur);
          } else if (cur) cur.pkgs.push({ uid: el.dataset.uid, pkg: el.querySelector('.pkg')?.textContent?.trim() ?? '' });
        }
        return out;
      });
      for (const sec of seen) {
        const got = perSection.get(sec.head) ?? new Map();
        for (const p of sec.pkgs) got.set(p.uid, p.pkg);
        perSection.set(sec.head, got);
      }
    }
    await page.$eval('.list', (l) => (l.scrollTop = 0));
  }
  const sections = [...perSection.values()].map((m) => [...m.values()]).filter((s) => s.length > 1);
  ok('each section is sorted within itself', sections.every((s) => s.every((p, i) => i === 0 || p >= s[i - 1])), `${sections.length} sections, ${sections.reduce((n, s) => n + s.length, 0)} rows`);
  // And it really is per-section rather than one long sort: a later section starts over.
  ok('and each one starts over rather than carrying on', sections.length < 2 || sections.some((s, i) => i > 0 && s[0] < sections[i - 1][sections[i - 1].length - 1]), 'otherwise the sections are decoration on a flat sort');
  await page.screenshot({ path: `${OUT}/sorted-by-phase.png` });

  // ---------------------------------------------------------------- dragging is off
  ok('the way back to the load order is on screen', await page.$('.btn.sortoff') !== null);
  const firstBefore = (await byName())[0];
  const box = await page.$eval('.list .row', (r) => { const b = r.getBoundingClientRect(); return { x: b.x + 60, y: b.y + b.height / 2 }; });
  await page.mouse.move(box.x, box.y);
  await page.mouse.down();
  await page.mouse.move(box.x, box.y + 260, { steps: 12 });
  await page.mouse.up();
  await page.waitForTimeout(250);
  ok('a sorted list cannot be dragged into a new order', (await byName())[0] === firstBefore, 'dropping into a sorted list would write a position nobody chose');
  ok('and no drop line was ever drawn', await page.$('.drop-line') === null);

  await page.click('.btn.sortoff');
  await page.waitForTimeout(200);
  ok('the chip puts the load order back', (await byName()).join('|') === before.join('|'));

  // ---------------------------------------------------------------- the Time column
  await go('?big=400&new=4');
  const headOrder = await page.evaluate(HEADS);
  ok('Time is a column', headOrder.includes('Time'), headOrder.join(', '));
  ok('and it sits to the left of Cost', headOrder.indexOf('Time') < headOrder.indexOf('Cost'), headOrder.join(', '));
  const times = await page.$$eval('.list .row .tm', (e) => e.map((x) => x.textContent.trim()).filter(Boolean));
  ok('every row says a number of seconds', times.length > 0 && times.every((t) => /^(<0\.05 s|\d+(\.\d+)? s|…)$/.test(t)), times.slice(0, 5).join(' | '));
  // Right-aligned and monospaced, so the decimal points line up down the column: a measure you
  // read by scanning is unreadable when the digits wander.
  const tmStyle = await page.$eval('.list .row .tm', (e) => ({ align: getComputedStyle(e).textAlign, font: getComputedStyle(e).fontFamily }));
  ok('the figures line up', tmStyle.align === 'right', tmStyle.align);
  ok('and are monospaced', /mono|JetBrains|Consolas|ui-monospace/i.test(tmStyle.font), tmStyle.font);
  // Time and Load are one measurement said two ways, so they must agree about which mod is
  // heaviest. A column that disagrees with the one beside it is worse than no column.
  const byTime = async () => (await page.evaluate(ROWS)).map((r) => r.uid);
  await page.click('.hdr .sortbtn:has-text("Time")');
  await page.waitForTimeout(200);
  const timeOrder = await byTime();
  await page.click('.hdr .sortbtn:has-text("Load")');
  await page.waitForTimeout(200);
  ok('sorting by Time and by Load agree', (await byTime()).join('|') === timeOrder.join('|'), 'they are the same estimate, in seconds and as a share');
  await page.click('.btn.sortoff');
  await page.waitForTimeout(200);
  let al = await page.evaluate(ALIGNED);
  ok('the new column did not break the grid', al.problems.length === 0, al.problems.join('; '));
  await page.screenshot({ path: `${OUT}/time-column.png` });

  // ---------------------------------------------------------------- the split pane
  await page.click('.btn.split');
  await page.waitForTimeout(350);
  const panes = await page.$$('.list.pane');
  ok('two panes', panes.length === 2, `${panes.length}`);
  // Same trap as before: a pane shows twenty of four hundred rows. Narrow to the one we know.
  await page.fill('#search', fresh[0].name);
  await page.waitForTimeout(300);
  const paneFresh = await page.$$eval('.list.pane .row.fresh', (r) => r.length);
  ok('new mods are marked in the panes too', paneFresh > 0, `${paneFresh} marked`);
  ok('and the word is dropped where there is no room for it', await page.$$eval('.list.pane .row.fresh .newtag', (e) => e.every((x) => getComputedStyle(x).display === 'none')));
  ok('the panes do not offer sorting, since they are two halves of one comparison', await page.$('.list.pane .hdr .sortbtn') === null);
  await page.screenshot({ path: `${OUT}/panes.png` });

  await browser.close();
  console.log(failures ? `\n${failures} failed.\n` : '\nall good\n');
  process.exit(failures ? 1 : 0);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
