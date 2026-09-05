// Build the Harmony scanner and put it where the app will find it.
//
// Two callers. `npm run release` runs this once on the machine (or the CI runner) that makes the
// installer, and the binary travels inside the installer; players never run it. `npm run tauri
// dev` runs it too, with --optional, so a developer who has the .NET SDK gets a working Patches
// view in the debug build without a second command, and one who does not loses nothing but that
// view. `npm run tauri build` does not run it and needs no .NET.
//
//   node scripts/sidecar.mjs [--target <rust target triple>] [--no-aot] [--skip-build] [--force] [--optional]
//
// --skip-build takes whatever dotnet last published for that target and only puts it in place,
// for when the binary was built elsewhere (a cross-build, another CI job) or when you are
// checking this script rather than the scanner.
//
// --force rebuilds even when the placed binary is newer than every source file. Without it a
// build whose inputs have not changed is skipped, because an AOT compile costs a couple of
// minutes and a dev restart should not.
//
// --optional is for the dev hook, which must never stop `tauri dev`: anything that would
// otherwise fail — no .NET, no Rust, a publish that does not compile, no network for NuGet —
// is reported in one sentence and the script exits 0. The Patches view then says the scanner
// is missing and how to get it, which is the same place a developer would look anyway.
//
// Where the binary goes. Tauri wants `src-tauri/binaries/harmony-scan-<target triple>[.exe]`
// and strips the triple when it bundles, so a release finds `harmony-scan(.exe)` next to its own
// executable. A dev build's executable lives in the cargo target directory's `debug/`, which
// Tauri never populates with sidecars, so a plain-named copy is put there (and in `release/`)
// as well. Overwriting those copies is also what keeps a rebuilt scanner from going stale:
// Tauri does not notice when a sidecar's source changes.

import { execFileSync, spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const project = join(root, "tools", "harmony-scan");
const outDir = join(root, "src-tauri", "binaries");

const args = process.argv.slice(2);
const flag = (name) => args.includes(name);
const optional = flag("--optional");
const skipBuild = flag("--skip-build");
const force = flag("--force");
const wantAot = !flag("--no-aot");

/** .NET names platforms differently from Rust; this is the whole mapping we need. */
const RIDS = {
  "x86_64-pc-windows-msvc": "win-x64",
  "aarch64-pc-windows-msvc": "win-arm64",
  "x86_64-apple-darwin": "osx-x64",
  "aarch64-apple-darwin": "osx-arm64",
  "x86_64-unknown-linux-gnu": "linux-x64",
  "aarch64-unknown-linux-gnu": "linux-arm64"
};

/** A path as a person would type it from the repo root, for messages; full when it is elsewhere. */
function short(p) {
  const rel = relative(root, p);
  return rel && !rel.startsWith("..") ? rel : p;
}

/**
 * Stop. Under --optional this is one sentence and exit 0, because the caller is a build hook
 * that must not take `tauri dev` down with it; otherwise the message, the hint, and exit 1.
 */
function die(message, hint) {
  if (optional) {
    console.warn(`sidecar: ${message}. Going on without the Harmony scanner; the Patches view will say it is missing.`);
    process.exit(0);
  }
  console.error(`\nsidecar: ${message}`);
  if (hint) console.error(`\n${hint}\n`);
  process.exit(1);
}

/** The Rust target triple this machine builds for, unless one was named. */
function hostTriple() {
  const named = args.indexOf("--target");
  if (named >= 0 && args[named + 1]) return args[named + 1];
  try {
    const out = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
    const line = out.split("\n").find((l) => l.startsWith("host:"));
    if (line) return line.slice(5).trim();
  } catch {
    /* fall through */
  }
  die("Rust is not installed, so this machine's target triple is unknown", "Install Rust (https://rustup.rs), or pass one: node scripts/sidecar.mjs --target x86_64-pc-windows-msvc");
}

function haveDotnet() {
  const r = spawnSync("dotnet", ["--version"], { encoding: "utf8" });
  return r.status === 0 ? r.stdout.trim() : null;
}

/**
 * Where cargo puts build output. It is not always `src-tauri/target`: this workspace's is at
 * the repo root, and CARGO_TARGET_DIR moves it anywhere. `cargo metadata` knows, and answers
 * from the same environment `tauri dev` will build in.
 */
function cargoTargetDir() {
  try {
    const out = execFileSync("cargo", ["metadata", "--format-version", "1", "--no-deps"], { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] });
    const dir = JSON.parse(out).target_directory;
    if (typeof dir === "string" && dir) return dir;
  } catch {
    /* fall through */
  }
  return process.env.CARGO_TARGET_DIR || join(root, "target");
}

