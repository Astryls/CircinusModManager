//! Subscribing and unsubscribing through the Steam client.
//!
//! Circinus opens the item's page in Steam and says so; it does not claim the mod is subscribed,
//! because pressing the button is the user's move (see `circinus_core::steam::client` for why the
//! Steamworks API is not used). What Circinus can do afterwards is watch Steam's own record of
//! installed Workshop items and report the moment the mod really arrives or really goes, which is
//! what `watch` below does: a poll of `appworkshop_294100.acf` for a few minutes, an event each
//! time something changes, and a rescan when the folders settle.

use crate::commands::Shared;
use circinus_core::paths::Locations;
use circinus_core::steam::client::{self, ClientStatus, ItemState, ItemSubscription};
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter, State};

type CmdResult<T> = std::result::Result<T, String>;

/// How long to keep watching for the item to arrive (or go) after the pages were opened, and how
/// often to look. Steam needs a moment to download; three minutes covers a small mod on a normal
/// connection without holding a task open all session.
const WATCH_FOR: Duration = Duration::from_secs(180);
const WATCH_EVERY: Duration = Duration::from_secs(4);

/// What a subscribe or unsubscribe request actually did. `opened` means a page was put in front
/// of the user, not that anything was subscribed.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribeOutcome {
    pub opened: Vec<u64>,
    pub skipped: Vec<(u64, String)>,
    /// Pages Steam or the desktop refused to open, with the reason.
    pub failed: Vec<(u64, String)>,
    /// One sentence about what happens next.
    pub note: String,
    pub client: ClientStatus,
    /// Every id asked about, as Steam records it right now.
    pub states: Vec<ItemSubscription>,
}

/// The states of some items, and whether Steam has finished the job we are waiting for.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionProgress {
    pub states: Vec<ItemSubscription>,
    /// True when every watched item reached the state that was asked for.
    pub settled: bool,
    /// "subscribe" or "unsubscribe": what was asked for.
    pub want: &'static str,
}

fn locations(state: &State<'_, Shared>) -> CmdResult<Locations> {
    state.inner().lock().map(|app| app.locations.clone()).map_err(|_| "state lock poisoned".to_string())
}

/// Is the Steam client here, is it running, and can its record of Workshop items be read.
#[tauri::command]
pub async fn steam_client_status(state: State<'_, Shared>) -> CmdResult<ClientStatus> {
    let loc = locations(&state)?;
    tauri::async_runtime::spawn_blocking(move || client::status(&loc)).await.map_err(|e| e.to_string())
}

/// What Steam says about these items right now: subscribed, installed some other way, or absent.
#[tauri::command]
pub async fn subscription_state(state: State<'_, Shared>, ids: Vec<u64>) -> CmdResult<Vec<ItemSubscription>> {
    let loc = locations(&state)?;
    tauri::async_runtime::spawn_blocking(move || client::states_for(&loc, &ids)).await.map_err(|e| e.to_string())
}

/// The Workshop ids of mods in the list that are not installed, and the packageIds no Steam id
/// could be found for. The banner needs these to offer subscribing to them.
#[tauri::command]
pub async fn missing_workshop_ids(state: State<'_, Shared>) -> CmdResult<(Vec<u64>, Vec<String>)> {
    let shared = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || shared.lock().map(|app| app.workshop_ids_for_missing()).map_err(|_| "state lock poisoned".to_string()))
        .await
        .map_err(|e| e.to_string())?
}

/// Open the Steam page for each item so the user can subscribe, then watch for them arriving.
#[tauri::command]
pub async fn subscribe_items(app_handle: AppHandle, state: State<'_, Shared>, ids: Vec<u64>) -> CmdResult<SubscribeOutcome> {
    act(app_handle, state, ids, true).await
}

/// The same page, for unsubscribing: Steam deletes the folder when the user presses the button.
#[tauri::command]
pub async fn unsubscribe_items(app_handle: AppHandle, state: State<'_, Shared>, ids: Vec<u64>) -> CmdResult<SubscribeOutcome> {
    act(app_handle, state, ids, false).await
}

