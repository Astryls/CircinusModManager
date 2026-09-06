# The update feed, and pushing a build to circinus.sh

Two halves of one thing: what an installed copy asks for and is answered with, and how a build
gets onto the site in the first place.

**What is built and what the site does.** The app uses Tauri's updater plugin
(`tauri-plugin-updater` 2.x) unchanged, and the plugin reads either of the two shapes Tauri
defines. The site serves the **static** one at

```
https://circinus.sh/api/v1/releases/latest.json
```

which is a single document naming one version and carrying a `url` and a `signature` per
platform key (`windows-x86_64`, `darwin-aarch64`, `linux-x86_64`). That is the first endpoint in
`src-tauri/tauri.conf.json`, and it is what an installed copy actually reads today.

The **dynamic** form below — one request per copy, carrying its own target, architecture and
version, answered with `204` or a single-platform body — is the second endpoint, kept because it
is strictly better: it can answer `msi` to an MSI install and `nsis` to an NSIS one, which one
static document cannot. The plugin tries the endpoints in order, so if the site ever implements
it, apps already in the field pick it up with no new build. Sections 1 and 2 are its
specification and remain the contract for that route.

**Pushing a build** is section 5. `tools/push-build.mjs` is the whole of it and the release
workflow runs it; `tools/loadtest/push-build.cjs` drives that script against a stub of the site.

## What the site refuses

- **Intel Macs.** `darwin-x86_64` is rejected rather than filed under Apple Silicon, so the
  release workflow does not build one.
- **Em dashes in release notes.** The push script flattens them, along with curly quotes.
- **A version older than the one published now.** `latest.json` names one version and every
  installed copy compares it to its own; a lower number leaves every app saying "up to date"
  forever, and it never sees the next real release either. Re-publishing the same version is
  fine; a pre-release (`0.3.0-beta.1`) is exempt, being never what `latest.json` names.

## 1. The request (the dynamic route)

Once, a few seconds after launch (when the user has left the setting on), and whenever the user
presses *Check now* in Settings, the app asks its endpoints in order. Against the dynamic route
that request is:

```
GET https://circinus.sh/modmanager/update/{target}/{arch}/{current_version}?installer={bundle_type}
Accept: application/json
User-Agent: tauri-plugin-updater/2.11.0
```

No cookies, no authentication, no body. The four values are filled in by the app:

| Placeholder       | Meaning                                  | Values the app can send                                                                 |
| ----------------- | ---------------------------------------- | --------------------------------------------------------------------------------------- |
| `target`          | operating system                         | `windows`, `darwin` (macOS), `linux`                                                    |
| `arch`            | CPU architecture                         | `x86_64`, `aarch64` (Apple Silicon), also `i686`, `armv7`, `riscv64` (not built today)  |
| `current_version` | the version this copy is running         | plain semver, no `v`: `0.1.0`, `0.2.0-beta.1`. A `+` in it is percent-encoded as `%2B`. |
| `bundle_type`     | how this copy was installed (query part) | `nsis`, `msi`, `appimage`, `deb`, `app` (macOS); `unknown` when it is not a packaged build |

`installer` is the only part that is not in Tauri's example URL. It is there because Windows
gets two installers (an NSIS `-setup.exe` and an `.msi`) and the app must be handed the same
kind it was installed with; the plugin runs whichever file it receives, and an `.msi` given to
an NSIS install leaves two registered copies behind. On macOS and Linux it is informational.
Treat a missing or `unknown` value as `nsis` on Windows, `appimage` on Linux.

Builds the release workflow produces today, and therefore the combinations that will actually
arrive:

| `target`  | `arch`    | `installer`            |
| --------- | --------- | ---------------------- |
| `windows` | `x86_64`  | `nsis` or `msi`        |
| `darwin`  | `aarch64` | `app`                  |
| `darwin`  | `x86_64`  | `app`                  |
| `linux`   | `x86_64`  | `appimage` or `deb`    |

Anything else (an arch there is no build for) should get `204 No Content`, not an error.

## 2. The response (the dynamic route)

Two answers are possible.

**Nothing newer:** `204 No Content` with an empty body. The app shows "Circinus 0.1.0 is the
newest". Any other status without a body the app can parse — a 404, a 500 — is reported to the
user as "the update feed may be down", so a 204 is the only right way to say "you are current".

