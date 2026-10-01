// The three number columns, and the one thing each of them is allowed to be.
//
// This file exists because of what the columns it replaces were. **Time** put a Loading
// Progress measurement and a folder model in the same cell, in the same mono digits, under a
// tooltip that said flatly the number was not measured -- so a stopwatch reading was presented
// as a guess and the mod that took it went uncredited. **Load** was not a second measurement:
// it was Time divided by the sum of Time, which is why sorting by Load and sorting by Time ran
// the same comparison in the same code. And Load sat beside **Cost**, which is a real pooled
// frame share, in the same unit and the same typeface, so the invented percentage and the
// measured one read as a pair.
//
// Every assertion here is therefore about *provenance* rather than about layout: that an
// unmeasured cell is empty instead of modelled, that the two frame columns can disagree, and
// that no two of the three sort by the same value. A geometry check would have passed on every
// one of the old columns.
//
//   npm run build && npx vite preview --port 4173 --strictPort
//   node tools/loadtest/columns.cjs [outdir]
const { chromium } = require('playwright');

const BASE = process.env.BASE || 'http://127.0.0.1:4173';
const OUT = process.argv[2] || '/tmp/columns';

let failures = 0;
function ok(label, cond, detail = '') {
  if (!cond) failures++;
  console.log(`${cond ? 'ok  ' : 'FAIL'}  ${label}${detail ? '  — ' + detail : ''}`);
}

/** Every drawn row as { name, startup, typical, yours, ver }, text exactly as rendered. */
const readRows = (p) =>
  p.evaluate(() =>
    [...document.querySelectorAll('.list .row')].map((r) => ({
      name: r.querySelector('.name b')?.textContent.trim() ?? '',
      startup: r.querySelector('.tm')?.textContent.trim() ?? null,
      typical: r.querySelector('.wt')?.textContent.trim() ?? null,
      yours: r.querySelector('.load')?.textContent.trim() ?? null,
      ver: r.querySelector('.vers')?.textContent.trim() ?? null,
      verState: ['on', 'behind', 'ahead'].find((c) => r.querySelector('.vers span')?.classList.contains(c)) ?? null
    }))
  );

const find = async (p, needle) => {
  await p.fill('#search', needle);
  await p.waitForTimeout(500);
  const rows = await readRows(p);
  await p.fill('#search', '');
  await p.waitForTimeout(400);
  return rows;
};

