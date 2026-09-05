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
feed. The app side of auto-update is built (`src-tauri/src/updater.rs`, `tauri-plugin-updater`
unchanged, a check-on-start setting, *Check now* in Settings, an install-and-restart banner); the
server side is not, and **`docs/update-feed.md` is the contract it implements** — the app defined
it, the site follows. The updater's private key is `/home/claude/circinus-updater.key`, outside
the repo, meant for the `TAURI_SIGNING_PRIVATE_KEY` secret; only the public key is committed.

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
- `clr`: reads .NET assemblies (PE, ECMA-335 metadata, custom attribute blobs, IL) without a
  .NET runtime and without ever loading a mod's code. `harmony` caches those readings and groups
  them into what one mod patches and who else patches the same method.

## Packaging

`npm run release` makes the installer players download: `tauri build`, nothing else. Rust and
Node are the whole toolchain. `.github/workflows/release.yml` does it per platform on a tag.

There used to be a .NET sidecar for reading mod assemblies. It is gone: `circinus-core::clr`
reads ECMA-335 in Rust, so the Patches view works in every build with nothing to ship or place.
`tools/harmony-scan` survives only as the test oracle — the C# implementation the Rust reader is
checked against (`crates/circinus-core/tests/clr_reader.rs`). Nothing in the app or the build
runs it. Do not reintroduce a build-time dependency on the .NET SDK.

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
