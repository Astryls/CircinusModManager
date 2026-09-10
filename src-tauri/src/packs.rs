//! Keep followed modpacks current, on a slow timer.
//!
//! Two things about a followed collection go stale on their own: what it holds, and what its
//! curator has said. Both were manual — `items` only moved when somebody pressed "Check for
//! changes", which meant a curator could add ten mods and Circinus would sit there insisting
//! nothing had happened until the day the user happened to press a button in the sidebar.
//!
//! So this asks, quietly, on a timer. Deliberately unlike `watch.rs`, which polls four local
//! mtimes every eight seconds because a stat call costs nothing and Steam can rewrite a mod
//! folder while you are looking at it. This one goes over the network to two services that owe
//! us nothing, for a signal that moves maybe weekly, so it is slow, sequential, and silent about
//! every way it can fail.
//!
//! It is also quiet in the other sense. `watch.rs` raises a desktop notification, because a mod
//! changing under a running game is something to interrupt for. A curator's note is not: it goes
//! in the banner, where it waits until somebody looks.

use crate::commands::Shared;
use crate::state::now_secs;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// After launch: behind the first scan, the update check and anything the window is doing.
const STARTUP_DELAY: Duration = Duration::from_secs(20);
/// Between rounds. A curator publishes on the order of weekly; four times a day is already
/// generous, and the interval is what keeps this from reading as chatter to Steam's API.
const INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

/// The ids worth asking about, and the ones whose curator has not been muted.
fn followed(st: &Shared) -> (Vec<u64>, Vec<(u64, i64)>) {
    let Ok(app) = st.lock() else { return (Vec::new(), Vec::new()) };
    let ids: Vec<u64> = app.user.collections.iter().map(|c| c.id).collect();
    let packs = app.user.collections.iter().filter(|c| !app.user.packs_muted.contains(&c.id)).map(|c| (c.id, 0i64)).collect();
    (ids, packs)
}

/// One round: what each collection holds now, and what its curator has said.
///
/// Nothing here returns an error. A round that fails is a round that changes nothing and is
/// tried again in six hours; there is no user waiting on it and nothing to tell them.
async fn round(handle: &AppHandle, st: &Shared) {
    let (ids, packs) = followed(st);
    if ids.is_empty() {
        return;
    }
    let Ok(client) = reqwest::Client::builder().user_agent(circinus_core::weight::USER_AGENT).timeout(Duration::from_secs(20)).build() else { return };

    // What the collections hold. `known` is never touched here: it is the user's mark for "I
    // have seen this", and moving it in the background is how a change gets silently swallowed
    // by the poll that found it.
    let details = circinus_core::steam::webapi::published_file_details(&client, &ids).await.unwrap_or_default();
    let mut fetched: Vec<(u64, Vec<u64>, std::collections::HashMap<u64, String>)> = Vec::new();
    for id in &ids {
        // Steam's own timestamp for the collection, which the app has always stored and never
        // read. If it has not moved since the last look, the contents cannot have changed, so
        // the expensive call is skipped entirely.
        let unchanged = match st.lock() {
            Ok(app) => app.user.collections.iter().find(|c| c.id == *id).is_some_and(|c| {
                c.checked_at > 0 && details.iter().find(|d| d.published_file_id == *id).is_some_and(|d| d.time_updated == c.time_updated)
            }),
            Err(_) => false,
        };
        if unchanged {
            continue;
        }
        if let Ok((items, names)) = crate::commands::fetch_collection(&client, *id).await {
            fetched.push((*id, items, names));
        }
    }

    let announcements = circinus_core::announce::fetch_all(&client, &packs).await;

    let changed = {
        let Ok(mut app) = st.lock() else { return };
        let now = now_secs();
        let before: Vec<(u64, usize)> = app.user.collections.iter().map(|c| (c.id, c.added().len() + c.removed().len())).collect();
        let before_posts = app.announcements.len();
        for (id, items, names) in fetched {
            if let Some(c) = app.user.collections.iter_mut().find(|c| c.id == id) {
                c.items = items;
                c.names.extend(names);
                c.checked_at = now;
            }
        }
        for d in &details {
            if let Some(c) = app.user.collections.iter_mut().find(|c| c.id == d.published_file_id) {
                c.name = d.title.clone();
                c.creator = d.creator.clone();
                c.time_updated = d.time_updated;
                if c.checked_at == 0 {
                    c.checked_at = now;
                }
            }
        }
        app.announcements = announcements;
        app.announcements_checked_at = now;
        let _ = app.persist();
        let after: Vec<(u64, usize)> = app.user.collections.iter().map(|c| (c.id, c.added().len() + c.removed().len())).collect();
        before != after || before_posts != app.announcements.len()
    };

    // Only when something moved: the window rebuilds a good deal on this event, and doing that
    // four times a day for no reason is four interruptions a user cannot see the point of.
    if changed {
        let _ = handle.emit("state-changed", ());
    }
}

pub fn start(handle: AppHandle, st: Shared) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(STARTUP_DELAY).await;
        loop {
            round(&handle, &st).await;
            tokio::time::sleep(INTERVAL).await;
        }
    });
}
