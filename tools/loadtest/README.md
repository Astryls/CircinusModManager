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
