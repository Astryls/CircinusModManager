// What HALO would change: the phases mods leave on the left, the phases they arrive in on the
// right, and a weighted arrow per journey. Driven against a thousand-mod list, because forty mods
// prove nothing about a view built for a thousand — the side-by-side lists this replaced looked
// fine on the small mock and were unreadable on a real install.
//
// npm run build && npx vite preview --port 4180 --strictPort
// NODE_PATH=$(npm root -g) node tools/loadtest/shots27.cjs <outdir>
const { chromium } = require('playwright');

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}

/** The groups: nothing clipped, nothing outside its column, the two columns clear of each other. */
const BOARD = () => {
  const board = document.querySelector('.board');
  if (!board) return { problems: ['no board'] };
  const pair = board.querySelector('.pair');
  const cols = [...pair.querySelectorAll(':scope > .col')];
  const problems = [];
  if (cols.length !== 2) problems.push(`expected two columns, found ${cols.length}`);
  const [lc, rc] = cols.map((e) => e.getBoundingClientRect());
  if (cols.length === 2 && lc.right > rc.left + 0.5) problems.push('the two columns overlap');
  for (const g of board.querySelectorAll('.grp')) {
    const gb = g.getBoundingClientRect();
    const col = g.closest('.col').getBoundingClientRect();
    if (gb.left < col.left - 0.5 || gb.right > col.right + 0.5) problems.push('a group sits outside its column');
    for (const cell of g.querySelectorAll('.ghead b, .ghead .n, .mv .num, .mv .to, .more')) {
      // Names and the "where they go" line may end in an ellipsis; the short facts may not.
      if (cell.scrollWidth > cell.clientWidth + 1) problems.push(`clipped: ${cell.textContent.trim().slice(0, 30)}`);
      const q = cell.getBoundingClientRect();
      if (q.width > 0 && (q.left < gb.left - 0.5 || q.right > gb.right + 0.5)) problems.push('a cell sits outside its group');
    }
    for (const row of g.querySelectorAll('.mv, .ghead')) {
      const cells = [...row.children].map((c) => c.getBoundingClientRect()).filter((c) => c.width > 0);
      for (let i = 1; i < cells.length; i++) if (cells[i - 1].right > cells[i].left + 0.5) problems.push('two cells of a row overlap');
    }
  }
  if (pair.scrollWidth > pair.clientWidth + 1) problems.push(`the board scrolls sideways (${pair.scrollWidth} > ${pair.clientWidth})`);
  return {
    problems,
    groups: board.querySelectorAll('.grp').length,
    rows: board.querySelectorAll('.mv').length,
    heads: [...board.querySelectorAll('.ghead')].map((h) => h.textContent.replace(/\s+/g, ' ').trim())
  };
};

/** Every arrow leaves a group on the left and reaches one on the right, and carries its count. */
const WIRES = () => {
  const board = document.querySelector('.board');
  const svg = board.querySelector('.lane svg');
  const s = svg.getBoundingClientRect();
  const paths = [...svg.querySelectorAll(':scope > path')];
  const counts = [...board.querySelectorAll('.lane .tally')];
  const outs = [...board.querySelectorAll('.col:first-of-type .grp')].map((e) => e.getBoundingClientRect());
  const ins = [...board.querySelectorAll('.col:last-of-type .grp')].map((e) => e.getBoundingClientRect());
  // An arrow leaves the card's edge, level with the phase's name.
  const heads = (sel) => [...board.querySelectorAll(`${sel} .ghead`)].map((e) => e.getBoundingClientRect());
  const outH = heads('.col:first-of-type'), inH = heads('.col:last-of-type');
  const near = (y, rects) => rects.some((r) => y >= r.top - 1 && y <= r.bottom + 1);
  const problems = [];
  for (const p of paths) {
    const a = p.getPointAtLength(0);
    const z = p.getPointAtLength(p.getTotalLength());
    const ax = a.x + s.left, zx = z.x + s.left, ay = a.y + s.top, zy = z.y + s.top;
    if (!outs.some((r) => Math.abs(ax - r.right) < 1.5)) problems.push('an arrow does not start at a group on the left');
    if (!ins.some((r) => zx <= r.left + 0.5 && zx >= r.left - 12)) problems.push('an arrow does not reach a group on the right');
    if (!near(ay, outH)) problems.push('an arrow starts off a phase name');
    if (!near(zy, inH)) problems.push('an arrow ends off a phase name');
    if (Number(p.getAttribute('stroke-width')) <= 0) problems.push('an arrow has no weight');
  }
  if (counts.length !== paths.length) problems.push(`${paths.length} arrows but ${counts.length} counts`);
  // A count stays in the lane, and no two of them sit on top of one another.
  const lane = board.querySelector('.lane').getBoundingClientRect();
  const boxes = counts.map((c) => c.getBoundingClientRect()).sort((a, z) => a.top - z.top);
  for (const q of boxes) if (q.left < lane.left - 14 || q.right > lane.right + 14) problems.push('a count sits outside the lane');
  for (let i = 1; i < boxes.length; i++) {
    const a = boxes[i - 1], z = boxes[i];
    if (z.top < a.bottom - 0.5 && z.left < a.right - 0.5 && z.right > a.left + 0.5) problems.push('two counts overlap');
  }
  return { count: paths.length, total: counts.reduce((n, c) => n + Number(c.textContent), 0), problems };
};

