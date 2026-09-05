//! The Steam client half of Workshop items: whether the client is here and running, what state
//! an item is in according to Steam's own records, and the links that ask the client to act.
//!
//! Why links rather than the Steamworks API. There are two ways to subscribe from outside Steam.
//! `ISteamUGC::SubscribeItem` does it without the user pressing anything, but it needs Valve's
//! redistributable (`steam_api64.dll`/`.so`) shipped alongside Circinus, the client running and
//! logged in as the account that owns RimWorld, and a helper process linked against an SDK we
//! may not redistribute freely. It also fails silently in several ways — no client, wrong
//! account, the item not visible to that account — each of which has to be told apart and
//! explained. `steam://url/CommunityFilePage/<id>` opens the item's page in the client the user
//! is already logged into, works on every platform with nothing installed, and is honest about
//! what it achieves: it puts the Subscribe button in front of the user and nothing more. So
//! Circinus opens pages, says so plainly, and then watches Steam's own record of installed items
//! to find out whether anything actually happened. `steam://subscribe/...` handling exists in
//! some client builds but is undocumented and silently ignored in others, so it is not used.

use crate::model::RIMWORLD_APP_ID;
use crate::paths::Locations;
use crate::steam::acf;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Steam opens one window per page, so a request for many mods is spread over several rounds
/// rather than burying the user under twenty windows.
pub const MAX_PAGES: usize = 8;

/// Where one Workshop item stands, from Steam's records and the folders on disk rather than
/// from anything Circinus asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ItemState {
    /// Steam lists it in `appworkshop_294100.acf`: it came from a subscription and Steam keeps
    /// it updated.
    Subscribed,
    /// The folder is on disk but Steam does not list it — a SteamCMD download or a hand-copied
    /// mod. Nothing updates it.
    Installed,
    /// Neither Steam nor the mod folders have it.
    Absent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemSubscription {
    pub id: u64,
    pub state: ItemState,
    /// Steam's `timeupdated` for a subscribed item, when it records one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_updated: Option<u64>,
    /// The folder holding the files, when one exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

/// What Circinus can and cannot do about subscriptions on this machine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientStatus {
    /// A Steam installation was found.
    pub installed: bool,
    /// The client is running right now.
    pub running: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steam_dir: Option<String>,
    /// Steam's record of RimWorld Workshop items was found; without it subscriptions cannot be
    /// read back.
    pub records_found: bool,
    /// One plain sentence for the UI.
    pub detail: String,
}

impl ClientStatus {
    pub fn new(installed: bool, running: bool, records_found: bool, steam_dir: Option<PathBuf>) -> ClientStatus {
        ClientStatus { installed, running, records_found, steam_dir: steam_dir.map(|p| p.display().to_string()), detail: describe(installed, running, records_found) }
    }
}

/// The sentence the UI shows for a given state of affairs. Kept apart from the detection so it
/// can be read and tested on its own.
pub fn describe(installed: bool, running: bool, records_found: bool) -> String {
    if !installed {
        return "No Steam installation was found on this machine. Circinus can open the Workshop page in your browser, but subscribing needs Steam. SteamCMD downloads do not.".into();
    }
    if !running {
        return "Steam is installed but not running. Opening a Workshop link starts it, which takes a moment.".into();
    }
    if !records_found {
        return "Steam is running. Circinus cannot find its record of RimWorld Workshop items yet, so it cannot say which mods you are subscribed to until one arrives.".into();
    }
    "Steam is running, so a Workshop link opens straight away.".into()
}

/// Look the client up: an installation, a live process, and Steam's workshop record.
pub fn status(loc: &Locations) -> ClientStatus {
    let dir = steam_dir();
    let records = loc.workshop_acf().map(|p| p.is_file()).unwrap_or(false);
    ClientStatus::new(dir.is_some(), is_running(), records, dir)
}

/// Steam's own folder, when it can be found the way `paths.rs` finds the game.
pub fn steam_dir() -> Option<PathBuf> {
    steamlocate::SteamDir::locate().ok().map(|s| s.path().to_path_buf())
}

// ---------------------------------------------------------------- is it running

