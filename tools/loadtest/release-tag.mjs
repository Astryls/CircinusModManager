#!/usr/bin/env node
// Cutting a tag, on a machine that has never been told who you are.
//
//   node tools/loadtest/release-tag.mjs
//
// v1.0.0 was tagged twice by hand before anyone found out that `git tag -a` needs a tagger and
// was not being given one. It cannot be found on a developer's machine: git falls back to the
// global config there, and there is always one. On a fresh CI runner there is not, so the commit
// was made, the tag right after it refused, and the release stopped half done -- after the
// version had been written into three files and before anything was pushed.
//
// So this cuts a real release in a throwaway repository with every git identity taken away, and
// then asks git what it recorded. Everything here runs in about a second and needs no network.
import { spawnSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync, mkdirSync, copyFileSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

// release.mjs works on the repository it lives in, not the one you are standing in, so testing
// it against a throwaway repository means putting a copy of it inside that repository.
const TOOLS = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

let bad = 0;
const ok = (what, got, want) => {
  const same = JSON.stringify(got) === JSON.stringify(want);
  if (!same) bad++;
  console.log(`  ${same ? "ok  " : "FAIL"}  ${what}${same ? "" : `\n          got  ${JSON.stringify(got)}\n          want ${JSON.stringify(want)}`}`);
};

// No global config, no system config, and a home directory with nothing in it: this is what a
// runner looks like, and it is the whole point of the test.
const NOBODY = { ...process.env, GIT_CONFIG_GLOBAL: "/dev/null", GIT_CONFIG_SYSTEM: "/dev/null" };
// Removed rather than emptied: an empty GIT_AUTHOR_NAME overrides the config instead of being
// absent from it, which would break every git command here including the ones setting the scene.
for (const k of ["GIT_AUTHOR_NAME", "GIT_AUTHOR_EMAIL", "GIT_COMMITTER_NAME", "GIT_COMMITTER_EMAIL"]) delete NOBODY[k];

const root = mkdtempSync(path.join(tmpdir(), "circinus-tag-"));
const origin = path.join(root, "origin.git");
const work = path.join(root, "work");
const git = (args, cwd = work) => spawnSync("git", args, { cwd, encoding: "utf8", env: NOBODY });

try {
  spawnSync("git", ["init", "-q", "--bare", origin], { encoding: "utf8", env: NOBODY });
  spawnSync("git", ["clone", "-q", origin, work], { encoding: "utf8", env: NOBODY });
  mkdirSync(path.join(work, "src-tauri"), { recursive: true });
  mkdirSync(path.join(work, "tools", "loadtest"), { recursive: true });
  copyFileSync(path.join(TOOLS, "release.mjs"), path.join(work, "tools", "release.mjs"));
  // cut() runs this one as part of its checks, so the copy needs it too.
  copyFileSync(path.join(TOOLS, "loadtest", "release-args.mjs"), path.join(work, "tools", "loadtest", "release-args.mjs"));
  writeFileSync(path.join(work, "package.json"), '{\n  "version": "0.1.0"\n}\n');
  writeFileSync(path.join(work, "src-tauri", "tauri.conf.json"), '{\n  "version": "0.1.0"\n}\n');
  writeFileSync(path.join(work, "Cargo.toml"), '[workspace.package]\nversion = "0.1.0"\n');
  // The base commit needs an identity of its own, and giving it a different one proves the tag
  // below is named by the script rather than inherited from whatever was here first.
  git(["-c", "user.name=Somebody", "-c", "user.email=somebody@example.com", "add", "-A"]);
  git(["-c", "user.name=Somebody", "-c", "user.email=somebody@example.com", "commit", "-qm", "base"]);
  git(["branch", "-M", "main"]);
  git(["push", "-q", "origin", "main"]);
  const base = git(["rev-parse", "HEAD"]).stdout.trim();

  console.log("\nCutting 1.0.0 where git does not know who anybody is");
  const notes = "First public release. HALO orders your mods.";
  const cut = spawnSync(process.execPath, [path.join(work, "tools", "release.mjs"), "1.0.0", "--tag-only", "--notes", notes], { cwd: work, encoding: "utf8", env: NOBODY });
  const said = `${cut.stdout ?? ""}${cut.stderr ?? ""}`;
  ok("it succeeds", cut.status, 0);
  if (cut.status !== 0) console.log(said.split("\n").map((l) => `          ${l}`).join("\n"));

  console.log("\nWhat git recorded");
  ok("the commit is there", git(["log", "-1", "--format=%s"]).stdout.trim(), "Circinus Mod Manager 1.0.0");
  ok("committed as Circinus", git(["log", "-1", "--format=%an <%ae>"]).stdout.trim(), "Circinus <noreply@circinus.sh>");
  // An annotated tag, not a lightweight one: push-build.mjs reads the tag's message as the
  // release notes, and a lightweight tag has no message to read.
  ok("the tag is annotated", git(["cat-file", "-t", "v1.0.0"]).stdout.trim(), "tag");
  ok("tagged as Circinus", git(["tag", "-l", "--format=%(taggername) <%(taggeremail)>", "v1.0.0"]).stdout.trim().replace(/<<|>>/g, (m) => m[0]), "Circinus <noreply@circinus.sh>");
  ok("the notes players read survived", git(["tag", "-l", "--format=%(contents)", "v1.0.0"]).stdout.trim(), notes);

  console.log("\nWhat reached the remote");
  const head = git(["rev-parse", "HEAD"]).stdout.trim();
  ok("the tag was pushed", git(["ls-remote", "--tags", origin, "v1.0.0"]).stdout.includes("refs/tags/v1.0.0"), true);
  // What the tag points through to. The build jobs check this commit out by hash, so it has to
  // have travelled with the tag even though no branch carries it.
  ok("and the commit it names went with it", git(["ls-remote", origin, "refs/tags/v1.0.0^{}"]).stdout.split(/\s/)[0], head);
  // And main was left alone. A release commit pushed to main is one that neither the container
  // writing the history nor the machine carrying it has, so the next sync finds two histories
  // that have both moved and stops -- once per release, forever.
  ok("main on the remote did not move", git(["ls-remote", origin, "refs/heads/main"]).stdout.split(/\s/)[0], base);
  ok("which is not where the release commit is", head === base, false);

  console.log("\nAnd it still refuses what it should");
  const again = spawnSync(process.execPath, [path.join(work, "tools", "release.mjs"), "1.0.0", "--tag-only"], { cwd: work, encoding: "utf8", env: NOBODY });
  ok("the same version twice is refused", again.status, 1);
  ok("and says why", /already exists/.test(`${again.stdout}${again.stderr}`), true);
  const back = spawnSync(process.execPath, [path.join(work, "tools", "release.mjs"), "0.9.0", "--tag-only"], { cwd: work, encoding: "utf8", env: NOBODY });
  ok("going backwards is refused", back.status, 1);

  // Every run starts from a clone of main, and main does not carry the release commit any more.
  // So the guards have to hold for a checkout where no tag is an ancestor of anything -- which
  // is what `git describe --tags` answers, and why it is not what decides this.
  console.log("\nFrom a fresh clone of main, where no tag is in the history");
  const fresh = path.join(root, "fresh");
  spawnSync("git", ["clone", "-q", "-b", "main", origin, fresh], { encoding: "utf8", env: NOBODY });
  ok("the release commit is not on main", spawnSync("git", ["merge-base", "--is-ancestor", head, "HEAD"], { cwd: fresh, env: NOBODY }).status !== 0, true);
  const freshBack = spawnSync(process.execPath, [path.join(fresh, "tools", "release.mjs"), "0.9.0", "--tag-only"], { cwd: fresh, encoding: "utf8", env: NOBODY });
  ok("going backwards is still refused", freshBack.status, 1);
  ok("and it names the version it will not go under", /1\.0\.0/.test(`${freshBack.stdout}${freshBack.stderr}`), true);
  const freshUp = spawnSync(process.execPath, [path.join(fresh, "tools", "release.mjs"), "1.0.1", "--tag-only", "--notes", "A fix."], { cwd: fresh, encoding: "utf8", env: NOBODY });
  ok("but the next version is allowed", freshUp.status, 0);
  if (freshUp.status !== 0) console.log(`${freshUp.stdout}${freshUp.stderr}`.split("\n").map((l) => `          ${l}`).join("\n"));

  // Reached through a symlink, which is not a curiosity: macOS puts every temporary directory
  // under /var/folders, a symlink to /private/var/folders, and Windows can name the same
  // directory RUNNER~1. A script that decides whether it was run by comparing the path it was
  // given against the path it resolved to then decides it was imported, does nothing at all,
  // and exits 0 -- which reads exactly like a release that worked.
  console.log("\nReached through a symlink");
  const linked = path.join(root, "linked");
  let madeLink = true;
  try {
    symlinkSync(work, linked, "junction");
  } catch {
    madeLink = false;
  }
  if (!madeLink) {
    console.log("  skipped  this machine does not allow making one");
  } else {
    const viaLink = spawnSync(process.execPath, [path.join(linked, "tools", "release.mjs"), "1.0.0", "--tag-only"], { cwd: linked, encoding: "utf8", env: NOBODY });
    // Refusing 1.0.0 as already cut is proof that it ran at all. Exiting 0 in silence is the bug.
    ok("it runs rather than quietly deciding it was imported", viaLink.status, 1);
    ok("and it is the version guard that stopped it", /already exists/.test(`${viaLink.stdout}${viaLink.stderr}`), true);
  }
} finally {
  rmSync(root, { recursive: true, force: true });
}

console.log(bad ? `\n${bad} failed.\n` : "\nAll good.\n");
process.exit(bad ? 1 : 0);
