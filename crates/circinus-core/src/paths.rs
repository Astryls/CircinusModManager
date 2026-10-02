//! Where RimWorld, its config and its mods live on this machine.

use crate::model::RIMWORLD_APP_ID;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Resolved folders. Any of them may be overridden by the user in settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Locations {
    /// Folder containing Version.txt and Data/.
    pub game_dir: Option<PathBuf>,
    /// Folder containing ModsConfig.xml.
    pub config_dir: Option<PathBuf>,
    /// `<game>/Mods`.
    pub local_mods_dir: Option<PathBuf>,
    /// `<library>/steamapps/workshop/content/294100`.
    pub workshop_dir: Option<PathBuf>,
}

impl Locations {
    /// Best-effort autodetection; every field may still be None.
    pub fn detect() -> Locations {
        let mut loc = Locations::default();
        if let Some((game, workshop)) = detect_steam() {
            loc.game_dir = Some(game);
            loc.workshop_dir = Some(workshop);
        }
        // Steam's own record of itself (the registry on Windows) can be missing or belong to
        // another account — a first launch straight from an installer runs as whoever ran the
        // installer. The library list in the usual Steam folders says the same thing.
        if loc.game_dir.is_none() {
            if let Some((game, workshop)) = detect_from_library_files() {
                loc.game_dir = Some(game);
                loc.workshop_dir = Some(workshop);
            }
        }
        if loc.game_dir.is_none() {
            loc.game_dir = fallback_game_dirs().into_iter().map(|p| game_root(&p)).find(|p| p.join("Version.txt").is_file());
            // A game folder inside a Steam library has its Workshop content two folders over.
            loc.workshop_dir = loc.game_dir.as_deref().and_then(workshop_dir_for_game);
        }
        loc.config_dir = default_config_dir().filter(|p| p.is_dir());
        loc.fill_derived();
        loc
    }

    /// Derive local mods dir from the game dir when not set.
    pub fn fill_derived(&mut self) {
        if self.local_mods_dir.is_none() {
            if let Some(g) = &self.game_dir {
                let mods = mods_dir_for_game(g);
                self.local_mods_dir = Some(mods);
            }
        }
    }

    pub fn data_dir(&self) -> Option<PathBuf> {
        self.game_dir.as_ref().map(|g| data_dir_for_game(g))
    }

    pub fn mods_config_path(&self) -> Option<PathBuf> {
        self.config_dir.as_ref().map(|c| c.join("ModsConfig.xml"))
    }

    /// Steam's record of installed Workshop items: `<library>/steamapps/workshop/appworkshop_294100.acf`
    /// (two levels above the content folder).
    pub fn workshop_acf(&self) -> Option<PathBuf> {
        let ws = self.workshop_dir.as_ref()?;
        Some(ws.parent()?.parent()?.join(format!("appworkshop_{RIMWORLD_APP_ID}.acf")))
    }

    /// Steam's `timeupdated` per installed Workshop item; empty when the file is not there.
    pub fn workshop_updated(&self) -> std::collections::HashMap<u64, u64> {
        self.workshop_acf()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| crate::steam::acf::installed_items(&t).into_iter().filter_map(|(id, t)| t.map(|t| (id, t))).collect())
            .unwrap_or_default()
    }

    /// Whether the game folder is one. Asked of the resolved root, so a Mac player who could
    /// only pick the folder the bundle sits in is not told their choice is wrong.
    pub fn is_usable(&self) -> bool {
        self.game_dir.as_ref().map(|g| game_root(g).join("Version.txt").is_file()).unwrap_or(false)
    }
}

/// Whether the game folder sits inside a Steam library (`…/steamapps/common/RimWorld`).
pub fn is_steam_install(game: &Path) -> bool {
    // Split on both separators: settings written on Windows may be read anywhere.
    game.to_string_lossy().split(['/', '\\']).any(|c| c.eq_ignore_ascii_case("steamapps"))
}

