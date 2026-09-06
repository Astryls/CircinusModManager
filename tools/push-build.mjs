#!/usr/bin/env node
// Push a built release to circinus.sh: create it, upload each file, publish, then read back
// what the updater will actually be served. The site's interface is four calls and is described
// in docs/update-feed.md; this is that interface and nothing else.
//
//   CIRCINUS_BUILD_KEY=cmk_... node tools/push-build.mjs --version 0.2.0 --dir artifacts
//
//   --version   the version to publish. Defaults to the one in src-tauri/tauri.conf.json, which
//               is the only place it is written down.
//   --dir       where the built files are. Every bundle directory under it is searched, so both
//               a local bundle directory (asked of cargo) and a directory of downloaded CI
//               artifacts work.
//   --notes     release notes. Defaults to the annotated tag's message, then to one sentence.
//   --force     publish without all three platforms. For a single-platform fix, not for a
//               release that is simply missing a build.
//   --dry-run   say what would be sent and send nothing.
//
// Exits non-zero on the first thing that is wrong, having changed nothing that matters: a
// release is invisible until it is published, which is the last call.
import { createHash } from "node:crypto";
import { readFile, readdir, stat } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import path from "node:path";

const BASE = process.env.CIRCINUS_BASE ?? "https://circinus.sh";
const KEY = process.env.CIRCINUS_BUILD_KEY;
const MAX_BYTES = 600 * 1024 * 1024;

const args = process.argv.slice(2);
const flag = (name) => {
  const i = args.indexOf(`--${name}`);
  return i >= 0 && args[i + 1] && !args[i + 1].startsWith("--") ? args[i + 1] : null;
};
const has = (name) => args.includes(`--${name}`);
const dry = has("dry-run");

const die = (msg) => {
  console.error(`\n${msg}`);
  process.exit(1);
};

/** The four slots the site accepts, and where Tauri writes each one. */
const SLOTS = [
  { platform: "windows", kind: "installer", ext: "-setup.exe", needsSig: true, what: "the Windows installer" },
  { platform: "mac", kind: "installer", ext: ".dmg", needsSig: false, what: "the Mac disk image" },
  { platform: "mac", kind: "update", ext: ".app.tar.gz", needsSig: true, what: "the Mac update bundle" },
  { platform: "linux", kind: "installer", ext: ".AppImage", needsSig: true, what: "the Linux AppImage" }
];

/** Every file under `root`, so a bundle directory and a folder of CI artifacts both work. */
async function walk(root, depth = 0) {
  if (depth > 6) return [];
  let entries;
  try {
    entries = await readdir(root, { withFileTypes: true });
  } catch {
    return [];
  }
  const out = [];
  for (const e of entries) {
    const p = path.join(root, e.name);
    if (e.isDirectory()) out.push(...(await walk(p, depth + 1)));
    else out.push(p);
  }
  return out;
}

/**
 * The one file for a slot, matched on how its name ends. That alone separates all four: the
 * directories Tauri writes them into do not survive being uploaded and downloaded as CI
 * artifacts, and `-setup.exe` is what tells the NSIS installer from the .msi beside it.
 */
function findFor(files, slot) {
  const hits = files.filter((f) => f.endsWith(slot.ext) && !f.endsWith(".sig"));
  if (hits.length > 1) {
    // Two Mac tarballs with the same name means an Intel build is in the folder; the site
    // refuses Intel, and guessing which is which would put a binary behind the wrong button.
    die(`More than one file could be ${slot.what}:\n  ${hits.join("\n  ")}\nLeave one of them in ${flag("dir") ?? "the folder"} and run again.`);
  }
  return hits[0] ?? null;
}

/** Where `cargo` puts its output, asked of cargo. */
async function bundleDir() {
  try {
    const meta = JSON.parse(execFileSync("cargo", ["metadata", "--format-version", "1", "--no-deps"], { encoding: "utf8", maxBuffer: 64 * 1024 * 1024 }));
    if (meta.target_directory) return path.join(meta.target_directory, "release", "bundle");
  } catch {
    /* no cargo on this machine, or not a cargo project: fall through */
  }
  return "src-tauri/target/release/bundle";
}

/** The site rejects em dashes, and notes go on a page people read: keep them plain. */
function plain(text) {
  return text
    .replace(/[—–]/g, "-")
    .replace(/[‘’]/g, "'")
    .replace(/[“”]/g, '"')
    .replace(/\r\n/g, "\n")
    .trim();
}

function tagNotes(version) {
  try {
    const out = execFileSync("git", ["tag", "-l", "--format=%(contents)", `v${version}`], { encoding: "utf8" }).trim();
    return out || null;
  } catch {
    return null;
  }
}

async function call(method, route, { body, headers = {}, raw = false } = {}) {
  const url = `${BASE}${route}`;
  if (dry) {
    console.log(`  would ${method} ${url}${body && !raw ? ` ${body}` : ""}`);
    return { ok: true, dry: true };
  }
  const res = await fetch(url, {
    method,
    headers: { Authorization: `Bearer ${KEY}`, ...headers },
    body
  });
  const text = await res.text();
  let json = null;
  try {
    json = text ? JSON.parse(text) : null;
  } catch {
    /* the site answers JSON; anything else is reported as it arrived */
  }
  if (!res.ok) {
    const said = json?.error ?? text.slice(0, 300) ?? "";
    die(`${method} ${route} answered ${res.status}.\n${said}`);
  }
  return json ?? {};
}

