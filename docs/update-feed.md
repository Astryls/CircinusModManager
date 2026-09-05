# The update feed at circinus.sh/modmanager

What the app asks for, what the server answers, and what a release upload consists of. This is
the whole contract between Circinus Mod Manager and the site; someone who has never seen Tauri
can build the server side from this page alone.

The app uses Tauri's updater plugin (`tauri-plugin-updater` 2.x) unchanged. Its wire format
*is* the contract: the plugin already does the version comparison, the download and the
signature check, so the server only has to answer one question — *is there something newer
than what this copy is running, and where is it?*

## 1. The request

Once, a few seconds after launch (when the user has left the setting on), and whenever the user
presses *Check now* in Settings, the app sends:

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

## 2. The response

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
answer; there is no per-platform map in this format (Tauri also accepts a static `platforms`
object, but the dynamic form above is the one the app expects and the one the server should
send).

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

`.github/workflows/release.yml` builds one artifact per platform. Each contains the installers
people download by hand and, for every file the updater can install, a `.sig` next to it. The
site should accept the whole set and know which file is the *update payload* per
`target`/`arch`/`installer`:

| Platform             | Update payload (the `url` to serve)                 | Its signature (the `signature` to serve)          | Also in the upload, for people, not for the updater |
| -------------------- | --------------------------------------------------- | ------------------------------------------------- | --------------------------------------------------- |
| windows / x86_64 / nsis | `Circinus Mod Manager_0.2.0_x64-setup.exe`        | `Circinus Mod Manager_0.2.0_x64-setup.exe.sig`    |                                                     |
| windows / x86_64 / msi  | `Circinus Mod Manager_0.2.0_x64_en-US.msi`        | `Circinus Mod Manager_0.2.0_x64_en-US.msi.sig`    |                                                     |
| darwin / aarch64 / app  | `Circinus Mod Manager.app.tar.gz` (from the aarch64 build) | `Circinus Mod Manager.app.tar.gz.sig`      | `Circinus Mod Manager_0.2.0_aarch64.dmg`           |
| darwin / x86_64 / app   | `Circinus Mod Manager.app.tar.gz` (from the x86_64 build)  | `Circinus Mod Manager.app.tar.gz.sig`      | `Circinus Mod Manager_0.2.0_x64.dmg`               |
| linux / x86_64 / appimage | `Circinus Mod Manager_0.2.0_amd64.AppImage`     | `Circinus Mod Manager_0.2.0_amd64.AppImage.sig`   |                                                     |
| linux / x86_64 / deb    | `Circinus Mod Manager_0.2.0_amd64.deb`            | `Circinus Mod Manager_0.2.0_amd64.deb.sig`        |                                                     |

Things to know about that table:

- **macOS updates are the `.app.tar.gz`, not the `.dmg`.** The `.dmg` is what a person
  downloads the first time; the updater replaces the app bundle in place from the tarball. The
  two Mac builds produce a tarball with the *same name*; keep them apart by the build they came
  from (the workflow artifact is named `circinus-aarch64-apple-darwin` /
  `circinus-x86_64-apple-darwin`), or rename them on upload — the name of the file does not
  matter to the app, only its bytes and its signature.
- The `.sig` is a small text file: base64 of a minisign signature block. Its contents, verbatim,
  are the `signature` string. It is generated by the build and cannot be produced any other way.
- Files are named by Tauri from the product name and version; spaces in them are real spaces,
  so percent-encode when composing `url`.
- A release without a `.sig` for a file must not be offered for that platform. The workflow
  refuses to upload such a build, but the server should check too: an installer without a
  signature is fine for a person to download and useless to the updater.

A minimal server therefore keeps, per release version, a table of
`(target, arch, installer) → (file, signature text, size)`, plus `pub_date` and `notes`, and
answers the request in section 1 with the newest release that has an entry for the asked
combination and is greater than `current_version`; otherwise `204`.

## 5. Where the app side lives

- `src-tauri/tauri.conf.json` — the endpoint, the public key, `bundle.createUpdaterArtifacts`.
- `src-tauri/src/updater.rs` — the `update_check` and `update_install` commands, the quiet
  check on start (once per launch, five seconds in, only when the setting is on), the
  `update-available` and `update-progress` events, and the errors in plain words.
- `src/components/Banner.svelte`, `TitleBar.svelte`, `SettingsView.svelte` — the banner with
  *Install and restart* / *Not now*, the title-bar mark, and the Updates section in Settings.
- `src/lib/mock.ts` — the browser build: open the UI with `?update` in the query string to see
  the whole flow without a server; `?offline` makes *Check now* fail the way a missing
  connection does.