/// The game's executable for a game folder: `RimWorldWin64.exe` on Windows, the `.app`
/// bundle on macOS, `RimWorldLinux` (or the launcher script) on Linux. None when nothing
/// recognisable is there (a GOG or DRM-free copy still uses these names).
pub fn detect_executable(game: &Path) -> Option<PathBuf> {
    let root = game_root(game);
    if root.extension().map(|e| e == "app").unwrap_or(false) {
        return Some(root);
    }
    let names: &[&str] = if cfg!(target_os = "windows") { &["RimWorldWin64.exe", "RimWorldWin.exe", "RimWorld.exe"] } else { &["RimWorldLinux", "RimWorldLinux.x86_64", "start_RimWorld.sh"] };
    names.iter().map(|n| root.join(n)).find(|p| p.is_file())
}

/// Split a command line the way a shell would for simple cases: whitespace separates,
/// single or double quotes group.
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut has = false;
    for c in s.chars() {
        match (quote, c) {
            (Some(q), ch) if ch == q => quote = None,
            (Some(_), ch) => cur.push(ch),
            (None, '"') | (None, '\'') => {
                quote = Some(c);
                has = true;
            }
            (None, ch) if ch.is_whitespace() => {
                if has || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            (None, ch) => cur.push(ch),
        }
    }
    if has || !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// The folder the game really is, given whatever the user was able to choose.
///
/// On macOS RimWorld is `RimWorldMac.app`, and the Finder treats a `.app` as a file rather than
/// a folder: a folder picker will not let anyone select one or look inside it. So the best a Mac
/// player can do is choose the folder the bundle sits in -- which is not the game, and nothing
/// under it holds Data or Mods. A folder holding exactly one `.app` therefore means that `.app`.
///
/// Not written as a macOS-only branch, although only macOS has app bundles. A rule that runs
/// nowhere but the platform we cannot run the tests on is a rule nobody checks; this one is
/// harmless where `.app` folders do not exist, and provable everywhere.
pub fn game_root(game: &Path) -> PathBuf {
    if game.extension().map(|e| e == "app").unwrap_or(false) {
        return game.to_path_buf();
    }
    // A folder that is already the game is left alone, whatever else is in it.
    if game.join("Version.txt").is_file() {
        return game.to_path_buf();
    }
    let Ok(entries) = std::fs::read_dir(game) else { return game.to_path_buf() };
    let apps: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir() && p.extension().map(|e| e == "app").unwrap_or(false)).collect();
    match apps.len() {
        1 => apps.into_iter().next().unwrap_or_else(|| game.to_path_buf()),
        // More than one: prefer the one that looks like RimWorld rather than guessing, and if
        // that does not single one out, leave the folder alone. A wrong game folder is worse
        // than none, because none of it says so.
        _ => apps.into_iter().find(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase().starts_with("rimworld")).unwrap_or(false)).unwrap_or_else(|| game.to_path_buf()),
    }
}

/// Where the game's own content lives: the folder holding Core and the DLC.
///
/// Both places are tried and the one that is really there wins, because the layout differs and
/// guessing it by platform got this wrong. On macOS `Contents/Resources/Data` exists too -- it
/// is Unity's, full of engine assets -- so preferring it by name pointed Circinus at a folder
/// that has no Core in it, which is exactly what "cannot see Core and the DLCs" looks like.
/// Core is the thing that tells the two apart, so Core is what is looked for.
pub fn data_dir_for_game(game: &Path) -> PathBuf {
    let root = game_root(game);
    let beside = root.join("Data");
    let inside = root.join("Contents").join("Resources").join("Data");
    if beside.join("Core").is_dir() {
        beside
    } else if inside.join("Core").is_dir() {
        inside
    } else if beside.is_dir() {
        beside
    } else if inside.is_dir() {
        inside
    } else {
        beside
    }
}

/// Where mods the player installed by hand live. Same two candidates, same rule: whichever is
/// really there, and the one beside the bundle when neither is (that is where the Finder's
/// "Show Package Contents" leads, and where every Mac instruction for installing a mod says).
pub fn mods_dir_for_game(game: &Path) -> PathBuf {
    let root = game_root(game);
    let beside = root.join("Mods");
    let inside = root.join("Contents").join("Resources").join("Mods");
    if beside.is_dir() {
        beside
    } else if inside.is_dir() {
        inside
    } else {
        beside
    }
}

