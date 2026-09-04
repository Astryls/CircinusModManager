# Circinus Mod Manager

A mod manager for RimWorld with **HALO** — the Harmonized Automated Load Order — groups and filters in the style of Nexus Mod Manager, validation against the community rule databases, and per-mod **Circinus weight** (frame-time share measured by the [Circinus profiler](https://circinus.sh)).

Built with Tauri 2 (Rust) and Svelte 5. MIT licensed, clean-room: it shares file formats and database schemas with RimSort/RimPy so mod lists and rules interoperate, but contains none of their code.

## Layout

```
crates/circinus-core/   Rust library: mod discovery and About.xml parsing, ModsConfig.xml,
                        list importers, rule databases, HALO ordering + validation,
                        texture-collision analysis, Circinus weight client. Fully unit-tested.
src-tauri/              Tauri shell: app state, commands, bundling config, icons.
src/                    Svelte 5 UI (the "Console" design). Runs in a plain browser with
                        example data when Tauri is absent (`npm run dev`).
```

## Develop

Prerequisites: [Rust](https://rustup.rs) (stable), Node 20+, and Tauri's platform prerequisites
(Windows: WebView2 is preinstalled on Windows 10/11 plus the Visual Studio C++ build tools;
macOS: Xcode command line tools).

```sh
npm install
npm run tauri dev        # full app: Rust backend + hot-reloading UI
npm run dev              # UI only, in the browser, with example data
cargo test -p circinus-core
npm run check            # svelte-check
```

`npm run tauri dev` builds a debug binary that loads the UI from Vite's dev server, so it only
runs while that command is running — it is not a standalone exe. For one, build a release:

## Release builds

```sh
npm run tauri build                       # Windows: .exe (NSIS) and .msi under src-tauri/target/release/bundle
npm run tauri build -- --bundles dmg      # macOS: .dmg (run on a Mac; sign + notarize with an Apple Developer ID)
```

Single self-contained binaries; SteamCMD and todds are downloaded on first use rather than bundled (Valve's and MPL terms).

## What works today (milestone 1)

- Finds RimWorld through Steam (`steamlocate`) or a chosen folder; reads `Version.txt`, `Data/`, `Mods/`, and the Workshop content folder in parallel with an mtime-keyed SQLite cache.
- Parses `About.xml` including `*ByVersion` blocks (which replace, not extend, the base lists — RimWorld semantics), `forceLoadAfter/Before`, dependencies with alternatives; Fluffy's `About/Manifest.xml`; `LoadFolders.xml`; `PublishedFileId.txt`; counts assemblies, patches, defs and textures.
- Reads and writes `ModsConfig.xml` (keeps a `.bak`, handles the `_steam` duplicate convention and `knownExpansions`).
- Imports lists from ModsConfig.xml, `.rws` saves (plain, gzip or zstd), `.rml`, RimSort/RimPy JSON and pasted text.
- Loads the community rules and Steam Workshop databases, Use This Instead and No Version Warning, with ETag-cached updates; your own rules live in `dbs/userRules.json` in the shared schema, with an `ignore` block that switches off lower-precedence rules.
- HALO: seven phases (Core, Prepatch, Frameworks, Content, Patches, Texture overrides, Optimization) classified from what each mod contains; rules are hard edges in a DAG, contradictions resolved by precedence (yours > community > Manifest > About), real cycles explained and cut, priority topological sort that keeps your arrangement where rules allow, pins.
- Validation: missing dependencies, incompatibilities, order violations, version mismatches, duplicates, misplaced optimization mods, and texture collisions (which mod wins each file).
- Circinus weight column and tile, fed by the public circinus.sh API and your local `Circinus/Runs`.

## Milestone 2 (downloads)

- Virtualized mod list: only visible rows exist in the DOM, so 1,000+ active mods render instantly.
- SteamCMD is installed on first use into Circinus's data folder (never bundled). Downloads run as batched runscripts with an anonymous login and land in `Mods/<workshop id>` with a `PublishedFileId.txt`.
- Smart throttle: batches start at 25 and halve when Steam refuses (login refused, half the batch failing, or a stall), with an exponential cooldown (30 s → 10 min) that decays after clean batches; stalls are killed after 150 s of silence; each item gets four tries, with `validate` after the first failure; the queue is persisted and resumes after a restart. Before every batch the items are purged from SteamCMD's workshop ACF and depot cache so re-downloads really download.
- Steam Web API (no key): names for queued items, collection expansion (one level of sub-collections), and Workshop update checks against on-disk timestamps.
- Import dialog accepts Steam collection links and Rentry links; missing items can be queued in one click, as can everything in ModsConfig.xml that is not installed.
- `tools/loadtest` renders the UI at ~2,000 mods in headless Chromium; `cargo run -p circinus --example dump_snapshot` produces a real backend snapshot for it.

## Next milestones

3. Def/patch flattening (who wins each XML value) and a .NET NativeAOT sidecar that lists Harmony patch targets per assembly.
4. Instances/profiles, todds integration, auto-update, Steam-client subscribe/unsubscribe.

## Data locations

- Settings, cache and databases: `%LOCALAPPDATA%\Circinus` (Windows) or `~/Library/Application Support/Circinus` (macOS).
- Your rules: `<data>/dbs/userRules.json`.
