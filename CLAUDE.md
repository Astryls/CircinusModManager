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

A development build does not check, and refuses to install. The version on `main` is a
placeholder — the real one is set on the release commit, which belongs to the tag — so a build
made from the branch says 0.1.0 and every release ever served looks newer than it. That put an
update banner in every `npm run tauri dev` a few seconds after launch, and pressing Install on it
runs the real installer over the machine being developed on. `tauri::is_dev()` is the guard, and
`updater::DEV_BUILD` is what it says instead.

## Shape of the thing

- `circinus-core`: `scan` (two-phase: About.xml quickly, folder contents in the background,
  sqlite cache keyed by a stamp that includes `PARSER_VERSION`), `order` (HALO), `rules`,
  `modsconfig`, `dds`, `textures`, `loadcost`, `playerlog`, `defs`, `harmony`, `steam`.
- HALO — Harmonized Automated Load Order. Eight phases in load order: Prepatch (shown as
  "Preloads"), Core ("Game and DLC"), Framework ("Libraries"), Content, Patch ("Patches"),
  Texture ("Texture packs"), Late ("Late loaders"), Optimization ("Performance"). Rules are hard
  DAG edges, phases are soft, and a real cycle is explained and cut rather than fatal. The HALO
  page lets a user switch built-in rules off or send them to another phase. *What changes* compares
  the order you have with the one HALO proposes as a diff rather than as two lists: the longest run
  of mods that keep their relative order is the backbone, and only the mods lifted out of it are
  moves. The rest is drift, and saying so is the point. Those moves are then shown as what leaves
  each phase beside what arrives in each, with one weighted arrow per journey — chosen from five
  mockups, because a row-by-row comparison of a thousand-mod list is unreadable however it is
  drawn (`src/lib/moves.ts`, `MovesView.svelte`).
- **Precedence: you > the author > the databases > HALO.** `RuleSource`'s declared order *is* the
  precedence system — `Ord` is derived from it, the contradiction resolver compares on it, and the
  cycle cutter drops the `min`. About.xml is the file the game itself sorts by and its author is
  the authority on their own mod, so nothing Circinus infers may outrank it. The databases sit
  below the author because they are other people's read of somebody else's mod and they ship off.
  HALO is last because everything at that rung is a guess. It used to be first, which is how a
  guess came to beat a declaration and how an author's own rule became the first thing cut out of
  a loop.
- Fluffy's `About/Manifest.xml` is not a tier. The game never reads it, its identifiers are
  free-form strings resolved through a lowercased folder-name lookup, and its load-order fields
  were bolted onto a version-check file. HALO reads it when a mod ships one and files what it
  finds at the HALO rung, where a fuzzy match can only ever produce the weakest edge in the graph.
- The official-content rule is a rule for silence. Anything that ships Defs loads after Core and
  the DLC *when nobody has said otherwise* — that is the drift vanilla's depth-first sort causes
  on its own, and the reason the rule exists. It was never an argument for overruling a person, so
  an author, a database in use, a user rule or the user filing a mod under Prepatch all put it
  above the game, Defs and all. `validate` then reports `AboveOfficial` with `declared: true`, a
  warning about what the placement may cost rather than an error about a list nobody chose: a mod
  whose defs inherit nothing from the game loads up there perfectly well, and only its author
  knows. Undeclared, it stays an error.
- `PREPATCH_IDS` is a set, not a sequence. There used to be an order in it, pushed in as hard
  edges labelled "the order their own pages ask for". Prepatcher's page says the opposite in as
  many words: "Its placement relative to Harmony doesn't matter, it can be put below or above it."
  Being in the set puts a mod in the Prepatch phase, which is what floats it; among mods no rule
  separates the order stays the player's.
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
- `dds`: a texture is flipped vertically before it is encoded (`Options::vflip`). A DDS stores
  its first row at the top, Unity takes the first row it is handed as the *bottom* one, so a file
  written the natural way is upside down in the game — which is what `-vf` is doing in the todds
  recipe the community uses. `validate` cannot catch this: it compares the decoded file against
  the very image the encoder fed to the block encoder, so it is blind to which way up that was,
  and it passed every inverted file the first version wrote. The test
  `a_texture_is_stored_bottom_row_first` is what holds the convention. One path asks for no flip:
  rebuilding a broken DDS that has no PNG beside it starts from pixels decoded out of a DDS, which
  are already the way round the game wants. `PARAMS_VERSION` is how a fix reaches files already on
  disk — `job::plan` re-converts anything the manifest says was encoded by an older set of rules.
- `defs`: builds the document the game builds — every active mod's Defs merged in load order,
  every PatchOperation applied in load order, then Name/ParentName inheritance — with the origin
  of every node recorded, so "who wins this value" has an answer. `defs::xpath` is XPath 1.0 as
  .NET's `SelectNodes` means it, because that is what RimWorld hands patches to.
- `clr`: reads .NET assemblies (PE, ECMA-335 metadata, custom attribute blobs, IL) without a
  .NET runtime and without ever loading a mod's code. `harmony` caches those readings and groups
  them into what one mod patches and who else patches the same method.

## The rule databases

- They ship **off**. They are other people's collections of what should load before what, a rule
  from one outranks the mod author's own About.xml, and downloading a few thousand of them on a
  first run and rearranging somebody's list on the strength of them is not a choice to make for
  them. Settings turns each on in a click and says what it is first.
- Switching one off deletes its files (`rules::forget_source`, including every game version's
  copy of a `{version}` source) *and* `Databases::load` refuses to read a disabled source's file.
  Both, on purpose: deleting alone fails when Windows holds a file open or somebody drops one
  back in the folder, and reading the switch alone leaves a database nobody wanted on disk. The
  switch used to do neither — it was written down and nothing read it, so the community rules
  went on ordering mods and the footer went on reporting them as loaded.
- `settings_version` 5 leaves an existing install's switches alone. Anyone already running with
  the community rules has a list built with them; turning them off on the strength of a new
  default would rearrange it overnight. What changes for them is that the switch now works.
- `forget_disabled_databases` also runs at startup, since a build before this one could leave a
  file behind for a source the user had already turned off.

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
