// The window's own caption bar.
//
// Geometry only, and that is the honest limit of what a browser can say about this. Whether
// `startDragging` really snaps, whether the native hit test really opens Snap Layouts, and
// whether the resize grips really resize are all questions about Win32 and none of them can
// be answered here. What *can* be answered is the half that goes wrong quietly: where the
// buttons are. The first version of this feature put them at the right-hand end of the
// search line, which is the end of the content column -- 340px short of the window's corner
// with the inspector open -- and nothing about that looked wrong in a screenshot.
//
// It also pins the two things that cost nothing and are easy to lose: that the bar is a drag
// region while the input and the buttons are not, and that with the system frame on none of
// it is drawn and the layout is back to costing nothing.
//
//   npm run build && npx vite preview --port 4173 --strictPort
//   node tools/loadtest/frame.cjs
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4173';
const OUT = process.argv[2] || '/tmp/frame';

(async () => {
  const b = await chromium.launch();
  let bad = 0;
  const ok = (l, c, d = '') => { if (!c) bad++; console.log(`${c ? 'ok  ' : 'FAIL'}  ${l}${d ? '  — ' + d : ''}`); };
  const p = await b.newPage({ viewport: { width: 1500, height: 900 }, deviceScaleFactor: 2, colorScheme: 'dark' });
  await p.goto(`${BASE}/?frame`);
  await p.waitForSelector('.list .row[data-uid]');
  await p.waitForTimeout(900);
  const g = await p.evaluate(() => {
    const wc = document.querySelector('.wc');
    if (!wc) return null;
    const r = wc.getBoundingClientRect();
    const btns = [...wc.querySelectorAll('.wb')].map((b) => { const x = b.getBoundingClientRect(); return { w: Math.round(x.width), h: Math.round(x.height), top: Math.round(x.top), left: Math.round(x.left), right: Math.round(x.right) }; });
    const bar = document.querySelector('.tbar').getBoundingClientRect();
    const rail = document.querySelector('.rail').getBoundingClientRect();
    return { wc: { top: Math.round(r.top), right: Math.round(r.right), h: Math.round(r.height) }, btns, bar: { top: Math.round(bar.top), left: Math.round(bar.left), right: Math.round(bar.right), h: Math.round(bar.height) }, railTop: Math.round(rail.top) };
  });
  ok('the window buttons render', !!g, JSON.stringify(g));
  if (!g) { await b.close(); process.exit(1); }
  ok('three of them', g.btns.length === 3, String(g.btns.length));
  ok('flush to the top of the window', g.wc.top === 0, `top ${g.wc.top}`);
  ok('flush to the right edge', Math.abs(g.wc.right - 1500) <= 1, `right ${g.wc.right} of 1500`);
  ok('filling the bar height', g.wc.h === g.bar.h, `${g.wc.h} vs bar ${g.bar.h}`);
  ok('the bar spans the whole window', g.bar.left === 0 && Math.abs(g.bar.right - 1500) <= 1, JSON.stringify(g.bar));
  ok('and it costs exactly its own height', g.railTop === g.bar.h, `rail starts at ${g.railTop}, bar is ${g.bar.h}`);
  ok('big enough to hit', g.btns.every((x) => x.w >= 40 && x.h >= 28), JSON.stringify(g.btns[0]));

  const drag = await p.evaluate(() => ({
    row: document.querySelector('.tbar')?.hasAttribute('data-tauri-drag-region'),
    input: document.querySelector('#search')?.hasAttribute('data-tauri-drag-region'),
    btn: document.querySelector('.wb')?.hasAttribute('data-tauri-drag-region')
  }));
  ok('the bar is a drag handle', drag.row === true, JSON.stringify(drag));
  ok('but the input and the buttons are not', drag.input === false && drag.btn === false, JSON.stringify(drag));
  const grips = await p.evaluate(() => document.querySelectorAll('.wgrip').length);
  ok('eight resize grips, or the window cannot be resized', grips === 8, String(grips));
  const z = await p.evaluate(() => { const g = document.querySelector('.wgrip'); return g ? Number(getComputedStyle(g).zIndex) : null; });
  ok('above everything, dialogs included', (z ?? 0) >= 9999, String(z));
  await p.locator('.tbar').screenshot({ path: `${OUT}/frame.png` });
  await p.screenshot({ path: `${OUT}/frame-full.png` });

  const q = await b.newPage({ viewport: { width: 1200, height: 800 } });
  await q.goto(BASE);
  await q.waitForSelector('.list .row[data-uid]');
  await q.waitForTimeout(600);
  ok('and nothing is drawn where there is no window to control', (await q.evaluate(() => document.querySelectorAll('.wc, .wgrip, .tbar').length)) === 0);
  ok('so the layout costs nothing with the system frame', (await q.evaluate(() => Math.round(document.querySelector('.rail').getBoundingClientRect().top))) === 0);
  await b.close();
  console.log(bad ? `\n${bad} failed.` : '\nall good');
  process.exit(bad ? 1 : 0);
})();
