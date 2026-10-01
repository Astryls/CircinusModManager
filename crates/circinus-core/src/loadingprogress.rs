//! The two boxes that have to be ticked before anything measures loading, and ticking them.
//!
//! `startupimpact` reads the report. This is about the reason there usually isn't one:
//! `ilyvion.LoadingProgress` ships with **Track startup loading impact** and **Auto-save startup
//! impact report** both off, and until both are on no file is written at all. Every build so far
//! has answered that with a paragraph telling the player to go and find two checkboxes in another
//! mod's settings, which is a worse answer than it sounds -- the settings window is three clicks
//! deep, the second box is indented under the first and only appears once the first is on, and
//! the whole thing has to be done before a launch that has already happened.
//!
//! So Circinus writes them itself. RimWorld keeps each mod's settings in its own file in the
//! Config folder and reads them at startup, so the edit is a small, legible XML file and it takes
//! effect the next time the game runs. Three things make that safe to do, and all three are load
//! bearing:
//!
//! 1. **Only while the game is closed.** RimWorld holds settings in memory and writes the whole
//!    file back when the player closes the mod settings window -- so an edit made under a running
//!    game is silently reverted, which is worse than refusing. The caller checks; this module
//!    does not know what a process is.
//! 2. **Nothing else in the file is touched.** The file also holds `loadingTimes`, the mod's own
//!    estimate samples, and `lastLoadingModHash`. Those are the mod's working data, and
//!    rewriting the file from a model of what Circinus thinks belongs in it would throw them
//!    away. Two elements are set or inserted; every other byte is carried through, indentation
//!    and all.
//! 3. **The field names are the mod's, not ours.** The settings class declares `bool`s named
//!    `_trackStartupLoadingImpact` and `_autoSaveStartupImpactReport`; ilyvion's Scribe helper
//!    drops the leading underscore for the element name, which is how `_loadingTimes` lands on
//!    disk as `<loadingTimes>`. If a future version renames them, the worst case is that
//!    Circinus writes two elements the mod ignores and keeps reporting the settings as off --
//!    the lie is visible and nothing is lost.
//!
//! Scribe omits any value equal to its type default, so a `false` is usually an *absent* element
//! rather than a written one. Absent reads as off, which is both correct and the common case: a
//! fresh install's file has neither element in it.

use std::path::{Path, PathBuf};

/// The mod, as `id_base` would give it. Both the Workshop copy and a local one answer to this.
pub const PACKAGE_ID: &str = "ilyvion.loadingprogress";

/// The `Mod` subclass RimWorld names the settings file after: `Mod_<folder>_<handle>.xml`.
pub const HANDLE: &str = "LoadingProgressMod";

/// What the `ModSettings` element says it is, used to recognise the file when the handle changes.
pub const SETTINGS_CLASS: &str = "ilyvion.LoadingProgress.Settings";

/// Track startup loading impact. Nothing is measured without it.
pub const TRACK: &str = "trackStartupLoadingImpact";

/// Automatically save the report to `StartupImpactData.xml`. Nothing is *written* without it.
pub const AUTOSAVE: &str = "autoSaveStartupImpactReport";

/// Which of the two boxes are ticked.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Tracking {
    pub track: bool,
    pub autosave: bool,
}

impl Tracking {
    /// Both, which is the only state that produces a file Circinus can read.
    pub fn complete(&self) -> bool {
        self.track && self.autosave
    }
}

/// Where RimWorld keeps this mod's settings, given the Config folder and the mod's folder name.
///
/// The game's own rule is `Mod_{folder}_{handle}.xml` with invalid filename characters stripped,
/// where `folder` is the directory the mod was loaded from -- the Workshop id for a subscribed
/// copy, the folder's own name for a local one. The handle is the `Mod` subclass name, which is
/// a thing only the assembly knows, so a file already sitting there that declares the right
/// settings class wins over the name this module would have guessed. That is what keeps this
/// working if ilyvion ever renames the class.
pub fn settings_path(config_dir: &Path, mod_folder: &str) -> PathBuf {
    let prefix = format!("Mod_{}_", sanitize(mod_folder));
    if let Ok(rd) = std::fs::read_dir(config_dir) {
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with(&prefix) || !name.ends_with(".xml") {
                continue;
            }
            let path = entry.path();
            if std::fs::read_to_string(&path).map(|t| t.contains(SETTINGS_CLASS)).unwrap_or(false) {
                return path;
            }
        }
    }
    config_dir.join(format!("{prefix}{HANDLE}.xml"))
}

