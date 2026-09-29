//! The queue of load runs waiting to go to circinus.sh, and the thing that sends them.
//!
//! A spool of JSON files on disk rather than an in-memory list, for one reason: the endpoint
//! does not exist yet. A build in the wild today measures, queues, fails to send, and keeps the
//! run; the day the site answers, everything held starts moving with no new release. An
//! in-memory queue would throw all of that away at every quit.
//!
//! Three rules the rest of the file exists to keep.
//!
//! **Nothing is queued without consent.** `Sharing::may_send` is checked before a file is
//! written, not before it is sent. A run spooled under a yes and then declined must not go, so
//! the gate is also re-checked at send time -- but not writing it in the first place means a
//! player who never says yes has nothing about their machine sitting in a folder either.
//!
//! **Failure is silent.** Sharing a run is not something anybody asked to watch succeed, and a
//! toast about a background upload nobody requested is noise about a thing they cannot fix.
//!
//! **A bad payload is dropped, a bad day is retried.** 400 and 422 mean this run will never be
//! accepted, so keeping it would be a file that fails forever. Timeouts, 5xx and 429 mean try
//! later. The distinction is the whole reason the status codes are handled one at a time rather
//! than as "not 2xx".

use crate::commands::Shared;
use circinus_core::telemetry::LoadRunReport;
use std::path::{Path, PathBuf};

/// Where the spool lives. One file per run, named by run id, so a half-written one is obvious
/// and a duplicate is impossible.
pub fn dir(data_dir: &Path) -> PathBuf {
    data_dir.join("outbox")
}

/// Keep at most this many. A player who has been offline for a month does not need three
/// hundred runs sent when they come back, and the newest are the ones worth having: the oldest
/// describe a list they have since changed.
const KEEP: usize = 20;

/// Write a run to the spool.
///
/// Best effort. A spool that cannot be written is a run that is not shared, which is a
/// perfectly acceptable outcome and not worth telling anybody about.
pub fn enqueue(data_dir: &Path, report: &LoadRunReport) {
    let d = dir(data_dir);
    if std::fs::create_dir_all(&d).is_err() {
        return;
    }
    let path = d.join(format!("{}.json", sanitise(&report.run_id)));
    // The install id is not in the serialised body -- it goes in a header -- so it is written
    // beside the payload rather than into it, and the spool file carries no more than the wire
    // does.
    let Ok(body) = serde_json::to_string(report) else { return };
    let wrapper = serde_json::json!({ "installId": report.install_id, "body": body, "tries": 0 });
    let _ = std::fs::write(&path, wrapper.to_string());
    prune(&d);
}

/// A run id is ours, but it still becomes a filename, so it is not trusted to be one.
fn sanitise(id: &str) -> String {
    id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').take(64).collect()
}

fn prune(d: &Path) {
    let Ok(rd) = std::fs::read_dir(d) else { return };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = rd
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    if files.len() <= KEEP {
        return;
    }
    files.sort_by_key(|(t, _)| *t);
    for (_, p) in files.iter().take(files.len() - KEEP) {
        let _ = std::fs::remove_file(p);
    }
}

/// Throw the queue away.
///
/// Called when sharing is switched off. A spooled run is a description of this machine sitting
/// in a folder, and somebody who has just said no should not have to trust that it will
/// eventually expire unsent.
pub fn discard(data_dir: &Path) {
    let _ = std::fs::remove_dir_all(dir(data_dir));
}

/// What the site said, reduced to what the queue needs to decide.
enum Verdict {
    /// Accepted, or already had it. Either way we are done with this run.
    Done,
    /// This run will never be accepted. Keeping it would be a file that fails for ever.
    Drop,
    /// The site is having a bad day, or we are asking too often. Keep it and try later.
    Later,
    /// The agreement the run was sent under is not one the site offers any more. Drop the run
    /// *and* clear the stored consent, so the player is asked again about the new one rather
    /// than having an old yes silently reinterpreted.
    Reconsent,
}

fn verdict(status: u16) -> Verdict {
    match status {
        200 | 201 | 202 | 409 => Verdict::Done,
        422 => Verdict::Reconsent,
        400 | 413 => Verdict::Drop,
        _ => Verdict::Later,
    }
}