fn detect_steam() -> Option<(PathBuf, PathBuf)> {
    let steam = steamlocate::SteamDir::locate().ok()?;
    let (app, library) = steam.find_app(RIMWORLD_APP_ID).ok().flatten()?;
    let game = library.resolve_app_dir(&app);
    let workshop = library.path().join("steamapps").join("workshop").join("content").join(RIMWORLD_APP_ID.to_string());
    // Steam installs `RimWorldMac.app` inside the app dir; `game_root` steps into it there and
    // changes nothing anywhere else.
    let game = game_root(&game);
    Some((game, workshop))
}

/// `<library>/steamapps/workshop/content/294100` for a game folder that sits in a Steam library
/// (`<library>/steamapps/common/RimWorld`); None for any other folder.
pub fn workshop_dir_for_game(game: &Path) -> Option<PathBuf> {
    let common = game.parent()?;
    let steamapps = common.parent()?;
    if !common.file_name().map(|n| n.eq_ignore_ascii_case("common")).unwrap_or(false) || !steamapps.file_name().map(|n| n.eq_ignore_ascii_case("steamapps")).unwrap_or(false) {
        return None;
    }
    Some(steamapps.join("workshop").join("content").join(RIMWORLD_APP_ID.to_string()))
}

/// Library roots listed in a `libraryfolders.vdf` (`"path"  "D:\\SteamLibrary"` lines), in order.
pub fn parse_library_folders(text: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("\"path\"") else { continue };
        let value = rest.trim().trim_matches('"').replace("\\\\", "\\");
        if !value.is_empty() {
            out.push(PathBuf::from(value));
        }
    }
    out
}

/// Where Steam usually lives; each is a library root itself and may list more in
/// `steamapps/libraryfolders.vdf`.
fn steam_roots() -> Vec<PathBuf> {
    let mut v = Vec::new();
    #[cfg(target_os = "windows")]
    {
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Ok(p) = std::env::var(var) {
                v.push(PathBuf::from(p).join("Steam"));
            }
        }
        v.push(PathBuf::from("C:\\Program Files (x86)\\Steam"));
        v.push(PathBuf::from("C:\\Program Files\\Steam"));
        for drive in ["C", "D", "E", "F", "G", "H"] {
            for dir in ["Steam", "SteamLibrary", "Games\\Steam"] {
                v.push(PathBuf::from(format!("{drive}:\\{dir}")));
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(h) = dirs::home_dir() {
            v.push(h.join("Library/Application Support/Steam"));
        }
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(h) = dirs::home_dir() {
            for root in [".steam/steam", ".local/share/Steam", ".steam/debian-installation", ".var/app/com.valvesoftware.Steam/.local/share/Steam"] {
                v.push(h.join(root));
            }
        }
    }
    v.dedup();
    v
}

/// Find RimWorld through the library lists in the usual Steam folders, without asking Steam
/// (or the registry) where it is.
fn detect_from_library_files() -> Option<(PathBuf, PathBuf)> {
    let mut libraries: Vec<PathBuf> = Vec::new();
    for root in steam_roots() {
        if !root.join("steamapps").is_dir() {
            continue;
        }
        libraries.push(root.clone());
        if let Ok(text) = std::fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")) {
            libraries.extend(parse_library_folders(&text));
        }
    }
    for lib in libraries {
        // `game_root` steps into the bundle on macOS and leaves the folder alone everywhere
        // else, so the same two lines are right on all three platforms -- and "is this the game"
        // is answered by Version.txt being there rather than by a file extension, which is a
        // guess that a folder named `Anything.app` would satisfy.
        let game = game_root(&lib.join("steamapps").join("common").join("RimWorld"));
        if game.join("Version.txt").is_file() {
            let workshop = lib.join("steamapps").join("workshop").join("content").join(RIMWORLD_APP_ID.to_string());
            return Some((game, workshop));
        }
    }
    None
}

fn fallback_game_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    #[cfg(target_os = "windows")]
    {
        for root in ["C:\\Program Files (x86)\\Steam", "C:\\Program Files\\Steam", "D:\\Steam", "D:\\SteamLibrary", "E:\\SteamLibrary"] {
            v.push(PathBuf::from(root).join("steamapps").join("common").join("RimWorld"));
        }
        v.push(PathBuf::from("C:\\Program Files (x86)\\RimWorld"));
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(h) = dirs::home_dir() {
            v.push(h.join("Library/Application Support/Steam/steamapps/common/RimWorld/RimWorldMac.app"));
        }
        v.push(PathBuf::from("/Applications/RimWorldMac.app"));
    }
    #[cfg(target_os = "linux")]
    {
        if let Some(h) = dirs::home_dir() {
            for root in [".steam/steam", ".local/share/Steam", ".steam/debian-installation", ".var/app/com.valvesoftware.Steam/.local/share/Steam"] {
                v.push(h.join(root).join("steamapps/common/RimWorld"));
            }
        }
    }
    v
}

