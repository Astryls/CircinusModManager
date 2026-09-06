#!/usr/bin/env node
// Wire this checkout up to release, once. Everything here is mechanical and everything here has
// been done by hand at least once, which is why it exists.
//
//   node tools/setup-release.mjs             do it
//   node tools/setup-release.mjs --dry-run   say what it would do
//   node tools/setup-release.mjs --key <path to circinus-updater.key>
//
// It puts the workflow where GitHub looks for it, clears out what the .NET sidecar left behind,
// and sets the two repository secrets a release needs. Run it again any time: it only does what
// is not already done, and says so either way.
import { readFile, writeFile, mkdir, rm, readdir, stat, access } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { createInterface } from "node:readline/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const args = process.argv.slice(2);
const has = (f) => args.includes(`--${f}`);
const flag = (f) => {
  const i = args.indexOf(`--${f}`);
  return i >= 0 && args[i + 1] && !args[i + 1].startsWith("--") ? args[i + 1] : null;
};
const dry = has("dry-run");

let done = 0;
let already = 0;
let left = [];
const did = (m) => { done++; console.log(`  done     ${m}`); };
const fine = (m) => { already++; console.log(`  already  ${m}`); };
const todo = (m) => { left.push(m); console.log(`  left     ${m}`); };
const exists = async (p) => access(p).then(() => true).catch(() => false);

/** gh, with the value on stdin so a secret never becomes a command-line argument. */
function gh(ghArgs, input) {
  if (dry) return { ok: true, out: "", dry: true };
  const r = spawnSync("gh", ghArgs, { cwd: ROOT, encoding: "utf8", input, shell: process.platform === "win32" });
  return { ok: r.status === 0, out: `${r.stdout ?? ""}${r.stderr ?? ""}`.trim() };
}

// ---------------------------------------------------------------- the workflow
async function workflow() {
  console.log("\nThe workflow");
  const src = path.join(ROOT, "tools", "release-workflow.yml");
  const dst = path.join(ROOT, ".github", "workflows", "release.yml");
  const want = await readFile(src, "utf8");
  const have = (await exists(dst)) ? await readFile(dst, "utf8") : null;
  if (have === want) return fine(".github/workflows/release.yml is up to date");
  if (!dry) {
    await mkdir(path.dirname(dst), { recursive: true });
    await writeFile(dst, want);
  }
  did(have === null ? "wrote .github/workflows/release.yml" : "updated .github/workflows/release.yml from tools/release-workflow.yml");
}

// ---------------------------------------------------------------- what the sidecar left
async function leftovers() {
  console.log("\nWhat the .NET sidecar left behind");
  // Removed from the repository when circinus-core::clr replaced it, but a file deleted in one
  // checkout stays on disk in another until something deletes it there too.
  const gone = ["scripts/sidecar.mjs", "src-tauri/tauri.release.conf.json", "src-tauri/binaries"];
  let any = false;
  for (const rel of gone) {
    const p = path.join(ROOT, rel);
    if (!(await exists(p))) continue;
    any = true;
    const size = (await stat(p)).isDirectory() ? "" : ` (${Math.round((await stat(p)).size / 1024)} KB)`;
    if (!dry) await rm(p, { recursive: true, force: true });
    did(`removed ${rel}${size}`);
  }
  // A directory that held only that file is itself left behind.
  for (const rel of ["scripts"]) {
    const p = path.join(ROOT, rel);
    if ((await exists(p)) && (await readdir(p)).length === 0) {
      if (!dry) await rm(p, { recursive: true, force: true });
      did(`removed the now-empty ${rel}/`);
      any = true;
    }
  }
  if (!any) fine("nothing of it is left");
}

// ---------------------------------------------------------------- the secrets
async function secrets() {
  console.log("\nRepository secrets");
  const auth = gh(["auth", "status"]);
  if (!auth.ok && !dry) {
    todo("the GitHub CLI is not signed in: run `gh auth login`, then run this again");
    todo("set TAURI_SIGNING_PRIVATE_KEY and CIRCINUS_BUILD_KEY by hand or with the CLI");
    return;
  }
  const repo = gh(["repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"]);
  const where = repo.ok && repo.out ? repo.out : "this repository";
  const listed = gh(["secret", "list"]);
  const set = new Set((listed.out ?? "").split("\n").map((l) => l.split(/\s/)[0]).filter(Boolean));
  console.log(`  ${dry ? "would use" : "using"} ${where}`);

  // The updater key. Without it the build fails outright, which is the good failure: the bad one
  // is a build that succeeds unsigned, uploads happily, and is never offered to anyone.
  if (set.has("TAURI_SIGNING_PRIVATE_KEY")) {
    fine("TAURI_SIGNING_PRIVATE_KEY is set");
  } else {
    const candidates = [flag("key"), path.join(ROOT, "..", "circinus-updater.key"), path.join(process.env.HOME ?? process.env.USERPROFILE ?? "", "circinus-updater.key")].filter(Boolean);
    let key = null;
    for (const c of candidates) if (await exists(c)) { key = c; break; }
    if (!key) {
      todo(`TAURI_SIGNING_PRIVATE_KEY is not set and circinus-updater.key was not found beside the repo. Pass --key <path>.`);
    } else {
      const body = await readFile(key, "utf8");
      const r = gh(["secret", "set", "TAURI_SIGNING_PRIVATE_KEY"], body);
      if (r.ok) did(`set TAURI_SIGNING_PRIVATE_KEY from ${path.relative(ROOT, key) || key}`);
      else todo(`could not set TAURI_SIGNING_PRIVATE_KEY: ${r.out}`);
    }
  }
  // Its password is deliberately absent rather than empty: a secret that does not exist already
  // arrives as an empty string, and the key has no password.

  if (set.has("CIRCINUS_BUILD_KEY")) {
    fine("CIRCINUS_BUILD_KEY is set");
  } else {
    let body = process.env.CIRCINUS_BUILD_KEY ?? null;
    if (!body && !dry && process.stdin.isTTY) {
      const rl = createInterface({ input: process.stdin, output: process.stdout });
      body = (await rl.question("  build key (cmk_... from circinus.sh/admin, or blank to skip): ")).trim();
      rl.close();
    }
    if (!body) todo("CIRCINUS_BUILD_KEY is not set: `gh secret set CIRCINUS_BUILD_KEY` and paste it when asked");
    else if (!/^cmk_/.test(body)) todo("that does not look like a build key (they start with cmk_); nothing was set");
    else {
      const r = gh(["secret", "set", "CIRCINUS_BUILD_KEY"], body);
      if (r.ok) did("set CIRCINUS_BUILD_KEY");
      else todo(`could not set CIRCINUS_BUILD_KEY: ${r.out}`);
    }
  }
}

const main = async () => {
  console.log(`Setting this checkout up to release.${dry ? "  (dry run)" : ""}`);
  await workflow();
  await leftovers();
  await secrets();

  console.log(`\n${done} done, ${already} already in place${left.length ? `, ${left.length} left` : ""}.`);
  if (left.length) {
    console.log("\nStill to do:");
    for (const l of left) console.log(`  - ${l}`);
  }
  const tracked = spawnSync("git", ["status", "--porcelain"], { cwd: ROOT, encoding: "utf8", shell: process.platform === "win32" }).stdout?.trim();
  if (tracked && !dry) console.log(`\nCommit what changed, then:  node tools/release.mjs <version>`);
  else if (!left.length) console.log(`\nReady:  node tools/release.mjs <version>`);
};

main().catch((e) => {
  console.error(`\n${e?.stack ?? e}`);
  process.exit(1);
});
