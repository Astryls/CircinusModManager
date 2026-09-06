#!/usr/bin/env node
// Cut a release: set the version everywhere it is written, check the thing still works, tag it,
// and let the tag build all three platforms and push them to circinus.sh.
//
//   node tools/release.mjs 0.2.0            the normal path: three platforms, built by CI
//   node tools/release.mjs 0.2.0 --dry-run  say what would happen and change nothing
//   node tools/release.mjs 0.2.0 --notes "..."   the release notes players will read
//   node tools/release.mjs --local          build this machine's platform and push only that
//   node tools/release.mjs --setup          wire this checkout up to release (tools/setup-release.mjs)
//   node tools/release.mjs 0.2.0 --checks-in-ci   do not run the tests here; let CI run them
//   node tools/release.mjs 0.2.0 --tag-only  set the version, commit, tag, push, and stop
//
// The easiest way to release is not to run this at all: Actions -> Release -> Run workflow, type
// the version, press the button. That runs this with --tag-only on GitHub's machine and then
// builds and publishes in the same run. This script by hand is for when you want the checks to
// run here first, or want to watch it from a terminal.
//
// `--checks-in-ci` is for when this machine cannot get through a test that has nothing to do
// with the release. CI runs the same tests on Windows, macOS and Linux before it builds
// anything and publishes nothing unless they pass, so the release is still gated — it just
// costs a tag instead of a rerun when something is genuinely broken.
//
// Why the normal path goes through CI: a Windows installer can only be made on Windows, a .dmg
// and its .app.tar.gz only on macOS, an AppImage only on Linux. No one machine can produce the
// set, so "build all three and push them" means pushing a tag and letting three machines do it.
// `--local` is the exception, for a fix that only affects the platform you are sitting at.
import { readFile, writeFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const BASE = process.env.CIRCINUS_BASE ?? "https://circinus.sh";

const args = process.argv.slice(2);
const has = (f) => args.includes(`--${f}`);
const flag = (f) => {
  const i = args.indexOf(`--${f}`);
  return i >= 0 && args[i + 1] && !args[i + 1].startsWith("--") ? args[i + 1] : null;
};
const dry = has("dry-run");
const version = args.find((a) => !a.startsWith("--")) ?? null;

const say = (m = "") => console.log(m);
const die = (m) => {
  console.error(`\n${m}`);
  process.exit(1);
};

/**
 * Windows cannot start npm or gh without a shell: they are `.cmd` files, and Node refuses to
 * execute one directly. But a shell means the arguments stop being a list and become a string
 * the shell splits again on spaces, so `-m Circinus Mod Manager 1.0.0` arrives as four arguments
 * and git reads three of them as pathspecs. Everything else here is a real `.exe` and needs no
 * shell, so it does not get one; what does get one gets its arguments quoted for it.
 */
const NEEDS_SHELL = new Set(["npm", "npx", "gh"]);
const shelled = (cmd) => process.platform === "win32" && NEEDS_SHELL.has(cmd);
/** Quote one argument the way cmd.exe will take it back apart into exactly what was meant. */
const quote = (a) => (/[\s"^&|<>()%!]/.test(a) ? `"${a.replace(/(\\*)"/g, '$1$1\\"').replace(/(\\+)$/, "$1$1")}"` : a);
const forShell = (cmd, a) => (shelled(cmd) ? a.map(quote) : a);

/** Run a command where the user can see it, and stop the release if it fails. */
function run(cmd, cmdArgs, { allowFail = false, quiet = false } = {}) {
  if (dry) {
    say(`  would run: ${cmd} ${cmdArgs.map(quote).join(" ")}`);
    return { ok: true, out: "" };
  }
  const r = spawnSync(cmd, forShell(cmd, cmdArgs), { cwd: ROOT, encoding: "utf8", stdio: quiet ? "pipe" : "inherit", shell: shelled(cmd) });
  const out = (r.stdout ?? "") + (r.stderr ?? "");
  if (r.status !== 0 && !allowFail) die(`${cmd} ${cmdArgs.map(quote).join(" ")} failed.${quiet ? `\n${out}` : ""}`);
  return { ok: r.status === 0, out: out.trim() };
}
const capture = (cmd, cmdArgs) => spawnSync(cmd, forShell(cmd, cmdArgs), { cwd: ROOT, encoding: "utf8", shell: shelled(cmd) });
const have = (cmd) => capture(process.platform === "win32" ? "where" : "which", [cmd]).status === 0;

/**
 * Who a release is by. Passed to every git command that records a person rather than read from
 * the machine's config, because the two commands that record one are a commit and an annotated
 * tag, and giving it to only the commit is a bug that cannot happen on a developer's machine:
 * git falls back to the global config there and there is always one. A fresh CI runner has no
 * config, so the commit was made and the tag right after it refused, with the release half done.
 */
const AS_CIRCINUS = ["-c", "user.name=Circinus", "-c", "user.email=noreply@circinus.sh"];

/** Semver, only as far as this needs it: is `a` newer than `b`? */
function newer(a, b) {
  const parts = (v) => v.replace(/^v/, "").split("-")[0].split(".").map(Number);
  const [x, y] = [parts(a), parts(b)];
  for (let i = 0; i < 3; i++) if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) > (y[i] ?? 0);
  // Same numbers: a release beats the pre-release of the same number, and nothing else counts.
  return !a.includes("-") && b.includes("-");
}

/** The three files that carry the version, and Cargo.lock which follows them. */
async function setVersion(v) {
  // Each is the place the version is written and a way to recognise it: the pattern says whether
  // this file has a version at all, and the group inside it is the number. Keeping those two
  // questions apart is what lets a re-run tell "already done" from "not found", which are the
  // same edit and opposite problems.
  const edits = [
    ["package.json", /("version":\s*")([^"]+)(")/],
    ["src-tauri/tauri.conf.json", /("version":\s*")([^"]+)(")/],
    ["Cargo.toml", /(\[workspace\.package\][\s\S]*?\nversion\s*=\s*")([^"]+)(")/]
  ];
  for (const [file, pattern] of edits) {
    const p = path.join(ROOT, file);
    const before = await readFile(p, "utf8");
    const found = before.match(pattern);
    if (!found) die(`Could not find the version in ${file}. It is written in three places and all three have to agree; set it by hand and run again.`);
    // A re-run after something later failed finds its own earlier work here. That is not an
    // error and saying it is leaves the run stuck with nothing it can fix.
    if (found[2] === v) {
      say(`  ${file} already ${v}`);
      continue;
    }
    if (!dry) await writeFile(p, before.replace(pattern, `$1${v}$3`));
    say(`  ${file} ${found[2]} -> ${v}`);
  }
  // Cargo.lock carries the workspace crates' own versions. If this cannot run, the next build
  // updates the lock itself, which is harmless as long as nothing builds with --locked.
  const lock = run("cargo", ["update", "--workspace", "--offline", "--quiet"], { allowFail: true, quiet: true });
  if (!lock.ok) say("  Cargo.lock not updated here; the next cargo build will do it.");
}

// ---------------------------------------------------------------- local, one platform
async function local() {
  if (!process.env.CIRCINUS_BUILD_KEY && !dry) {
    die("No build key. Set CIRCINUS_BUILD_KEY to the cmk_... key from circinus.sh/admin, then run again.\n" +
      "  PowerShell:  $env:CIRCINUS_BUILD_KEY = 'cmk_...'\n" +
      "  bash:        export CIRCINUS_BUILD_KEY=cmk_...");
  }
  const conf = JSON.parse(await readFile(path.join(ROOT, "src-tauri/tauri.conf.json"), "utf8"));
  const what = { win32: "the Windows installer", darwin: "the Mac disk image and update bundle", linux: "the Linux AppImage" }[process.platform] ?? process.platform;
  say(`Building ${conf.version} on this machine: ${what}.`);
  say("The other platforms are not built here and will not be part of this release.\n");

  if (!process.env.TAURI_SIGNING_PRIVATE_KEY) {
    die("No TAURI_SIGNING_PRIVATE_KEY. The build would produce installers with no .sig beside them,\n" +
      "which upload happily and are never offered to anyone's updater. Set it to the contents of\n" +
      "the updater private key (docs/update-feed.md says where it lives) and run again.");
  }
  run("npm", ["run", "release"]);
  run("node", ["tools/push-build.mjs", "--force", ...(dry ? ["--dry-run"] : [])]);
  say("\nDone. Only this platform is in that release; the others still have whatever was published before.");
}

// ---------------------------------------------------------------- the normal path
/**
 * What has to be true before a tag is worth pushing. Every one of these has been the reason a
 * release did not happen, and every one of them is quiet: the tag goes up and nothing builds,
 * or three things build and none of them can be published.
 */
async function preflight() {
  const problems = [];
  const src = path.join(ROOT, "tools", "release-workflow.yml");
  const dst = path.join(ROOT, ".github", "workflows", "release.yml");
  const read = async (p) => readFile(p, "utf8").catch(() => null);
  const [want, have] = [await read(src), await read(dst)];
  if (want && have === null) problems.push("there is no .github/workflows/release.yml, so the tag will build nothing");
  else if (want && have !== want) problems.push(".github/workflows/release.yml has drifted from tools/release-workflow.yml");

  const auth = capture("gh", ["auth", "status"]);
  if (auth.status !== 0) {
    problems.push("the GitHub CLI is not signed in, so the secrets cannot be checked from here");
  } else {
    const listed = capture("gh", ["secret", "list"]).stdout ?? "";
    const set = new Set(listed.split("\n").map((l) => l.split(/\s/)[0]).filter(Boolean));
    // Its password is not checked: a secret that does not exist arrives as an empty string,
    // which is what a key with no password wants.
    if (!set.has("TAURI_SIGNING_PRIVATE_KEY")) problems.push("TAURI_SIGNING_PRIVATE_KEY is not set, so the build will fail rather than write unsigned installers");
    if (!set.has("CIRCINUS_BUILD_KEY")) problems.push("CIRCINUS_BUILD_KEY is not set, so the build will not be sent to circinus.sh");
  }
  if (problems.length) {
    die(`Not ready to release:\n${problems.map((p) => `  - ${p}`).join("\n")}\n\nMost of that is what \`node tools/setup-release.mjs\` is for.`);
  }
}

async function cut(v) {
  if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(v)) die(`"${v}" is not a version. Use 0.2.0, or 0.3.0-beta.1 for a pre-release.`);
  // `--tag-only` is this same script running inside the workflow, doing the half a release
  // needs before there is anything to build: set the version, commit, tag, push. Everything
  // after that is the workflow's own jobs, so it stops there. It skips preflight because
  // preflight asks whether the workflow is in place and its secrets are set, which is a strange
  // question to ask from inside a run of that workflow using those secrets.
  const inCi = has("tag-only");
  if (!dry && !inCi) await preflight();

  // Nothing half-finished, and nothing that only exists on this machine.
  const status = capture("git", ["status", "--porcelain"]).stdout.trim();
  if (status && !dry) die(`The working tree is not clean:\n${status}\nCommit or stash before cutting a release.`);
  const branch = capture("git", ["rev-parse", "--abbrev-ref", "HEAD"]).stdout.trim();
  const tag = `v${v}`;
  if (capture("git", ["rev-parse", "-q", "--verify", `refs/tags/${tag}`]).status === 0) {
    die(`${tag} already exists. Releases are not re-cut: a version people may already have must not be replaced, because two machines reporting the same number would be running different builds. Use ${v.replace(/(\d+)$/, (n) => Number(n) + 1)}.`);
  }
  const last = capture("git", ["describe", "--tags", "--abbrev=0"]).stdout.trim();
  if (last && !newer(v, last) && !v.includes("-")) {
    die(`${v} is not newer than ${last}. latest.json names one version and every installed copy compares it to its own; a lower number leaves every app saying it is up to date forever, and it never sees the next real release either.`);
  }

  say(`Cutting ${tag}${last ? ` (after ${last})` : ""} on ${branch}.${dry ? "  (dry run)" : ""}\n`);
  say("Version:");
  await setVersion(v);

  // The same `cargo test --workspace` and `npm run check` run in CI on all three platforms
  // before anything is built, and nothing is published unless all three pass. Running them here
  // first is worth the minutes because a failure found now costs a rerun and a failure found
  // there costs a version number. But when the only thing failing is this machine's platform,
  // waiting on it means the release cannot happen at all, and CI is the better judge anyway:
  // it runs the same tests on three machines instead of one.
  if (inCi) {
    say("\nChecks: the three build jobs in this run do them, on the platform each one ships.");
  } else if (has("checks-in-ci")) {
    say("\nChecks: skipped here at your asking. CI runs cargo test and npm run check on all three");
    say("platforms before it builds anything, and publishes nothing unless they pass. If they fail");
    say(`there, ${tag} is spent: no build goes out under it and the next attempt needs a new number.`);
    run("npm", ["run", "build"]);
  } else {
    say("\nChecks:");
    run("cargo", ["test", "--workspace"]);
    run("npm", ["run", "check"]);
    run("npm", ["run", "build"]);
    run("node", ["tools/loadtest/push-build.cjs"]);
    // Cuts a whole release in a throwaway repository with no git identity anywhere, which is
    // what a runner is. Not run under --tag-only, where this script is already partway through
    // cutting one for real.
    run("node", ["tools/loadtest/release-tag.mjs"], { quiet: true });
    say("  tagging works on a machine that has never been told who you are");
  }
  // This one runs either way: it is a check on the release machinery itself rather than on the
  // app, it takes a second, and skipping it would skip the test for the bug that made this flag
  // necessary.
  run("node", ["tools/loadtest/release-args.mjs"], { quiet: true });
  say("  the release's own command lines hold together");

  // The tag's message becomes the release notes: what the download page shows and what the app
  // puts in the update banner. Left to itself that is the version number, which tells a player
  // nothing about whether to take the update.
  const notes = (flag("notes") ?? `Circinus Mod Manager ${v}`).replace(/[—–]/g, "-").replace(/[‘’]/g, "'").replace(/[“”]/g, '"');
  if (!flag("notes")) {
    say(`\nNo release notes given, so they will read "${notes}".`);
    say("Pass --notes \"what changed, in a sentence or two\" to say something a player can act on.");
  }
  say("\nTagging:");
  // A second attempt at a version whose files were already set has nothing to commit, and git
  // exits non-zero saying so. That is not a problem: the commit to tag is the one already here.
  // Only an empty commit would be wrong, because it would put a second commit on main claiming
  // to be the release and leave the tag on neither obvious one.
  const pending = capture("git", ["status", "--porcelain"]).stdout.trim();
  if (pending) {
    run("git", ["add", "-A"]);
    run("git", [...AS_CIRCINUS, "commit", "-m", `Circinus Mod Manager ${v}`]);
  } else {
    say(`  nothing to commit: this checkout already says ${v}, so ${tag} goes on the commit that is here`);
  }
  // An annotated tag, because its message is what tools/push-build.mjs sends as the notes.
  run("git", [...AS_CIRCINUS, "tag", "-a", tag, "-m", notes]);
  run("git", ["push", "origin", branch]);
  run("git", ["push", "origin", tag]);
  if (dry) return say("\nDry run: nothing was changed, committed or pushed.");
  // Inside the workflow there is nothing left to say: the jobs that build and publish are the
  // rest of the run this is part of, and a tag pushed by a workflow deliberately does not start
  // a second one.
  if (inCi) return say(`\n${tag} is pushed at ${capture("git", ["rev-parse", "HEAD"]).stdout.trim()}.`);

  say(`\n${tag} is pushed. Three machines are now building it:`);
  say("  Windows  the NSIS installer");
  say("  macOS    the disk image and the update bundle (Apple Silicon; the site refuses Intel)");
  say("  Linux    the AppImage");
  say("Then one job uploads all four files to circinus.sh, publishes, and checks latest.json.\n");

  if (!have("gh")) {
    say("Watch it at:  https://github.com/<owner>/<repo>/actions");
    say("Install the GitHub CLI and this script will follow the run for you.");
    return;
  }
  say("Following the run (Ctrl C leaves it running):\n");
  const watched = run("gh", ["run", "watch", "--exit-status", "--compact"], { allowFail: true });
  if (!watched.ok) {
    say("\nThe run did not finish cleanly. `gh run view --log-failed` says which step, and nothing is");
    say("published unless the push step got all the way through: a release stays invisible until it");
    say("is published, which is the last thing that happens.");
    process.exit(1);
  }
  await confirm(v);
}

/** What an installed copy will actually be served. The only answer that matters. */
async function confirm(v) {
  try {
    const res = await fetch(`${BASE}/api/v1/releases/latest.json`, { headers: { Accept: "application/json" } });
    if (!res.ok) return say(`\nPublished, but ${BASE}/api/v1/releases/latest.json answered ${res.status}. Check the site.`);
    const j = await res.json();
    const named = String(j.version ?? "").replace(/^v/, "");
    const platforms = Object.keys(j.platforms ?? {});
    const unsigned = Object.entries(j.platforms ?? {}).filter(([, p]) => !p?.signature).map(([k]) => k);
    say(`\nlatest.json names ${named} for ${platforms.join(", ") || "nothing"}.`);
    if (named !== v) say(`That is not ${v}. Installed copies will not be offered this release.`);
    else if (unsigned.length) say(`No signature for ${unsigned.join(", ")}; those copies will refuse the update.`);
    else say("Signed for every platform in it. An installed copy asking for an update will be offered this one.");
  } catch (e) {
    say(`\nCould not read latest.json: ${e.message}`);
  }
}

const main = async () => {
  if (has("setup")) return run("node", ["tools/setup-release.mjs", ...args.filter((a) => a !== "--setup")]);
  if (has("local")) return local();
  if (!version) {
    die("Which version? `node tools/release.mjs 0.2.0`.\n\n" +
      "That sets the version in package.json, src-tauri/tauri.conf.json and Cargo.toml, runs the\n" +
      "checks, tags it and pushes; the tag is what builds Windows, macOS and Linux and pushes all\n" +
      "three to circinus.sh. One machine cannot build the other two, so `--local` builds and pushes\n" +
      "only the platform you are sitting at.");
  }
  await cut(version.replace(/^v/, ""));
};

// Run when invoked, importable when tested: the argument quoting below is the reason 1.0.0 did
// not get tagged the first time, and a rule that decides how a command line is built deserves a
// test that does not involve cutting a release to find out.
export { quote, forShell, newer };
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((e) => die(e?.stack ?? String(e)));
}