/// RimWorld's config folder (`ModsConfig.xml`, `Prefs.xml`, `KeyPrefs.xml`).
pub fn default_config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let home = dirs::home_dir()?;
        Some(home.join("AppData").join("LocalLow").join("Ludeon Studios").join("RimWorld by Ludeon Studios").join("Config"))
    }
    #[cfg(target_os = "macos")]
    {
        let home = dirs::home_dir()?;
        Some(home.join("Library").join("Application Support").join("RimWorld").join("Config"))
    }
    #[cfg(target_os = "linux")]
    {
        let home = dirs::home_dir()?;
        let native = home.join(".config/unity3d/Ludeon Studios/RimWorld by Ludeon Studios/Config");
        if native.is_dir() {
            return Some(native);
        }
        // Proton prefix
        let proton = home.join(".steam/steam/steamapps/compatdata/294100/pfx/drive_c/users/steamuser/AppData/LocalLow/Ludeon Studios/RimWorld by Ludeon Studios/Config");
        if proton.is_dir() {
            return Some(proton);
        }
        Some(native)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a Mac install: the folder Steam makes, with the bundle inside it. Both Data folders
    /// are created, because both are really there -- `Contents/Resources/Data` is Unity's, and
    /// mistaking it for the game's is what hid Core.
    fn mac_install(root: &std::path::Path) -> PathBuf {
        let app = root.join("RimWorldMac.app");
        std::fs::create_dir_all(app.join("Data").join("Core")).unwrap();
        std::fs::create_dir_all(app.join("Data").join("Royalty")).unwrap();
        std::fs::create_dir_all(app.join("Mods")).unwrap();
        std::fs::create_dir_all(app.join("Contents").join("Resources").join("Data")).unwrap();
        std::fs::create_dir_all(app.join("Contents").join("MacOS")).unwrap();
        std::fs::write(app.join("Version.txt"), "1.6.4530 rev1235").unwrap();
        app
    }

    /// A Mac player cannot choose the `.app`: the Finder treats it as a file, so the folder
    /// picker offers only the folder it sits in. That folder has to work.
    #[test]
    fn the_folder_a_mac_player_can_actually_pick_is_the_game() {
        let tmp = tempfile::tempdir().unwrap();
        let outer = tmp.path().join("common").join("RimWorld");
        let app = mac_install(&outer);

        assert_eq!(game_root(&outer), app, "the folder holding the bundle means the bundle");
        assert_eq!(game_root(&app), app, "and the bundle itself still means itself");

        let loc = Locations { game_dir: Some(outer.clone()), ..Default::default() };
        assert!(loc.is_usable(), "Version.txt is inside the bundle, so looking beside it found nothing");
        assert_eq!(crate::game::GameVersion::read(&outer).map(|v| v.major_minor), Some("1.6".into()));
        assert_eq!(detect_executable(&outer), Some(app.clone()));
    }

    /// Unity keeps its own Data inside the bundle. Preferring it by name pointed Circinus at
    /// engine assets and left the player with no Core and no DLC, which is the report.
    #[test]
    fn core_is_found_in_the_games_data_and_not_in_unitys() {
        let tmp = tempfile::tempdir().unwrap();
        let outer = tmp.path().join("RimWorld");
        let app = mac_install(&outer);

        for picked in [&outer, &app] {
            let data = data_dir_for_game(picked);
            assert_eq!(data, app.join("Data"), "picked {picked:?}");
            assert!(data.join("Core").is_dir(), "Core has to be in the folder we point at");
            assert!(data.join("Royalty").is_dir());
            assert_eq!(mods_dir_for_game(picked), app.join("Mods"));
        }

        // And through Locations, which is how the rest of the app asks.
        let mut loc = Locations { game_dir: Some(outer.clone()), ..Default::default() };
        loc.fill_derived();
        assert_eq!(loc.data_dir(), Some(app.join("Data")));
        assert_eq!(loc.local_mods_dir, Some(app.join("Mods")));
    }

    /// The Windows and Linux shape, unchanged: Data and Mods sit beside Version.txt.
    #[test]
    fn a_plain_game_folder_is_left_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path().join("RimWorld");
        std::fs::create_dir_all(game.join("Data").join("Core")).unwrap();
        std::fs::create_dir_all(game.join("Mods")).unwrap();
        std::fs::write(game.join("Version.txt"), "1.6.4530 rev1235").unwrap();

        assert_eq!(game_root(&game), game);
        assert_eq!(data_dir_for_game(&game), game.join("Data"));
        assert_eq!(mods_dir_for_game(&game), game.join("Mods"));
        assert!(Locations { game_dir: Some(game.clone()), ..Default::default() }.is_usable());
    }

    /// A folder holding a game and something else's bundle is not guessed at twice over.
    #[test]
    fn two_bundles_are_not_guessed_between() {
        let tmp = tempfile::tempdir().unwrap();
        let outer = tmp.path().join("games");
        let app = mac_install(&outer);
        std::fs::create_dir_all(outer.join("Something Else.app").join("Contents")).unwrap();
        assert_eq!(game_root(&outer), app, "the one called RimWorld is the one meant");

        // Neither of them named RimWorld: leave the folder alone rather than pick one. A wrong
        // game folder is worse than none, because none of it says so.
        let other = tmp.path().join("two");
        std::fs::create_dir_all(other.join("A.app")).unwrap();
        std::fs::create_dir_all(other.join("B.app")).unwrap();
        assert_eq!(game_root(&other), other);
    }

    /// A folder that is already the game wins over anything sitting in it, so an install that
    /// happens to contain a bundle is not redirected into it.
    #[test]
    fn a_folder_with_version_txt_is_the_game_whatever_else_is_in_it() {
        let tmp = tempfile::tempdir().unwrap();
        let game = tmp.path().join("RimWorld");
        std::fs::create_dir_all(game.join("Data").join("Core")).unwrap();
        std::fs::create_dir_all(game.join("Tools.app")).unwrap();
        std::fs::write(game.join("Version.txt"), "1.6.4530 rev1235").unwrap();
        assert_eq!(game_root(&game), game);
        assert_eq!(data_dir_for_game(&game), game.join("Data"));
    }

    #[test]
    fn splits_arguments() {
        assert_eq!(split_args("-popupwindow -screen-width 1920"), vec!["-popupwindow", "-screen-width", "1920"]);
        assert_eq!(split_args(r#"-savedatafolder "D:\My Saves" -quicktest"#), vec!["-savedatafolder", r"D:\My Saves", "-quicktest"]);
        assert_eq!(split_args("  "), Vec::<String>::new());
        assert_eq!(split_args("'' x"), vec!["", "x"]);
    }

    #[test]
    fn steam_paths() {
        assert!(is_steam_install(Path::new("D:\\SteamLibrary\\steamapps\\common\\RimWorld")));
        assert!(is_steam_install(Path::new("/home/x/.steam/steam/steamapps/common/RimWorld")));
        assert!(!is_steam_install(Path::new("C:\\Games\\RimWorld")));
        // The Workshop folder follows from a game folder inside a library, and only from one.
        let ws = workshop_dir_for_game(Path::new("/lib/steamapps/common/RimWorld")).unwrap();
        assert_eq!(ws, PathBuf::from("/lib/steamapps/workshop/content/294100"));
        assert!(workshop_dir_for_game(Path::new("/games/RimWorld")).is_none());
        assert!(workshop_dir_for_game(Path::new("/lib/steamapps/RimWorld")).is_none());
    }

    #[test]
    fn library_folders_are_read_from_the_vdf() {
        let vdf = r#""libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
		"apps"
		{
			"228980"		"267880374"
		}
	}
	"1"
	{
		"path"		"D:\\SteamLibrary"
		"apps"
		{
			"294100"		"6255543018"
		}
	}
}"#;
        assert_eq!(parse_library_folders(vdf), vec![PathBuf::from("C:\\Program Files (x86)\\Steam"), PathBuf::from("D:\\SteamLibrary")]);
        assert!(parse_library_folders("").is_empty());
    }

    /// Without Steam's registry entry, RimWorld is still found through a library list on disk.
    #[test]
    fn game_is_found_through_a_library_list() {
        let tmp = tempfile::tempdir().unwrap();
        let lib = tmp.path().join("SteamLibrary");
        let game = lib.join("steamapps").join("common").join("RimWorld");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("Version.txt"), "1.6.4530 rev1235").unwrap();
        let root = tmp.path().join("Steam");
        std::fs::create_dir_all(root.join("steamapps")).unwrap();
        let listed = lib.to_string_lossy().replace('\\', "\\\\");
        std::fs::write(root.join("steamapps").join("libraryfolders.vdf"), format!("\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{listed}\"\n\t}}\n}}\n")).unwrap();
        let text = std::fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")).unwrap();
        let libs = parse_library_folders(&text);
        assert_eq!(libs, vec![lib.clone()]);
        let found = libs.iter().map(|l| l.join("steamapps").join("common").join("RimWorld")).find(|g| g.join("Version.txt").is_file()).unwrap();
        assert_eq!(found, game);
        assert_eq!(workshop_dir_for_game(&found).unwrap(), lib.join("steamapps").join("workshop").join("content").join("294100"));
    }
}

