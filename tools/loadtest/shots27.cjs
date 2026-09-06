// What HALO would change: the board of lines between the order you have and the one it proposes.
// Driven against a thousand-mod list, because forty mods prove nothing about a view built for a
// thousand — the previous side-by-side lists looked fine on the small mock and were unreadable on
// a real install.
//
// npm run build && npx vite preview --port 4180 --strictPort
// NODE_PATH=$(npm root -g) node tools/loadtest/shots27.cjs <outdir>
const { chromium } = require('playwright');

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}

/** Nothing on the board may sit outside it, and the two bars must not drift out of line. */
const BOARD = () => {
  const board = document.querySelector('.board');
  if (!board) return { problems: ['no board'] };
  const b = board.getBoundingClientRect();
  const problems = [];
  const bars = [...board.querySelectorAll('.bar')];
  if (bars.length !== 2) problems.push(`expected two bars, found ${bars.length}`);
  const [l, r] = bars.map((e) => e.getBoundingClientRect());
  if (bars.length === 2) {
    if (Math.abs(l.top - r.top) > 0.5 || Math.abs(l.height - r.height) > 0.5) problems.push('the two bars are not level');
    if (l.right >= r.left) problems.push('the bars overlap');
    if (l.height < 120) problems.push(`the bars are only ${Math.round(l.height)}px tall`);
  }
  for (const e of board.querySelectorAll('.bar, .side, .wires, .cap, .hint')) {
    const q = e.getBoundingClientRect();
    if (q.width <= 0 || q.height <= 0) continue;
    if (q.left < b.left - 0.5 || q.right > b.right + 0.5 || q.top < b.top - 0.5 || q.bottom > b.bottom + 0.5)
      problems.push(`${e.className.split(' ')[0]} outside the board`);
  }
  // Every phase label has to fit its column, stay on the board, and clear the ones above it.
  for (const side of board.querySelectorAll('.side')) {
    let prev = null;
    for (const lbl of [...side.querySelectorAll('.lbl')].sort((a, z) => a.getBoundingClientRect().top - z.getBoundingClientRect().top)) {
      if (lbl.scrollWidth > lbl.clientWidth + 1) problems.push(`label clipped: ${lbl.textContent}`);
      const q = lbl.getBoundingClientRect();
      if (q.top < b.top - 0.5 || q.bottom > b.bottom + 0.5) problems.push(`label off the board: ${lbl.textContent}`);
      if (prev && q.top < prev.bottom - 0.5) problems.push(`labels collide: ${lbl.textContent}`);
      prev = q;
    }
  }
  const caps = [...board.querySelectorAll('.cap')].map((e) => e.getBoundingClientRect());
  if (caps.length === 2 && caps[0].right > caps[1].left + 0.5) problems.push('the two captions run into each other');
  // Bands tile their bar exactly: no gaps, no overlaps, nothing past either end.
  for (const [name, bar] of [['left', bars[0]], ['right', bars[1]]]) {
    if (!bar) continue;
    const rect = bar.getBoundingClientRect();
    const bands = [...bar.querySelectorAll('.band')].map((e) => e.getBoundingClientRect());
    if (!bands.length) { problems.push(`${name} bar has no bands`); continue; }
    if (bands[0].top < rect.top - 1 || bands[bands.length - 1].bottom > rect.bottom + 1) problems.push(`${name} bands past the bar`);
    for (let i = 1; i < bands.length; i++) if (Math.abs(bands[i].top - bands[i - 1].bottom) > 1.5) problems.push(`${name} bands leave a gap at ${i}`);
  }
  return { problems, bar: bars[0] ? Math.round(bars[0].getBoundingClientRect().height) : 0 };
};

/** Every line starts on the left bar and ends on the right one, at the mod's own place in each. */
const WIRES = () => {
  const board = document.querySelector('.board');
  const svg = board.querySelector('.wires');
  // A path's own coordinates, read against the box it is drawn in rather than the board: getting
  // those two mixed up is what put the lines a column's width to the right of where they belonged.
  const s = svg.getBoundingClientRect();
  const [l, r] = [...board.querySelectorAll('.bar')].map((e) => e.getBoundingClientRect());
  // Direct children only: the arrowheads inside <defs> are paths too.
  const paths = [...svg.querySelectorAll(':scope > path')];
  const problems = [];
  for (const p of paths) {
    const a = p.getPointAtLength(0);
    const z = p.getPointAtLength(p.getTotalLength());
    if (Math.abs(a.x + s.left - l.right) > 1.5) problems.push('a line does not start at the left bar');
    if (z.x + s.left > r.left + 0.5 || z.x + s.left < r.left - 12) problems.push('a line does not reach the right bar');
    for (const y of [a.y + s.top, z.y + s.top]) if (y < l.top - 1 || y > l.bottom + 1) problems.push('a line ends outside the bars');
    // Nothing may be drawn over the sentence under the board.
    if (Math.min(a.y + s.top, z.y + s.top) < l.top - 1) problems.push('a line runs above the bars');
  }
  return { count: paths.length, problems };
};

