//! A log on disk, and a report a player can hand over.
//!
//! Circinus logged to stdout, which a packaged app throws away: a user reporting that the window
//! went black had nothing to send and nobody had anything to read. Everything here exists so that
//! the next report arrives with the answer attached.
//!
//! Two pieces. `install` points tracing at a file in the app's own data folder, so the record
//! outlives the run that made it. `report` gathers that file's tail together with the things
//! worth knowing before reading it -- which build, which platform, which folders, how much was
//! found -- into one block of text to paste into Discord.
//!
//! Paths are written with the home directory replaced by `~`. A player asking for help should not
//! have to choose between being helped and posting their name in public, and a path is the one
//! thing in here that carries it.

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Bytes of log kept before the file is rolled over. Two files exist at most, so the worst case
/// on disk is twice this.
const MAX_LOG_BYTES: u64 = 4 * 1024 * 1024;
/// How much of the log the report carries. Enough for the run that went wrong, short enough to
/// paste into a chat window.
const REPORT_TAIL_BYTES: u64 = 24 * 1024;

pub fn log_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("logs")
}
pub fn log_path(data_dir: &Path) -> PathBuf {
    log_dir(data_dir).join("circinus.log")
}

/// A file every tracing line is appended to, shared by whichever threads write.
#[derive(Clone)]
pub struct LogFile(Arc<Mutex<File>>);

impl Write for &LogFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // A log that cannot be written must not take the app down with it: a full disk is a
        // reason to lose the log, not the session.
        match self.0.lock() {
            Ok(mut f) => f.write(buf).or(Ok(buf.len())),
            Err(_) => Ok(buf.len()),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self.0.lock() {
            Ok(mut f) => f.flush().or(Ok(())),
            Err(_) => Ok(()),
        }
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogFile {
    type Writer = &'a LogFile;
    fn make_writer(&'a self) -> Self::Writer {
        self
    }
}

/// Start logging to `<data>/logs/circinus.log`, keeping the previous run's file beside it.
///
/// Rolling happens on startup rather than while running: a size checked on every line is a lock
/// held on every line, and one file per run is what somebody reading a report actually wants.
pub fn install(data_dir: &Path) -> Option<PathBuf> {
    let dir = log_dir(data_dir);
    std::fs::create_dir_all(&dir).ok()?;
    let path = log_path(data_dir);
    if std::fs::metadata(&path).map(|m| m.len() > MAX_LOG_BYTES).unwrap_or(false) {
        let _ = std::fs::rename(&path, dir.join("circinus.previous.log"));
    }
    let file = OpenOptions::new().create(true).append(true).open(&path).ok()?;
    let writer = LogFile(Arc::new(Mutex::new(file)));
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive("circinus=info".parse().ok()?))
        .with_writer(writer)
        .with_ansi(false)
        .init();
    Some(path)
}

/// A path with the home directory written as `~`, on any platform.
pub fn tidy(p: &Path) -> String {
    let s = p.display().to_string();
    let home = circinus_core::paths::home_dir().map(|h| h.display().to_string()).unwrap_or_default();
    if !home.is_empty() && s.starts_with(&home) {
        return format!("~{}", &s[home.len()..]);
    }
    s
}

/// The last `REPORT_TAIL_BYTES` of the log, whole lines only.
fn tail(path: &Path) -> String {
    let Ok(mut f) = File::open(path) else { return "(no log file yet)".into() };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let from = len.saturating_sub(REPORT_TAIL_BYTES);
    if f.seek(SeekFrom::Start(from)).is_err() {
        return "(could not read the log)".into();
    }
    let mut text = String::new();
    if f.read_to_string(&mut text).is_err() {
        // A log with a stray byte in it is still worth most of its lines.
        let mut raw = Vec::new();
        let _ = File::open(path).and_then(|mut g| g.seek(SeekFrom::Start(from)).map(|_| g)).and_then(|mut g| g.read_to_end(&mut raw));
        text = String::from_utf8_lossy(&raw).into_owned();
    }
    // The first line is probably half a line.
    if from > 0 {
        if let Some(i) = text.find('\n') {
            text = text[i + 1..].to_string();
        }
    }
    let home = circinus_core::paths::home_dir().map(|h| h.display().to_string()).unwrap_or_default();
    if home.is_empty() {
        text
    } else {
        text.replace(&home, "~")
    }
}

