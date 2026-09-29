# Sharing a load run

How the mod manager sends load-time findings to circinus.sh, and what the site has to do with
them when it arrives.

This is the contract between the app and the site. The app half is in
`crates/circinus-core/src/telemetry.rs`; the site half does not exist yet. The app is written so
that this does not matter: an endpoint that is not there is a queued run that stays queued and
is retried later, so a build in the wild today sends nothing and starts working the day the
endpoint answers, with no new release.

The one thing here that is not inherited is the install id, and it is stricter rather than
looser: the manager generates its own, so load runs and frame runs cannot be tied to the same
machine. See below for why that costs nothing.

The policy below is **not new**. It is the one already published at
<https://circinus.sh/privacy> for the Circinus Performance Analyzer, and the manager is held to
it word for word. A second tool sending a slightly different set of things under the same
promise would make the promise worthless, so where this document and that page disagree, that
page wins and this one is the bug.

## What is being collected, and why it is worth collecting

The manager can already read what the Loading Progress mod measured: how long each mod took to
load, on one machine, once. Pooled across installs that becomes the thing nobody has — a figure
for what a mod costs at start-up, the way the site already has one for what it costs per frame.

The catch is that milliseconds are not comparable between machines. The site already refuses to
average raw ms across installs for frame cost, and load is worse: the same list on an NVMe and
a spinning disk are minutes apart, and nothing in a mod's folder says which you have.

What makes these runs poolable is `vanillaMs`: the part of the measured total that no mod
accounted for. It is the game's own start, measured on that machine in that same run, so it is
a per-machine constant taken under the same conditions as everything beside it. **Rank on the
ratio.** A mod that costs 0.4 × vanilla is saying something about the mod; a mod that costs
9,200 ms is saying something about the disk. Keep the raw figure, but only ever compare it
within a machine class.

## The endpoint

```
POST https://circinus.sh/api/v1/load-runs
Content-Type: application/json
User-Agent: CircinusModManager/<version>
```

No key and no account, like `/mods`. Rate limiting is by hashed install id (below).

### Request body

```json
{
  "consentVersion": 1,
  "installId": "019fabdd75a720ad9cba70522627",
  "source": "modmanager",
  "managerVersion": "1.6.0",
  "gameVersion": "1.6",
  "machine": {
    "cpu": "Ryzen 7 9800X3D",
    "cores": 16,
    "memoryGb": 32,
    "gpu": "RTX 3070",
    "os": "Windows 11"
  },
  "totalMs": 462986.0,
  "vanillaMs": 161742.0,
  "modsLoaded": 708,
  "defsParsed": 91244,
  "patchOps": 14022,
  "listHash": "9f2a1c77b4e8d0a3c6b1f5e2",
  "mods": [
    { "packageId": "brrainz.harmony", "workshopId": 2009463077, "version": "2.3.1", "totalMs": 905.75, "offThreadMs": 12.0 },
    { "packageId": "telardo.rimmsqol", "version": "1.6.0", "totalMs": 42155.25 }
  ]
}
```

Every field is exactly what the privacy page permits, and nothing else is in the struct. Note in
particular what is **absent and must never be accepted if a future client sends it**:

| Not sent | Why |
|---|---|
| Mod names | Unverifiable typed strings. The site takes real titles from Steam's listing, which is the whole reason `workshopId` is here. The source file the manager reads has a name for every mod; it is dropped before the payload is built. |
| File paths | On the never-sent list, and the payload is built by a function that has no access to one. |
| The versions of unmeasured mods | Only mods in this run's cost table carry a version, so a rework's cost can be told apart from the version before it without shipping a whole load order's versions, which would make an install far easier to fingerprint. |
| OS build numbers | `"Windows 11"`, never `"Windows 11 (10.0.22631)"`. |
| Anything from the player's world | Saves, colonies, pawns, names. None of it is anywhere near this path. |

`offThreadMs` is omitted when zero. `workshopId` is omitted for non-Workshop mods. `modsLoaded`,
`defsParsed` and `patchOps` are omitted when the mod did not record them.

### Responses