/** Rows: two lines each, nothing clipped, nothing spilling out of the row. */
const ROWS = () => {
  const problems = [];
  const rows = [...document.querySelectorAll('.rows .mv')];
  for (const row of rows.slice(0, 60)) {
    const rb = row.getBoundingClientRect();
    for (const cell of row.querySelectorAll('.dist, .pill, .pos, .within, .two > span')) {
      const q = cell.getBoundingClientRect();
      if (q.width <= 0) continue;
      if (q.left < rb.left - 0.5 || q.right > rb.right + 0.5) problems.push(`${cell.className.split(' ')[0]} outside the row`);
      // Names and the sentences under them may end in an ellipsis; the short facts may not.
      if (/dist|pill|pos|within/.test(cell.className) && cell.scrollWidth > cell.clientWidth + 1) problems.push(`${cell.className.split(' ')[0]} clipped: ${cell.textContent.trim()}`);
    }
    const lines = [...row.querySelectorAll('.line')].map((e) => e.getBoundingClientRect());
    if (lines.length === 2 && lines[0].bottom > lines[1].top + 0.5) problems.push('the two lines of a row overlap');
  }
  const box = document.querySelector('.rows');
  if (box && box.scrollWidth > box.clientWidth + 1) problems.push('the list scrolls sideways');
  return { rows: rows.length, problems };
};

const summary = (p) => p.evaluate(() => document.querySelector('.sum p').textContent.replace(/\s+/g, ' ').trim());