/// Whether the Steam client is running. Steam has no cross-platform way of saying so, so each
/// platform is asked in the way it can answer without a new dependency.
#[cfg(target_os = "windows")]
pub fn is_running() -> bool {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW: the user must not see a console flash for a status check.
    std::process::Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq steam.exe", "/NH", "/FO", "CSV"])
        .creation_flags(0x0800_0000)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_ascii_lowercase().contains("steam.exe"))
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
pub fn is_running() -> bool {
    // Steam writes its process id where its own scripts look for it; `/proc` says whether that
    // process is still alive. Flatpak and other layouts fall back to the process list.
    if let Some(home) = dirs::home_dir() {
        for rel in [".steam/steam.pid", ".steam/root/steam.pid", ".var/app/com.valvesoftware.Steam/.steam/steam.pid"] {
            if let Some(pid) = std::fs::read_to_string(home.join(rel)).ok().and_then(|t| t.trim().parse::<u32>().ok()) {
                if Path::new(&format!("/proc/{pid}")).is_dir() {
                    return true;
                }
            }
        }
    }
    pgrep("steam")
}

#[cfg(target_os = "macos")]
pub fn is_running() -> bool {
    pgrep("steam_osx") || pgrep("Steam")
}

#[cfg(all(unix, not(target_os = "windows")))]
fn pgrep(name: &str) -> bool {
    std::process::Command::new("pgrep").arg("-x").arg(name).output().map(|o| o.status.success() && !o.stdout.is_empty()).unwrap_or(false)
}

// ---------------------------------------------------------------- links

/// The item's page inside the Steam client, where Subscribe and Unsubscribe live.
pub fn community_file_page_url(id: u64) -> String {
    format!("steam://url/CommunityFilePage/{id}")
}

/// The same page on the web, for machines without the client.
pub fn web_page_url(id: u64) -> String {
    format!("https://steamcommunity.com/sharedfiles/filedetails/?id={id}")
}

/// RimWorld's Workshop section in the client.
pub fn workshop_page_url() -> String {
    format!("steam://url/SteamWorkshopPage/{RIMWORLD_APP_ID}")
}

/// Steam's downloads page, where a fresh subscription shows its progress.
pub fn downloads_page_url() -> String {
    "steam://open/downloads".to_string()
}

/// The page to open for an item: the client's when Steam is installed, the web page when it is
/// not, because a `steam://` link on a machine without Steam does nothing at all.
pub fn page_url(id: u64, steam_installed: bool) -> String {
    if steam_installed {
        community_file_page_url(id)
    } else {
        web_page_url(id)
    }
}

// ---------------------------------------------------------------- item state

/// Work out the state of each id from Steam's workshop record and the folders on disk.
pub fn item_states(acf_text: Option<&str>, workshop_dir: Option<&Path>, mods_dir: Option<&Path>, ids: &[u64]) -> Vec<ItemSubscription> {
    let installed: std::collections::HashMap<u64, Option<u64>> = acf_text.map(|t| acf::installed_items(t).into_iter().collect()).unwrap_or_default();
    ids.iter()
        .map(|id| {
            let folder = [workshop_dir, mods_dir].into_iter().flatten().map(|d| d.join(id.to_string())).find(|p| p.is_dir());
            let (state, time_updated) = match installed.get(id) {
                // Steam lists it, so Steam owns it and keeps it current — even if the folder was
                // moved away underneath, which is what "subscribed but no folder" means.
                Some(t) => (ItemState::Subscribed, *t),
                None if folder.is_some() => (ItemState::Installed, None),
                None => (ItemState::Absent, None),
            };
            ItemSubscription { id: *id, state, time_updated, path: folder.map(|p| p.display().to_string()) }
        })
        .collect()
}

/// The same, reading Steam's record from the configured locations.
pub fn states_for(loc: &Locations, ids: &[u64]) -> Vec<ItemSubscription> {
    let text = loc.workshop_acf().and_then(|p| std::fs::read_to_string(p).ok());
    item_states(text.as_deref(), loc.workshop_dir.as_deref(), loc.local_mods_dir.as_deref(), ids)
}

