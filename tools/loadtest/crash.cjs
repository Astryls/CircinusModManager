// What happens when something in the window throws.
//
// A user reported that switching instance "unloads everything and shows a black screen". A black
// window is the worst error message there is: it tells the player nothing and leaves nobody
// anything to read. There was no boundary around the outermost layer and no handler for what
// escaped, so a single bad value did exactly that.
//
// This breaks the window on purpose and asks three things: does anything at all appear, does it
// say what happened, and was it written down somewhere a report can carry.
//
//   npm run build && npx vite preview --port 4182 --strictPort
//   NODE_PATH=$(npm root -g) node tools/loadtest/crash.cjs [outdir]
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4182';
const OUT = process.argv[2] || '/tmp/crash';

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1400, height: 900 } });

  // Everything the app tried to write to the log, as the backend would have received it.
  const logged = [];
  await page.exposeFunction('__circinusLogged', (message, stack) => logged.push({ message, stack }));

  // Re-applied on every navigation, since each one is a fresh document.
  await page.addInitScript(() => {
    const w = window;
    const orig = console.error;
    console.error = (...a) => {
      const s = a.map(String).join(' ');
      if (s.includes('the window would have logged') && w.__circinusLogged) w.__circinusLogged(s, '');
      orig(...a);
    };
  });

  await page.goto(BASE);
  await page.waitForSelector('.list .row', { timeout: 15000 });
  await page.waitForTimeout(400);
  ok('the app starts', await page.$('.list .row') !== null);

  await page.evaluate(() => {
    const w = window;
    w.__origError = console.error;
    console.error = (...a) => {
      const s = a.map(String).join(' ');
      if (s.includes('the window would have logged')) w.__circinusLogged(s, '');
      w.__origError(...a);
    };
  });

  // Break something every render path reads. A thrown $derived is what a real bad snapshot does.
  await page.evaluate(() => {
    const el = document.querySelector('.list');
    if (!el) throw new Error('no list');
    // Force a real error out of the framework: remove the mount point mid-flight is too blunt,
    // so throw from an event handler the app owns instead.
    window.dispatchEvent(new ErrorEvent('error', { error: new Error('deliberate: pretend a derived threw'), message: 'deliberate: pretend a derived threw' }));
  });
  await page.waitForTimeout(400);
  ok('an uncaught error is written down rather than only shown in a console', logged.some((l) => /deliberate/.test(l.message)), JSON.stringify(logged.slice(0, 2)));

  // An unhandled rejection is the other way it happens: a failed invoke nobody awaited.
  logged.length = 0;
  await page.evaluate(() => window.dispatchEvent(new PromiseRejectionEvent('unhandledrejection', { promise: Promise.resolve(), reason: new Error('deliberate: a command failed') })));
  await page.waitForTimeout(400);
  ok('and so is an unhandled rejection', logged.some((l) => /deliberate/.test(l.message)), JSON.stringify(logged.slice(0, 2)));

  // Now the real thing: a snapshot the store cannot read, which is what a bad value looks like
  // from the window's point of view. Without a boundary this is the black screen.
  await page.goto(`${BASE}/?crash=1`);
  await page.waitForTimeout(1200);
  const text = await page.evaluate(() => document.body.innerText.trim());
  ok('the window is not blank', text.length > 40, `${text.length} characters`);
  ok('something says what happened', /could not read what the backend sent/i.test(text), text.slice(0, 200));
  // The real cause, named. A snapshot that cannot be read is caught where it arrives rather than
  // inside a derived, because Svelte swallows that one and reports its own confusion instead.
  ok('it names the real cause', /listColumns|settings/.test(text), text.slice(0, 300));
  ok('and says nothing on disk was changed', /Nothing on disk has been changed/i.test(text), text.slice(0, 300));
  ok('and offers a way out', /Try again/i.test(text), text.slice(0, 300));
  await page.screenshot({ path: `${OUT}/crashed.png` });

  // And it has to reach the log, or the report a player pastes carries nothing about it.
  ok('the failure was written down', logged.some((l) => /snapshot/i.test(l.message)), JSON.stringify(logged.slice(0, 2).map((l) => l.message.slice(0, 110))));

  // Back to a working page for the rest.
  await page.goto(BASE);
  await page.waitForSelector('.list .row', { timeout: 15000 });
  await page.waitForTimeout(300);

  // And the report a player is asked to paste exists and says the things it promises.
  const diag = await page.evaluate(async () => {
    const m = await import('/assets/index.js').catch(() => null);
    return null; // the bundle is hashed; ask through the UI instead
  });
  await page.click('.rail button:has-text("Settings")').catch(() => {});
  await page.waitForTimeout(400);
  const btn = await page.$('button:has-text("Copy diagnostics")');
  ok('Settings offers Copy diagnostics', btn !== null);
  if (btn) {
    await page.context().grantPermissions(['clipboard-read', 'clipboard-write']).catch(() => {});
    await btn.click();
    await page.waitForTimeout(300);
    const said = await page.evaluate(() => document.body.innerText);
    ok('and says it copied', /Copied/.test(said), said.slice(0, 0));
  }
  await page.screenshot({ path: `${OUT}/settings.png` });

  await browser.close();
  console.log(failures ? `\n${failures} failed.\n` : '\nall good\n');
  process.exit(failures ? 1 : 0);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