/// RimWorld's `GenText.SanitizeFilename`: drop what a filename may not contain.
fn sanitize(name: &str) -> String {
    name.chars().filter(|c| !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') && !c.is_control()).collect()
}

/// What the file says, which for a file that does not exist is "neither".
pub fn read(path: &Path) -> Tracking {
    std::fs::read_to_string(path).map(|t| parse(&t)).unwrap_or_default()
}

/// Read both flags out of a settings file's text.
///
/// Deliberately not an XML parse. This file belongs to another program, it is read on every
/// snapshot, and the only question being asked of it is whether two elements are present and
/// say `true` -- a question `bool_of` answers without allocating a document for it. A file that
/// cannot be understood reads as off, which is the same answer the mod itself would give.
pub fn parse(text: &str) -> Tracking {
    Tracking { track: bool_of(text, TRACK), autosave: bool_of(text, AUTOSAVE) }
}

/// The text between `<key>` and `</key>`, read as a Scribe bool. Absent is `false`.
fn bool_of(text: &str, key: &str) -> bool {
    let open = format!("<{key}>");
    let Some(s) = text.find(&open) else { return false };
    let rest = &text[s + open.len()..];
    let Some(e) = rest.find(&format!("</{key}>")) else { return false };
    rest[..e].trim().eq_ignore_ascii_case("true")
}

/// The same file with both boxes set the way the caller asked, or a fresh one when there is
/// nothing to amend.
///
/// `None` means no file on disk yet, which is the ordinary case for a mod whose settings have
/// never been opened. Anything that is not recognisably a settings block is treated the same
/// way: a file Circinus cannot place is not a file it should be splicing into.
///
/// Off writes an explicit `false` rather than deleting the elements. Scribe would have omitted
/// them, and the two are read identically -- but an element that says `false` is a record that
/// somebody decided, where an absent one is indistinguishable from a file nothing has touched.
pub fn ticked(current: Option<&str>, on: bool) -> String {
    match current.filter(|t| t.contains("</ModSettings>")) {
        Some(text) => set(&set(text, TRACK, on), AUTOSAVE, on),
        None => blank(on),
    }
}

/// A settings file with nothing in it but the two flags.
fn blank(on: bool) -> String {
    let v = if on { "true" } else { "false" };
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<SettingsBlock>\n\t<ModSettings Class=\"{SETTINGS_CLASS}\">\n\t\t<{TRACK}>{v}</{TRACK}>\n\t\t<{AUTOSAVE}>{v}</{AUTOSAVE}>\n\t</ModSettings>\n</SettingsBlock>\n"
    )
}

/// Set one element, in place if it is there and by insertion if it is not.
///
/// Textual rather than a parse-and-serialise round trip, and that is the point: the file is
/// RimWorld's and a round trip would reformat every line of it, including the thousand-element
/// `loadingTimes` list, turning a two-line change into a diff nobody could check.
fn set(text: &str, key: &str, on: bool) -> String {
    let open = format!("<{key}>");
    let close = format!("</{key}>");
    let v = if on { "true" } else { "false" };
    if let Some(s) = text.find(&open) {
        if let Some(e) = text[s..].find(&close).map(|e| s + e + close.len()) {
            return format!("{}{open}{v}{close}{}", &text[..s], &text[e..]);
        }
    }
    // Scribe never writes the self-closing form, but a hand-edited file might.
    for empty in [format!("<{key} />"), format!("<{key}/>")] {
        if let Some(s) = text.find(&empty) {
            return format!("{}{open}{v}{close}{}", &text[..s], &text[s + empty.len()..]);
        }
    }
    let Some(end) = text.rfind("</ModSettings>") else { return text.to_string() };
    // Indent the new element one step past the line `</ModSettings>` sits on, so an inserted
    // line looks like the ones Scribe wrote around it.
    let line_start = text[..end].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let indent = &text[line_start..end];
    let step = if indent.contains('\t') { "\t" } else { "  " };
    format!("{}{indent}{step}{open}{v}{close}\n{}", &text[..line_start], &text[line_start..])
}

