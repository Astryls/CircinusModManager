// What a modpack's curator said.
//
// Two things to prove, and the second matters more than the first.
//
// The first is that it works: a curator's posts reach the banner, the banner opens the panel,
// the posts are attributed to the person who wrote them, reading them clears the count, and
// muting a curator silences them without unfollowing the pack.
//
// The second is that it is invisible when there is nothing. circinus.sh does not serve this
// endpoint yet, so *every* copy of the release ships into the empty case, and an empty feature
// that leaves a banner, a stray heading or an error toast behind is worse than no feature. That
// is what `?nopacks` is for.
//
//   npm run build && npx vite preview --port 4173 --strictPort
//   node tools/loadtest/packs.cjs [outdir]
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4173';
const OUT = process.argv[2] || '/tmp/packs';

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}

const banner = (page) =>
  page.evaluate(() => {
    const row = [...document.querySelectorAll('.banner')].find((n) => /modpack/i.test(n.textContent));
    return row ? row.textContent.replace(/\s+/g, ' ').trim() : null;
  });

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1600, height: 950 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));
  page.on('console', (m) => { if (m.type() === 'error') errors.push('console: ' + m.text().slice(0, 200)); });

  // ---- the case the release ships in ---------------------------------------------------------
  await page.goto(`${BASE}/?nopacks`, { waitUntil: 'load' });
  await page.waitForSelector('.list .row[data-uid]', { timeout: 15000 });
  await page.waitForTimeout(900);
  ok('with nothing served, no banner', (await banner(page)) === null, (await banner(page)) ?? '');
  const quiet = await page.evaluate(() => ({
    rail: [...document.querySelectorAll('.cols .clear')].map((b) => b.textContent.trim()),
    toast: document.querySelector('.toast')?.textContent?.trim() ?? ''
  }));
  ok('and nothing in the sidebar about curators', !quiet.rail.some((t) => /curator|unread/i.test(t)), quiet.rail.join(' | '));
  ok('and no error toast', !/error|could not|failed/i.test(quiet.toast), quiet.toast);

  // ---- a curator who has something to say -----------------------------------------------------
  await page.goto(BASE, { waitUntil: 'load' });
  await page.waitForSelector('.list .row[data-uid]', { timeout: 15000 });
  await page.waitForTimeout(900);
  const b = await banner(page);
  ok('with a feed, the banner says how many', !!b && /3 updates/.test(b), b ?? 'no banner');
  ok('and who wrote them', !!b && /Curator/.test(b), b ?? '');
  await page.screenshot({ path: `${OUT}/packs-banner.png` });

  // ---- the panel ------------------------------------------------------------------------------
  await page.evaluate(() => {
    const row = [...document.querySelectorAll('.banner')].find((n) => /modpack/i.test(n.textContent));
    [...row.querySelectorAll('button')].find((x) => x.textContent.trim() === 'Read')?.click();
  });
  await page.waitForSelector('.post', { timeout: 5000 });
  await page.waitForTimeout(400);
  const panel = await page.evaluate(() => ({
    posts: document.querySelectorAll('.post').length,
    unread: document.querySelectorAll('.post.new').length,
    authors: [...document.querySelectorAll('.post .who')].map((w) => w.textContent.trim()),
    pack: document.querySelector('.pack h4')?.textContent.trim().split('\n')[0] ?? '',
    lead: document.querySelector('.lead')?.textContent ?? '',
    link: document.querySelectorAll('.post .go').length
  }));
  ok('the panel holds every post', panel.posts === 3, `${panel.posts}`);
  ok('all three unread to begin with', panel.unread === 3, `${panel.unread}`);
  // The whole point of the screen: these are somebody else's words and it has to say so.
  ok('every post names its author', panel.authors.length === 3 && panel.authors.every((a) => a === 'Curator'), panel.authors.join(', '));
  ok('grouped under the pack they came from', /Rim of Madness/.test(panel.pack), panel.pack);
  ok('and the page says Circinus did not write them', /does not check them|as they wrote it/i.test(panel.lead));
  ok('a post with a link offers it', panel.link === 1, `${panel.link}`);
  await page.screenshot({ path: `${OUT}/packs-panel.png` });

  // Curator text is text. A post full of markup must not become markup.
  const raw = await page.evaluate(() => {
    const p = document.querySelector('.post .body-text');
    p.textContent = '<b>bold</b><img src=x onerror="window.__x=1">';
    return { html: p.innerHTML, ran: !!window.__x };
  });
  ok('a post is rendered as text, not markup', raw.html.includes('&lt;b&gt;') && !raw.ran, raw.html.slice(0, 60));

  // ---- reading clears it ----------------------------------------------------------------------
  await page.evaluate(() => [...document.querySelectorAll('button')].find((x) => x.textContent.trim() === 'Mark all read')?.click());
  await page.waitForTimeout(600);
  const after = await page.evaluate(() => ({ unread: document.querySelectorAll('.post.new').length, posts: document.querySelectorAll('.post').length }));
  ok('marking read clears the marks', after.unread === 0, `${after.unread}`);
  ok('but keeps the posts', after.posts === 3, `${after.posts}`);
  // It is a modal, so the way out is Escape or the X, not the logo behind the scrim.
  await page.keyboard.press('Escape');
  await page.waitForTimeout(500);
  ok('Escape closes it', (await page.evaluate(() => document.querySelectorAll('.post').length)) === 0);
  ok('and the banner is gone', (await banner(page)) === null, (await banner(page)) ?? '');

  // ---- muting silences the curator, not the pack ----------------------------------------------
  await page.evaluate(() => [...document.querySelectorAll('.cols .clear')].find((x) => /curator/i.test(x.textContent))?.click());
  await page.waitForSelector('.post', { timeout: 5000 });
  await page.evaluate(() => [...document.querySelectorAll('button')].find((x) => x.textContent.trim() === 'Mute this curator')?.click());
  await page.waitForTimeout(700);
  const muted = await page.evaluate(() => ({
    posts: document.querySelectorAll('.post').length,
    unmute: !!document.querySelector('.muted .btn'),
    stillFollowed: document.querySelectorAll('.cols .col').length
  }));
  ok('muting empties the feed', muted.posts === 0, `${muted.posts}`);
  ok('and offers a way back', muted.unmute);
  ok('while the pack is still followed', muted.stillFollowed > 0, `${muted.stillFollowed} followed`);
  await page.screenshot({ path: `${OUT}/packs-muted.png` });

  // Closing the panel with everything muted must not be a one-way door: the unmute row is only
  // in here, so the sidebar has to keep offering the way in on an empty feed.
  await page.keyboard.press('Escape');
  await page.waitForTimeout(400);
  const wayBack = await page.evaluate(() => [...document.querySelectorAll('.cols .clear')].map((x) => x.textContent.trim()));
  ok('and the way back in is still in the sidebar', wayBack.some((t) => /curator/i.test(t)), wayBack.join(' | '));
  await page.evaluate(() => [...document.querySelectorAll('.cols .clear')].find((x) => /curator/i.test(x.textContent))?.click());
  await page.waitForTimeout(400);

  await page.evaluate(() => document.querySelector('.muted .btn')?.click());
  await page.waitForTimeout(700);
  ok('unmuting brings them back', (await page.evaluate(() => document.querySelectorAll('.post').length)) === 3);

  console.log('errors:', errors.length ? errors.join('\n') : 'none');
  if (errors.length) failures += errors.length;
  await browser.close();
  console.log(failures ? `\n${failures} failed.` : '\nall good');
  process.exit(failures ? 1 : 0);
})();
