//! What the Loading Progress mod measured, last time the game started.
//!
//! `ilyvion.LoadingProgress` times each mod's share of start-up and writes the result to
//! `StartupImpactData.xml` in RimWorld's save-data folder, through the game's own Scribe. That
//! file is the only per-mod measurement of loading that exists anywhere in the ecosystem, and it
//! is keyed by package id -- which is Circinus's own join key -- so it can be matched against a
//! list mod for mod.
//!
//! Why this matters more than the total. `playerlog` already recovers a total from the log, and
//! a total is enough to say how long the last launch took. It is not enough to say anything
//! about a list you have *changed*, which is the question somebody standing in a mod manager is
//! usually asking. Per-mod figures are: the mods you have measured contribute what they actually
//! cost, and `loadcost`'s model covers the rest -- scaled by how the model compared to the
//! measurement on the mods where both are known (see `loadcost::calibration`).
//!
//! Three things to know before trusting a file that exists:
//!
//! 1. **Everything here is float milliseconds.** The mod's profiler returns
//!    `Stopwatch.Elapsed.TotalMilliseconds` and Scribe writes it verbatim. Scaling it twice is
//!    the easy mistake and it is off by a factor of a thousand, which looks plausible.
//! 2. **The file only exists if two settings are on**, `TrackStartupLoadingImpact` and then
//!    `AutoSaveStartupImpactReport`, and both are off out of the box. Absent is the normal case,
//!    not a fault, and it is worth telling the player which two boxes to tick rather than
//!    showing nothing.
//! 3. **It holds one session and is overwritten every launch.** There is no history in it.
//!
//! Scribe omits a value equal to its type default, so every field here is optional and a missing
//! one means zero rather than a broken file. Anything that cannot be understood yields `None`:
//! an estimate is a perfectly good answer and a wrong measurement is not.

use serde::{Deserialize, Serialize};

/// What one mod cost, as measured on this machine.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModImpact {
    /// Lowercased, and without the `_steam` suffix: the shape Circinus joins on.
    pub package_id: String,
    /// What the mod called itself in that run, for a name when the mod is no longer installed.
    pub name: String,
    /// Milliseconds on the loading thread.
    pub total_ms: f64,
    /// Milliseconds this mod spent on other threads. Real time, but not time the player waited
    /// for in series, so it is kept apart rather than added in.
    pub off_thread_ms: f64,
}

/// One measured start-up.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartupImpact {
    /// The whole start-up, in milliseconds.
    pub total_ms: f64,
    /// Per-mod, in the order the file lists them (the mod writes them worst first).
    pub mods: Vec<ModImpact>,
    /// What the mod counted, when it said. Used to notice a file describing another list.
    pub mods_loaded: Option<u32>,
    pub defs_parsed: Option<u32>,
    pub patch_ops: Option<u32>,
}

impl StartupImpact {
    /// The game's own start, with every mod's measured time taken out.
    ///
    /// This is the figure `loadcost::VANILLA_SECS` has been standing in for with a round 40, and
    /// on a short list it is the larger half of the total. Clamped at zero because the mod's
    /// per-mod accounting and its total are taken by different stopwatches: they can cross over
    /// slightly on a list where almost everything is a mod, and a negative floor would be worse
    /// than a zero one.
    pub fn vanilla_ms(&self) -> f64 {
        let mods: f64 = self.mods.iter().map(|m| m.total_ms).sum();
        (self.total_ms - mods).max(0.0)
    }

    /// Measured milliseconds by package id, for joining against a list.
    pub fn by_package(&self) -> std::collections::HashMap<String, f64> {
        self.mods.iter().map(|m| (m.package_id.clone(), m.total_ms)).collect()
    }
}

/// The shape Circinus joins on: lowercase, and without Steam's suffix.
///
/// RimWorld's `PackageIdPlayerFacing` already drops `_steam`, but a file written by an older
/// build of the mod may not have, and a copy kept by hand certainly will not.
fn key(id: &str) -> String {
    let id = id.trim().to_ascii_lowercase();
    id.strip_suffix("_steam").unwrap_or(&id).to_string()
}

fn text_of(node: roxmltree::Node, tag: &str) -> Option<String> {
    node.children()
        .find(|c| c.is_element() && c.tag_name().name() == tag)
        .and_then(|c| c.text())
        .map(|t| t.trim().to_string())
}

fn f64_of(node: roxmltree::Node, tag: &str) -> f64 {
    text_of(node, tag).and_then(|t| t.parse::<f64>().ok()).unwrap_or(0.0)
}

fn u32_of(node: roxmltree::Node, tag: &str) -> Option<u32> {
    text_of(node, tag).and_then(|t| t.parse::<u32>().ok())
}