| Status | Meaning | What the client does |
|---|---|---|
| `202` | Accepted, or accepted-and-ignored as a duplicate. Body may be `{"runId": "..."}`. | Marks the run sent and drops it from the queue. |
| `409` | Already have this `(installId, listHash, totalMs)`. | Same as 202. A duplicate is not an error. |
| `400` | Malformed. | Drops the run and logs. Never retried — a bad payload will be bad forever. |
| `422` | `consentVersion` is not one the site currently accepts. | Drops the run, and clears the stored consent so the player is asked again. |
| `429` | Rate limited. Honour `Retry-After`. | Keeps the run queued. |
| `5xx`, timeout, connection refused | The site is having a bad day. | Keeps the run queued, retries with backoff. |

**Every failure is silent to the player.** Sharing a run is not something they asked to watch
succeed, and a toast saying a background upload failed is noise about a thing they cannot fix.

## The install id

A random identifier the manager generates once and keeps in its own user data. It is **not**
the Performance Analyzer's, and that is a deliberate reversal of the obvious choice.

Reusing the mod's id would let the site read one machine's load runs and frame runs together.
Nothing needs that. **A load cost and a frame cost meet on the mod's page, keyed by
`packageId`** — that is the only join the site actually performs, and it works whether or not
the same person produced both numbers. Sharing an id would have made an install trackable
across two tools in exchange for a correlation nobody had a use for.

So the two data sets are not joinable at the machine, and the privacy page can say so.

Storage is the same shape the analyzer already uses:

```
stored = sha256(server_salt + ":" + install_id)[0:24]
```

The salt lives only in the server's configuration and the raw id is never written to the
database. It is used for two things — stopping one run being counted twice, and rate limiting —
and for nothing else. Because it is separate from the analyzer's, its rate-limit bucket and its
deletion scope are separate too: deleting from the mod does not delete load runs, and the
Sharing panel has to offer its own delete with its own id shown beside it.

## What the site should do on arrival

1. **Hash the install id** before anything is written.
2. **Reject a `consentVersion` the site does not currently accept**, with `422`. A run sent
   under an older agreement was not agreed to. Do not guess, do not accept-and-downgrade.
3. **Reject a payload carrying a field this document does not list.** A client that has learned
   to send mod names is a client with a bug, and accepting it would break the promise on behalf
   of the player who was never asked.
4. **Drop runs from a broken list.** The analyzer already refuses to rank measurements taken on
   a list the game complained about. A load run has no error counts in it yet, so the equivalent
   for now is coarser: if `modsLoaded` is present and disagrees with `mods.length` by more than
   a little, the run described a list the manager could not fully account for.
5. **Normalise before pooling.** Store `totalMs` and `vanillaMs` as sent, and rank on
   `totalMs / vanillaMs` per mod. Never average raw milliseconds across machine classes.
6. **Apply the same floors as frame cost** — nothing ranked until 25 clean runs from 10 separate
   installs. The numbers are new and thin at first and a mod page that names a figure from three
   runs will be wrong in public.
7. **Deletion is by the manager's own install id**, and must reach the working database and the
   archive in one operation, as the analyzer's already does. Since the ids are separate, a
   player who shares from both tools has two things to delete and the interface has to say so
   rather than implying one button covers both.

## The player's side of it

Recording is local and always on: the manager reads the measurement whether or not anything is
shared, because the load-time card uses it either way. Sharing is a separate choice, off until
switched on, asked once with two buttons and changeable afterwards in Settings.

`CONSENT_VERSION` in `telemetry.rs` is what makes "you'll be asked again if it changes"
mechanical rather than a good intention: consent is stored against a number, and a stored number
lower than the current one does not authorise a send. **Bump it whenever a field is added; never
when one is removed.** Removing a field cannot surprise anybody.

## Tests that have to keep passing

`telemetry.rs` carries two that are not ordinary unit tests but the published guarantee in
executable form, and the privacy page says out loud that they run on every build:

- `no_path_can_reach_the_wire` — serialises a report and asserts no drive letter, no home
  directory, no file extension appears anywhere in it.
- `no_mod_name_can_reach_the_wire` — builds a report from a parsed file full of mod names and
  asserts not one of them survives into the JSON.

If either ever needs relaxing, the privacy page needs editing first.
