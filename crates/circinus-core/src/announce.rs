//! What a modpack's curator wants to tell the people running their pack.
//!
//! A collection tells you *what* changed — these mods arrived, those left. It cannot tell you
//! why, or that you should remove a mod before loading a save, or that 1.6 broke something and
//! the fix is coming Tuesday. Curators say all of that on Discord, and a player who follows the
//! pack in Circinus and not on Discord never hears it.
//!
//! So the app reads a feed, per followed collection, from circinus.sh. Deliberately not from
//! Discord: the app has no Discord account, no token, and no business holding one. Whatever
//! writes the feed on the far side — a bot, a form on the site, something not invented yet — is
//! the site's problem and can change without an app release. `docs/announcements.md` is the
//! contract between the two.
//!
//! Everything here treats "the site does not serve this" as *no announcements*, never as an
//! error. The endpoint does not exist yet at the time of writing, and a mod manager that opens
//! with a red banner because a feature has not shipped on the server is worse than one that
//! quietly has nothing to say.

use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::weight::{API_BASE, USER_AGENT};

/// One thing a curator said, about one pack.
///
/// `author` is the curator's own display name and is shown as the author on screen: this text is
/// somebody else's, and presenting it as Circinus talking would be a lie about who is asking the
/// player to do something.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Announcement {
    /// The site's own id for this post, stable across fetches; used to tell two posts apart when
    /// they share a second.
    pub id: String,
    /// The Workshop collection this belongs to.
    pub pack: u64,
    /// Unix seconds. What "unread" is measured against.
    pub at: i64,
    pub author: String,
    pub text: String,
    /// Somewhere to read more. Shown as a link, never followed by the app.
    pub link: Option<String>,
}

/// The longest post the app will show. A curator with more to say than this has a link.
///
/// Not a protocol limit — a rendering one. Something has to decide, and deciding here means one
/// answer rather than one per component, and means the cap is tested.
pub const MAX_TEXT: usize = 2000;

fn text_of(v: &Value, key: &str) -> String {
    v.get(key).and_then(|x| x.as_str()).unwrap_or("").trim().to_string()
}

fn seconds(v: &Value, key: &str) -> i64 {
    match v.get(key) {
        Some(Value::Number(n)) => n.as_i64().unwrap_or(0),
        // ISO 8601 is what a web backend reaches for first, so take it as well as a number
        // rather than making the site's choice of clock the reason nothing shows.
        Some(Value::String(s)) => s.parse::<i64>().unwrap_or_else(|_| iso_seconds(s).unwrap_or(0)),
        _ => 0,
    }
}

/// `2026-09-10T01:35:00Z` → unix seconds. Only the shape a JSON API actually emits; anything
/// else is nothing, which sorts as oldest rather than as an error.
fn iso_seconds(s: &str) -> Option<i64> {
    let (date, rest) = s.split_once('T')?;
    let time = rest.trim_end_matches('Z').split(['+', '-']).next()?;
    let mut d = date.split('-');
    let (y, m, day) = (d.next()?.parse::<i64>().ok()?, d.next()?.parse::<i64>().ok()?, d.next()?.parse::<i64>().ok()?);
    let mut t = time.split(':');
    let (hh, mm) = (t.next()?.parse::<i64>().ok()?, t.next()?.parse::<i64>().ok()?);
    let ss = t.next().and_then(|x| x.split('.').next()?.parse::<i64>().ok()).unwrap_or(0);
    // Days since the epoch by the civil-from-days algorithm, so this needs no date library.
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = if y2 >= 0 { y2 } else { y2 - 399 } / 400;
    let yoe = y2 - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    Some(days * 86400 + hh * 3600 + mm * 60 + ss)
}

/// Read one entry. An entry with no text is not an announcement, however well formed the rest of
/// it is, and is dropped rather than drawn as an empty card.
pub fn parse_one(v: &Value, pack: u64) -> Option<Announcement> {
    let mut text = text_of(v, "text");
    if text.is_empty() {
        text = text_of(v, "content");
    }
    if text.is_empty() {
        return None;
    }
    if text.chars().count() > MAX_TEXT {
        text = text.chars().take(MAX_TEXT).collect::<String>() + "…";
    }
    let at = match seconds(v, "at") {
        0 => seconds(v, "createdAt"),
        n => n,
    };
    let link = v.get("link").and_then(|x| x.as_str()).map(str::trim).filter(|s| s.starts_with("https://")).map(str::to_string);
    Some(Announcement {
        id: match text_of(v, "id") {
            s if s.is_empty() => format!("{pack}:{at}"),
            s => s,
        },
        pack: v.get("pack").and_then(|x| x.as_u64()).unwrap_or(pack),
        at,
        author: match text_of(v, "author") {
            s if s.is_empty() => text_of(v, "authorName"),
            s => s,
        },
        text,
        link,
    })
}

/// The list out of a response, whichever of the two obvious shapes the site picked.
pub fn parse_list(v: &Value, pack: u64) -> Vec<Announcement> {
    let items = match v {
        Value::Array(a) => a.clone(),
        _ => v.get("announcements").or_else(|| v.get("items")).and_then(|x| x.as_array()).cloned().unwrap_or_default(),
    };
    let mut out: Vec<Announcement> = items.iter().filter_map(|i| parse_one(i, pack)).collect();
    // Newest first, and by id where a curator posted twice in one second, so the order is the
    // same on every fetch rather than however the server happened to serialise it.
    out.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| b.id.cmp(&a.id)));
    out
}