/// Read `StartupImpactData.xml`.
///
/// `None` for anything not understood -- absent, truncated, half-written by a game that was
/// killed mid-save, or a future version of the mod that moved the tags. The caller falls back to
/// the model, which is a fine answer; a number invented from a file we could not read is not.
pub fn parse(xml: &str) -> Option<StartupImpact> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    // Scribe wraps everything in the name given to InitSaving, then the label of the deep-looked
    // object. Rather than depend on both, find the element that has the fields.
    let session = doc
        .descendants()
        .find(|n| n.is_element() && n.children().any(|c| c.is_element() && c.tag_name().name() == "mods"))?;

    let mods = session
        .children()
        .find(|c| c.is_element() && c.tag_name().name() == "mods")?
        .children()
        .filter(|c| c.is_element() && c.tag_name().name() == "li")
        .filter_map(|li| {
            let package_id = key(&text_of(li, "modPackageId")?);
            if package_id.is_empty() {
                return None;
            }
            Some(ModImpact {
                name: text_of(li, "modName").unwrap_or_default(),
                package_id,
                total_ms: f64_of(li, "totalImpact"),
                off_thread_ms: f64_of(li, "offThreadTotalImpact"),
            })
        })
        .collect::<Vec<_>>();

    let total_ms = f64_of(session, "loadingTime");
    // A file with no total and no mods in it says nothing; treat it as absent rather than as a
    // measurement of zero, which would read as an instant load.
    if total_ms <= 0.0 && mods.is_empty() {
        return None;
    }

    Some(StartupImpact {
        total_ms,
        mods,
        mods_loaded: u32_of(session, "modsLoaded"),
        defs_parsed: u32_of(session, "defsParsed"),
        patch_ops: u32_of(session, "patchOperationsApplied"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shape RimWorld's Scribe actually writes: a named root, a deep-looked object inside it,
    /// dictionaries as parallel `keys`/`values` lists, and a list of `li`.
    const SAMPLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<StartupImpactSession>
  <sessionData>
    <loadingTime>462986.4</loadingTime>
    <metrics>
      <keys><li>LoadedModManagerParseAndProcessXML</li><li>BakeStaticAtlases</li></keys>
      <values><li>81022.1</li><li>19004.5</li></values>
    </metrics>
    <totalImpact>301244.8</totalImpact>
    <mods>
      <li>
        <modName>RIMMSqol</modName>
        <modPackageId>telardo.RIMMSqol</modPackageId>
        <totalImpact>42155.25</totalImpact>
        <offThreadTotalImpact>1200.5</offThreadTotalImpact>
      </li>
      <li>
        <modName>Performance Fish</modName>
        <modPackageId>bs.performance_steam</modPackageId>
        <totalImpact>31044.0</totalImpact>
      </li>
      <li>
        <modName>Harmony</modName>
        <modPackageId>brrainz.harmony</modPackageId>
        <totalImpact>905.75</totalImpact>
      </li>
    </mods>
    <modsLoaded>708</modsLoaded>
    <defsParsed>91244</defsParsed>
    <patchOperationsApplied>14022</patchOperationsApplied>
  </sessionData>
</StartupImpactSession>"#;

    #[test]
    fn reads_a_session() {
        let s = parse(SAMPLE).expect("the sample should parse");
        assert_eq!(s.total_ms, 462986.4);
        assert_eq!(s.mods.len(), 3);
        assert_eq!(s.mods_loaded, Some(708));
        assert_eq!(s.defs_parsed, Some(91244));
        assert_eq!(s.patch_ops, Some(14022));
    }

    #[test]
    fn package_ids_come_back_in_the_shape_circinus_joins_on() {
        let s = parse(SAMPLE).unwrap();
        let ids: Vec<&str> = s.mods.iter().map(|m| m.package_id.as_str()).collect();
        // Lowercased, and Steam's suffix gone -- both are how the same mod is written elsewhere.
        assert_eq!(ids, vec!["telardo.rimmsqol", "bs.performance", "brrainz.harmony"]);
    }

    #[test]
    fn a_missing_value_is_zero_rather_than_a_broken_file() {
        // Scribe leaves out a value equal to its default, so Performance Fish has no off-thread
        // figure at all. That must read as zero, not as a parse failure.
        let s = parse(SAMPLE).unwrap();
        let fish = s.mods.iter().find(|m| m.package_id == "bs.performance").unwrap();
        assert_eq!(fish.off_thread_ms, 0.0);
        assert_eq!(fish.total_ms, 31044.0);
    }

    #[test]
    fn the_game_start_is_what_the_mods_did_not_account_for() {
        let s = parse(SAMPLE).unwrap();
        // 462986.4 - (42155.25 + 31044.0 + 905.75)
        assert!((s.vanilla_ms() - 388881.4).abs() < 0.01, "{}", s.vanilla_ms());
    }

    #[test]
    fn the_game_start_never_goes_negative() {
        // The total and the per-mod figures come off different stopwatches and can cross over.
        let xml = SAMPLE.replace("<loadingTime>462986.4</loadingTime>", "<loadingTime>100.0</loadingTime>");
        assert_eq!(parse(&xml).unwrap().vanilla_ms(), 0.0);
    }

    #[test]
    fn joins_by_package_id() {
        let by = parse(SAMPLE).unwrap().by_package();
        assert_eq!(by.get("telardo.rimmsqol"), Some(&42155.25));
        assert_eq!(by.len(), 3);
    }

    #[test]
    fn nothing_understandable_is_none() {
        assert!(parse("").is_none());
        assert!(parse("not xml at all <<<").is_none());
        // Half-written by a game that was killed mid-save.
        assert!(parse("<StartupImpactSession><sessionData><mods><li><modNa").is_none());
        // Well-formed, and says nothing.
        assert!(parse("<StartupImpactSession><sessionData><mods /></sessionData></StartupImpactSession>").is_none());
    }

    #[test]
    fn a_mod_with_no_package_id_is_skipped_rather_than_taking_the_file_down() {
        let xml = SAMPLE.replace("<modPackageId>brrainz.harmony</modPackageId>", "<modPackageId></modPackageId>");
        let s = parse(&xml).expect("the rest of the file is still good");
        assert_eq!(s.mods.len(), 2);
    }
}
