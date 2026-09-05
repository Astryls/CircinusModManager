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
            loc.game_dir = fallback_game_dirs().into_iter().find(|p| p.join("Version.txt").is_file());
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

    pub fn is_usable(&self) -> bool {
        self.game_dir.as_ref().map(|g| g.join("Version.txt").is_file()).unwrap_or(false)
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
    if cfg!(target_os = "macos") {
        if game.extension().map(|e| e == "app").unwrap_or(false) {
            return Some(game.to_path_buf());
        }
        return std::fs::read_dir(game).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| p.extension().map(|e| e == "app").unwrap_or(false));
    }
    let names: &[&str] = if cfg!(target_os = "windows") { &["RimWorldWin64.exe", "RimWorldWin.exe", "RimWorld.exe"] } else { &["RimWorldLinux", "RimWorldLinux.x86_64", "start_RimWorld.sh"] };
    names.iter().map(|n| game.join(n)).find(|p| p.is_file())
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

/// On macOS the game is an app bundle; Data/ and Mods/ live inside it.
pub fn data_dir_for_game(game: &Path) -> PathBuf {
    let inside = game.join("Contents").join("Resources").join("Data");
    if cfg!(target_os = "macos") && inside.is_dir() {
        inside
    } else {
        game.join("Data")
    }
}

pub fn mods_dir_for_game(game: &Path) -> PathBuf {
    let inside = game.join("Contents").join("Resources").join("Mods");
    if cfg!(target_os = "macos") && (inside.is_dir() || game.extension().map(|e| e == "app").unwrap_or(false)) {
        inside
    } else {
        game.join("Mods")
    }
}

fn detect_steam() -> Option<(PathBuf, PathBuf)> {
    let steam = steamlocate::SteamDir::locate().ok()?;
    let (app, library) = steam.find_app(RIMWORLD_APP_ID).ok().flatten()?;
    let game = library.resolve_app_dir(&app);
    let workshop = library.path().join("steamapps").join("workshop").join("content").join(RIMWORLD_APP_ID.to_string());
    let game = if cfg!(target_os = "macos") {
        // Steam installs `RimWorldMac.app` inside the app dir.
        std::fs::read_dir(&game)
            .ok()
            .and_then(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| p.extension().map(|e| e == "app").unwrap_or(false)))
            .unwrap_or(game)
    } else {
        game
    };
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
        let game = lib.join("steamapps").join("common").join("RimWorld");
        let game = if cfg!(target_os = "macos") { std::fs::read_dir(&game).ok().and_then(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).find(|p| p.extension().map(|e| e == "app").unwrap_or(false))).unwrap_or(game) } else { game };
        let is_game = if cfg!(target_os = "macos") { game.extension().map(|e| e == "app").unwrap_or(false) } else { game.join("Version.txt").is_file() };
        if is_game {
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
pub fn app_data_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("Circinus")
}
