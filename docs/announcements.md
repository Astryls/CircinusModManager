# Modpack announcements

What a modpack's curator wants to tell the people running their pack, and how it reaches them.

This is the contract between the app and circinus.sh. The app half shipped in 1.4.0; the site
half does not exist yet, and the app is written so that it does not matter — every failure is an
empty list, so a build in the wild today shows nothing at all and starts working the day the
endpoint answers, with no new release.

`crates/circinus-core/src/announce.rs` is the client. `tools/loadtest/packs.cjs` covers both the
working case and the unserved one.

## Why there is a feed at all

A followed Steam collection already tells a player *what* changed: these mods arrived, those
left. It cannot say why, or that a save needs a mod removed before it will load, or that 1.6
broke something and the fix is coming Tuesday. Curators say all of that on Discord, and a player
who follows the pack in Circinus and not on Discord never hears any of it.

## The endpoint

```
GET https://circinus.sh/api/v1/packs/{collectionId}/announcements?since={unixSeconds}
```

`collectionId` is the Steam Workshop collection id — the same number the user follows in the
sidebar. No key, no account: this is public, like `/mods`. `since` is a hint the app currently
always sends as `0`, because the panel shows a curator's recent posts rather than a queue that
empties as you read it; honour it or ignore it.

Answer with either shape:

```json
{ "announcements": [ … ] }
```
```json
[ … ]
```

`items` also works instead of `announcements`. One entry:

| field | type | required | notes |
|---|---|---|---|
| `id` | string | no | Stable across fetches. Absent, the app makes one from the pack and timestamp, so two posts in the same second would collide — send one. |
| `at` | number \| string | yes in practice | Unix seconds, or ISO 8601 (`2026-09-10T01:35:00Z`). Absent or unparseable is 0, which sorts oldest and reads as unread-forever. `createdAt` is accepted as an alias. |
| `author` | string | yes in practice | The curator's display name. Shown on every post. Empty falls back to the pack's name, which is worse — send it. `authorName` is accepted as an alias. |
| `text` | string | **yes** | Plain text. Rendered as text, never as markup; newlines are kept. Cut at 2000 characters. An entry without it is dropped rather than drawn empty. `content` is accepted as an alias. |
| `link` | string | no | Must start with `https://`. Anything else is dropped without comment, `http://` included. |

Anything else in an entry is ignored.

**Every failure is silence.** A 404, a 500, a timeout, HTML instead of JSON, a connection that
never opens: all of them are an empty list. There is no error path to the user, on purpose — a
curator's note is not worth an error in front of somebody trying to launch a game, and the app
has to survive being newer than the site.

## What the app does with it

- Asks 20 seconds after launch and every 6 hours after that (`src-tauri/src/packs.rs`), one call
  per followed pack, sequentially. Muted packs are not asked about at all.
- Unread is per pack: a post is unread when its `at` is newer than the mark the user has for that
  pack. Reading one pack does not silence another.
- Banner only. No desktop notification: a mod changing under a running game is worth
  interrupting for, somebody's note about it is not.
- The text is somebody else's and the UI says so — the curator's name on every post, and a line
  above them saying Circinus passes these along and does not check them.

That last point is the constraint the whole design hangs off. If the feed ever carries something
Circinus appears to be saying, this is broken regardless of what the JSON looked like.

## Where the text comes from

The app never talks to Discord and holds no Discord credential. Whatever writes the feed is the
site's business and can change without an app release — a bot, a form on a pack page, something
not invented yet.

The intended first writer is a Discord bot invited to the **curator's own server**:

1. The curator signs in to circinus.sh with Steam. Steam OpenID, not a new account: the Workshop
   already records who owns a collection (`creator` is a steamid the app fetches anyway), so
   "prove you are the curator" is automatic rather than a support ticket.
2. They invite the bot with an OAuth2 link. Two permissions: View Channel, Read Message History.
3. In their announcement channel, `/circinus link <collection link>`. The bot checks the invoking
   Discord account against the Steam account that owns that collection, and saves the mapping.

**Publishing is a slash command, not a channel mirror.** `/announce <text>`, with a preview and a
Confirm before it goes out. Two reasons, and the second is the one that decides it:

- Reading arbitrary message content needs Discord's **Message Content** privileged intent. Under
  this shape the bot joins every curator's server, so it crosses Discord's 100-server line and
  the intent needs a verification review. Slash-command payloads arrive directly and need no
  privileged intent at any scale, so the review never happens.
- A channel that quietly ships everything said in it to a mod manager is a footgun. Somebody
  chatting in their own announcements channel does not expect it inside somebody else's app.
  Publishing should be an act.

The alternative shape — curators using Discord's Follow feature to push an announcement channel
into a Circinus-owned server — was considered and is a fair fallback: it keeps the bot in exactly
one server, which stays under the 100-server line without a slash command. It costs a server to
host and moderate, and it was not the shape chosen.

## Things the site has to get right

- **Revocation.** A curator leaving, a channel deleted, a collection changing hands. A pack whose
  mapping is gone should serve an empty list, not the last thing that was said.
- **A takedown route.** Circinus is showing this text inside its own window. There has to be a
  way to remove a post that should not be there, and it has to be faster than an app release.
- **Rate limits.** Every copy of the app asks once per followed pack every six hours. That is
  cheap per user and not cheap in aggregate; cache hard, and prefer a CDN in front of it.
