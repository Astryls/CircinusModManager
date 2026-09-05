# Circinus Mod Manager — working notes

Standing decisions and context that outlive any one session. Keep it short; if something here
stops being true, change it rather than adding a second answer.

## What this is

A RimWorld mod manager: Tauri 2 shell, Svelte 5 (runes) front end, Rust `circinus-core` for
everything that knows about mods. Original clean-room implementation, MIT. File formats, XML
tags, JSON schemas and Steam endpoints are shared with RimSort and RimPy on purpose, so lists and
rule databases interoperate; none of the code is theirs.

## Commits

Authored `Circinus <noreply@circinus.sh>` deliberately — no personal identifiers in the public
history. GitHub shows these as "Unverified"; that is the accepted cost, and a hook that asks for
them to be re-authored to `noreply@anthropic.com` has been declined twice. Do not re-author.

Commit messages end with:

```
Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
Claude-Session: <the session URL>
```

The session runs in a cloud container; the repo also lives at `D:\CircinusModManager` on the
user's PC, which is where pushes happen. Deliver changed files there (SendUserFile →
`device_commit_files`); do not expect to push from here.

## circinus.sh

`circinus.sh` is the project's site; it already serves the performance weights the Cost column
uses. A separate session (`cse_01471kgqd8sGWxexmsnbouqE`) is building
**https://circinus.sh/modmanager** — a place to upload builds that also serves the auto-update
feed. **Auto-update in the app is deliberately not built yet**: it waits for that endpoint to
exist, and the user will say when. When it lands, the app side is `tauri-plugin-updater` pointed
at whatever `latest.json` equivalent the site serves, plus a check-on-start setting, a *Check now*
button in Settings, and an install-and-restart banner.

## Shape of the thing

- `circinus-core`: `scan` (two-phase: About.xml quickly, folder contents in the background,
  sqlite cache keyed by a stamp that includes `PARSER_VERSION`), `order` (HALO), `rules`,
  `modsconfig`, `dds`, `textures`, `loadcost`, `playerlog`, `defs`, `harmony`, `steam`.
- HALO — Harmonized Automated Load Order. Eight phases in load order: Prepatch (shown as
  "Preloads"), Core ("Game and DLC"), Framework ("Libraries"), Content, Patch ("Patches"),
  Texture ("Texture packs"), Late ("Late loaders"), Optimization ("Performance"). Rules are hard
  DAG edges, phases are soft, the official-content invariant holds (anything with Defs loads
  after Core and the DLC), and a real cycle is explained and cut rather than fatal. The HALO page
  lets a user switch built-in rules off or send them to another phase.
- `defs`: builds the document the game builds — every active mod's Defs merged in load order,
  every PatchOperation applied in load order, then Name/ParentName inheritance — with the origin
  of every node recorded, so "who wins this value" has an answer. `defs::xpath` is XPath 1.0 as
  .NET's `SelectNodes` means it, because that is what RimWorld hands patches to.
- `harmony`: runs `tools/harmony-scan` (a .NET sidecar that reads assembly metadata and IL and
  never loads a mod assembly) and groups the result into what one mod patches and who else
  patches the same method.

## Packaging

`npm run release` is what makes an installer for players: it builds the Harmony scanner for the
host platform (`scripts/sidecar.mjs`, needs the .NET 8 SDK) into
`src-tauri/binaries/harmony-scan-<triple>`, then runs `tauri build` with
`src-tauri/tauri.release.conf.json` merged in, which carries it as an `externalBin`. Tauri
strips the triple at bundle time, so the app finds `harmony-scan(.exe)` beside its own
executable. Players never touch .NET.

Plain `npm run tauri build` stays sidecar-free on purpose: naming the scanner in the main
config makes its absence a hard build failure, and nobody should need the .NET SDK to compile
Circinus. `npm run tauri dev` runs `scripts/sidecar.mjs --optional` first, which builds the
scanner when .NET is there (skipped when nothing under `tools/harmony-scan/` changed) and copies
it beside the debug executable in the cargo target directory; when .NET is missing or the
build fails it says so in one line and exits 0, so the dev build never depends on it.
`.github/workflows/release.yml` does the whole thing per platform on a tag and fails if the
scanner is missing from the output.

## House style

Comments explain *why*, in prose, and the code is written to be read. User-facing text is plain
English with no jargon and no exclamation marks: say what happened and what it means. Errors name
the thing that went wrong and what it costs the user. No `unsafe`, no `unwrap` in library paths.
Rust lines are wide (see `rustfmt.toml` if present); run `cargo test`, `cargo clippy` and
`npx svelte-check` before calling anything done.

Verify UI work in a real browser rather than by eye: `npm run build`, `npx vite preview`, then a
script under `tools/loadtest/` driving Playwright against `127.0.0.1:4173` with the mock data in
`src/lib/mock.ts`. Assert geometry (column alignment, overlaps, clipping) rather than taking a
screenshot and hoping.