/// Write the file with tracking switched on or off, creating the folder if it is not there.
///
/// The caller is responsible for the one rule this cannot check: the game must not be running.
pub fn set_tracking(path: &Path, on: bool) -> std::io::Result<Tracking> {
    let current = std::fs::read_to_string(path).ok();
    let next = ticked(current.as_deref(), on);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Keep one copy of whatever was there. This is another program's file and the player did not
    // ask Circinus to be the last thing that touched it.
    if let Some(prev) = &current {
        let _ = std::fs::write(path.with_extension("xml.circinus-bak"), prev);
    }
    std::fs::write(path, &next)?;
    Ok(parse(&next))
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<SettingsBlock>\n\t<ModSettings Class=\"ilyvion.LoadingProgress.Settings\">\n\t\t<lastLoadingModHash>544764584</lastLoadingModHash>\n\t\t<loadingTimes>\n\t\t\t<li>1640.87878</li>\n\t\t</loadingTimes>\n\t</ModSettings>\n</SettingsBlock>";

    #[test]
    fn a_fresh_install_reads_as_off() {
        // Scribe omits a false, so the file from a real machine has neither element in it.
        assert_eq!(parse(REAL), Tracking::default());
        assert!(!parse(REAL).complete());
    }

    #[test]
    fn ticking_the_boxes_keeps_the_mods_own_data() {
        let out = ticked(Some(REAL), true);
        assert_eq!(parse(&out), Tracking { track: true, autosave: true });
        assert!(parse(&out).complete());
        // The mod's estimate samples and its hash survive, indentation and all.
        assert!(out.contains("<lastLoadingModHash>544764584</lastLoadingModHash>"));
        assert!(out.contains("\t\t\t<li>1640.87878</li>"));
        assert!(out.contains(&format!("\t\t<{TRACK}>true</{TRACK}>")));
        assert_eq!(out.matches("<ModSettings").count(), 1);
    }

    #[test]
    fn a_box_already_ticked_is_left_alone_and_an_untick_is_put_back() {
        let on = ticked(Some(REAL), true);
        assert_eq!(ticked(Some(&on), true), on, "running it twice changes nothing");
        let off = on.replace(&format!("<{TRACK}>true</{TRACK}>"), &format!("<{TRACK}>False</{TRACK}>"));
        assert!(!parse(&off).track, "Scribe's capitalisation still reads");
        assert!(parse(&ticked(Some(&off), true)).track);
    }

    #[test]
    fn switching_it_off_says_so_rather_than_going_quiet() {
        // An explicit false and an absent element read the same to the mod, but only one of
        // them is a record that somebody decided.
        let off = ticked(Some(&ticked(Some(REAL), true)), false);
        assert_eq!(parse(&off), Tracking::default());
        assert!(off.contains(&format!("<{TRACK}>false</{TRACK}>")));
        assert!(off.contains("<lastLoadingModHash>544764584</lastLoadingModHash>"));
    }

    #[test]
    fn nothing_on_disk_yields_a_file_the_game_can_read() {
        let out = ticked(None, true);
        assert!(out.starts_with("<?xml"));
        assert!(out.contains(SETTINGS_CLASS));
        assert_eq!(parse(&out), Tracking { track: true, autosave: true });
        // Something that is not a settings block is not spliced into either.
        assert_eq!(ticked(Some("not xml at all"), true), out);
    }

    #[test]
    fn the_file_is_found_by_what_it_declares_not_by_a_guessed_class_name() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = dir.path();
        // A handle Circinus would never have guessed, and a decoy with the right prefix.
        std::fs::write(cfg.join("Mod_3535481557_Renamed.xml"), REAL).unwrap();
        std::fs::write(cfg.join("Mod_3535481557_Other.xml"), "<SettingsBlock />").unwrap();
        assert_eq!(settings_path(cfg, "3535481557"), cfg.join("Mod_3535481557_Renamed.xml"));
        // A local copy is named after its folder, and with no file there the handle is the guess.
        assert_eq!(settings_path(cfg, "LoadingProgress"), cfg.join(format!("Mod_LoadingProgress_{HANDLE}.xml")));
    }

    #[test]
    fn enabling_keeps_a_copy_of_what_was_there() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Mod_3535481557_LoadingProgressMod.xml");
        std::fs::write(&path, REAL).unwrap();
        assert_eq!(set_tracking(&path, true).unwrap(), Tracking { track: true, autosave: true });
        assert_eq!(read(&path), Tracking { track: true, autosave: true });
        assert_eq!(std::fs::read_to_string(dir.path().join("Mod_3535481557_LoadingProgressMod.xml.circinus-bak")).unwrap(), REAL);
        assert_eq!(set_tracking(&path, false).unwrap(), Tracking::default());
    }
}