/// Send whatever is queued, oldest first.
///
/// Returns true if the stored consent was cleared, so the caller can put the card back up.
pub async fn flush(shared: &Shared, base: &str) -> bool {
    let (data_dir, may, install_id) = match shared.lock() {
        Ok(a) => (a.data_dir.clone(), a.user.sharing.may_send(), a.user.sharing.install_id.clone()),
        Err(_) => return false,
    };
    // Checked again here and not only at enqueue time: a run spooled under a yes must not go
    // out after the player has changed their mind.
    if !may {
        return false;
    }

    let d = dir(&data_dir);
    let Ok(rd) = std::fs::read_dir(&d) else { return false };
    let mut files: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();

    let client = match reqwest::Client::builder()
        .user_agent(concat!("CircinusModManager/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(20))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    let url = format!("{base}/load-runs");

    for path in files {
        let Ok(raw) = std::fs::read_to_string(&path) else { continue };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&raw) else {
            // Not something we wrote, or written half-way through a power cut.
            let _ = std::fs::remove_file(&path);
            continue;
        };
        let body = v.get("body").and_then(|b| b.as_str()).unwrap_or_default().to_string();
        if body.is_empty() {
            let _ = std::fs::remove_file(&path);
            continue;
        }
        let id = v.get("installId").and_then(|b| b.as_str()).unwrap_or(&install_id).to_string();

        let sent = client
            .post(&url)
            .header("X-Circinus-Install", &id)
            .header("Content-Type", "application/json")
            .body(body)
            .send()
            .await;

        match sent {
            Ok(r) => match verdict(r.status().as_u16()) {
                Verdict::Done | Verdict::Drop => {
                    let _ = std::fs::remove_file(&path);
                }
                Verdict::Reconsent => {
                    let _ = std::fs::remove_file(&path);
                    if let Ok(mut a) = shared.lock() {
                        // Not a no: the answer is forgotten, not reversed, so the card comes
                        // back and asks about what is collected now.
                        a.user.sharing.consent_version = 0;
                        let _ = a.persist();
                    }
                    return true;
                }
                Verdict::Later => return false,
            },
            // Offline, or the endpoint does not exist yet. Both are "try later", and the
            // second is the normal case until the site half ships.
            Err(_) => return false,
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use circinus_core::telemetry::{LoadRunReport, Machine};

    fn report(id: &str) -> LoadRunReport {
        LoadRunReport {
            consent_version: 1,
            run_id: id.into(),
            install_id: "abc123".into(),
            source: "modmanager",
            manager_version: "1.6.0".into(),
            game_version: "1.6".into(),
            machine: Machine::default(),
            total_ms: 1000.0,
            vanilla_ms: 400.0,
            mods_loaded: None,
            defs_parsed: None,
            patch_ops: None,
            list_hash: "deadbeef".into(),
            mods: vec![],
        }
    }

    #[test]
    fn a_queued_run_survives_being_written_and_read() {
        let tmp = std::env::temp_dir().join(format!("cx-outbox-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        enqueue(&tmp, &report("run-one"));
        let files: Vec<_> = std::fs::read_dir(dir(&tmp)).unwrap().flatten().collect();
        assert_eq!(files.len(), 1);
        let raw = std::fs::read_to_string(files[0].path()).unwrap();
        // The install id is beside the payload, not in it: the spool carries no more than the
        // wire does.
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["installId"], "abc123");
        assert!(!v["body"].as_str().unwrap().contains("abc123"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn the_spool_does_not_grow_without_bound() {
        let tmp = std::env::temp_dir().join(format!("cx-outbox-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        for i in 0..KEEP + 7 {
            enqueue(&tmp, &report(&format!("run-{i:03}")));
        }
        let n = std::fs::read_dir(dir(&tmp)).unwrap().flatten().count();
        assert!(n <= KEEP, "{n} files kept");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn saying_no_leaves_nothing_behind() {
        // A spooled run is a description of this machine sitting in a folder. Somebody who has
        // just switched sharing off should not have to trust that it expires unsent.
        let tmp = std::env::temp_dir().join(format!("cx-outbox-discard-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        enqueue(&tmp, &report("run-one"));
        enqueue(&tmp, &report("run-two"));
        assert_eq!(std::fs::read_dir(dir(&tmp)).unwrap().flatten().count(), 2);
        discard(&tmp);
        assert!(!dir(&tmp).exists());
        // And discarding an empty queue is not an error.
        discard(&tmp);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn a_run_id_cannot_become_a_path() {
        assert_eq!(sanitise("../../etc/passwd"), "etcpasswd");
        assert_eq!(sanitise("01JBQ7X2K9"), "01JBQ7X2K9");
    }

    #[test]
    fn a_bad_payload_is_dropped_and_a_bad_day_is_retried() {
        // The distinction the whole queue turns on: 400 will be 400 for ever, 503 will not.
        assert!(matches!(verdict(201), Verdict::Done));
        assert!(matches!(verdict(409), Verdict::Done));
        assert!(matches!(verdict(400), Verdict::Drop));
        assert!(matches!(verdict(422), Verdict::Reconsent));
        assert!(matches!(verdict(429), Verdict::Later));
        assert!(matches!(verdict(503), Verdict::Later));
        assert!(matches!(verdict(404), Verdict::Later), "an endpoint that does not exist yet is not a bad payload");
    }
}
