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

`shots27.cjs` covers **What changes**, the board of lines between the order you have and the one HALO proposes. Same port and shape as `shots26`. It drives the mock at `?big=1100&jumble=30` and again at `?big=1100&jumble=600`: forty mods prove nothing about a view built for a thousand, and the side-by-side lists it replaced looked fine on the small mock and were unreadable on a real install. It checks that the two bars stay level and their bands tile them exactly, that every line runs from one bar to the other and no further, that the phase labels clear one another, that lines and rows agree in number, and that hovering, the phase filter and the search box all narrow both together.

`moves.cjs` checks the arithmetic behind that board with no browser at all — it bundles `src/lib/moves.ts` with esbuild and calls it: `node tools/loadtest/moves.cjs`.
