//! Sharing a load run with circinus.sh. Off until switched on, and built so that the things the
//! site promises never to receive cannot be put in the envelope in the first place.
//!
//! The policy this implements is not invented here. It is the one already published at
//! <https://circinus.sh/privacy> for the Circinus Performance Analyzer, and the manager is held
//! to it word for word: package ids and performance figures go, file paths and mod names and
//! anything from the player's world do not, recording is local and sharing is a separate choice,
//! and deleting is self-serve. A second tool sending a slightly different set under the same
//! promise would make the promise worthless.
//!
//! The one place the manager departs from the analyzer is the install id: it generates its own
//! rather than reusing the mod's. Nothing here needs the two data sets joined at the machine --
//! a load cost and a frame cost meet on the mod's page, keyed by package id -- so sharing an id
//! would have made an install trackable across two tools in exchange for nothing.
//!
//! # Why the payload is built from parts rather than from the file
//!
//! `StartupImpactData.xml` carries a `modName` for every mod, and it is read from a path inside
//! the player's home directory. Both are on the never-sent list. The defence is not to remember
//! to strip them: it is that [`build`] cannot see them. It takes the numbers and the ids as
//! arguments and has no access to a path, a name, or the filesystem, so there is no line anyone
//! could add later that leaks one by accident. `no_path_can_reach_the_wire` and
//! `no_mod_name_can_reach_the_wire` hold that shut, and run on every build.
//!
//! # Why milliseconds are not the interesting number
//!
//! The same list on an NVMe and on a spinning disk are minutes apart, which is why the site
//! already refuses to average raw milliseconds across machines for frame cost. Load is worse.
//! What makes these runs poolable is `vanilla_ms`: the part of the measured total that no mod
//! accounted for, measured on the same machine in the same run. Divide a mod's cost by it and
//! the result is a property of the mod rather than of the disk it was on -- the load-time
//! equivalent of share of frame time. Both are sent, and the site is expected to rank on the
//! ratio and keep the raw figure only for the machine class it came from.

use crate::startupimpact::StartupImpact;
use serde::{Deserialize, Serialize};

/// The contract version of what gets collected.
///
/// Consent is to a *set of fields*, not to the idea of sharing. The privacy page promises "if
/// what gets collected ever changes, you'll be asked again", and this is what makes that
/// mechanical: a stored consent recording an older number does not authorise this payload, and
/// the player is asked afresh. Bump it whenever a field is added, never when one is removed.
pub const CONSENT_VERSION: u32 = 1;

/// One mod, as the wire is allowed to describe it.
///
/// Be precise about what the omissions are for, because two different reasons are at work and
/// conflating them makes a privacy claim that is not true.
///
/// The **package id identifies the mod**, and it is sent. Anyone can read `brrainz.harmony` and
/// know which mod that is, and the Workshop id beside it exists so the site can look up the
/// real title. The site knows perfectly well which mods a run contained; that is the point of
/// the payload, and nothing here should be described as hiding it.
///
/// What is left out for *quality* is the display name: an unverifiable string typed by whoever
/// packaged the mod, which has given mods the wrong title before. Steam's listing is the
/// better source and is what the site uses.
///
/// What is left out for *privacy* is everything that describes this install rather than the
/// mod: the folder path, the source, the position in the load order (see the sort in `build`),
/// and the versions of mods this run did not measure -- a whole load order's versions would
/// make an install much easier to pick out than the ids alone already do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModCost {
    pub package_id: String,
    /// The public Workshop item id, for Workshop mods. It identifies a public listing and says
    /// nothing about this copy or this player.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workshop_id: Option<u64>,
    /// Only for mods in this run's cost table.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub total_ms: f64,
    #[serde(skip_serializing_if = "is_zero")]
    pub off_thread_ms: f64,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// What the site is told about the machine, at the coarseness the privacy page states, and
