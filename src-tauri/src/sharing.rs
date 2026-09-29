//! Whether load runs are shared with circinus.sh, and the id they are shared under.
//!
//! Two things live here and they are deliberately separate: the *decision*, which is the
//! player's and is stored with their settings, and the *id*, which is a random string generated
//! once and then never changed.
//!
//! The policy is the one published at <https://circinus.sh/privacy>. The parts this file is
//! responsible for:
//!
//!   - **Off until switched on.** `Sharing::default()` shares nothing. There is no build, no
//!     locale and no upgrade path where it starts on.
//!   - **Recording is not sharing.** Reading the measurement is unconditional, because the load
//!     card uses it whether or not anything leaves. Only sending is gated.
//!   - **Asked again if what is collected changes.** Consent is stored against
//!     `telemetry::CONSENT_VERSION`, so agreeing to version 1 does not authorise a version 2
//!     payload; the player is asked afresh and the queue waits.
//!   - **The id is the manager's own**, not the Performance Analyzer's. A load cost and a frame
//!     cost meet on the mod's page by package id, so nothing needs the two joined at the
//!     machine, and sharing an id would have made an install trackable across two tools for a
//!     correlation nobody uses.

use circinus_core::telemetry::CONSENT_VERSION;
use serde::{Deserialize, Serialize};

/// The player's answer, and what they answered about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Sharing {
    /// The id this install shares under. Empty until something is first shared; generating it
    /// lazily means an install that never turns sharing on never has one to leak.
    pub install_id: String,
    /// Whether load runs may be sent at all.
    pub share_load_runs: bool,
    /// Which set of fields the answer was given about. Zero means never asked.
    pub consent_version: u32,
    /// Unix seconds of the answer, so Settings can say when it was given.
    pub answered_at: i64,
}

impl Default for Sharing {
    fn default() -> Self {
        Self { install_id: String::new(), share_load_runs: false, consent_version: 0, answered_at: 0 }
    }
}

impl Sharing {
    /// Has the player been asked about *this* set of fields?
    ///
    /// A stored answer about an older set is not an answer about this one, which is what makes
    /// "you will be asked again if it changes" mechanical rather than a good intention.
    pub fn answered(&self) -> bool {
        self.consent_version == CONSENT_VERSION
    }

    /// May a run be sent right now?
    ///
    /// Both halves matter. An old yes does not authorise a new payload, and a yes with no id
    /// yet is a yes that has not been acted on.
    pub fn may_send(&self) -> bool {
        self.share_load_runs && self.answered() && !self.install_id.is_empty()
    }

    /// Record an answer. `yes` false is a real answer and is stored as one: the difference
    /// between "said no" and "never asked" is the difference between asking again next launch
    /// and leaving them alone.
    pub fn answer(&mut self, yes: bool, now: i64) {
        self.share_load_runs = yes;
        self.consent_version = CONSENT_VERSION;
        self.answered_at = now;
        if yes && self.install_id.is_empty() {
            self.install_id = new_install_id();
        }
    }

    /// Forget the id and start again under a new one.
    ///
    /// The site's delete is keyed on the id, so this is only offered *after* a delete has been
    /// asked for -- rotating first would strand the old data under an id nobody holds any more.
    pub fn rotate(&mut self) {
        self.install_id = new_install_id();
    }
}

/// A fresh install id: 28 lowercase hex characters, from the OS random source.
///
/// Not derived from hardware, an account, a username or anything else about the machine, which
/// is the property the privacy page actually claims -- "it isn't derived from your hardware,
/// your account or anything else about you". A hash of anything real would break that however
/// irreversible it looked.
///
/// The shape matches what the analyzer already shows players (`019fabdd75a720ad9cba70522627`),
/// so the two are recognisably the same kind of thing in the same kind of panel, even though
/// they are deliberately different values.
pub fn new_install_id() -> String {
    let mut bytes = [0u8; 14];
    getrandom::fill(&mut bytes).expect("the OS random source");
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_shared_out_of_the_box() {
        let s = Sharing::default();
        assert!(!s.share_load_runs);
        assert!(!s.answered(), "never asked is not the same as answered no");
        assert!(!s.may_send());
        assert!(s.install_id.is_empty(), "an install that never shares never has an id");
    }

    #[test]
    fn saying_yes_mints_an_id_and_saying_no_does_not() {
        let mut yes = Sharing::default();
        yes.answer(true, 1_700_000_000);
        assert!(yes.may_send());
        assert_eq!(yes.install_id.len(), 28);

        let mut no = Sharing::default();
        no.answer(false, 1_700_000_000);
        assert!(no.answered(), "a no is an answer and stops us asking again");
        assert!(!no.may_send());
        assert!(no.install_id.is_empty());
    }

    #[test]
    fn an_old_yes_does_not_authorise_a_new_payload() {
        let mut s = Sharing::default();
        s.answer(true, 1_700_000_000);
        // What gets collected changed since they agreed.
        s.consent_version = CONSENT_VERSION - 1;
        assert!(!s.answered());
        assert!(!s.may_send(), "the queue waits and the player is asked again");
        // And the answer they gave is still there, so the card can say what it was.
        assert!(s.share_load_runs);
    }

    #[test]
    fn the_id_is_not_derived_from_anything() {
        // Two ids minted in the same process on the same machine must differ, which is the
        // observable half of "not derived from your hardware or your account".
        assert_ne!(new_install_id(), new_install_id());
    }

    #[test]
    fn rotating_keeps_sharing_on_under_a_new_id() {
        let mut s = Sharing::default();
        s.answer(true, 1);
        let before = s.install_id.clone();
        s.rotate();
        assert_ne!(s.install_id, before);
        assert!(s.may_send());
    }
}
