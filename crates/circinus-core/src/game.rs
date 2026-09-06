//! Game version handling and RimWorld's `*ByVersion` / load-folder matching rules.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// The installed game's version, from `<game>/Version.txt` (e.g. `1.6.4530 rev1235`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameVersion {
    /// Full string as written in Version.txt.
    pub full: String,
    /// `major.minor`, the only part mods care about.
    pub major_minor: String,
}

impl GameVersion {
    pub fn parse(text: &str) -> Option<GameVersion> {
        let full = text.trim().trim_start_matches('\u{feff}').to_string();
        let mut parts = full.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty());
        let major = parts.next()?;
        let minor = parts.next()?;
        Some(GameVersion { major_minor: format!("{major}.{minor}"), full })
    }

    /// Read from the folder the game really is: on macOS a player can only pick the folder the
    /// `.app` sits in, and Version.txt is inside the bundle.
    pub fn read(game_dir: &Path) -> Option<GameVersion> {
        std::fs::read_to_string(crate::paths::game_root(game_dir).join("Version.txt")).ok().and_then(|t| GameVersion::parse(&t))
    }

    pub fn fallback(major_minor: &str) -> GameVersion {
        GameVersion { full: major_minor.to_string(), major_minor: major_minor.to_string() }
    }
}

/// Normalize a version tag such as `v1.6`, `1.6`, `1.6.4530` to `major.minor`.
pub fn major_minor_of(tag: &str) -> Option<String> {
    let t = tag.trim().trim_start_matches(['v', 'V']);
    let mut parts = t.split('.');
    let major = parts.next()?.trim();
    let minor = parts.next()?.trim();
    let minor_digits: String = minor.chars().take_while(|c| c.is_ascii_digit()).collect();
    if major.chars().all(|c| c.is_ascii_digit()) && !major.is_empty() && !minor_digits.is_empty() {
        Some(format!("{major}.{minor_digits}"))
    } else {
        None
    }
}

/// Does a `*ByVersion` child tag (`v1.6`, `1.6`, `v1.6.4530`) apply to this game version?
pub fn version_tag_matches(tag: &str, major_minor: &str) -> bool {
    major_minor_of(tag).map(|mm| mm == major_minor).unwrap_or(false)
}

/// Compare two `major.minor` strings numerically.
pub fn cmp_major_minor(a: &str, b: &str) -> std::cmp::Ordering {
    let num = |s: &str| -> (u32, u32) {
        let mut it = s.split('.');
        let ma = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
        let mi = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
        (ma, mi)
    };
    num(a).cmp(&num(b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_version_txt() {
        let v = GameVersion::parse("1.6.4530 rev1235\n").unwrap();
        assert_eq!(v.major_minor, "1.6");
        assert_eq!(GameVersion::parse("1.5.4243 rev947").unwrap().major_minor, "1.5");
        assert!(GameVersion::parse("garbage").is_none());
    }

    #[test]
    fn version_tags() {
        assert!(version_tag_matches("v1.6", "1.6"));
        assert!(version_tag_matches("1.6", "1.6"));
        assert!(version_tag_matches("v1.6.1234", "1.6"));
        assert!(!version_tag_matches("v1.5", "1.6"));
        assert!(!version_tag_matches("default", "1.6"));
        assert_eq!(major_minor_of("1.6.4530 rev1235"), Some("1.6".into()));
    }
}