async fn act(handle: AppHandle, state: State<'_, Shared>, ids: Vec<u64>, subscribe: bool) -> CmdResult<SubscribeOutcome> {
    if ids.is_empty() {
        return Err("No Workshop ids were given".into());
    }
    let loc = locations(&state)?;
    let (status, states) = {
        let loc = loc.clone();
        let ids = ids.clone();
        tauri::async_runtime::spawn_blocking(move || (client::status(&loc), client::states_for(&loc, &ids))).await.map_err(|e| e.to_string())?
    };
    let plan = if subscribe { client::plan_subscribe(&status, &states) } else { client::plan_unsubscribe(&status, &states) };

    let mut opened = Vec::new();
    let mut failed = Vec::new();
    for (id, url) in &plan.open {
        match tauri_plugin_opener::open_url(url, None::<&str>) {
            Ok(()) => opened.push(*id),
            Err(e) => failed.push((*id, format!("could not open {url}: {e}"))),
        }
        // Steam drops links that arrive on top of each other; give it a moment between pages.
        tokio::time::sleep(Duration::from_millis(400)).await;
    }
    if !opened.is_empty() {
        watch(handle, state.inner().clone(), loc, opened.clone(), subscribe);
    }
    let note = if opened.is_empty() && !failed.is_empty() { "Nothing opened. Steam links are not handled on this machine.".to_string() } else { plan.note.clone() };
    Ok(SubscribeOutcome { opened, skipped: plan.skipped, failed, note, client: status, states })
}

/// Poll Steam's workshop record until the watched items reach the state that was asked for, or
/// the watch runs out. Every change is published as `subscription-changed`; when everything has
/// settled the mod folders are rescanned so the new mod appears in the list on its own.
fn watch(handle: AppHandle, shared: Shared, loc: Locations, ids: Vec<u64>, subscribe: bool) {
    let want = if subscribe { "subscribe" } else { "unsubscribe" };
    tauri::async_runtime::spawn(async move {
        let mut last = client::states_for(&loc, &ids);
        let deadline = std::time::Instant::now() + WATCH_FOR;
        while std::time::Instant::now() < deadline {
            tokio::time::sleep(WATCH_EVERY).await;
            let now = client::states_for(&loc, &ids);
            let settled = now.iter().all(|s| if subscribe { s.state == ItemState::Subscribed } else { s.state != ItemState::Subscribed });
            if now != last {
                let _ = handle.emit("subscription-changed", SubscriptionProgress { states: now.clone(), settled, want });
                last = now;
            }
            if settled {
                // Steam has moved folders around; the list should show what is really there.
                let st = shared.clone();
                let h = handle.clone();
                tauri::async_runtime::spawn_blocking(move || crate::run_scan(h, st, false));
                return;
            }
        }
        let states = client::states_for(&loc, &ids);
        let _ = handle.emit("subscription-changed", SubscriptionProgress { states, settled: false, want });
    });
}

#[cfg(test)]
mod tests {
    use circinus_core::steam::client::{self, ClientStatus};

    /// The command path with no Steam client: pages still open (on the web), and the sentence the
    /// user reads says what that does and does not achieve.
    #[test]
    fn no_client_is_explained_not_swallowed() {
        let status = ClientStatus::new(false, false, false, None);
        let states = client::item_states(None, None, None, &[2009463077]);
        let plan = client::plan_subscribe(&status, &states);
        assert_eq!(plan.open.len(), 1);
        assert!(plan.open[0].1.starts_with("https://"));
        assert!(plan.note.contains("browser"));
        // And unsubscribing from something Steam never had is refused with a reason.
        let plan = client::plan_unsubscribe(&status, &states);
        assert!(plan.open.is_empty());
        assert!(plan.skipped[0].1.contains("nothing to unsubscribe from"));
    }
}