// ---------------------------------------------------------------- planning

/// What opening pages would actually achieve, decided before anything opens so the UI can say
/// what it is about to do and what it will not do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenPlan {
    /// (id, url) for the pages to open, in order.
    pub open: Vec<(u64, String)>,
    /// (id, why) for everything else.
    pub skipped: Vec<(u64, String)>,
    /// What the user should expect, in one sentence.
    pub note: String,
}

/// Pages to open for subscribing, and what is being left alone.
pub fn plan_subscribe(status: &ClientStatus, states: &[ItemSubscription]) -> OpenPlan {
    let mut open = Vec::new();
    let mut skipped = Vec::new();
    for s in states {
        if s.state == ItemState::Subscribed {
            skipped.push((s.id, "Steam already has this one".to_string()));
        } else if open.len() < MAX_PAGES {
            open.push((s.id, page_url(s.id, status.installed)));
        } else {
            skipped.push((s.id, format!("not opened: Steam gets {MAX_PAGES} pages at a time. Ask again for the rest")));
        }
    }
    OpenPlan { note: note_for(status, open.len(), true), open, skipped }
}

/// Pages to open for unsubscribing. A copy Steam did not put there is left alone: Steam has
/// nothing to remove, and saying so is better than opening a page that will not help.
pub fn plan_unsubscribe(status: &ClientStatus, states: &[ItemSubscription]) -> OpenPlan {
    let mut open = Vec::new();
    let mut skipped = Vec::new();
    for s in states {
        match s.state {
            ItemState::Absent => skipped.push((s.id, "Steam does not list this one, so there is nothing to unsubscribe from".to_string())),
            ItemState::Installed => skipped.push((s.id, "this copy did not come from a subscription, so Steam has nothing to remove. Delete the folder instead".to_string())),
            ItemState::Subscribed if open.len() < MAX_PAGES => open.push((s.id, page_url(s.id, status.installed))),
            ItemState::Subscribed => skipped.push((s.id, format!("not opened: Steam gets {MAX_PAGES} pages at a time. Ask again for the rest"))),
        }
    }
    OpenPlan { note: note_for(status, open.len(), false), open, skipped }
}

