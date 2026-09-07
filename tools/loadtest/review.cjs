// Walking the issues with Next.
//
// A user reported that pressing Next moved the list to the next problem but left the words above
// it describing the previous one. The banner read the whole list and named whichever issue was
// worst in it, which never changes however far you walk; the button counted the mod it was about
// to go to, so its number did not match the mod being described either.
//
// This presses Next several times and asks the only question that matters: does the text change,
// and does it name the mod the list has just moved to.
//
//   npm run build && npx vite preview --port 4183 --strictPort
//   NODE_PATH=$(npm root -g) node tools/loadtest/review.cjs [outdir]
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4183';
const OUT = process.argv[2] || '/tmp/review';

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1500, height: 950 } });
  await page.goto(BASE);
  await page.waitForSelector('.list .row', { timeout: 15000 });
  await page.waitForTimeout(400);

  const banner = page.locator('.banner').filter({ has: page.locator('button:has-text("Review"), button:has-text("Next")') }).first();
  const button = banner.locator('button:has-text("Review"), button:has-text("Next")').first();
  const read = async () => ({
    title: (await banner.locator('.t').innerText()).split('\n')[0].trim(),
    all: (await banner.innerText()).replace(/\s+/g, ' '),
    action: (await button.innerText()).trim(),
    // The row's name only: the cell also carries the author underneath it.
    selected: await page.evaluate(() => document.querySelector('.list .row.sel')?.querySelector('.name b')?.textContent?.trim() ?? null)
  });

  const before = await read();
  ok('nothing is being reviewed yet, so the button offers to start', /^Review \(\d+\)$/.test(before.action), before.action);
  const total = Number(before.action.match(/\((\d+)\)/)?.[1] ?? 0);
  ok('and it knows how many there are', total > 3, `${total}`);
  await page.screenshot({ path: `${OUT}/before.png` });

  const seen = [];
  for (let step = 1; step <= 5; step++) {
    await button.click();
    await page.waitForTimeout(350);
    const now = await read();
    seen.push(now);
    ok(`step ${step}: the button now says Next`, now.action === 'Next', now.action);
    ok(`step ${step}: the banner says where you are`, now.all.includes(`${step} of ${total}`), now.all.slice(0, 220));
    if (step > 1) {
      const prev = seen[step - 2];
      ok(`step ${step}: the words changed`, now.title !== prev.title || now.all !== prev.all, `still "${now.title}"`);
    }
    if (now.selected) {
      // The banner has to be about the row the list moved to, not about whatever is worst overall.
      ok(`step ${step}: it names the mod the list moved to`, now.all.toLowerCase().includes(now.selected.toLowerCase()), `banner "${now.title}" vs row "${now.selected}"`);
    }
  }
  await page.screenshot({ path: `${OUT}/reviewing.png` });

  const titles = new Set(seen.map((s) => s.title));
  ok('five steps described more than one thing', titles.size > 1, [...titles].join(' | '));

  // Closing a banner about the mod under review leaves the review rather than hiding that one
  // mod for the session, which would take the Next button with it.
  const x = banner.locator('button.x');
  if (await x.count()) {
    await x.click();
    await page.waitForTimeout(300);
    const after = (await banner.locator('button:has-text("Review"), button:has-text("Next")').first().innerText()).trim();
    ok('closing it leaves the review and offers to start again', /^Review/.test(after), after);
  } else {
    console.log('ok    (the banner under review is an error, which has no close button)');
  }

  // Nothing in the window should be a stray piece of source. A literal backslash-n after
  // ModList's closing style tag was being drawn under the list, in the app, for months.
  const strays = await page.evaluate(() => {
    const out = [];
    const w = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    for (let n = w.nextNode(); n; n = w.nextNode()) if (/\\[nrt]/.test(n.nodeValue) && n.nodeValue.trim()) out.push(n.nodeValue.slice(0, 40));
    return out;
  });
  ok('no escape sequence is being drawn as text', strays.length === 0, JSON.stringify(strays));

  await browser.close();
  console.log(failures ? `\n${failures} failed.\n` : '\nall good\n');
  process.exit(failures ? 1 : 0);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