/// Circinus' own data folder (cache, rule databases, settings).
/// The user's home directory, for writing paths without their name in them.
pub fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

pub fn app_data_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("Circinus")
}

/// What Circinus found where it is looking, and whether that is the right thing.
///
/// Four folders decide whether anything in this application works, and the window used to say
/// one word about each of them: the resolved path, or "not found". That is enough to notice a
/// blank and nothing else. A path that is *present and wrong* -- the Steam library root rather
/// than the game folder, last playthrough's config folder, a Mods folder on a drive that is no
/// longer plugged in -- looked exactly like a path that was right, and the only symptom was
/// everything downstream being empty or stale.
///
/// So each folder now says what is in it. "RimWorld 1.6.4518, Core and 4 DLC" is a sentence
/// somebody can check against what they believe; so is "no Version.txt here". The check is
/// deliberately a handful of `is_file` and `read_dir` calls against folders the scan has
/// already walked, because it runs with every snapshot and must not cost anything.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderCheck {
    /// The settings key, so the window can line this up with its own field list.
    pub key: String,
    /// Where Circinus is actually looking. None when nothing was detected and nothing was set.
    pub path: Option<String>,
    /// Chosen by hand rather than found. "Auto" only makes sense against one that was.
    pub custom: bool,
    /// `ok`, `warn` or `error`. An error means something that obviously matters is broken;
    /// a warning means something that matters in one place is. Nothing else carries colour.
    pub state: FolderState,
    /// What is in there, in plain words. Always said, including when the answer is "nothing".
    pub found: String,
    /// What would have to be in there instead. Empty when the folder is right.
    pub wanted: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FolderState {
    Ok,
    Warn,
    Error,
}

