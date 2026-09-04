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
        if loc.game_dir.is_none() {
            loc.game_dir = fallback_game_dirs().into_iter().find(|p| p.join("Version.txt").is_file());
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

    pub fn is_usable(&self) -> bool {
        self.game_dir.as_ref().map(|g| g.join("Version.txt").is_file()).unwrap_or(false)
    }
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

/// Circinus' own data folder (cache, rule databases, settings).
pub fn app_data_dir() -> PathBuf {
    dirs::data_local_dir().unwrap_or_else(|| PathBuf::from(".")).join("Circinus")
}