**Something newer:** `200 OK`, `Content-Type: application/json`, and this body:

```json
{
  "version": "0.2.0",
  "pub_date": "2026-09-05T18:00:00Z",
  "url": "https://circinus.sh/modmanager/download/0.2.0/Circinus%20Mod%20Manager_0.2.0_x64-setup.exe",
  "signature": "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVRRnFpNTVWTkhL...",
  "notes": "HALO learns late loaders. Fixes for linked mod folders."
}
```

| Field       | Required | What it is                                                                                                                                                            |
| ----------- | -------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `version`   | yes      | semver of the build on offer. A leading `v` is tolerated and stripped. The app installs it only if it is **greater** than `current_version` by semver rules (so `0.2.0-beta.1` is *older* than `0.2.0`, and equal versions are not offered). Answering with the same or an older version is harmless: the app says it is current. |
| `pub_date`  | no       | RFC 3339 timestamp (`2026-09-05T18:00:00Z`). If present it must parse, or the whole answer is rejected. Shown to the user.                                              |
| `url`       | yes      | Absolute `https` URL of the file to download for *this* `target`/`arch`/`installer` (section 4 says which file). Percent-encode spaces. The download request sends `Accept: application/octet-stream` and follows redirects; a `Content-Length` header makes the progress bar show a total. |
| `signature` | yes      | The **complete contents of the `.sig` file** that was uploaded beside that installer, as one string. It is already base64: do not decode or re-encode it, and do not add anything — the app's base64 decoder rejects whitespace, so a newline appended on the way through the database makes every update fail with "not signed with Circinus's key". Trim it to be safe. The app verifies the downloaded bytes against it with the public key baked into the app; a wrong or missing signature means the update is refused and nothing is installed. |
| `notes`     | no       | Plain text release notes, shown in the banner and in Settings. Keep it to a few sentences; the UI does not render Markdown.                                              |

Extra fields are ignored. Only the `url` and `signature` for the requested platform go in the
answer; there is no per-platform map in this format.

The static form the site serves today is the same fields with the per-platform parts moved into
a map, and no 204:

```json
{
  "version": "0.2.0",
  "pub_date": "2026-09-05T18:00:00Z",
  "notes": "HALO learns late loaders.",
  "platforms": {
    "windows-x86_64": { "url": "https://circinus.sh/...-setup.exe", "signature": "<the .sig>" },
    "darwin-aarch64": { "url": "https://circinus.sh/....app.tar.gz", "signature": "<the .sig>" },
    "linux-x86_64":   { "url": "https://circinus.sh/....AppImage",   "signature": "<the .sig>" }
  }
}
```

The plugin compares `version` with the running one itself, so "you are current" needs no special
answer. Everything section 3 says about signatures applies unchanged: the `signature` is the
`.sig` file's contents verbatim, and the bytes served must be the bytes that were signed.

### Worked example

A Windows user on 0.1.0, installed with the NSIS installer, asks:

```
GET /modmanager/update/windows/x86_64/0.1.0?installer=nsis
```

The newest published release is 0.2.0, so the server answers `200`:

```json
{
  "version": "0.2.0",
  "pub_date": "2026-09-05T18:00:00Z",
  "url": "https://circinus.sh/modmanager/download/0.2.0/Circinus%20Mod%20Manager_0.2.0_x64-setup.exe",
  "signature": "<contents of Circinus Mod Manager_0.2.0_x64-setup.exe.sig>",
  "notes": "HALO learns late loaders. Fixes for linked mod folders."
}
```

The app shows a banner. When the user presses *Install and restart* it downloads the `url`,
checks the bytes against `signature`, runs the installer in passive mode (a progress bar, no
questions) and the installer restarts Circinus.

A Mac on Apple Silicon running 0.2.0 asks `GET /modmanager/update/darwin/aarch64/0.2.0?installer=app`
and, 0.2.0 being the newest, gets `204`.

A pre-release: if the site publishes `0.3.0-beta.1` only to people who opted in, that is a
server-side decision; the app has no channel setting today and treats whatever `version` it is
given by semver alone.

## 3. Signing

Every file the app can install is signed with a **minisign** key pair generated by the Tauri
CLI. The public key is in the repository (`src-tauri/tauri.conf.json`, `plugins.updater.pubkey`)
and compiled into the app. The private key signs files at build time and lives nowhere else.