impl FolderCheck {
    fn new(key: &str, path: Option<&Path>, custom: bool, state: FolderState, found: impl Into<String>, wanted: impl Into<String>) -> FolderCheck {
        FolderCheck { key: key.to_string(), path: path.map(|p| p.display().to_string()), custom, state, found: found.into(), wanted: wanted.into() }
    }
}

/// How many entries a folder holds, counting only directories. `None` when it cannot be read.
fn dir_count(path: &Path) -> Option<usize> {
    Some(std::fs::read_dir(path).ok()?.filter_map(|e| e.ok()).filter(|e| e.file_type().map(|t| t.is_dir() || t.is_symlink()).unwrap_or(false)).count())
}

/// Check the game folder. `version` is what was read out of Version.txt, when anything was.
pub fn check_game_dir(path: Option<&Path>, custom: bool, version: Option<&str>) -> FolderCheck {
    let wanted = "A folder with Version.txt and Data in it. On macOS, the folder RimWorldMac.app sits in.";
    let Some(p) = path else {
        return FolderCheck::new("gameDir", None, custom, FolderState::Error, "Nothing found, and nothing set. Almost everything needs this one.", wanted);
    };
    let root = game_root(p);
    if !root.is_dir() {
        return FolderCheck::new("gameDir", Some(p), custom, FolderState::Error, "That folder is not there. An unplugged drive or a moved install does this.", wanted);
    }
    if !root.join("Version.txt").is_file() {
        return FolderCheck::new("gameDir", Some(p), custom, FolderState::Error, "No Version.txt here, so this is not the game folder.", wanted);
    }
    let data = data_dir_for_game(&root);
    let official = dir_count(&data).unwrap_or(0);
    let v = version.unwrap_or("an unreadable version");
    let found = match official {
        0 => format!("RimWorld {v}, but Data holds no Core -- the game itself will not start from here."),
        1 => format!("RimWorld {v}, Core and no DLC."),
        n => format!("RimWorld {v}, Core and {} other official folders.", n - 1),
    };
    let state = if official == 0 { FolderState::Error } else { FolderState::Ok };
    FolderCheck::new("gameDir", Some(&root), custom, state, found, if official == 0 { wanted } else { "" })
}