async function open(p, url) {
  await p.goto(url, { waitUntil: 'load' });
  for (let i = 0; i < 80; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
  await p.waitForTimeout(600);
}

async function check(p, label, widths) {
  for (const w of widths) {
    await p.setViewportSize({ width: w, height: 1000 });
    await p.waitForTimeout(500);
    const board = await p.evaluate(BOARD);
    const wires = await p.evaluate(WIRES);
    const rows = await p.evaluate(ROWS);
    ok(`${label} @${w}: the board lays out cleanly`, board.problems.length === 0, board.problems.slice(0, 4).join('; '));
    ok(`${label} @${w}: every line runs bar to bar`, wires.problems.length === 0, [...new Set(wires.problems)].join('; '));
    ok(`${label} @${w}: one line per move`, wires.count === rows.rows, `${wires.count} lines, ${rows.rows} rows`);
    ok(`${label} @${w}: the rows lay out cleanly`, rows.problems.length === 0, [...new Set(rows.problems)].slice(0, 4).join('; '));
    console.log(`      ${label} @${w}: bars ${board.bar}px · ${wires.count} lines · ${rows.rows} rows`);
  }
}

(async () => {
  const out = process.argv[2] || '/tmp';
  const base = 'http://127.0.0.1:4180/';
  const b = await chromium.launch();
  const p = await b.newPage({ viewport: { width: 1700, height: 1000 }, deviceScaleFactor: 2 });
  const errors = [];
  p.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));
  p.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 300)); });

  // ---- a real install: 1100 mods with 30 of them out of place ----
  await open(p, `${base}?big=1100&jumble=30`);
  const total = await p.evaluate(() => Number((document.querySelector('.foot').textContent.match(/(\d+) active/) ?? [])[1]));
  ok('the big mock really is a big list', total > 900, `${total} active`);
  await p.click('.btn.primary:has-text("Sort with HALO")');
  await p.waitForTimeout(1200);
  const proposed = await p.evaluate(() => Number((document.querySelector('.btn.primary').textContent.match(/(\d+) move/) ?? [])[1]));
  ok('HALO proposes hundreds of new numbers on a list this long', proposed > 100, `${proposed} moves`);
  await p.click('.btn.split:has-text("What changes")');
  await p.waitForTimeout(900);
  ok('the button opens the board rather than two lists', await p.evaluate(() => !!document.querySelector('.board') && !document.querySelector('.list[data-pane]')));

  const sum = await summary(p);
  const moved = Number((sum.match(/move (\d+) mod/) ?? [])[1]);
  const drift = Number((sum.match(/([\d,]+) other mods/) ?? [])[1]?.replace(/,/g, ''));
  ok('the moves are told apart from the drift', moved > 0 && drift > 0, sum);
  ok('the moves are far fewer than the changed numbers', moved < proposed / 3, `${moved} moves against ${proposed} new numbers`);
  ok('the two add up to what HALO reported', moved + drift <= proposed + 2 && moved + drift >= proposed - 2, `${moved} + ${drift} vs ${proposed}`);
  console.log(`      ${sum}`);
  await check(p, 'big list', [1700, 1440, 1180]);
  await p.setViewportSize({ width: 1700, height: 1000 });
  await p.waitForTimeout(400);
  await p.screenshot({ path: `${out}/m27-board.png` });

  // hovering a row lights its line and both of its ends
  await p.hover('.rows .mv');
  await p.waitForTimeout(250);
  const lit = await p.evaluate(() => ({
    hot: document.querySelectorAll('.wires path.hot').length,
    cold: document.querySelectorAll('.wires path.cold').length,
    ticks: document.querySelectorAll('.tick.hot').length
  }));
  ok('hovering a move lights exactly its line', lit.hot === 1, JSON.stringify(lit));
  ok('and dims the others', lit.cold >= 1);
  ok('and marks where it leaves and where it lands', lit.ticks === 2, `${lit.ticks} marks`);
  await p.screenshot({ path: `${out}/m27-hover.png` });

  // clicking a phase keeps only the moves that touch it
  const before = (await p.evaluate(ROWS)).rows;
  await p.click('.bar.r .band:nth-of-type(3)');
  await p.waitForTimeout(500);
  const after = await p.evaluate(ROWS);
  const chip = await p.evaluate(() => document.querySelector('.sum .chip')?.textContent.trim());
  ok('clicking a phase narrows the board to it', after.rows < before && after.rows > 0, `${before} → ${after.rows} moves, chip "${chip}"`);
  ok('and says which phase, with a way back', !!chip);
  ok('the lines follow the narrowing', (await p.evaluate(WIRES)).count === after.rows);
  await p.screenshot({ path: `${out}/m27-phase.png` });
  await p.click('.sum .chip');
  await p.waitForTimeout(400);
  ok('the chip puts every move back', (await p.evaluate(ROWS)).rows === before);

  // the search box narrows the board to the mod you are looking for
  const one = await p.evaluate(() => document.querySelector('.rows .mv .nm').textContent.trim());
  await p.fill('#search', one);
  await p.waitForTimeout(600);
  const found = await p.evaluate(ROWS);
  ok('searching narrows the board to the mods that match', found.rows >= 1 && found.rows < before, `"${one}" → ${found.rows} of ${before}`);
  ok('and the lines narrow with it', (await p.evaluate(WIRES)).count === found.rows);
  ok('and the count says how many are shown', /shown/.test(await p.evaluate(() => document.querySelector('.sum').textContent)));
  await p.fill('#search', 'zzzz no such mod');
  await p.waitForTimeout(500);
  ok('a search that matches nothing says so instead of showing an empty board', await p.evaluate(() => !!document.querySelector('.rows .empty')));
  await p.fill('#search', '');
  await p.waitForTimeout(600);
  ok('clearing the search puts every move back', (await p.evaluate(ROWS)).rows === before);

  // clicking a move selects that mod, so the inspector explains it
  const name = await p.evaluate(() => document.querySelector('.rows .mv .nm').textContent.trim());
  await p.click('.rows .mv .hit');
  await p.waitForTimeout(400);
  ok('clicking a move selects the mod it names', await p.evaluate((n) => (document.querySelector('.inspector')?.textContent ?? '').includes(n), name), name);

  // ---- the worst case: a list that has been shuffled from end to end ----
  await open(p, `${base}?big=1100&jumble=600`);
  await p.click('.btn.primary:has-text("Sort with HALO")');
  await p.waitForTimeout(1500);
  await p.click('.btn.split:has-text("What changes")');
  await p.waitForTimeout(1500);
  console.log(`      ${await summary(p)}`);
  await check(p, 'shuffled', [1700, 1180]);
  await p.setViewportSize({ width: 1700, height: 1000 });
  await p.waitForTimeout(400);
  await p.screenshot({ path: `${out}/m27-shuffled.png` });

  // ---- the ordinary mock: the toolbar, and a list HALO barely touches ----
  await open(p, base);
  await p.click('.btn.primary:has-text("Sort with HALO")');
  await p.waitForTimeout(700);
  await p.click('.btn.split:has-text("What changes")');
  await p.waitForTimeout(700);
  ok('the small list gets the same board', await p.evaluate(() => !!document.querySelector('.board')));
  await check(p, 'small list', [1700, 1180]);
  const bar = await p.evaluate(() => {
    const t = document.querySelector('.toolbar');
    const kids = [...t.children].filter((e) => e.getBoundingClientRect().height > 0);
    const top = Math.min(...kids.map((e) => e.getBoundingClientRect().top));
    return { over: kids.filter((e) => e.getBoundingClientRect().top > top + 8).length, wide: t.scrollWidth > t.clientWidth + 1, arr: !!t.querySelector('.seg.arr'), tabs: !!t.querySelector('[role=tablist]') };
  });
  ok('the toolbar stays on one row with the board open', bar.over === 0 && !bar.wide, JSON.stringify(bar));
  ok('the tabs and the arrangement toggle step aside, having nothing to arrange', !bar.arr && !bar.tabs);
  await p.screenshot({ path: `${out}/m27-small.png` });

  // discarding the preview closes the board, since there is nothing left to compare
  await p.click('.btn:has-text("Discard")');
  await p.waitForTimeout(600);
  ok('discarding the preview closes the board and gives the list back', await p.evaluate(() => !document.querySelector('.board') && !!document.querySelector('.list')));

  ok('no errors in the console', errors.length === 0, errors.slice(0, 3).join(' | '));
  await b.close();
  console.log(failures ? `\n${failures} failed` : '\nall good');
  process.exit(failures ? 1 : 0);
})();
