// The rule and workshop databases panel in Settings.
//
// A user reported that switching the community rules off did nothing: the toggle went off, the
// file stayed, the rules went on moving mods, and the footer went on listing the file as loaded.
// The fix is in Rust -- a disabled source is neither read nor kept -- but the panel is where a
// player sees whether it worked, so this checks what the panel actually says.
//
//   npm run build && npx vite preview --port 4183 --strictPort
//   NODE_PATH=$(npm root -g) node tools/loadtest/databases.cjs [outdir]
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4183';
const OUT = process.argv[2] || '/tmp/databases';

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}

(async () => {
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1400, height: 1000 } });
  await page.goto(BASE);
  await page.waitForSelector('.list .row', { timeout: 15000 });
  await page.click('.rail button:has-text("Settings")');
  await page.waitForSelector('h3:has-text("Rule and workshop databases")', { timeout: 8000 });

  const section = page.locator('section.card').filter({ has: page.locator('h3:has-text("Rule and workshop databases")') });
  const boxes = section.locator('label.dbrow input[type=checkbox]');
  const n = await boxes.count();
  ok('every source has a switch', n === 4, `${n} switches`);

  // The mock ships one on and three off, which is the state a first run is now in for all four.
  const checked = [];
  for (let i = 0; i < n; i++) checked.push(await boxes.nth(i).isChecked());
  ok('the switches show what is actually set', JSON.stringify(checked) === JSON.stringify([true, false, false, false]), JSON.stringify(checked));

  const text = (await section.innerText()).replace(/\s+/g, ' ');
  ok('it says what switching one off does', /deletes the copy Circinus downloaded/i.test(text), text.slice(0, 160));
  ok('and why they start off', /outranks what the mod's own author wrote/i.test(text), text.slice(0, 200));

  await section.scrollIntoViewIfNeeded();
  await page.screenshot({ path: `${OUT}/databases.png` });

  // Nothing loaded and nothing switched on is a choice, not a failure, and must not read as one.
  await page.evaluate(() => {
    document.querySelectorAll('label.dbrow input[type=checkbox]').forEach((b) => {
      if (b.checked) b.click();
    });
  });
  await page.waitForTimeout(500);
  const after = (await section.innerText()).replace(/\s+/g, ' ');
  ok('all off does not read as something gone wrong', /All off, so nothing here is affecting your load order/i.test(after), after.slice(0, 160));
  ok('and it stops telling you to press Update now', !/nothing downloaded yet/i.test(after), after.slice(0, 160));
  await page.screenshot({ path: `${OUT}/all-off.png` });

  await browser.close();
  console.log(failures ? `\n${failures} failed.\n` : '\nall good\n');
  process.exit(failures ? 1 : 0);
})().catch((e) => {
  console.error(e);
  process.exit(1);
});