/// Check the config folder. `active` is how many mods ModsConfig.xml currently lists.
pub fn check_config_dir(path: Option<&Path>, custom: bool, active: usize) -> FolderCheck {
    let wanted = "The folder holding ModsConfig.xml. RimWorld writes it the first time it runs.";
    let Some(p) = path else {
        return FolderCheck::new("configDir", None, custom, FolderState::Error, "Nothing found, and nothing set. Your load order lives here.", wanted);
    };
    if !p.is_dir() {
        return FolderCheck::new("configDir", Some(p), custom, FolderState::Error, "That folder is not there.", wanted);
    }
    if !p.join("ModsConfig.xml").is_file() {
        return FolderCheck::new("configDir", Some(p), custom, FolderState::Error, "No ModsConfig.xml here. Run the game once, or point this at the folder that has one.", wanted);
    }
    FolderCheck::new("configDir", Some(p), custom, FolderState::Ok, format!("ModsConfig.xml, listing {active} mods."), "")
}

/// Check the local Mods folder. `here` is how many installed mods were read out of it.
///
/// A warning rather than an error when it is missing, and the warning says what it costs:
/// SteamCMD downloads have nowhere to land, which is the whole of report #3 -- thirteen minutes
/// of downloading that failed at the end because this was unset and nothing had said so.
pub fn check_local_mods_dir(path: Option<&Path>, custom: bool, here: usize) -> FolderCheck {
    let wanted = "Usually RimWorld/Mods. Downloads land here and the game reads it.";
    let Some(p) = path else {
        return FolderCheck::new("localModsDir", None, custom, FolderState::Warn, "Not set, so downloads have nowhere to go and local mods are not read.", wanted);
    };
    if !p.is_dir() {
        return FolderCheck::new("localModsDir", Some(p), custom, FolderState::Warn, "That folder is not there, so downloads have nowhere to go.", wanted);
    }
    let found = match here {
        0 => "Empty, which is fine. Downloads will land here.".to_string(),
        1 => "1 mod here.".to_string(),
        n => format!("{n} mods here."),
    };
    FolderCheck::new("localModsDir", Some(p), custom, FolderState::Ok, found, "")
}

/// Check the Workshop content folder. `here` is how many subscribed mods were read out of it.
///
/// Missing is only a problem for somebody who has Steam mods, so the state depends on whether
/// any were found anywhere. A player with a GOG copy and a Mods folder is told this is fine
/// rather than being shown a warning they can do nothing about and should ignore.
pub fn check_workshop_dir(path: Option<&Path>, custom: bool, here: usize, any_workshop_mods: bool) -> FolderCheck {
    let wanted = "steamapps/workshop/content/294100, in whichever Steam library holds RimWorld.";
    let idle = "Not found. Nothing is installed through the Steam Workshop, so nothing needs it.";
    let Some(p) = path else {
        return if any_workshop_mods {
            FolderCheck::new("workshopDir", None, custom, FolderState::Warn, "Not found, although subscribed mods are installed. Steam updates will not be noticed.", wanted)
        } else {
            FolderCheck::new("workshopDir", None, custom, FolderState::Ok, idle, "")
        };
    };
    if !p.is_dir() {
        return if any_workshop_mods {
            FolderCheck::new("workshopDir", Some(p), custom, FolderState::Warn, "That folder is not there, although subscribed mods are installed.", wanted)
        } else {
            FolderCheck::new("workshopDir", Some(p), custom, FolderState::Ok, idle, "")
        };
    }
    let found = match here {
        0 => "No subscribed mods in it yet.".to_string(),
        1 => "1 subscribed mod here.".to_string(),
        n => format!("{n} subscribed mods here."),
    };
    FolderCheck::new("workshopDir", Some(p), custom, FolderState::Ok, found, "")
}