/// One folder, and whether it is really there. "Not set" and "set to somewhere that is gone" are
/// different problems and the report should not blur them.
fn folder(label: &str, p: Option<&PathBuf>) -> String {
    match p {
        None => format!("  {label}: not set"),
        Some(p) => format!("  {label}: {}{}", tidy(p), if p.exists() { "" } else { "   <- MISSING" }),
    }
}

/// What a player pastes into Discord when something went wrong.
pub fn report(app: &crate::state::App) -> String {
    let loc = &app.locations;
    let mut out = String::new();
    out.push_str(&format!("Circinus Mod Manager {}\n", env!("CARGO_PKG_VERSION")));
    out.push_str(&format!("{} {}\n", std::env::consts::OS, std::env::consts::ARCH));
    out.push_str(&format!("Game version: {}\n", app.game_version.full));
    out.push_str(&format!("Instance: {} (of {})\n", app.instance.name, app.instance_count()));
    out.push_str("\nFolders\n");
    out.push_str(&format!("{}\n", folder("Game", loc.game_dir.as_ref())));
    out.push_str(&format!("{}\n", folder("Config", loc.config_dir.as_ref())));
    out.push_str(&format!("{}\n", folder("Local mods", loc.local_mods_dir.as_ref())));
    out.push_str(&format!("{}\n", folder("Workshop", loc.workshop_dir.as_ref())));
    if let Some(d) = loc.data_dir() {
        out.push_str(&format!("{}\n", folder("Game data", Some(&d))));
    }
    out.push_str("\nWhat it found\n");
    out.push_str(&format!("  {} mods installed, {} active, {} in the list but not installed\n", app.mods.len(), app.active.len(), app.missing.len()));
    out.push_str(&format!("  {} still being inspected, {} entries it could not read\n", app.shallow.len(), app.unreadable.len()));
    out.push_str(&format!("  rule databases: {}\n", if app.db.loaded.is_empty() { "none loaded".to_string() } else { app.db.loaded.join(", ") }));
    out.push_str(&format!("  unsaved changes: {}\n", if app.dirty { "yes" } else { "no" }));
    out.push_str(&format!("\nLog ({})\n", tidy(&log_path(&app.data_dir))));
    out.push_str("----------------------------------------\n");
    out.push_str(&tail(&log_path(&app.data_dir)));
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str("----------------------------------------\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_home_directory_does_not_go_out_with_the_report() {
        let home = circinus_core::paths::home_dir().expect("a home directory to hide");
        let inside = home.join("Games").join("RimWorld");
        let said = tidy(&inside);
        assert!(said.starts_with('~'), "{said}");
        assert!(!said.contains(&home.display().to_string()), "the report would carry the user's name: {said}");
        // A path outside it is left alone: hiding what is not private only makes the report harder
        // to read.
        let outside = PathBuf::from("/opt/games/RimWorld");
        assert_eq!(tidy(&outside), "/opt/games/RimWorld");
    }

    #[test]
    fn the_tail_is_whole_lines_and_the_end_of_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("circinus.log");
        let mut f = File::create(&p).unwrap();
        for i in 0..4000 {
            writeln!(f, "line {i} ------------------------------------------------------------").unwrap();
        }
        drop(f);
        let t = tail(&p);
        assert!(t.len() as u64 <= REPORT_TAIL_BYTES, "{} bytes", t.len());
        assert!(t.contains("line 3999"), "the end of the log is the part worth having");
        assert!(!t.contains("line 0 "), "and the start of it is not");
        let first = t.lines().next().unwrap_or("");
        assert!(first.starts_with("line "), "a half line is not worth reading: {first:?}");
    }

    #[test]
    fn no_log_yet_is_said_rather_than_left_blank() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(tail(&tmp.path().join("nothing.log")), "(no log file yet)");
    }
}