/// never an OS build number.
///
/// No GPU, unlike the analyzer's runs. Loading a mod list is XML parsing, patch application and
/// disk reads; the graphics card has no bearing on any of it, and a field that cannot explain
/// any of the variance is a field that only makes an install easier to pick out. The analyzer
/// sends one because frame time genuinely depends on it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Machine {
    pub cpu: String,
    pub cores: u32,
    pub memory_gb: u32,
    /// "Windows 11", never "Windows 11 (10.0.22631)".
    pub os: String,
}

/// One shared load run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadRunReport {
    /// Which set of fields the player agreed to. The server rejects a mismatch rather than
    /// guessing, because a run sent under an older agreement was not agreed to.
    pub consent_version: u32,
    /// The run's own id, so the site can refuse a duplicate before it writes anything. Matches
    /// what `/api/v1/ingest` already does with `RunDocument.Id`, rather than inventing a
    /// dedupe key out of the payload's contents.
    pub run_id: String,

    /// A random id generated by the manager, for this install of the manager alone.
    ///
    /// **Not serialised.** It travels in `X-Circinus-Install`, which is how the site's existing
    /// ingest receives one and is already in the CORS header allowlist. Keeping it out of the
    /// body also keeps it out of the append-only raw submission row.
    ///
    /// Deliberately *not* the Performance Analyzer's, even though reusing it was the obvious
    /// move: nothing here needs the two data sets joined at the machine. A load cost and a
    /// frame cost meet on the mod's page, keyed by `packageId`, and that is the only join the
    /// site actually performs. Sharing an id would have bought correlation nobody asked for
    /// and made an install trackable across two tools to get it.
    ///
    /// Salted and hashed on arrival; the raw value is never stored. It exists for two things,
    /// stopping one run being counted twice and rate limiting, and for nothing else.
    #[serde(skip)]
    pub install_id: String,
    /// Which tool sent this, so the site can tell a load run from a frame run.
    pub source: &'static str,
    pub manager_version: String,
    /// `major.minor` only.
    pub game_version: String,
    pub machine: Machine,

    /// The whole start-up.
    pub total_ms: f64,
    /// The part of it no mod accounted for: this machine's own baseline, and the divisor that
    /// makes the per-mod figures comparable with anyone else's.
    pub vanilla_ms: f64,
    pub mods_loaded: Option<u32>,
    pub defs_parsed: Option<u32>,
    pub patch_ops: Option<u32>,

    /// A hash of the active list's package ids in order, so identical lists match quickly
    /// without sending anything about the list that is not already here.
    pub list_hash: String,
    pub mods: Vec<ModCost>,
}

/// What the caller knows about a mod, reduced to the three things the wire may carry.
#[derive(Debug, Clone, PartialEq)]
pub struct ModFacts {
    pub package_id: String,
    pub workshop_id: Option<u64>,
    pub version: Option<String>,
}

/// Build the report.
///
/// Takes no path and no name, on purpose -- see the module comment. `mods` is the active list in
/// load order; only the ones the run measured end up in the payload, and only their versions are
/// sent.
pub fn build(
    impact: &StartupImpact,
    mods: &[ModFacts],
    machine: Machine,
    run_id: &str,
    install_id: &str,
    manager_version: &str,
    game_version: &str,
) -> LoadRunReport {
    let measured = impact.by_package();
    let off: std::collections::HashMap<&str, f64> =
        impact.mods.iter().map(|m| (m.package_id.as_str(), m.off_thread_ms)).collect();

    let mut costs: Vec<ModCost> = mods
        .iter()
        .filter_map(|m| {
            let total_ms = *measured.get(&m.package_id)?;
            Some(ModCost {
                package_id: m.package_id.clone(),
                workshop_id: m.workshop_id,
                version: m.version.clone(),
                total_ms,
                off_thread_ms: off.get(m.package_id.as_str()).copied().unwrap_or(0.0),
            })
        })
        .collect();
    // Sorted by package id, which throws the load order away.
    //
    // Nothing downstream wants it: a load cost is a property of a mod, not of where it sat in
    // somebody's list. Leaving the array in list order would have shipped the order anyway, as
    // a side effect of how it was built, and the order of a large list is close to unique --
    // it is one of the better fingerprints an install has. Sorting costs nothing and makes the
    // claim that the order is not sent true rather than nearly true.
    costs.sort_by(|a, b| a.package_id.cmp(&b.package_id));

    LoadRunReport {
        consent_version: CONSENT_VERSION,
        run_id: run_id.to_string(),
        install_id: install_id.to_string(),
        source: "modmanager",
        manager_version: manager_version.to_string(),
        game_version: game_version.to_string(),
        machine,
        total_ms: impact.total_ms,
        vanilla_ms: impact.vanilla_ms(),
        mods_loaded: impact.mods_loaded,
        defs_parsed: impact.defs_parsed,
        patch_ops: impact.patch_ops,
        list_hash: list_hash(mods.iter().map(|m| m.package_id.as_str())),
        mods: costs,
    }
}

