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
uses. It also takes build uploads and serves the auto-update feed, both of which are live: builds go up
through four calls under `/api/v1/ci/releases/…` with a `cmk_` build key, and an installed copy
reads `/api/v1/releases/latest.json`. The site chose Tauri's **static** feed format rather than
the per-copy dynamic route this repo specified, so `tauri.conf.json` lists both endpoints in
order and the app takes whichever answers. `docs/update-feed.md` describes both and is the record
of what is actually served; `tools/push-build.mjs` is the whole push and the release workflow
runs it on a tag, given the `CIRCINUS_BUILD_KEY` secret. The site refuses Intel Macs and em
dashes in release notes, so the workflow builds no Intel Mac and the script flattens the dashes.

The updater's private key is `/home/claude/circinus-updater.key`, outside the repo, meant for the
`TAURI_SIGNING_PRIVATE_KEY` secret; only the public key is committed. It can never be rotated
without one manual reinstall for everyone, since an app carries the public key it was built with.

## Shape of the thing

- `circinus-core`: `scan` (two-phase: About.xml quickly, folder contents in the background,
  sqlite cache keyed by a stamp that includes `PARSER_VERSION`), `order` (HALO), `rules`,
  `modsconfig`, `dds`, `textures`, `loadcost`, `playerlog`, `defs`, `harmony`, `steam`.
- HALO — Harmonized Automated Load Order. Eight phases in load order: Prepatch (shown as
  "Preloads"), Core ("Game and DLC"), Framework ("Libraries"), Content, Patch ("Patches"),
  Texture ("Texture packs"), Late ("Late loaders"), Optimization ("Performance"). Rules are hard
  DAG edges, phases are soft, the official-content invariant holds (anything with Defs loads
  after Core and the DLC), and a real cycle is explained and cut rather than fatal. The HALO page
  lets a user switch built-in rules off or send them to another phase. *What changes* compares the
  order you have with the one HALO proposes as a diff rather than as two lists: the longest run of
  mods that keep their relative order is the backbone, and only the mods lifted out of it are
  moves. The rest is drift, and saying so is the point. Those moves are then shown as what leaves
  each phase beside what arrives in each, with one weighted arrow per journey — chosen from five
  mockups, because a row-by-row comparison of a thousand-mod list is unreadable however it is
  drawn (`src/lib/moves.ts`, `MovesView.svelte`).
- New mods: `arrivals` records when Circinus first saw each folder, durably and per instance,
  because `changes` retakes its baseline on every scan and a mark on a row has to outlive the
  rescan the eight second folder poll runs a moment later. The first record stamps everything 0
  ("already here"), so a first run announces nothing; a mark fades after 14 days or when the user
  says they have seen it. The list marks them green on the *right* edge, since amber on the left
  already means HALO would move this and one row can be both. There is a New tab, shown only when
  something is in it.
- The list's measures: **Time** is the seconds a mod is expected to add to loading, **Load** the
  same estimate as a share of the list, **Cost** its frame-time share from circinus.sh. Time and
  Load come from one number (`contents.load.scoreMs`) so they always agree; only Load is
  coloured, since colouring both would draw one fact twice. All three are estimates from what the
  folder holds, and every tooltip says so.
- Sorting: any column heading orders the list; the load order is the default and the only real
  one. Sorting `visibleActive` rather than the sections is what makes "by phase" sort within each
  section for free, since `layout` builds sections by filtering. Dragging is refused while
  sorted — a drop between two rows of a list sorted by name writes a position nobody chose.
- macOS paths: RimWorld is `RimWorldMac.app`, and the Finder treats a `.app` as a file, so a
  folder picker only ever offers the folder it sits in. `paths::game_root` resolves that folder
  to the bundle, and everything (Version.txt, Data, Mods, the executable, Steam detection) asks
  through it. The game's Data is `<app>/Data`, holding Core and the DLC; `Contents/Resources/Data`
  is Unity's and preferring it by name is what hid Core. Both candidates are tried and the one
  with Core in it wins. None of this is behind `cfg!(target_os = "macos")` on purpose: a rule
  that only runs where the tests cannot is a rule nobody checks.
- `defs`: builds the document the game builds — every active mod's Defs merged in load order,
  every PatchOperation applied in load order, then Name/ParentName inheritance — with the origin
  of every node recorded, so "who wins this value" has an answer. `defs::xpath` is XPath 1.0 as
  .NET's `SelectNodes` means it, because that is what RimWorld hands patches to.
- `clr`: reads .NET assemblies (PE, ECMA-335 metadata, custom attribute blobs, IL) without a
  .NET runtime and without ever loading a mod's code. `harmony` caches those readings and groups
  them into what one mod patches and who else patches the same method.

## Loops, and warnings a player disagrees with

- A reported loop is a real path. It used to be the strongly connected component -- the set of
  mods that can all reach one another -- printed in Tarjan's order with arrows between them, so a
  tangle of eight was shown as an eight-step loop naming steps nobody wrote. `order::find_cycle`
  walks the component for an actual cycle; `Issue::Cycle` carries one rule per step and the one
  rule that was `cut`, and the message says where each came from. "The weakest rule was set aside"
  named neither the rule nor its source.
- The cut is one edge per pass, not every edge of the lowest precedence.
- `Issue::Incompatible` can be hidden per pair (`UserData::muted`, keyed by package ids so it
  survives a reinstall). The databases are a community's best guess and a patch can make two mods
  work together without the entry changing; a warning nobody can dismiss is one people learn to
  look past, along with the true ones beside it. Settings brings them all back.

## When something goes wrong

- `diag` writes tracing to `<app data>/logs/circinus.log`, rolled once at 4 MB. Logging to stdout
  is logging to nowhere for a packaged app, which is how a black window got reported with nothing
  to read. `Copy diagnostics` (Settings, and on the crash screen) gathers version, platform,
  folders and whether each is there, what the scan found, and the log's tail, with the home
  directory written as `~` so a bug report does not carry the user's name.
- `Root.svelte` puts the whole window inside one boundary. `Panel` already caught a panel, but
  App's outermost layer sits outside every one, and Svelte re-throws a cached `$derived` failure
  to every later reader -- so one bad value took down the panels *and* the frame.
- `store.accept()` reads the handful of snapshot fields everything depends on the moment one
  arrives. A snapshot that cannot be read used to fail later, inside a derived, and surface as an
  error from inside the framework rather than the one that mattered. With something on screen the
  old copy is kept; with nothing, the message says what happened.
- `?crash=1` in the browser mock returns a snapshot missing `settings`, so all of the above is
  tested rather than assumed (`tools/loadtest/crash.cjs`).

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