(async () => {
  const browser = await chromium.launch();
  const errors = [];
  const page = await browser.newPage({ viewport: { width: 1600, height: 940 }, colorScheme: 'dark' });
  page.on('pageerror', (e) => errors.push('PAGEERROR ' + e.message));

  await page.goto(BASE, { waitUntil: 'load' });
  await page.waitForSelector('.list .row[data-uid]', { timeout: 15000 });
  await page.waitForTimeout(800);

  // ---- the headings say what the columns are ---------------------------------------------
  const heads = await page.evaluate(() => [...document.querySelectorAll('.hdr .h')].map((h) => h.textContent.trim()).filter(Boolean));
  ok('the start-up column is called Start-up', heads.includes('Start-up'), heads.join(' | '));
  ok('the pooled frame column is called Typical', heads.includes('Typical'), heads.join(' | '));
  ok('the local frame column is called Yours', heads.includes('Yours'), heads.join(' | '));
  // The words that named the two dishonest columns must be gone, not merely moved: "Load" in
  // particular described a number that no longer exists anywhere in the program.
  ok('and nothing is called Time, Cost or Load any more', !heads.some((h) => /^(Time|Cost|Load)$/.test(h)), heads.join(' | '));

  // Every heading obeys one type rule. The notice heading used to opt out of it, which is what
  // made that column read as belonging to a different table.
  const types = await page.evaluate(() =>
    [...document.querySelectorAll('.hdr .h')]
      .filter((h) => h.textContent.trim() && !h.querySelector('svg'))
      .map((h) => {
        const c = getComputedStyle(h);
        return { t: h.textContent.trim(), size: c.fontSize, weight: c.fontWeight, tf: c.textTransform };
      })
  );
  const odd = types.filter((t) => t.size !== types[0]?.size || t.weight !== types[0]?.weight || t.tf !== types[0]?.tf);
  ok('every worded heading shares one type', odd.length === 0, JSON.stringify(odd.slice(0, 3)));

  // And every heading fits the column it names. Renaming a column and resizing it are two
  // edits, and doing only one of them gives you "VERSI…" -- which happened, between the first
  // build of this change and this assertion existing.
  // Measured on the innermost element that holds the word, not on the cell.
  //
  // A sortable heading is a `<button>` inside the `.h` span, and the button is what clips: the
  // span reports scrollWidth === clientWidth however far the text overruns, because the
  // overflow is the button's, one level down. Measuring the span looked right, passed, and
  // could not see a heading cut in half -- which was checked by narrowing the column until it
  // was, and watching this assertion stay green.
  //
  // The two resizable headings are skipped for the opposite reason: they carry an absolutely
  // positioned drag handle hanging 6px past their edge, and are `overflow: visible` so that it
  // can, which means they always overrun and can never clip.
  const clipped = await page.evaluate(() =>
    [...document.querySelectorAll('.hdr .h')]
      .filter((h) => h.textContent.trim() && !h.querySelector('.grab'))
      .map((h) => {
        const inner = h.querySelector('.sortbtn') ?? h;
        return { t: h.textContent.trim(), over: Math.round(inner.scrollWidth - inner.clientWidth) };
      })
      .filter((h) => h.over > 1)
  );
  ok('and no heading is cut off by its own column', clipped.length === 0, JSON.stringify(clipped));

  // ---- Start-up is measured or it is empty -------------------------------------------------
  // The whole point. A model cannot be allowed to fill this cell, because once it is set in the
  // same digits as a reading there is no way for a reader to tell them apart.
  const rows = await readRows(page);
  ok('some rows carry a start-up figure', rows.some((r) => /\d/.test(r.startup ?? '')), String(rows.length));
  const blanks = rows.filter((r) => !/\d/.test(r.startup ?? ''));
  ok('and some do not, so the empty case is drawn', blanks.length > 0, String(blanks.length));
  ok('an unmeasured start-up is a dash, never a zero or a guess', blanks.every((r) => (r.startup ?? '').trim() === '—'), JSON.stringify(blanks.slice(0, 3)));

  // The tooltip must not repeat the old lie, in either direction: it may not call a measurement
  // an estimate, and it must name whoever took it.
  const tips = await page.evaluate(() => [...document.querySelectorAll('.list .row .tm span[title]:not(.unread)')].map((s) => s.getAttribute('title')));
  ok('a measured cell credits Loading Progress', tips.length > 0 && tips.every((t) => /Loading Progress/.test(t)), tips[0]?.slice(0, 70));
  ok('and never calls its own measurement an estimate', !tips.some((t) => /estimat/i.test(t)), tips.find((t) => /estimat/i.test(t))?.slice(0, 70));

  // ---- Typical and Yours are two measurements, not one drawn twice -------------------------
  const ce = (await find(page, 'rocketman')).find((r) => /RocketMan/i.test(r.name));
  ok('a mod with both figures shows both', !!ce && /\d/.test(ce.typical ?? '') && /\d/.test(ce.yours ?? ''), JSON.stringify(ce));
  // The case the pair exists for. If Yours ever silently fell back to the pooled share, this is
  // the assertion that catches it: the corpus deliberately puts this mod far off its own line.
  const num = (s) => parseFloat((s ?? '').replace(/[^\d.]/g, ''));
  ok('and they are allowed to disagree', !!ce && Math.abs(num(ce.typical) - num(ce.yours)) > 0.5, `${ce?.typical} vs ${ce?.yours}`);

  ok('and the local figure can be the larger of the two', !!ce && num(ce.yours) > num(ce.typical), `${ce?.typical} vs ${ce?.yours}`);
  const anim = (await find(page, 'combat extended')).find((r) => /^Combat Extended$/i.test(r.name));
  ok('a mod can also cost less here than it costs everybody else', !!anim && num(anim.yours) < num(anim.typical), `${anim?.typical} vs ${anim?.yours}`);

  // Neither column may borrow from the other. Both empty states have to appear somewhere.
  const all = await page.evaluate(() => {
    const out = [];
    for (const r of document.querySelectorAll('.list .row')) {
      out.push({ t: r.querySelector('.wt')?.textContent.trim() ?? '', y: r.querySelector('.load')?.textContent.trim() ?? '' });
    }
    return out;
  });
  ok('a mod with no local run shows a dash under Yours', all.some((r) => /\d/.test(r.t) && r.y === '—'), JSON.stringify(all.filter((r) => /\d/.test(r.t) && r.y === '—').slice(0, 2)));
  ok('and neither column ever reads as a zero', !all.some((r) => /^0(\.0)?\s*%$/.test(r.t) || /^0(\.0)?\s*%$/.test(r.y)), JSON.stringify(all.filter((r) => /^0(\.0)?\s*%$/.test(r.t) || /^0(\.0)?\s*%$/.test(r.y)).slice(0, 2)));

  // ---- one version chip, and it says what the mod declares ---------------------------------
  const chipCounts = await page.evaluate(() => [...new Set([...document.querySelectorAll('.list .row .vers')].map((v) => v.querySelectorAll('span').length))]);
  ok('a row draws at most one version chip', chipCounts.every((n) => n <= 1), chipCounts.join(', '));
  // The bug the single chip fixes: two greyed chips said the same thing for "stopped at 1.4"
  // and for "declares nothing", so the column could not tell those apart on a 1.6 install.
  // Now the label is the declared version itself.
  //
  // Named mods, found by search. Every mod behind the game in this corpus is inactive, and the
  // list is virtualised either way -- so scanning what happens to be drawn proves the column in
  // the one case where every mod agrees with the game, which is the case the old two-chip
  // column also got right.
  await page.evaluate(() => [...document.querySelectorAll('.rail .nav button, .nav button')].find((b) => b.textContent.trim().startsWith('Library'))?.click());
  await page.waitForTimeout(700);
  const chipFor = async (needle) => (await find(page, needle)).find((r) => r.ver)?.ver ?? null;
  const stateFor = async (needle) => (await find(page, needle)).find((r) => r.verState)?.verState ?? null;
  ok('a mod that stopped at 1.4 says 1.4', (await chipFor('runtimegc')) === '1.4', String(await chipFor('runtimegc')));
  ok('and is marked as behind the game', (await stateFor('runtimegc')) === 'behind', String(await stateFor('runtimegc')));
  ok('a mod that keeps up says 1.6', (await chipFor('rocketman')) === '1.6', String(await chipFor('rocketman')));
  ok('and is not marked', (await stateFor('rocketman')) === 'on', String(await stateFor('rocketman')));
  // The pair that used to be indistinguishable: 1.5 and 1.4 both drew as two grey chips.
  ok('a mod that stopped at 1.5 says 1.5, not 1.4', (await chipFor('save our ship')) === '1.5', String(await chipFor('save our ship')));

  // ---- no two columns sort by the same value -----------------------------------------------
  // Time and Load did. Both `case`s returned `loadOf(uid).ms`, so two headings ran one
  // comparison and a reader had no way to know. Sorting by each and comparing the resulting
  // order is the only check that can tell.
  await page.evaluate(() => [...document.querySelectorAll('.rail .nav button, .nav button')].find((b) => b.textContent.trim().startsWith('Load order'))?.click());
  await page.waitForTimeout(700);
  const sortBy = async (label) => {
    await page.evaluate((l) => [...document.querySelectorAll('.hdr .sortbtn')].find((b) => b.textContent.trim().startsWith(l))?.click(), label);
    await page.waitForTimeout(600);
    return (await readRows(page)).map((r) => r.name);
  };
  const bySt = await sortBy('Start-up');
  const byTy = await sortBy('Typical');
  const byYo = await sortBy('Yours');
  ok('Start-up and Typical are different sorts', JSON.stringify(bySt) !== JSON.stringify(byTy));
  ok('Typical and Yours are different sorts', JSON.stringify(byTy) !== JSON.stringify(byYo));
  ok('Start-up and Yours are different sorts', JSON.stringify(bySt) !== JSON.stringify(byYo));
  // And an unmeasured mod sorts as "no answer" rather than as the cheapest thing in the list.
  await sortBy('Start-up');
  const top = await readRows(page);
  ok('a mod nobody timed does not sort to the top of Start-up', /\d/.test(top[0]?.startup ?? ''), JSON.stringify(top.slice(0, 2)));

  // ---- and the Inspector, which was the third place the model leaked out ------------------
  // The column and the summary card were fixed first; this panel went on printing
  // "412 ms of an estimated 6.0 min" under a heading that said "estimated from the folder".
  // Honest wording around a figure nobody observed is still a figure nobody observed, and a
  // detail panel is exactly where somebody goes to find out what a number really is.
  const inspect = async (needle) => {
    await page.fill('#search', needle);
    await page.waitForTimeout(500);
    await page.evaluate(() => document.querySelector('.list .row')?.dispatchEvent(new MouseEvent('click', { bubbles: true })));
    await page.waitForTimeout(600);
    const out = await page.evaluate(() => {
      const c = [...document.querySelectorAll('.inspector .card, .card')].find((x) => /^\s*Loading time/.test(x.querySelector('h3')?.textContent ?? ''));
      return c ? { aside: c.querySelector('.aside')?.textContent.trim(), text: c.textContent.replace(/\s+/g, ' ').trim() } : null;
    });
    await page.fill('#search', '');
    await page.waitForTimeout(400);
    return out;
  };
  const timed = await inspect('prepatcher');
  ok('the Inspector has a loading-time panel', !!timed, JSON.stringify(timed));
  ok('and for a measured mod it credits Loading Progress', /Loading Progress/.test(timed?.text ?? '') && /measured by/i.test(timed?.aside ?? ''), `${timed?.aside} | ${timed?.text?.slice(0, 70)}`);
  ok('and never calls that measurement an estimate', !/estimated/i.test(timed?.text ?? ''), timed?.text?.slice(0, 120));

  // A mod Loading Progress never timed: the panel must say so rather than model one.
  const untimed = await inspect('visual exceptions');
  ok('an untimed mod says it was not measured', /not measured/i.test(untimed?.aside ?? ''), `${untimed?.aside}`);
  ok('and offers no milliseconds at all', !/\d+\s*ms|\d+(\.\d+)?\s*s\b|\d+(\.\d+)?\s*min/.test(untimed?.text ?? ''), untimed?.text?.slice(0, 140));
  ok('while still saying what is in the folder', /What is in it|has not timed/i.test(untimed?.text ?? ''), untimed?.text?.slice(0, 110));

  await page.screenshot({ path: `${OUT}/columns.png` });

  console.log('errors:', errors.length ? errors.join('\n') : 'none');
  if (errors.length) failures += errors.length;
  await browser.close();
  console.log(failures ? `\n${failures} failed.` : '\nall good');
  process.exit(failures ? 1 : 0);
})();
