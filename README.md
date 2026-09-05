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

Single self-contained binaries; SteamCMD is downloaded on first use rather than bundled (Valve's terms).

## What works today (milestone 1)

- Finds RimWorld through Steam (`steamlocate`) or a chosen folder; reads `Version.txt`, `Data/`, `Mods/`, and the Workshop content folder in parallel with an mtime-keyed SQLite cache.
- Linked mod folders. A `Mods/` entry that is a symlink or (on Windows) a junction, the way Modmixer, Circinus Dev Tools and hand-made `mklink` links deploy a mod, is read through to the folder it names, as are links inside a mod; RimWorld loads such mods and so must we. The link target is read directly (`circinus-core::fsx`) rather than left to the OS to follow, because opening a link and letting Windows resolve it fails in some setups. The mod keeps the Mods-folder path as its identity (the one RimWorld reports), the details panel says where the files are, and links to nowhere are listed under Settings rather than silently dropped.
- Parses `About.xml` including `*ByVersion` blocks (which replace, not extend, the base lists — RimWorld semantics), `forceLoadAfter/Before`, dependencies with alternatives; Fluffy's `About/Manifest.xml`; `LoadFolders.xml`; `PublishedFileId.txt`; counts assemblies, patches, defs and textures.
- Reads and writes `ModsConfig.xml` (keeps a `.bak`, handles the `_steam` duplicate convention and `knownExpansions`).
- Imports lists from ModsConfig.xml, `.rws` saves (plain, gzip or zstd), `.rml`, RimSort/RimPy JSON and pasted text.
- Loads the community rules and Steam Workshop databases, Use This Instead and No Version Warning, with ETag-cached updates; your own rules live in `dbs/userRules.json` in the shared schema, with an `ignore` block that switches off lower-precedence rules.
- HALO: eight phases (Before the game, Game and DLC, Libraries, Content, Patches, Texture packs, Late loaders, Performance) classified from what each mod contains; rules are hard edges in a DAG, contradictions resolved by precedence (yours > community > Manifest > About), real cycles explained and cut, priority topological sort that keeps your arrangement where rules allow, pins.
- Dependencies are rules: every `modDependencies` entry becomes a "load after" rule (shown as "needs" in the rules panel and checked like any other rule), and phases are lifted along the hard edges so a mod never sits in an earlier group than something it must load after. A `loadBottom` rule puts a content mod in Late loaders rather than among the performance mods, and its add-ons follow it there, so the performance mods still end the list; "works best at the end" is only raised for mods that no rule forces after a performance mod, and it can be closed.
- The official-content invariant: RimWorld's `XmlInheritance` resolves a def's `ParentName` only against mods loaded at or before its own, so a mod with Defs above Core loses `BuildingBase`, `MoteBase` and friends and the game fails in def generation, resets the list and (with a quick-start mod) crashes. HALO therefore puts Core and the DLCs in release order and everything that ships Defs below all of them, as hard edges that outrank every declared rule; only Defs-less mods that are known pre-patchers or declare they load before official content (Harmony, Prepatcher, Fishery, loading-screen mods) may sit above. A rule that would break this is set aside and reported rather than obeyed.
- Load order view: drag rows (or a multi-selection) to reorder, with edge scrolling; the Show menu narrows the list to mods with errors, warnings, conflicts, HALO notes, changes since the last launch, or moves HALO would make, and by source and game version.
- Validation: anything with Defs above Core or a DLC (an error — the game will reset the list), missing dependencies, incompatibilities, order violations, version mismatches, duplicates, misplaced optimization mods, and texture collisions (which mod wins each file).
- Circinus weight column and tile, fed by the public circinus.sh API and your local `Circinus/Runs`.

## Milestone 2 (downloads)

- Virtualized mod list: only visible rows exist in the DOM, so 1,000+ active mods render instantly.
- SteamCMD is installed on first use into Circinus's data folder (never bundled). Downloads run as batched runscripts with an anonymous login and land in `Mods/<workshop id>` with a `PublishedFileId.txt`.
- Smart throttle: batches start at 25 and halve when Steam refuses (login refused, half the batch failing, or a stall), with an exponential cooldown (30 s → 10 min) that decays after clean batches; stalls are killed after 150 s of silence; each item gets four tries, with `validate` after the first failure; the queue is persisted and resumes after a restart. Before every batch the items are purged from SteamCMD's workshop ACF and depot cache so re-downloads really download.
- Steam Web API (no key): names for queued items, collection expansion (one level of sub-collections), and Workshop update checks against on-disk timestamps.
- Import dialog accepts Steam collection links and Rentry links; missing items can be queued in one click, as can everything in ModsConfig.xml that is not installed.
- `tools/loadtest` renders the UI at ~2,000 mods in headless Chromium; `cargo run -p circinus --example dump_snapshot` produces a real backend snapshot for it.

## Milestone 3a (change detection, SteamCMD everywhere)

- After every scan Circinus stores a fingerprint of each installed mod (name, packageId, version, Workshop id, source, newest mtime, and Steam's `timeupdated` from `appworkshop_294100.acf`). On the next launch the fresh scan is diffed against it: mods that were added, removed or updated are listed with the reason (Workshop update, version change, files changed, renamed) and the date; edits to `ModsConfig.xml` made outside Circinus (RimWorld's own mod menu, another manager) are reported as well. The banner, the title bar and the row flags all point at the "What changed" list; **Got it** re-baselines.
- Steam can replace files deep inside a Workshop item without touching the folder's mtime, so `timeupdated` is part of the cache stamp and of `modified` for Workshop items. That also makes the Workshop update check exact.
- While Circinus is open a watcher polls four mtimes every 8 s (the ACF, ModsConfig.xml, Mods/, the Workshop folder); a change triggers a cached re-read (about 150 ms for 2,000 mods), a toast, and a desktop notification via `tauri-plugin-notification` — so an update Steam applies while the game is starting is not missed.
- SteamCMD is now visible from everywhere: a status chip in the title bar (not set up / ready / downloading / cooling down / failed), a **set up** hint in the sidebar, a Settings section with paths, **Test SteamCMD** (`+login anonymous +quit`, output in the log — on Windows this also proves the console-log tail works) and **Reinstall**, a first-run banner when the list has mods that are not installed, and **Re-download** on every Workshop mod in the Inspector and in the change list.

## Launching the game

Play starts RimWorld through Steam (`steam://rungameid/294100`) when the game folder is inside a Steam library, and otherwise runs the executable Circinus detects (`RimWorldWin64.exe`, `RimWorldMac.app`, `RimWorldLinux`). Settings → Launching RimWorld lets you force either method, pick the executable by hand (GOG, DRM-free, a copy outside Steam), add arguments such as `-popupwindow`, and choose whether unsaved changes to ModsConfig.xml are written first (default on). Non-standard folders — game, config, local mods, Workshop — are set in Settings → Where RimWorld lives.

## Milestone 3c (after a crash)

- **List history.** Every real list Circinus reads or writes is archived under `<data>/lists/` (`<unix>-saved|seen|before-reset.xml`, newest forty kept, deduplicated). When RimWorld's "Resetting mods config and trying again" leaves only official content behind, Circinus notices on the next read, archives the list the session started with, and offers *Restore and save*; Import → Previous lists browses the archive.
- **Game log analyzer** (Analyzer → Last game run). Reads `Player.log` (or `Player-prev.log`, or any file) and says what happened: loaded / load failed and reset / crashed; the exception that broke loading with its innermost mod frame and Harmony patch owners; the native crash with its first mod frame and whether it ran off the main thread during a quick-start; XML errors and missing parents grouped by mod; exception groups (Unity's `[Ref]`/"Duplicate stacktrace" folding respected); DDS files Unity refused; textures not found; duplicates; timings; environment. Each finding is tied to an installed mod by `[Source:]` name, Workshop id or folder in a path, Harmony id, or the assembly a frame's namespace lives in, and a mod sitting above Core is called out as the reason for missing Core parents. Parser in `circinus-core::playerlog` (`cargo run -p circinus-core --example playerlog -- Player.log`).
- **DDS audit** (Textures → DDS files the game will refuse). Reads the header of every `.dds` Circinus did not write and flags what Unity 2022 cannot create a texture from: block-compressed files with a side that is not a multiple of four, truncated files, unreadable headers. *Fix* rebuilds a file through the same encoder and validator — from the PNG beside it, or from the file's own BC1/BC3/BC7 top level — and keeps the original as `.dds.circinus-orig`; the manifest records the replacement, so a mod update hands the file back to its author and *Revert* restores the original instead of just deleting.

## Milestone 4a (texture optimisation)

- Native DDS pipeline in `circinus-core::dds`, no external tool: PNG → BC1 (opaque) or BC7/BC3 (alpha) with a full mip chain to 1×1, dimensions rounded up to multiples of four by resizing (never padding), colour bled into transparent texels at every level so neither the mips nor the GPU's bilinear filter pick up dark fringes. Encoding is Intel's ISPC texture compressor (`intel_tex_2`, MIT/Apache, prebuilt kernels for x86_64 and aarch64 on Windows, macOS and Linux); quality presets map to its very-fast/fast/basic/slow BC7 modes.
- Every file is validated before it is put in place: header, flags, mip count, exact length, then the top level is decoded (`bcdec_rs`) and compared with the source — PSNR in premultiplied space and alpha coverage. Failures leave the PNG alone. Writes are temp-file-then-rename.
- A manifest in the cache (`dds_files`) records every file Circinus wrote: source size/mtime/xxh3, DDS size/hash, format, dimensions. Re-runs skip unchanged sources; a DDS an author ships is never touched; **Revert** deletes only files whose hash still matches what Circinus wrote. When the change detector sees a mod update, its converted files are re-checked and stale DDS removed before the game can load old art; with the auto option, changed and new mods are converted again.
- Textures view: totals (files, disk, VRAM saving), Optimise active / everything / Remove all, live progress with rate and ETA, per-mod table with exclude/optimise/revert; Inspector shows per-mod counts with the same actions. Official content (Data/) is left as shipped.
- `cargo run -p circinus-core --example dds_convert -- in.png` converts one file; `cargo run -p circinus --example dds_job -- <game> <data> convert|revalidate|revert <mod folder>…` runs the job headless; `cargo run -p circinus --example halo_check -- <game> <data>` sorts an install and checks the official-content invariant on the result.

## Next milestones

3b. Def/patch flattening (who wins each XML value) and a .NET NativeAOT sidecar that lists Harmony patch targets per assembly.
4b. Instances/profiles, auto-update, Steam-client subscribe/unsubscribe.

## Data locations

- Settings, cache and databases: `%LOCALAPPDATA%\Circinus` (Windows) or `~/Library/Application Support/Circinus` (macOS).
- Your rules: `<data>/dbs/userRules.json`.
