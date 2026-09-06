//! When Circinus first saw each mod, and which of those still count as new.
//!
//! `changes` already answers "what is different from the baseline", but it answers it once: the
//! baseline is retaken on every scan, so a mod is Added for exactly as long as it takes the next
//! scan to run. That is the right shape for a "here is what happened while you were away" dialog
//! and the wrong shape for a mark on a row, which has to survive rescans, restarts and the eight
//! second folder poll.
//!
//! So this is a small durable record of its own: uid to the moment that folder first appeared.
//! Two rules make it behave:
//!
//! * The first time a record is made, everything installed is stamped 0 — "already here when we
//!   started looking". Without that, opening Circinus for the first time, or opening a second
//!   instance, would announce twelve hundred new mods, and a mark that is on everything marks
//!   nothing.
//! * New is not forever. A mod stops being new when the user says so, and in any case after
//!   [`NEW_FOR_DAYS`], because a mark nobody cleared should fade rather than accumulate.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// How long a mod stays marked new without anyone saying they have seen it.
pub const NEW_FOR_DAYS: i64 = 14;

const DAY: i64 = 86_400;

/// The record. Scoped per instance, for the reason the baseline is (see `instances`): switching
/// instance changes which mods are installed wholesale, and a shared record would call the other
/// instance's entire library new the moment you looked at it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Arrivals {
    /// Unix seconds when this record was started. Zero means it never has been.
    pub started_at: i64,
    /// uid → unix seconds Circinus first saw that folder. Zero means it was already there when
    /// the record was started, which is not the same as arriving at the epoch.
    pub seen: HashMap<String, i64>,
    /// Unix seconds the user last said they had seen what was new.
    pub cleared_at: i64,
}

impl Arrivals {
    /// Fold one scan in: stamp anything not seen before, and forget folders that are gone.
    ///
    /// Returns whether anything changed, so a poll that finds nothing new writes nothing.
    ///
    /// Forgetting removed folders is deliberate. A mod uninstalled and installed again is new
    /// again, which is what a player means by it; keeping the old stamp would leave a
    /// reinstalled mod silently unmarked. The cost is that a folder made unreachable for a
    /// moment — an unplugged drive, a Steam library being verified — comes back as new. That is
    /// the safer way round: an extra mark is noise, a missing one is a mod that quietly arrived.
    pub fn observe<I: IntoIterator<Item = String>>(&mut self, uids: I, now: i64) -> bool {
        let live: HashSet<String> = uids.into_iter().collect();
        // Nothing was ever recorded, so this is not an install: it is the first look. Everything
        // here now predates the record and none of it is new.
        if self.started_at == 0 {
            self.started_at = now;
            self.seen = live.into_iter().map(|u| (u, 0)).collect();
            return true;
        }
        let mut changed = false;
        for uid in &live {
            if !self.seen.contains_key(uid) {
                self.seen.insert(uid.clone(), now);
                changed = true;
            }
        }
        let before = self.seen.len();
        self.seen.retain(|uid, _| live.contains(uid));
        changed || self.seen.len() != before
    }

    /// When Circinus first saw this mod, or `None` for one that predates the record.
    pub fn first_seen(&self, uid: &str) -> Option<i64> {
        match self.seen.get(uid) {
            Some(&t) if t > 0 => Some(t),
            _ => None,
        }
    }

    /// Is this mod still worth marking?
    pub fn is_new(&self, uid: &str, now: i64) -> bool {
        match self.first_seen(uid) {
            Some(t) => t > self.cleared_at && now - t < NEW_FOR_DAYS * DAY,
            None => false,
        }
    }

    /// Every mod still worth marking, the most recent arrival first.
    pub fn new_uids(&self, now: i64) -> Vec<String> {
        let mut out: Vec<(&String, i64)> = self.seen.iter().filter(|(u, _)| self.is_new(u, now)).map(|(u, &t)| (u, t)).collect();
        // Newest first, and by uid within one scan so the order does not wander between runs:
        // everything a single scan finds shares a timestamp to the second.
        out.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        out.into_iter().map(|(u, _)| u.clone()).collect()
    }