/** The newest modification time among the scanner's sources, so an unchanged tree is not rebuilt. */
function newestSource(dir) {
  let newest = 0;
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    // bin/ and obj/ are dotnet's own output; the fixture under testdata/ is source like the rest.
    if (entry.isDirectory()) {
      if (entry.name === "bin" || entry.name === "obj") continue;
      newest = Math.max(newest, newestSource(join(dir, entry.name)));
    } else if (/\.(cs|csproj)$/i.test(entry.name)) {
      newest = Math.max(newest, statSync(join(dir, entry.name)).mtimeMs);
    }
  }
  return newest;
}

/** Run dotnet publish; returns the publish directory, or null when it failed. */
function publish(rid, aot) {
  const pubArgs = ["publish", project, "-c", "Release", "-r", rid, "--nologo", "-v", "minimal"];
  if (aot) pubArgs.push("-p:Aot=true");
  else pubArgs.push("--self-contained", "true");
  console.log(`sidecar: dotnet ${pubArgs.join(" ")}`);
  const r = spawnSync("dotnet", pubArgs, { stdio: "inherit" });
  if (r.status !== 0) return null;
  return join(project, "bin", "Release", "net8.0", rid, "publish");
}

/** Copy unless the destination already is this file (same size, not older); says what it did. */
function place(from, to, what) {
  mkdirSync(dirname(to), { recursive: true });
  const src = statSync(from);
  const have = existsSync(to) ? statSync(to) : null;
  if (have && have.size === src.size && have.mtimeMs >= src.mtimeMs) {
    console.log(`sidecar: ${short(to)} already has this build`);
    return;
  }
  copyFileSync(from, to);
  console.log(`sidecar: ${what}: ${short(to)}`);
}

const triple = hostTriple();
const rid = RIDS[triple];
if (!rid) die(`no .NET runtime identifier is known for ${triple}`, `Add it to RIDS in scripts/sidecar.mjs if .NET supports that platform.`);

const version = haveDotnet();
if (!version) {
  die(
    "the .NET SDK is not installed, so the Harmony scanner cannot be built",
    "Install .NET 8 (https://dotnet.microsoft.com/download/dotnet/8.0) and run this again.\n" +
      "Or build without it: `npm run tauri build` makes an installer with no scanner, and the\n" +
      "Patches view explains that it is missing instead of failing."
  );
}

const windows = triple.includes("windows");
const exe = windows ? "harmony-scan.exe" : "harmony-scan";
const placed = join(outDir, windows ? `harmony-scan-${triple}.exe` : `harmony-scan-${triple}`);
const publishDir = join(project, "bin", "Release", "net8.0", rid, "publish");

// Decide where the binary comes from: the last publish, the one already in place, or a build.
let built;
if (skipBuild) {
  if (!existsSync(publishDir)) die(`nothing published for ${rid} at ${short(publishDir)}`, "Drop the built binary there, or run without --skip-build.");
  built = join(publishDir, exe);
  console.log(`sidecar: --skip-build, taking ${short(built)}`);
} else if (!force && existsSync(placed) && statSync(placed).mtimeMs > newestSource(project)) {
  built = placed;
  console.log(`sidecar: ${short(placed)} is up to date (newer than every file under ${short(project)}); pass --force to rebuild`);
} else {
  console.log(`sidecar: .NET ${version}, building for ${rid} (${triple})`);
  let dir = wantAot ? publish(rid, true) : null;
  if (wantAot && !dir) {
    // AOT wants a native toolchain (MSVC on Windows, clang and zlib headers on Linux). A
    // self-contained build is bigger and slower to start but behaves identically, and a release
    // that ships a working scanner beats one that ships none.
    console.warn("\nsidecar: the AOT build failed; falling back to a self-contained build.\n");
    dir = publish(rid, false);
  }
  if (!dir) die("the scanner did not build", "The output above says why. On Linux, AOT needs clang and zlib development headers; try --no-aot.");
  built = join(dir, exe);
}
if (!existsSync(built)) die(`there is no ${exe} in ${short(dirname(built))}`);

// Where Tauri's bundler looks, then plain-named copies beside the debug and release executables
// so a development build finds the scanner the same way an installed one does. Only folders
// cargo has already made get one: creating `release/` on a machine that never builds one would
// leave a stray file for nothing.
if (built !== placed) place(built, placed, "for the installer");
const targetDir = cargoTargetDir();
for (const profile of ["debug", "release"]) {
  const dir = join(targetDir, profile);
  if (existsSync(dir)) place(built, join(dir, exe), `beside the ${profile} build`);
}

const kb = Math.round(statSync(placed).size / 1024);
console.log(`sidecar: ${short(placed)} (${kb.toLocaleString()} KB)`);
const others = readdirSync(outDir).filter((f) => f.startsWith("harmony-scan-") && !placed.endsWith(f));
if (others.length) console.log(`sidecar: also present for ${others.map((f) => f.replace(/^harmony-scan-|\.exe$/g, "")).join(", ")}`);
