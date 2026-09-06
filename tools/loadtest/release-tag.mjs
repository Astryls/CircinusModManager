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
import { mkdtempSync, rmSync, writeFileSync, mkdirSync, copyFileSync } from "node:fs";
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
  ok("main was pushed", git(["rev-parse", "main"]).stdout.trim(), git(["rev-parse", "origin/main"], work).stdout.trim());
  ok("the tag was pushed", git(["ls-remote", "--tags", origin, "v1.0.0"]).stdout.includes("refs/tags/v1.0.0"), true);

  console.log("\nAnd it still refuses what it should");
  const again = spawnSync(process.execPath, [path.join(work, "tools", "release.mjs"), "1.0.0", "--tag-only"], { cwd: work, encoding: "utf8", env: NOBODY });
  ok("the same version twice is refused", again.status, 1);
  ok("and says why", /already exists/.test(`${again.stdout}${again.stderr}`), true);
  const back = spawnSync(process.execPath, [path.join(work, "tools", "release.mjs"), "0.9.0", "--tag-only"], { cwd: work, encoding: "utf8", env: NOBODY });
  ok("going backwards is refused", back.status, 1);
} finally {
  rmSync(root, { recursive: true, force: true });
}

console.log(bad ? `\n${bad} failed.\n` : "\nAll good.\n");
process.exit(bad ? 1 : 0);
