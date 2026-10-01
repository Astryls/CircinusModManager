// One copy of each URL that can change.
//
// An invite code is not forever. The Discord link changed, and updating it meant finding four
// places because two of them had pasted the URL instead of importing the constant -- including
// the crash screen, which is the one that matters most and the one nobody looks at until
// something has already gone wrong. A dead link there is a bug report that never gets filed.
//
// This is a cheap guard against the whole class: a URL that lives in a constant may appear
// exactly once in the source, in the file that declares it. Everything else imports.
//
// README.md is exempt and listed as such: markdown cannot import, and a README that tells
// people to go somewhere without saying where is worse than a second copy.
//
//   node tools/links-check.mjs
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;

/** Each URL that changes, where its one copy belongs, and what may repeat it. */
const SINGLE = [
  {
    what: "the Discord invite",
    pattern: /https:\/\/discord\.gg\/[A-Za-z0-9]+/g,
    home: "src/lib/types.ts",
    constant: "DISCORD",
    // Cannot import the constant: prose for humans, and the standalone mockup scripts under
    // tools/, which are run directly by node and have no bundler behind them. They are still
    // covered by the agreement check below -- the rule that actually prevents a dead link.
    exempt: ["README.md", "docs/", "CLAUDE.md", "ADD-LOAD-COSTS.md", "tools/"]
  }
];

const SKIP_DIR = new Set(["node_modules", "target", "dist", ".git", "build", "gen"]);
const EXT = new Set([".ts", ".tsx", ".js", ".mjs", ".cjs", ".svelte", ".rs", ".html", ".json", ".yml", ".yaml", ".md"]);

function* walk(dir) {
  for (const name of readdirSync(dir)) {
    if (SKIP_DIR.has(name)) continue;
    const p = join(dir, name);
    const st = statSync(p);
    if (st.isDirectory()) yield* walk(p);
    else if (EXT.has(p.slice(p.lastIndexOf(".")))) yield p;
  }
}

let bad = 0;
for (const rule of SINGLE) {
  const found = [];
  for (const file of walk(ROOT)) {
    const rel = relative(ROOT, file).split("\\").join("/");
    if (rule.exempt.some((e) => rel === e || rel.startsWith(e))) continue;
    const text = readFileSync(file, "utf8");
    for (const m of text.matchAll(rule.pattern)) found.push({ rel, url: m[0] });
  }
  const strays = found.filter((f) => f.rel !== rule.home);
  if (strays.length) {
    bad++;
    console.error(`links: ${rule.what} is written out in ${strays.length} place(s) that should import \`${rule.constant}\` from ${rule.home}:`);
    for (const s of strays) console.error(`  ${s.rel}  ${s.url}`);
  }
  // And the constant has to still be there: a rule guarding a home that no longer exists
  // passes by accident and guards nothing.
  const home = found.filter((f) => f.rel === rule.home);
  if (!home.length) {
    bad++;
    console.error(`links: ${rule.what} is not in ${rule.home} at all. Has \`${rule.constant}\` moved? Point this rule at its new home.`);
  }
  // Every copy, exempt ones included, must agree. A README pointing at a dead invite is the
  // same failure as a component doing it.
  const all = new Set();
  for (const file of walk(ROOT)) {
    const rel = relative(ROOT, file).split("\\").join("/");
    const text = readFileSync(file, "utf8");
    for (const m of text.matchAll(rule.pattern)) all.add(`${m[0]}`);
  }
  if (all.size > 1) {
    bad++;
    console.error(`links: ${rule.what} has ${all.size} different values in the tree: ${[...all].join(", ")}`);
  }
  if (!strays.length && all.size <= 1) {
    console.log(`links: ${rule.what} is in one place (${rule.home}) and agrees everywhere it is quoted.`);
  }
}

process.exit(bad ? 1 : 0);