fn note_for(status: &ClientStatus, opening: usize, subscribing: bool) -> String {
    if opening == 0 {
        return "Nothing to open.".to_string();
    }
    let page = if opening == 1 { "the page".to_string() } else { format!("{opening} pages") };
    if !status.installed {
        return format!("No Steam installation was found, so {page} open in your browser. Subscribing there still needs Steam to download the mod.");
    }
    let start = if status.running { String::new() } else { "Steam is not running, so it starts first. ".to_string() };
    if subscribing {
        format!("{start}Steam has been asked for {page}. Press Subscribe on each; the mod appears here once Steam has downloaded it.")
    } else {
        format!("{start}Steam has been asked for {page}. Press Unsubscribe on each; Steam then deletes the mod's folder.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ACF: &str = r#""AppWorkshop"
{
	"appid"		"294100"
	"WorkshopItemsInstalled"
	{
		"2009463077"
		{
			"size"		"100"
			"timeupdated"		"1700000000"
		}
		"818773962"
		{
			"size"		"200"
		}
	}
}
"#;

    fn status(installed: bool, running: bool) -> ClientStatus {
        ClientStatus::new(installed, running, installed, None)
    }

    #[test]
    fn reads_subscription_state_from_the_acf() {
        let tmp = tempfile::tempdir().unwrap();
        let workshop = tmp.path().join("content");
        let mods = tmp.path().join("Mods");
        std::fs::create_dir_all(workshop.join("2009463077")).unwrap();
        std::fs::create_dir_all(mods.join("3333333333")).unwrap();
        let states = item_states(Some(ACF), Some(&workshop), Some(&mods), &[2009463077, 818773962, 3333333333, 4444444444]);
        assert_eq!(states[0].state, ItemState::Subscribed);
        assert_eq!(states[0].time_updated, Some(1700000000));
        assert!(states[0].path.is_some());
        // Steam lists it but the folder was moved away: still a subscription.
        assert_eq!(states[1].state, ItemState::Subscribed);
        assert_eq!(states[1].time_updated, None);
        assert_eq!(states[1].path, None);
        // A SteamCMD or hand-copied mod: on disk, but nothing keeps it current.
        assert_eq!(states[2].state, ItemState::Installed);
        assert_eq!(states[3].state, ItemState::Absent);
        // No record at all: nothing is claimed to be subscribed.
        let none = item_states(None, None, None, &[2009463077]);
        assert_eq!(none[0].state, ItemState::Absent);
    }

    #[test]
    fn builds_the_links() {
        assert_eq!(community_file_page_url(2009463077), "steam://url/CommunityFilePage/2009463077");
        assert_eq!(web_page_url(2009463077), "https://steamcommunity.com/sharedfiles/filedetails/?id=2009463077");
        assert_eq!(workshop_page_url(), "steam://url/SteamWorkshopPage/294100");
        assert_eq!(downloads_page_url(), "steam://open/downloads");
        assert_eq!(page_url(1, true), "steam://url/CommunityFilePage/1");
        assert_eq!(page_url(1, false), "https://steamcommunity.com/sharedfiles/filedetails/?id=1");
        // The id round-trips through the parser the Import dialog uses.
        assert_eq!(crate::steam::webapi::parse_workshop_id(&community_file_page_url(818773962)), Some(818773962));
    }

    #[test]
    fn plans_say_what_will_and_will_not_happen() {
        let states = item_states(Some(ACF), None, None, &[2009463077, 5, 6]);
        let plan = plan_subscribe(&status(true, true), &states);
        assert_eq!(plan.open.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![5, 6]);
        assert_eq!(plan.skipped, vec![(2009463077, "Steam already has this one".to_string())]);
        assert!(plan.note.starts_with("Steam has been asked for 2 pages"), "{}", plan.note);

        // Only what Steam put there can be unsubscribed from.
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("7")).unwrap();
        let states = item_states(Some(ACF), Some(tmp.path()), None, &[2009463077, 7, 8]);
        let plan = plan_unsubscribe(&status(true, true), &states);
        assert_eq!(plan.open.iter().map(|(id, _)| *id).collect::<Vec<_>>(), vec![2009463077]);
        assert!(plan.skipped[0].1.contains("did not come from a subscription"));
        assert!(plan.skipped[1].1.contains("nothing to unsubscribe from"));

        // More than one round's worth: the rest is named, not silently dropped.
        let many: Vec<u64> = (100..100 + MAX_PAGES as u64 + 3).collect();
        let plan = plan_subscribe(&status(true, true), &item_states(None, None, None, &many));
        assert_eq!(plan.open.len(), MAX_PAGES);
        assert_eq!(plan.skipped.len(), 3);
    }

    #[test]
    fn says_plainly_when_the_client_is_not_there() {
        // Not running: the link still works, it just starts Steam first.
        let s = status(true, false);
        assert!(s.detail.contains("not running"));
        let plan = plan_subscribe(&s, &item_states(None, None, None, &[5]));
        assert_eq!(plan.open[0].1, "steam://url/CommunityFilePage/5");
        assert!(plan.note.starts_with("Steam is not running, so it starts first."), "{}", plan.note);

        // Not installed at all: a steam:// link would do nothing, so the web page is opened and
        // the sentence says what that does and does not achieve.
        let s = ClientStatus::new(false, false, false, None);
        assert!(s.detail.contains("No Steam installation"));
        let plan = plan_subscribe(&s, &item_states(None, None, None, &[5]));
        assert_eq!(plan.open[0].1, "https://steamcommunity.com/sharedfiles/filedetails/?id=5");
        assert!(plan.note.contains("open in your browser"), "{}", plan.note);
        assert!(describe(true, true, false).contains("cannot find its record"));

        // Nothing to do is said as such, not dressed up as success.
        let plan = plan_unsubscribe(&status(true, true), &item_states(None, None, None, &[5]));
        assert!(plan.open.is_empty());
        assert_eq!(plan.note, "Nothing to open.");
    }
}