/// Everything one pack's curator has said since `since` (unix seconds; 0 for everything the
/// site will give).
///
/// `Ok(vec![])` covers every way the site can decline: no such pack, the endpoint not built yet,
/// a gateway between here and it. A curator's note is not worth an error in front of somebody
/// trying to launch a game.
pub async fn fetch_pack(client: &reqwest::Client, pack: u64, since: i64) -> Result<Vec<Announcement>> {
    let url = format!("{API_BASE}/packs/{pack}/announcements?since={}", since.max(0));
    let resp = match client.get(&url).header(reqwest::header::USER_AGENT, USER_AGENT).send().await {
        Ok(r) => r,
        Err(_) => return Ok(Vec::new()),
    };
    if !resp.status().is_success() {
        return Ok(Vec::new());
    }
    match resp.json::<Value>().await {
        Ok(v) => Ok(parse_list(&v, pack)),
        Err(_) => Ok(Vec::new()),
    }
}

/// Every followed pack, one call each.
///
/// Sequential on purpose. This runs on a six hour timer behind everything else the user is
/// doing, so there is nothing to be gained by opening twenty connections at once, and a curator
/// feed is not worth looking like a burst of traffic to anybody's rate limiter.
pub async fn fetch_all(client: &reqwest::Client, packs: &[(u64, i64)]) -> Vec<Announcement> {
    let mut out = Vec::new();
    for &(pack, since) in packs {
        if let Ok(mut got) = fetch_pack(client, pack, since).await {
            out.append(&mut got);
        }
    }
    out.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| b.id.cmp(&a.id)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_the_shape_the_contract_describes() {
        let v = json!({"announcements": [
            {"id": "a1", "at": 1_757_000_000i64, "author": "Oskar", "text": "Remove Foo before loading a save.", "link": "https://example.com/x"},
            {"id": "a2", "at": 1_757_000_500i64, "author": "Oskar", "text": "1.6 patch is up."}
        ]});
        let got = parse_list(&v, 42);
        assert_eq!(got.len(), 2);
        // Newest first, whatever order the server sent.
        assert_eq!(got[0].id, "a2");
        assert_eq!(got[0].pack, 42);
        assert_eq!(got[1].link.as_deref(), Some("https://example.com/x"));
    }

    #[test]
    fn a_bare_array_works_too() {
        let v = json!([{"id": "x", "at": 1, "author": "A", "text": "hello there"}]);
        assert_eq!(parse_list(&v, 7).len(), 1);
    }

    #[test]
    fn an_entry_with_nothing_to_say_is_not_an_announcement() {
        let v = json!({"items": [{"id": "x", "at": 1, "author": "A", "text": "   "}, {"id": "y", "at": 2, "author": "A"}]});
        assert!(parse_list(&v, 7).is_empty());
    }

    #[test]
    fn a_date_is_taken_as_a_number_or_as_iso() {
        let a = json!({"id": "a", "at": 1_757_000_000i64, "text": "x y"});
        let b = json!({"id": "b", "at": "2025-09-04T15:33:20Z", "text": "x y"});
        assert_eq!(parse_one(&a, 1).unwrap().at, parse_one(&b, 1).unwrap().at);
    }

    #[test]
    fn iso_matches_known_instants() {
        assert_eq!(iso_seconds("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(iso_seconds("2000-03-01T00:00:00Z"), Some(951_868_800));
        assert_eq!(iso_seconds("2026-09-10T01:35:00Z"), Some(1_789_004_100));
    }

    #[test]
    fn a_link_that_is_not_https_is_dropped() {
        // The text is somebody else's; a link out of it is followed by a person, in a browser,
        // and `javascript:` or `file:` has no business being offered as one.
        for bad in ["javascript:alert(1)", "file:///etc/passwd", "http://plain", "not a url"] {
            let v = json!({"id": "a", "at": 1, "text": "x y", "link": bad});
            assert_eq!(parse_one(&v, 1).unwrap().link, None, "{bad} should not survive");
        }
    }

    #[test]
    fn a_very_long_post_is_cut_rather_than_drawn_forever() {
        let v = json!({"id": "a", "at": 1, "text": "x".repeat(MAX_TEXT + 500)});
        let got = parse_one(&v, 1).unwrap();
        assert_eq!(got.text.chars().count(), MAX_TEXT + 1, "cut to the cap plus the ellipsis");
    }

    #[test]
    fn an_id_is_made_up_when_the_site_omits_one() {
        let v = json!({"at": 99, "text": "x y"});
        assert_eq!(parse_one(&v, 5).unwrap().id, "5:99");
    }

    #[test]
    fn nothing_served_is_nothing_to_say() {
        // Whatever the site sends that is not a feed, the answer is an empty list.
        for v in [json!({}), json!({"error": "not found"}), json!("no"), json!(null)] {
            assert!(parse_list(&v, 1).is_empty());
        }
    }
}
