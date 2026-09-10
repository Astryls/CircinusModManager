# UI load test

Renders the built UI with a backend-shaped snapshot at a large scale (about 2,000 mods, 1,081
active, thousands of issues) in headless Chromium and reports render time and console errors.

```sh
npm run build
node tools/loadtest/gen-snapshot.js                    # writes tools/loadtest/snapshot-js.json
npx vite preview --port 4173 &                         # serves dist/
node tools/loadtest/loadtest.js [path/to/snapshot.json]
```

A snapshot from the real backend can be produced with the Rust example:

```sh
cargo run -p circinus --example dump_snapshot -- <game dir> <data dir> > snapshot.json
```

Playwright is needed for the browser (`npm i -D playwright && npx playwright install chromium`).

## Screenshots of the mock UI

`shots.cjs`, `shots2.cjs` and `shots3.cjs` (Textures view) drive the built UI against the browser mock (`npx vite preview --port 4173`) and save the main views; `shots2` opens `/?nosteamcmd` for the first-run state. Run them with Playwright on `NODE_PATH`, e.g. `NODE_PATH=$(npm root -g) node tools/loadtest/shots.cjs out/`.

`shots26.cjs` covers Inactive | Active, the two lists side by side. It asserts rather than prints, and exits non-zero on the first thing that is wrong: `npx vite preview --port 4180 --strictPort` then `NODE_PATH=$(npm root -g) node tools/loadtest/shots26.cjs out/`.

`shots27.cjs` covers **What changes**, the phases mods leave beside the phases they arrive in. Same port and shape as `shots26`. It drives the mock at `?big=1100&jumble=30` and again at `?big=1100&jumble=600`: forty mods prove nothing about a view built for a thousand, and the side-by-side lists it replaced looked fine on the small mock and were unreadable on a real install. It checks that the group cards stay inside their columns with nothing clipped or overlapping, that every arrow leaves a card's edge level with a phase name and reaches one on the other side, that the arrows' counts add up to exactly the number of moves, that no two counts sit on top of each other, and that opening a group, hovering an arrow, clicking an arrow or a phase, and the search box all keep the cards and the arrows in agreement.

`shots28.cjs` is the toolbar, swept from 2200px down to 640px in every state it has — the list alone, a preview up, the board open, two lists open — plus the two places a player is told where to ask for help. Same port as `shots26`. It exists because the toolbar overflowed at ordinary desktop widths and drew Discard under the panel beside it, and the checks that were meant to catch that only looked at three widths in one state.

`push-build.cjs` drives the real `tools/push-build.mjs` against a stub of circinus.sh with fake bundles in a temporary folder: `node tools/loadtest/push-build.cjs`. No browser and no network. The push runs once per release, from CI, unattended, and the ways it goes wrong are quiet ones - an unsigned bundle that uploads happily and is never offered, a truncated transfer stored as if it were whole, a tag that does not match the build it publishes - so each of those is a case here.

`moves.cjs` checks the arithmetic behind that board with no browser at all — it bundles `src/lib/moves.ts` with esbuild and calls it: `node tools/loadtest/moves.cjs`.

`patches.cjs` is the patch report in its four tabs: `npx vite preview --port 4173 --strictPort` then `node tools/loadtest/patches.cjs out/`. It exists because that screen was one page with four answers stacked on it, the long ones cut off at a hundred and twenty rows. So it asks whether the contested list holds every contested method rather than a hundred and twenty of them, whether the method list's box is as tall as the whole list while only a screenful is drawn in it, whether each drawn row and each sticky column name sits where its offset says, whether an opened mod's methods are capped, and whether clicking a mod, landing in the load order and coming back returns to the same tab at the same pixel — with each tab keeping its own place rather than sharing one.

`openfolder.cjs` checks that "Open folder" opens the folder rather than its parent: `node tools/loadtest/openfolder.cjs` against the usual `4173`. Outside Tauri the opener calls are no-ops, so `api.ts` logs what it would have done and this reads that back — right-click a mod, press Open folder, and the app must have asked to *open* the mod's own path, not to reveal it. It also checks the other half stayed put: SteamCMD's folder button opens, and the executable beside it, being a file, is still revealed.

`packs.cjs` covers modpack announcements — what a curator says, reaching a player who follows the pack here and not on Discord. Usual port; `node tools/loadtest/packs.cjs out/`. It proves two things, and the second matters more: that a feed reaches the banner, is attributed to whoever wrote it, is rendered as text rather than markup, clears when read, and that muting a curator silences them without unfollowing the pack — and that with `?nopacks`, the case every copy of the release ships into until circinus.sh serves the endpoint, there is no banner, nothing in the sidebar and no error toast.

`chords.cjs` is the keystroke rules with no browser at all — it bundles `src/lib/chord.ts` with esbuild and calls it, the way `moves.cjs` does. The platform is a parameter rather than a sniffed global, which is the only way this machine can check what a Mac would be told to press: that Ctrl+S is not Cmd+S on a Mac, that Ctrl+Shift+I is not Ctrl+I anywhere (devtools used to open the Import dialog), and that `?` matches with Shift held because that is how a keyboard sends it.

`keyboard.cjs` drives real keystrokes through Playwright, because every bug here was about *where* a keystroke went and none of them are visible from calling a function: Escape clearing the search box and then letting it go, Ctrl+K selecting what is already typed, arrows still walking the list after a wheel scroll has destroyed the focused row, Alt+Arrow moving a mod one place rather than two, Escape closing the context menu without also wiping the selection behind it, and the palette and the shortcuts reference. Usual port; `node tools/loadtest/keyboard.cjs out/`.

`../commands-check.mjs` is not a loadtest and needs no browser: `node tools/commands-check.mjs`, and `npm run check` runs it. It checks the agreement between the window and the backend that nothing else checks — every `invoke("x")` in `api.ts` names a command in `generate_handler!`, and nothing reaches for the opener plugin's `openPath` from the window. It exists because a shipped build had "Open folder" fail for every user: the capability listed `opener:allow-open-path`, which enables the command with an *empty* path scope that refuses everything, and `openfolder.cjs` passed throughout because the browser mock short-circuits before the call.