    /// The user has seen them. Nothing already here counts as new again.
    pub fn clear(&mut self, now: i64) {
        self.cleared_at = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_first_look_is_not_an_installation() {
        let mut a = Arrivals::default();
        assert!(a.observe(uids(&["a", "b", "c"]), 1_000));
        assert!(a.new_uids(1_000).is_empty(), "a fresh install would otherwise announce every mod it found");
        assert_eq!(a.first_seen("a"), None);
        assert_eq!(a.started_at, 1_000);
    }

    #[test]
    fn a_folder_that_was_not_there_before_is_new() {
        let mut a = Arrivals::default();
        a.observe(uids(&["a", "b"]), 1_000);
        assert!(a.observe(uids(&["a", "b", "c"]), 2_000));
        assert_eq!(a.new_uids(2_000), uids(&["c"]));
        assert_eq!(a.first_seen("c"), Some(2_000));
        assert!(a.is_new("c", 2_000));
        assert!(!a.is_new("a", 2_000));
    }

    #[test]
    fn scanning_again_with_nothing_new_writes_nothing() {
        let mut a = Arrivals::default();
        a.observe(uids(&["a", "b"]), 1_000);
        assert!(!a.observe(uids(&["a", "b"]), 2_000), "the eight second poll must not rewrite the record every tick");
        assert!(!a.observe(uids(&["b", "a"]), 3_000), "and the order the scan happens to return is not a change");
    }

    #[test]
    fn a_mod_reinstalled_is_new_again() {
        let mut a = Arrivals::default();
        a.observe(uids(&["a", "b"]), 1_000);
        assert!(a.observe(uids(&["a"]), 2_000), "b was uninstalled");
        assert!(!a.seen.contains_key("b"));
        a.observe(uids(&["a", "b"]), 3_000);
        assert_eq!(a.new_uids(3_000), uids(&["b"]));
    }

    #[test]
    fn saying_you_have_seen_them_clears_the_mark_without_forgetting_when_they_came() {
        let mut a = Arrivals::default();
        a.observe(uids(&["a"]), 1_000);
        a.observe(uids(&["a", "b"]), 2_000);
        a.clear(2_500);
        assert!(a.new_uids(2_600).is_empty());
        // The date is still there, because the list still wants to say when a mod arrived.
        assert_eq!(a.first_seen("b"), Some(2_000));
        // And the next arrival is unaffected.
        a.observe(uids(&["a", "b", "c"]), 3_000);
        assert_eq!(a.new_uids(3_000), uids(&["c"]));
    }

    #[test]
    fn new_fades_on_its_own() {
        let mut a = Arrivals::default();
        a.observe(uids(&["a"]), 1_000);
        a.observe(uids(&["a", "b"]), 2_000);
        let almost = 2_000 + NEW_FOR_DAYS * DAY - 1;
        assert!(a.is_new("b", almost));
        assert!(!a.is_new("b", almost + 1), "a mark nobody cleared should fade rather than pile up");
    }

    #[test]
    fn the_newest_arrival_is_first_and_the_order_does_not_wander() {
        let mut a = Arrivals::default();
        a.observe(uids(&["a"]), 1_000);
        a.observe(uids(&["a", "m", "d"]), 2_000);
        a.observe(uids(&["a", "m", "d", "z"]), 3_000);
        // z arrived last, and the two that came together are in a fixed order rather than
        // whatever order the hash map happened to hand back.
        assert_eq!(a.new_uids(3_000), uids(&["z", "d", "m"]));
        assert_eq!(a.new_uids(3_000), uids(&["z", "d", "m"]));
    }

    #[test]
    fn it_survives_a_round_trip_through_json() {
        let mut a = Arrivals::default();
        a.observe(uids(&["a"]), 1_000);
        a.observe(uids(&["a", "b"]), 2_000);
        let text = serde_json::to_string(&a).expect("serialises");
        let back: Arrivals = serde_json::from_str(&text).expect("reads back");
        assert_eq!(a, back);
        assert_eq!(back.new_uids(2_000), uids(&["b"]));
    }
}
