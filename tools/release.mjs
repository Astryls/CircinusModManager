#!/usr/bin/env node
// Cut a release: set the version everywhere it is written, check the thing still works, tag it,
// and let the tag build all three platforms and push them to circinus.sh.
//
//   node tools/release.mjs 0.2.0            the normal path: three platforms, built by CI
//   node tools/release.mjs 0.2.0 --dry-run  say what would happen and change nothing
//   node tools/release.mjs --local          build this machine's platform and push only that
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
const dry = has("dry-run");
const version = args.find((a) => !a.startsWith("--")) ?? null;

const say = (m = "") => console.log(m);
const die = (m) => {
  console.error(`\n${m}`);
  process.exit(1);
};

/** Run a command where the user can see it, and stop the release if it fails. */
function run(cmd, cmdArgs, { allowFail = false, quiet = false } = {}) {
  if (dry) {
    say(`  would run: ${cmd} ${cmdArgs.join(" ")}`);
    return { ok: true, out: "" };
  }
  const r = spawnSync(cmd, cmdArgs, { cwd: ROOT, encoding: "utf8", stdio: quiet ? "pipe" : "inherit", shell: process.platform === "win32" });
  const out = (r.stdout ?? "") + (r.stderr ?? "");
  if (r.status !== 0 && !allowFail) die(`${cmd} ${cmdArgs.join(" ")} failed.${quiet ? `\n${out}` : ""}`);
  return { ok: r.status === 0, out: out.trim() };
}
const capture = (cmd, cmdArgs) => spawnSync(cmd, cmdArgs, { cwd: ROOT, encoding: "utf8", shell: process.platform === "win32" });
const have = (cmd) => capture(process.platform === "win32" ? "where" : "which", [cmd]).status === 0;

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
  const edits = [
    ["package.json", (s) => s.replace(/("version":\s*")[^"]+(")/, `$1${v}$2`)],
    ["src-tauri/tauri.conf.json", (s) => s.replace(/("version":\s*")[^"]+(")/, `$1${v}$2`)],
    ["Cargo.toml", (s) => s.replace(/(\[workspace\.package\][\s\S]*?\nversion\s*=\s*")[^"]+(")/, `$1${v}$2`)]
  ];
  for (const [file, edit] of edits) {
    const p = path.join(ROOT, file);
    const before = await readFile(p, "utf8");
    const after = edit(before);
    if (before === after) die(`Could not find the version in ${file}. It is written in three places and all three have to agree; set it by hand and run again.`);
    if (!dry) await writeFile(p, after);
    say(`  ${file} -> ${v}`);
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
async function cut(v) {
  if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(v)) die(`"${v}" is not a version. Use 0.2.0, or 0.3.0-beta.1 for a pre-release.`);

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

  say("\nChecks:");
  run("cargo", ["test", "--workspace"]);
  run("npm", ["run", "check"]);
  run("npm", ["run", "build"]);
  run("node", ["tools/loadtest/push-build.cjs"]);

  say("\nTagging:");
  run("git", ["add", "-A"]);
  run("git", ["-c", "user.name=Circinus", "-c", "user.email=noreply@circinus.sh", "commit", "-m", `Circinus Mod Manager ${v}`]);
  // An annotated tag, because its message becomes the release notes on the site and in the app.
  run("git", ["tag", "-a", tag, "-m", `Circinus Mod Manager ${v}`]);
  run("git", ["push", "origin", branch]);
  run("git", ["push", "origin", tag]);
  if (dry) return say("\nDry run: nothing was changed, committed or pushed.");

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

main().catch((e) => die(e?.stack ?? String(e)));