#[cfg(test)]
mod folder_tests {
    use super::*;

    #[test]
    fn a_folder_that_is_present_and_wrong_is_not_a_folder_that_is_right() {
        // The failure this whole check exists for: the path is set, the path exists, and the
        // window used to print it with no more comment than a path that was correct.
        let dir = tempfile::tempdir().unwrap();
        let empty = dir.path().join("SteamLibrary");
        std::fs::create_dir_all(&empty).unwrap();
        let wrong = check_game_dir(Some(&empty), true, Some("1.6.4518"));
        assert_eq!(wrong.state, FolderState::Error);
        assert!(wrong.found.contains("Version.txt"), "{}", wrong.found);
        assert!(!wrong.wanted.is_empty(), "an error says what should be there instead");

        let game = dir.path().join("RimWorld");
        std::fs::create_dir_all(game.join("Data").join("Core")).unwrap();
        std::fs::create_dir_all(game.join("Data").join("Royalty")).unwrap();
        std::fs::write(game.join("Version.txt"), "1.6.4518 rev123").unwrap();
        let right = check_game_dir(Some(&game), true, Some("1.6.4518"));
        assert_eq!(right.state, FolderState::Ok);
        // What is in there, checkable against what the player believes is in there.
        assert!(right.found.contains("1.6.4518") && right.found.contains('1'), "{}", right.found);
        assert!(right.wanted.is_empty(), "nothing is wanted of a folder that is right");
    }

    #[test]
    fn a_game_folder_with_no_core_is_an_error_even_with_a_version_file() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("RimWorld");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join("Version.txt"), "1.6.4518").unwrap();
        let c = check_game_dir(Some(&game), false, Some("1.6.4518"));
        assert_eq!(c.state, FolderState::Error, "{}", c.found);
    }

    #[test]
    fn the_config_folder_says_how_many_mods_the_file_lists() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path().join("Config");
        std::fs::create_dir_all(&cfg).unwrap();
        let none = check_config_dir(Some(&cfg), false, 0);
        assert_eq!(none.state, FolderState::Error);
        assert!(none.found.contains("ModsConfig.xml"), "{}", none.found);
        std::fs::write(cfg.join("ModsConfig.xml"), "<ModsConfigData />").unwrap();
        let some = check_config_dir(Some(&cfg), false, 214);
        assert_eq!(some.state, FolderState::Ok);
        assert!(some.found.contains("214"), "{}", some.found);
    }

    #[test]
    fn no_mods_folder_warns_about_the_thing_it_actually_breaks() {
        // Report #3: a collection downloaded for thirteen minutes and then failed per item,
        // because there was nowhere to put it and nothing had ever said so.
        let c = check_local_mods_dir(None, false, 0);
        assert_eq!(c.state, FolderState::Warn);
        assert!(c.found.contains("downloads"), "{}", c.found);
        // Empty is a state, not a fault: a fresh install has nothing in Mods.
        let dir = tempfile::tempdir().unwrap();
        let empty = check_local_mods_dir(Some(dir.path()), false, 0);
        assert_eq!(empty.state, FolderState::Ok);
        assert!(empty.found.contains("fine"), "{}", empty.found);
    }

    #[test]
    fn a_missing_workshop_folder_is_only_a_problem_for_somebody_with_workshop_mods() {
        // A GOG copy has no Steam library and can do nothing about it, so a standing warning
        // there is a warning that teaches people to ignore warnings.
        let idle = check_workshop_dir(None, false, 0, false);
        assert_eq!(idle.state, FolderState::Ok);
        assert!(idle.wanted.is_empty());
        let broken = check_workshop_dir(None, false, 0, true);
        assert_eq!(broken.state, FolderState::Warn);
        assert!(!broken.wanted.is_empty());
    }
}