The server never signs anything. It stores the `.sig` files the build produced and returns
their contents in `signature`. If the site ever re-packages, re-compresses or otherwise alters
an installer after the build, the signature no longer matches and every update is refused, so
serve the uploaded bytes untouched.

### The private key

- It is at `/home/claude/circinus-updater.key` on the machine of the session that generated
  it, **outside the repository**, and must never be committed. `circinus-updater.key.pub` beside
  it is the public key already pasted into `tauri.conf.json`.
- It has **no password**. That is deliberate: it makes the CI secret one value with nothing to
  keep in step, and the key is only ever used from the GitHub secret store.
- Put the private key file's contents into the GitHub Actions secret
  `TAURI_SIGNING_PRIVATE_KEY`, and create `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` with an empty
  value. `.github/workflows/release.yml` passes both to `tauri build`, which then writes a
  `.sig` beside every installer and fails the build if the key is missing.
- If you would rather the key had never passed through a cloud session, make a fresh pair and
  paste the new public key into `tauri.conf.json`:

  ```sh
  npx tauri signer generate -w ~/circinus-updater.key --ci --force
  ```

  (`--ci` skips the password prompt and produces a key with no password; drop it to be asked
  for one, in which case that password goes in `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.) Any
  build signed with the old key stops being installable as an *update* by apps carrying the new
  public key, so rotate before the first public release or accept one manual reinstall.

## 4. What a release upload consists of

The site keeps **four slots** per release. Everything else the build produces (the `.msi`, the
`.deb`) is for people who come looking on GitHub and is not uploaded.

| Slot | The file Tauri wrote | Signed? | What it is |
| --- | --- | --- | --- |
| `windows` / `installer` | `bundle/nsis/..._x64-setup.exe` | yes | what a person downloads *and* what the updater installs |
| `mac` / `installer` | `bundle/dmg/..._aarch64.dmg` | no | what a person downloads the first time |
| `mac` / `update` | `bundle/macos/....app.tar.gz` | yes | what the updater installs, replacing the app bundle in place |
| `linux` / `installer` | `bundle/appimage/....AppImage` | yes | both, as Tauri re-uses the AppImage for updates |

Things to know about that table:

- **macOS updates are the `.app.tar.gz`, not the `.dmg`.** The two are different files with
  different jobs, and only the tarball is signed.
- The `.sig` is a small text file beside each bundle: base64 of a minisign signature block. Its
  contents, verbatim, are the `signature` the site serves. It is written by the build and cannot
  be produced any other way. **A bundle uploaded without one is stored and listed and never
  offered to the updater, and nothing on the page says why** — so the push script refuses to
  send an unsigned bundle at all.
- Files are named by Tauri from the product name and version; spaces in them are real spaces.

## Building for machines you do not have

Nobody needs to own a Mac or a Linux box. `.github/workflows/release.yml` runs on GitHub's
machines: `windows-latest`, `macos-latest` (Apple Silicon, which is the only Mac the site takes)
and `ubuntu-22.04`. The tag is the only thing you push; three machines build, and one job uploads
all four files.

Before the first tag, run the workflow by hand from the **Actions** tab. It builds all three and
attaches them as artifacts, publishing nothing. That is the cheapest way to find out that a
platform does not build.

Each build then opens its own bundle once, since nobody here can run it: the AppImage is
extracted, the Mac tarball is unpacked and its binary checked for `arm64` (an Intel binary is
refused at upload, twenty minutes later, and this catches it at the build), and the Windows
installer's size is printed. None of them launches the app, which would want a display.

Two things that owning a Mac would otherwise tell you:

- **Gatekeeper.** Without an Apple Developer certificate the `.dmg` builds and works, but macOS
  says the developer cannot be verified and the first launch needs right-click → Open. The
  workflow signs and notarizes when `APPLE_*` secrets are set and builds unsigned when they are
  not. Updates are unaffected: the updater checks the minisign signature, not Apple's.
- **Actually using it.** A build that compiles is not a build that works. The Discord is the
  place to hand a first Mac or Linux build to someone who has one.

## 5. Cutting a release

```sh
node tools/release.mjs 0.2.0            # the whole thing
node tools/release.mjs 0.2.0 --dry-run  # say what would happen, change nothing
node tools/release.mjs --local          # build this machine's platform only, and push that
```

**A Windows installer can only be made on Windows, a `.dmg` and its `.app.tar.gz` only on macOS,
an AppImage only on Linux.** No one machine can produce the set, so cutting a release means
pushing a tag and letting three machines build it. `tools/release.mjs 0.2.0` does that: it writes
the version into the three files that carry it (`package.json`, `src-tauri/tauri.conf.json`, the
workspace `Cargo.toml`), runs `cargo test`, `npm run check`, the front-end build and the push
test, commits, makes an **annotated** tag whose message becomes the release notes, and pushes.
From there `.github/workflows/release.yml` builds the three platforms and one job uploads and
publishes all four files. If the GitHub CLI is installed the script follows the run and finishes
by reading `latest.json` to say what an installed copy will actually be offered.

It refuses to re-cut a tag that exists, or to cut a version that is not newer than the last one.
Both would be published over people who already have that number.

`--local` is the exception: build the platform you are sitting at and push only that slot, for a
fix that affects one platform. It needs `TAURI_SIGNING_PRIVATE_KEY` set, because without it the
build writes installers with no `.sig` beside them and every one of them is stored, listed, and
never offered.

## 6. Pushing a build

`tools/push-build.mjs` is the whole interface. The release workflow runs it on a tag, and it can
be run by hand against a local build:

```sh
CIRCINUS_BUILD_KEY=cmk_... node tools/push-build.mjs            # from src-tauri/target/release/bundle
CIRCINUS_BUILD_KEY=cmk_... node tools/push-build.mjs --dir artifacts --dry-run
```

The **build key** is made at <https://circinus.sh/admin> under *Mod Manager builds → Build keys*
and is shown once. It can create a release, upload its files, publish or retract it, and read the
list back; it cannot delete anything or make another key, and every call it makes is recorded
against its id. In CI it is the repository secret `CIRCINUS_BUILD_KEY`; without it the workflow
still builds and drafts a GitHub release and says in the log why nothing was sent.

What the script does, in order:

1. **Create** the release with its notes. A release starts *retracted* — invisible on the
   download page and absent from the feed — because uploading four files is four requests and a
   release that went live on the first would spend a minute offering Windows to Mac visitors.
2. **Upload** each of the four slots, one file per request as the whole body, with the file name,
   the sha256 of the bytes (the site re-hashes and refuses a mismatch, which is the only way a
   truncated transfer is ever caught), and the `.sig` contents where there is one. If a slot
   replaces a file people have already downloaded the site says so, and the script repeats that
   warning rather than swallowing it: two machines reporting one version number while running
   different builds is something no updater can reconcile.
3. **Publish**, which is the moment anything becomes visible. A release missing a platform needs
   `--force`; the script passes it and names what is missing rather than failing on a
   single-platform build.
4. **Read `latest.json` back** and fail if it does not name the version just published, or if any
   platform in it has no signature. Both are silent in every other way: the release looks fine on
   the site and no installed copy is ever offered it.

It refuses before sending anything when a bundle that needs a signature has none, when the
version asked for is not the version this checkout builds (`src-tauri/tauri.conf.json` is where
the number is written, once), or when a file is empty or over the site's 600 MB limit.

`node tools/loadtest/push-build.cjs` runs the script against a stub of the site with fake
bundles and checks the whole sequence, including each of those refusals.

## 7. Where the app side lives

- `src-tauri/tauri.conf.json` — the version (written once, here), the two endpoints in order,
  the public key, `bundle.createUpdaterArtifacts`.
- `tools/release.mjs` — cutting a release; `tools/push-build.mjs` — the push itself, with
  `tools/loadtest/push-build.cjs` as its test.
- `.github/workflows/release.yml` — builds three platforms on a tag, pushes them, drafts a
  GitHub release as a copy.
- `src-tauri/src/updater.rs` — the `update_check` and `update_install` commands, the quiet
  check on start (once per launch, five seconds in, only when the setting is on), the
  `update-available` and `update-progress` events, and the errors in plain words.
- `src/components/Banner.svelte`, `TitleBar.svelte`, `SettingsView.svelte` — the banner with
  *Install and restart* / *Not now*, the title-bar mark, and the Updates section in Settings.
- `src/lib/mock.ts` — the browser build: open the UI with `?update` in the query string to see
  the whole flow without a server; `?offline` makes *Check now* fail the way a missing
  connection does.