const summary = (p) => p.evaluate(() => document.querySelector('.sum p').textContent.replace(/\s+/g, ' ').trim());

async function open(p, url) {
  await p.goto(url, { waitUntil: 'load' });
  for (let i = 0; i < 80; i++) { if (!(await p.evaluate(() => !!document.querySelector('.loading')))) break; await p.waitForTimeout(250); }
  await p.waitForTimeout(600);
}

async function check(p, label, widths, expected) {
  for (const w of widths) {
    await p.setViewportSize({ width: w, height: 1000 });
    await p.waitForTimeout(500);
    const board = await p.evaluate(BOARD);
    const wires = await p.evaluate(WIRES);
    ok(`${label} @${w}: the groups lay out cleanly`, board.problems.length === 0, [...new Set(board.problems)].slice(0, 4).join('; '));
    ok(`${label} @${w}: every arrow runs group to group`, wires.problems.length === 0, [...new Set(wires.problems)].join('; '));
    ok(`${label} @${w}: the arrows account for every move`, wires.total === expected, `${wires.total} on ${wires.count} arrows, ${expected} moves`);
    console.log(`      ${label} @${w}: ${board.groups} groups · ${wires.count} arrows · ${board.rows} rows shown`);
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

  // ---- a real install: 1 100 mods with 30 of them out of place ----
  await open(p, `${base}?big=1100&jumble=30`);
  const active = await p.evaluate(() => Number((document.querySelector('.foot').textContent.match(/(\d+) active/) ?? [])[1]));
  ok('the big mock really is a big list', active > 900, `${active} active`);
  await p.click('.btn.primary:has-text("Sort with HALO")');
  await p.waitForTimeout(1200);
  const renumbered = await p.evaluate(() => Number((document.querySelector('.btn.primary').textContent.match(/(\d+) move/) ?? [])[1]));
  ok('HALO proposes hundreds of new numbers on a list this long', renumbered > 100, `${renumbered} new numbers`);
  await p.click('.btn.split:has-text("What changes")');
  await p.waitForTimeout(900);
  ok('the button opens the board rather than two lists', await p.evaluate(() => !!document.querySelector('.board .pair') && !document.querySelector('.list[data-pane]')));

  const sum = await summary(p);
  const moved = Number((sum.match(/move (\d+) mod/) ?? [])[1]);
  const drift = Number((sum.match(/([\d,]+) other mods/) ?? [])[1]?.replace(/,/g, ''));
  ok('the moves are told apart from the drift', moved > 0 && drift > 0, sum);
  ok('the moves are far fewer than the changed numbers', moved < renumbered / 3, `${moved} moves against ${renumbered} new numbers`);
  ok('the two add up to what HALO reported', Math.abs(moved + drift - renumbered) <= 2, `${moved} + ${drift} vs ${renumbered}`);
  console.log(`      ${sum}`);

  // The groups have to hold every move, on both sides, whatever is on screen.
  const held = await p.evaluate(() => {
    const n = (sel) => [...document.querySelectorAll(sel)].reduce((t, e) => t + Number(e.textContent), 0);
    return { left: n('.col:first-of-type .ghead .n'), right: n('.col:last-of-type .ghead .n') };
  });
  ok('every move leaves a phase and arrives in one', held.left === moved && held.right === moved, `${held.left} leaving, ${held.right} arriving, ${moved} moves`);
  ok('a group past its tenth mod offers the rest', await p.evaluate(() => {
    const g = [...document.querySelectorAll('.grp')].find((e) => e.querySelector('.more'));
    return !g || g.querySelectorAll('.mv').length === 10;
  }));
  await check(p, 'big list', [1700, 1440, 1180], moved);
  await p.setViewportSize({ width: 1700, height: 1000 });
  await p.waitForTimeout(400);
  await p.screenshot({ path: `${out}/m27-board.png` });

  // opening a group shows the rest of its mods
  const grew = await p.evaluate(async () => {
    const g = [...document.querySelectorAll('.grp')].find((e) => e.querySelector('.more'));
    if (!g) return null;
    const was = g.querySelectorAll('.mv').length;
    g.querySelector('.more').click();
    await new Promise((r) => setTimeout(r, 400));
    return { was, now: g.querySelectorAll('.mv').length, said: Number(g.querySelector('.ghead .n').textContent) };
  });
  if (grew) {
    ok('opening a group shows every mod it holds', grew.now === grew.said, `${grew.was} → ${grew.now} of ${grew.said}`);
    const after = await p.evaluate(WIRES);
    ok('and the arrows follow the group that grew', after.problems.length === 0, [...new Set(after.problems)].join('; '));
    await p.evaluate(() => [...document.querySelectorAll('.more')].find((e) => e.textContent.includes('fewer'))?.click());
    await p.waitForTimeout(400);
  }

  // hovering an arrow dims everything that is not making that journey
  await p.hover('.lane .tally');
  await p.waitForTimeout(300);
  const lit = await p.evaluate(() => ({
    cold: document.querySelectorAll('.lane svg > path.cold').length,
    all: document.querySelectorAll('.lane svg > path').length,
    dimRows: document.querySelectorAll('.mv.dim').length,
    rows: document.querySelectorAll('.mv').length
  }));
  ok('hovering an arrow dims the others', lit.all > 1 ? lit.cold === lit.all - 1 : true, JSON.stringify(lit));
  ok('and dims the mods not making that journey', lit.dimRows > 0 && lit.dimRows < lit.rows, JSON.stringify(lit));
  await p.screenshot({ path: `${out}/m27-hover.png` });

  // clicking an arrow keeps only that journey
  await p.click('.lane .tally');
  await p.waitForTimeout(500);
  const one = await p.evaluate(BOARD);
  const chip = await p.evaluate(() => document.querySelector('.sum .chip')?.textContent.replace(/\s+/g, ' ').trim());
  ok('clicking an arrow keeps only that journey', one.groups === 2 && (await p.evaluate(WIRES)).count === 1, `${one.groups} groups`);
  ok('and says which journey, with a way back', !!chip && /→/.test(chip), chip);
  await p.screenshot({ path: `${out}/m27-leg.png` });
  await p.click('.sum .chip');
  await p.waitForTimeout(400);
  ok('the chip puts every journey back', (await p.evaluate(BOARD)).groups === (await p.evaluate(() => document.querySelectorAll('.grp').length)));

  // clicking a phase keeps everything that touches it
  const headText = await p.evaluate(() => document.querySelector('.col:last-of-type .ghead b').textContent);
  await p.click('.col:last-of-type .ghead');
  await p.waitForTimeout(500);
  const only = await p.evaluate(() => ({
    right: document.querySelectorAll('.col:last-of-type .grp').length,
    chip: document.querySelector('.sum .chip')?.textContent.replace(/\s+/g, ' ').trim()
  }));
  ok('clicking a phase keeps only the moves that touch it', only.right === 1, JSON.stringify(only));
  ok('and names the phase in the chip', (only.chip ?? '').toLowerCase().includes(headText.toLowerCase()), `${only.chip} / ${headText}`);
  await p.click('.sum .chip');
  await p.waitForTimeout(400);

  // the search box narrows the columns too
  const name = await p.evaluate(() => document.querySelector('.mv .nm').textContent.trim());
  await p.fill('#search', name);
  await p.waitForTimeout(600);
  const found = await p.evaluate(BOARD);
  ok('searching narrows the board to the mods that match', found.rows >= 1 && found.rows < moved, `"${name}" → ${found.rows} rows`);
  ok('and the count says how many are shown', /shown/.test(await p.evaluate(() => document.querySelector('.sum').textContent)));
  await p.fill('#search', 'zzzz no such mod');
  await p.waitForTimeout(500);
  ok('a search that matches nothing says so', await p.evaluate(() => !!document.querySelector('.col .empty')));
  await p.fill('#search', '');
  await p.waitForTimeout(600);
  ok('clearing the search puts every move back', (await p.evaluate(BOARD)).rows === (await p.evaluate(() => document.querySelectorAll('.mv').length)));

  // clicking a mod selects it, so the panel can say why
  const pickName = await p.evaluate(() => document.querySelector('.mv .nm').textContent.trim());
  await p.click('.mv');
  await p.waitForTimeout(400);
  ok('clicking a mod selects the one it names', await p.evaluate((n) => (document.querySelector('.inspector')?.textContent ?? '').includes(n), pickName), pickName);

  // ---- the worst case: a list shuffled from end to end ----
  await open(p, `${base}?big=1100&jumble=600`);
  await p.click('.btn.primary:has-text("Sort with HALO")');
  await p.waitForTimeout(1500);
  await p.click('.btn.split:has-text("What changes")');
  await p.waitForTimeout(1500);
  const bigSum = await summary(p);
  const bigMoved = Number((bigSum.match(/move (\d+) mod/) ?? [])[1]);
  console.log(`      ${bigSum}`);
  ok('hundreds of moves still make a handful of arrows', (await p.evaluate(WIRES)).count <= 64, `${(await p.evaluate(WIRES)).count} arrows for ${bigMoved} moves`);
  await check(p, 'shuffled', [1700, 1180], bigMoved);
  await p.setViewportSize({ width: 1700, height: 1000 });
  await p.waitForTimeout(400);
  await p.screenshot({ path: `${out}/m27-shuffled.png` });

  // ---- the ordinary mock ----
  await open(p, base);
  await p.click('.btn.primary:has-text("Sort with HALO")');
  await p.waitForTimeout(700);
  await p.click('.btn.split:has-text("What changes")');
  await p.waitForTimeout(700);
  ok('the small list gets the same board', await p.evaluate(() => !!document.querySelector('.board .pair')));
  const smallMoved = Number(((await summary(p)).match(/move (\d+) mod/) ?? [])[1]);
  await check(p, 'small list', [1700, 1180], smallMoved);
  const bar = await p.evaluate(() => {
    const t = document.querySelector('.toolbar');
    const kids = [...t.children].filter((e) => e.getBoundingClientRect().height > 0);
    const top = Math.min(...kids.map((e) => e.getBoundingClientRect().top));
    return { over: kids.filter((e) => e.getBoundingClientRect().top > top + 8).length, wide: t.scrollWidth > t.clientWidth + 1, arr: !!t.querySelector('.seg.arr'), tabs: !!t.querySelector('[role=tablist]') };
  });
  ok('the toolbar stays on one row with the board open', bar.over === 0 && !bar.wide, JSON.stringify(bar));
  ok('the tabs and the arrangement toggle step aside, having nothing to arrange', !bar.arr && !bar.tabs);
  await p.screenshot({ path: `${out}/m27-small.png` });

  await p.click('.btn:has-text("Discard")');
  await p.waitForTimeout(600);
  ok('discarding the preview closes the board and gives the list back', await p.evaluate(() => !document.querySelector('.board') && !!document.querySelector('.list')));

  ok('no errors in the console', errors.length === 0, errors.slice(0, 3).join(' | '));
  await b.close();
  console.log(failures ? `\n${failures} failed` : '\nall good');
  process.exit(failures ? 1 : 0);
})();
