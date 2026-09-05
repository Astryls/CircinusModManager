// Build the Harmony scanner and put it where Tauri expects a sidecar.
//
// Players never run this: it runs once per release, on the machine (or the CI runner) that
// makes the installer, and the binary it produces travels inside the installer. A developer
// only needs it if they are making a release build; `npm run tauri dev` and `npm run tauri
// build` work without .NET and without this script, and the Patches view says so when the
// scanner is absent.
//
//   node scripts/sidecar.mjs [--target <rust target triple>] [--no-aot] [--skip-build]
//
// --skip-build takes whatever dotnet last published for that target and only puts it in place,
// for when the binary was built elsewhere (a cross-build, another CI job) or when you are
// checking this script rather than the scanner.
//
// Tauri wants `src-tauri/binaries/harmony-scan-<target triple>[.exe]`; it strips the triple
// when it bundles, so the app finds `harmony-scan(.exe)` next to its own executable at runtime.

import { execFileSync, spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readdirSync, rmSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const project = join(root, "tools", "harmony-scan");
const outDir = join(root, "src-tauri", "binaries");

/** .NET names platforms differently from Rust; this is the whole mapping we need. */
const RIDS = {
  "x86_64-pc-windows-msvc": "win-x64",
  "aarch64-pc-windows-msvc": "win-arm64",
  "x86_64-apple-darwin": "osx-x64",
  "aarch64-apple-darwin": "osx-arm64",
  "x86_64-unknown-linux-gnu": "linux-x64",
  "aarch64-unknown-linux-gnu": "linux-arm64"
};

function die(message, hint) {
  console.error(`\nsidecar: ${message}`);
  if (hint) console.error(`\n${hint}\n`);
  process.exit(1);
}

/** The Rust target triple this machine builds for, unless one was named. */
function hostTriple() {
  const named = process.argv.indexOf("--target");
  if (named >= 0 && process.argv[named + 1]) return process.argv[named + 1];
  try {
    const out = execFileSync("rustc", ["-vV"], { encoding: "utf8" });
    const line = out.split("\n").find((l) => l.startsWith("host:"));
    if (line) return line.slice(5).trim();
  } catch {
    /* fall through */
  }
  die("could not work out this machine's Rust target triple", "Install Rust (https://rustup.rs), or pass one: node scripts/sidecar.mjs --target x86_64-pc-windows-msvc");
}

function haveDotnet() {
  const r = spawnSync("dotnet", ["--version"], { encoding: "utf8" });
  return r.status === 0 ? r.stdout.trim() : null;
}

/** Run dotnet publish; returns the publish directory, or null when it failed. */
function publish(rid, aot) {
  const args = ["publish", project, "-c", "Release", "-r", rid, "--nologo", "-v", "minimal"];
  if (aot) args.push("-p:Aot=true");
  else args.push("--self-contained", "true");
  console.log(`sidecar: dotnet ${args.join(" ")}`);
  const r = spawnSync("dotnet", args, { stdio: "inherit" });
  if (r.status !== 0) return null;
  return join(project, "bin", "Release", "net8.0", rid, "publish");
}

const triple = hostTriple();
const rid = RIDS[triple];
if (!rid) die(`no .NET runtime identifier is known for ${triple}`, `Add it to RIDS in ${"scripts/sidecar.mjs"} if .NET supports that platform.`);

const version = haveDotnet();
if (!version) {
  die(
    "the .NET SDK is not installed, so the Harmony scanner cannot be built",
    "Install .NET 8 (https://dotnet.microsoft.com/download/dotnet/8.0) and run this again.\n" +
      "Or build without it: `npm run tauri build` makes an installer with no scanner, and the\n" +
      "Patches view explains that it is missing instead of failing."
  );
}
console.log(`sidecar: .NET ${version}, building for ${rid} (${triple})`);

const skipBuild = process.argv.includes("--skip-build");
const publishDir = join(project, "bin", "Release", "net8.0", rid, "publish");
const wantAot = !process.argv.includes("--no-aot");
let dir = skipBuild ? publishDir : wantAot ? publish(rid, true) : null;
if (skipBuild && !existsSync(dir)) die(`nothing published for ${rid} at ${dir}`, "Drop the built binary there, or run without --skip-build.");
if (!skipBuild && wantAot && !dir) {
  // AOT wants a native toolchain (MSVC on Windows, clang and zlib headers on Linux). A
  // self-contained build is bigger and slower to start but behaves identically, and a release
  // that ships a working scanner beats one that ships none.
  console.warn("\nsidecar: the AOT build failed; falling back to a self-contained build.\n");
  dir = publish(rid, false);
}
if (!dir) die("the scanner did not build", "The output above says why. On Linux, AOT needs clang and zlib development headers; try --no-aot.");

const exe = triple.includes("windows") ? "harmony-scan.exe" : "harmony-scan";
const built = join(dir, exe);
if (!existsSync(built)) die(`the build produced no ${exe} in ${dir}`);

mkdirSync(outDir, { recursive: true });
const target = join(outDir, triple.includes("windows") ? `harmony-scan-${triple}.exe` : `harmony-scan-${triple}`);
copyFileSync(built, target);

// Tauri copies the sidecar into target/{debug,release} with the triple stripped, and does not
// notice when the source changes — a known trap that quietly ships a stale scanner. Clear the
// copies so the next build takes this one.
for (const profile of ["debug", "release"]) {
  const stale = join(root, "src-tauri", "target", profile, exe);
  if (existsSync(stale)) {
    rmSync(stale, { force: true });
    console.log(`sidecar: removed the stale copy in target/${profile}`);
  }
}

const kb = Math.round(statSync(target).size / 1024);
console.log(`sidecar: ${target.replace(root + "/", "").replace(root + "\\", "")} (${kb.toLocaleString()} KB)`);
const others = readdirSync(outDir).filter((f) => f.startsWith("harmony-scan-") && !target.endsWith(f));
if (others.length) console.log(`sidecar: also present for ${others.map((f) => f.replace(/^harmony-scan-|\.exe$/g, "")).join(", ")}`);