const main = async () => {
  if (!KEY && !dry) die("No build key. Set CIRCINUS_BUILD_KEY to the cmk_... key from circinus.sh/admin.");

  const conf = JSON.parse(await readFile(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"));
  const version = (flag("version") ?? conf.version).replace(/^v/, "");
  // The version is written down once, in tauri.conf.json, and the build stamps it into every
  // file name. A tag that disagrees with it would publish one number over another's bytes.
  if (version !== conf.version) {
    die(`Asked to publish ${version}, but this checkout builds ${conf.version} (src-tauri/tauri.conf.json).\nChange the version there and rebuild, or push the tag that matches.`);
  }
  // Where cargo writes depends on whether src-tauri is its own workspace or a member of the one
  // at the repository root, and it is a member: that puts the bundles at the root, not under
  // src-tauri. Ask cargo instead of choosing, and keep the old place as a fallback so a checkout
  // that is arranged differently still works.
  const dir = flag("dir") ?? (await bundleDir());
  const files = await walk(dir);
  if (!files.length) die(`Nothing to upload: no files under ${dir}.`);

  // ---- gather, and refuse before sending anything ----
  const uploads = [];
  const missing = [];
  for (const slot of SLOTS) {
    const file = findFor(files, slot);
    if (!file) {
      missing.push(slot);
      continue;
    }
    const { size } = await stat(file);
    if (size > MAX_BYTES) die(`${path.basename(file)} is ${(size / 1048576).toFixed(0)} MB; the site takes at most 600 MB per file.`);
    if (size === 0) die(`${path.basename(file)} is empty.`);
    let signature = null;
    try {
      signature = (await readFile(`${file}.sig`, "utf8")).trim();
    } catch {
      /* handled next */
    }
    if (slot.needsSig && !signature) {
      die(`${path.basename(file)} has no .sig beside it.\nThe app installs an update only if it is signed, and a bundle uploaded without one is stored, listed, and never offered — with nothing on the page saying why. Build with TAURI_SIGNING_PRIVATE_KEY set.`);
    }
    uploads.push({ ...slot, file, size, signature, sha256: createHash("sha256").update(await readFile(file)).digest("hex") });
  }
  if (!uploads.length) die(`Found files under ${dir} but none of them is a release bundle.`);

  const notes = plain(flag("notes") ?? tagNotes(version) ?? `Circinus Mod Manager ${version}.`);
  console.log(`Circinus Mod Manager ${version} -> ${BASE}${dry ? "  (dry run)" : ""}`);
  for (const u of uploads) console.log(`  ${u.platform}/${u.kind}  ${path.basename(u.file)}  ${(u.size / 1048576).toFixed(1)} MB  ${u.signature ? "signed" : "unsigned"}`);
  for (const m of missing) console.log(`  ${m.platform}/${m.kind}  not built: ${m.what} is missing`);

  // ---- 1. create, still retracted ----
  const made = await call("POST", `/api/v1/ci/releases/${version}`, {
    body: JSON.stringify({ notes }),
    headers: { "Content-Type": "application/json" }
  });
  if (!dry) console.log(`\n${made.created ? "Created" : "Updated"} ${version}, not yet visible.`);

  // ---- 2. one file per request ----
  for (const u of uploads) {
    const headers = {
      "Content-Type": "application/octet-stream",
      "X-Circinus-Filename": path.basename(u.file),
      "X-Circinus-Sha256": u.sha256
    };
    if (u.signature) headers["X-Circinus-Signature"] = u.signature;
    const res = await call("PUT", `/api/v1/ci/releases/${version}/${u.platform}/${u.kind}`, {
      body: dry ? null : await readFile(u.file),
      headers,
      raw: true
    });
    if (!dry) console.log(`  sent ${u.platform}/${u.kind}`);
    // Replacing a file people already have leaves two machines on one version number running
    // different builds. The site says so; do not let it scroll past.
    if (res.warning) console.log(`  NOTE  ${res.warning}`);
  }

  // ---- 3. publish: the moment anything becomes visible ----
  const force = has("force") || missing.length > 0;
  if (missing.length && !has("force")) {
    console.log(`\nPublishing without ${missing.map((m) => m.what).join(" and ")}.`);
  }
  const pub = await call("POST", `/api/v1/ci/releases/${version}/publish`, {
    body: JSON.stringify(force ? { force: true } : {}),
    headers: { "Content-Type": "application/json" }
  });
  if (dry) return console.log("\nDry run: nothing was sent.");

  console.log(`\nPublished ${pub.version}${pub.preRelease ? " as a pre-release" : ""}. latest is ${pub.latest}.`);
  const offers = pub.updaterOffers ?? [];
  if (!offers.length) die("Published, but the updater is offered nothing: no file in the release is signed. Every installed copy will stay where it is.");
  console.log(`The updater will offer: ${offers.join(", ")}`);

  // ---- 4. read back what an installed copy will actually be served ----
  const res = await fetch(`${BASE}/api/v1/releases/latest.json`);
  if (!res.ok) die(`Published, but /api/v1/releases/latest.json answered ${res.status}.`);
  const latest = await res.json();
  if (latest.version?.replace(/^v/, "") !== version) {
    die(`Published, but latest.json still names ${latest.version}. An installed copy asking for an update would not be offered this one.`);
  }
  const unsigned = Object.entries(latest.platforms ?? {}).filter(([, p]) => !p?.signature).map(([k]) => k);
  if (unsigned.length) die(`Published, but latest.json carries no signature for ${unsigned.join(", ")}. Those copies will refuse the update.`);
  console.log(`latest.json names ${latest.version} and is signed for ${Object.keys(latest.platforms ?? {}).join(", ")}.`);
};

main().catch((e) => die(e?.stack ?? String(e)));