/// A stable hash of *which* mods were in the list, not what order they were in.
///
/// Sorted first, for the same reason the cost array is: order is not wanted downstream and an
/// order-sensitive hash would smuggle it back in. It could not be read back out of a hash, but
/// it would still make the value more particular to one install than the set alone, and
/// "matching runs with similar lists" wants the set.
///
/// Not a secret and not reversible into anything the site does not already have: every id that
/// went into it is sent beside it.
pub fn list_hash<'a>(ids: impl Iterator<Item = &'a str>) -> String {
    use sha2::{Digest, Sha256};
    let mut sorted: Vec<&str> = ids.collect();
    sorted.sort_unstable();
    sorted.dedup();
    let mut h = Sha256::new();
    for id in sorted {
        h.update(id.as_bytes());
        h.update(b"\n");
    }
    hex::encode(&h.finalize()[..12])
}

/// Coarsen an OS string to the shape the privacy page promises.
///
/// "Windows 11 (10.0.22631)" and "Windows 11 Pro 23H2" both become "Windows 11". A build number
/// is a fingerprint and the site has no use for one.
pub fn coarse_os(raw: &str) -> String {
    let raw = raw.trim();
    // Everything from the first bracket or digit-dot-digit onwards is detail.
    let cut = raw.find('(').unwrap_or(raw.len());
    let head = &raw[..cut];
    let mut out = String::new();
    for word in head.split_whitespace() {
        // A word with a dot in it is a build or a point version; stop there.
        if word.contains('.') {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
        // Two tokens of "Windows 11" or "macOS 15" is as specific as this gets.
        if out.split_whitespace().count() >= 2 {
            break;
        }
    }
    if out.is_empty() { raw.to_string() } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::startupimpact::{ModImpact, StartupImpact};

    fn impact() -> StartupImpact {
        StartupImpact {
            total_ms: 100_000.0,
            mods: vec![
                ModImpact { package_id: "brrainz.harmony".into(), name: "Harmony".into(), total_ms: 900.0, off_thread_ms: 12.0 },
                ModImpact { package_id: "telardo.rimmsqol".into(), name: "RIMMSqol".into(), total_ms: 42_000.0, off_thread_ms: 0.0 },
            ],
            mods_loaded: Some(44),
            defs_parsed: Some(9000),
            patch_ops: Some(1400),
        }
    }
    fn facts() -> Vec<ModFacts> {
        vec![
            ModFacts { package_id: "brrainz.harmony".into(), workshop_id: Some(2009463077), version: Some("2.3.1".into()) },
            ModFacts { package_id: "telardo.rimmsqol".into(), workshop_id: None, version: Some("1.6.0".into()) },
            // In the list, not in the run: installed after it was measured.
            ModFacts { package_id: "some.newmod".into(), workshop_id: None, version: Some("0.1".into()) },
        ]
    }
    fn machine() -> Machine {
        Machine { cpu: "Ryzen 7 9800X3D".into(), cores: 16, memory_gb: 32, os: "Windows 11".into() }
    }
    fn report() -> LoadRunReport {
        build(&impact(), &facts(), machine(), "01JBQ7X2K9", "019fabdd75a720ad9cba70522627", "1.6.0", "1.6")
    }

    #[test]
    fn only_the_mods_the_run_measured_are_in_it() {
        let r = report();
        let ids: Vec<&str> = r.mods.iter().map(|m| m.package_id.as_str()).collect();
        assert_eq!(ids, vec!["brrainz.harmony", "telardo.rimmsqol"]);
    }

    #[test]
    fn the_machines_own_baseline_travels_with_the_figures() {
        // Without it the milliseconds cannot legitimately be pooled with anyone else's.
        let r = report();
        assert_eq!(r.vanilla_ms, 100_000.0 - 42_900.0);
        assert_eq!(r.total_ms, 100_000.0);
    }

    /// The guarantee the privacy page makes, checked on every build.
    #[test]
    fn no_path_can_reach_the_wire() {
        let json = serde_json::to_string(&report()).unwrap();
        for needle in ["C:\\", "/home/", "/Users/", "AppData", "\\\\", "Steam", ".xml", ".log"] {
            assert!(!json.contains(needle), "a path-shaped string reached the payload: {needle}\n{json}");
        }
        // No drive letter in any form.
        assert!(!json.contains(":\\") && !json.contains(":/"), "{json}");
    }

    /// The other half of it: a mod's name is never sent, and the parsed file is full of them.
    #[test]
    fn no_mod_name_can_reach_the_wire() {
        let json = serde_json::to_string(&report()).unwrap();
        for name in impact().mods.iter().map(|m| m.name.clone()) {
            assert!(!json.contains(&name), "a mod name reached the payload: {name}\n{json}");
        }
    }

    #[test]
    fn consent_is_to_a_set_of_fields_and_the_payload_says_which() {
        assert_eq!(report().consent_version, CONSENT_VERSION);
    }

    #[test]
    fn the_load_order_does_not_travel() {
        // The mods go out sorted, not in list order. The order of a large list is close to
        // unique and nothing downstream wants it, so shipping it as a side effect of how the
        // array was built would have been a fingerprint given away for free.
        let mut reversed = facts();
        reversed.reverse();
        let a = build(&impact(), &facts(), machine(), "r", "i", "1.6.0", "1.6");
        let b = build(&impact(), &reversed, machine(), "r", "i", "1.6.0", "1.6");
        assert_eq!(a.mods, b.mods, "two orders of the same list must send the same thing");
        let ids: Vec<&str> = a.mods.iter().map(|m| m.package_id.as_str()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }

    #[test]
    fn the_install_id_is_not_in_the_body() {
        // It goes in X-Circinus-Install, the way the site's existing ingest takes one. Keeping
        // it out of the body keeps it out of the append-only raw submission row as well.
        let json = serde_json::to_string(&report()).unwrap();
        assert!(!json.contains("019fabdd75a720ad9cba70522627"), "{json}");
        assert!(!json.contains("installId"), "{json}");
        assert!(json.contains("01JBQ7X2K9"), "the run id does travel in the body");
    }

    #[test]
    fn the_list_hash_is_of_the_membership_and_not_the_order() {
        // Reordering a list does not change what was measured, and an order-sensitive hash
        // would carry the order past the sort that deliberately drops it.
        assert_eq!(list_hash(["a", "b", "c"].into_iter()), list_hash(["c", "a", "b"].into_iter()));
        // Different mods are a different list.
        assert_ne!(list_hash(["a", "b", "c"].into_iter()), list_hash(["a", "b", "d"].into_iter()));
    }

    #[test]
    fn the_operating_system_loses_its_build_number() {
        assert_eq!(coarse_os("Windows 11 (10.0.22631)"), "Windows 11");
        assert_eq!(coarse_os("Windows 11 Pro 23H2"), "Windows 11");
        assert_eq!(coarse_os("macOS 15.1.1"), "macOS");
        assert_eq!(coarse_os("Windows 11"), "Windows 11");
    }
}
