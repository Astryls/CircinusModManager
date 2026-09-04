//! Converting a mod's textures in bulk: find the PNGs RimWorld would load, skip the ones an
//! author already ships as DDS and the ones already converted from an unchanged source, encode
//! the rest in parallel, validate each file before it is put in place, and keep a manifest so
//! everything Circinus wrote can be recognised later — re-checked when the mod changes, and
//! deleted on request without touching anyone else's files.

use super::encode::{self, Encoded, Format, Options, PARAMS_VERSION};
use super::validate;
use crate::{Error, Result};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use walkdir::WalkDir;

/// Suffix of the copy kept when Circinus replaces a DDS someone else wrote.
pub const BACKUP_SUFFIX: &str = ".circinus-orig";

/// One converted file as the manifest records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// Path of the source relative to the mod folder, forward slashes, original case: the PNG,
    /// or — for a foreign DDS rebuilt from its own pixels — the DDS itself.
    pub rel: String,
    pub src_len: u64,
    /// Milliseconds since the epoch.
    pub src_mtime: u64,
    pub src_hash: u64,
    pub dds_len: u64,
    pub dds_hash: u64,
    pub format: Format,
    pub params: u32,
    pub created_at: i64,
    /// DDS dimensions (multiples of 4); what the GPU holds uncompressed is w×h×4 bytes ×1.33 with mips.
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    /// When this replaced a DDS someone else wrote: the backup of the original, relative to the
    /// mod folder. Revert puts it back instead of just deleting ours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaced: Option<String>,
}

impl Entry {
    /// The DDS this entry accounts for, relative to the mod folder.
    pub fn dds_rel(&self) -> String {
        Path::new(&self.rel).with_extension("dds").to_string_lossy().replace('\\', "/")
    }
}

/// A PNG under a `Textures` folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub rel: String,
    pub png: PathBuf,
    pub dds: PathBuf,
    pub len: u64,
    pub mtime: u64,
}

/// Milliseconds: files copied within the same second are still told apart.
fn mtime_ms(md: &std::fs::Metadata) -> u64 {
    md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub fn hash(bytes: &[u8]) -> u64 {
    xxhash_rust::xxh3::xxh3_64(bytes)
}

/// Every PNG that lives under a `Textures` directory anywhere in the mod (root, version
/// folders, LoadFolders targets). `About/Preview.png` and the like are not textures.
pub fn find_pngs(mod_root: &Path) -> Vec<Candidate> {
    let mut out = Vec::new();
    let walker = WalkDir::new(mod_root).follow_links(false).into_iter().filter_entry(|e| {
        let n = e.file_name().to_string_lossy();
        !(e.depth() > 0 && (n.starts_with('.') || n.eq_ignore_ascii_case("Source") || n.eq_ignore_ascii_case("About") || n.eq_ignore_ascii_case("obj")))
    });
    for entry in walker.filter_map(|e| e.ok()) {
        let Ok(md) = entry.metadata() else { continue };
        if !md.is_file() {
            continue;
        }
        let path = entry.path();
        if !path.extension().map(|e| e.eq_ignore_ascii_case("png")).unwrap_or(false) {
            continue;
        }
        let rel_path = path.strip_prefix(mod_root).unwrap_or(path);
        let segs: Vec<String> = rel_path.iter().map(|s| s.to_string_lossy().to_string()).collect();
        if segs.len() < 2 || !segs[..segs.len() - 1].iter().any(|s| s.eq_ignore_ascii_case("textures")) {
            continue;
        }
        let rel = segs.join("/");
        out.push(Candidate { rel, png: path.to_path_buf(), dds: path.with_extension("dds"), len: md.len(), mtime: mtime_ms(&md) });
    }
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    out
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Skip {
    /// A `.dds` exists that Circinus did not write: the author's, or another tool's.
    Shipped,
    /// Converted before from this very source with these settings.
    Current,
}

#[derive(Debug, Clone)]
pub struct Work {
    pub candidate: Candidate,
    /// Why it needs doing: new, or the previous conversion is stale/missing.
    pub reason: &'static str,
}

#[derive(Debug, Default, Clone)]
pub struct Plan {
    pub work: Vec<Work>,
    pub shipped: usize,
    pub current: usize,
    pub current_dds_bytes: u64,
}

/// Decide what to do with each PNG given the manifest and the options.
pub fn plan(candidates: Vec<Candidate>, entries: &HashMap<String, Entry>, opts: &Options) -> Plan {
    let mut plan = Plan::default();
    for c in candidates {
        let dds_exists = c.dds.is_file();
        match entries.get(&c.rel) {
            None if dds_exists => plan.shipped += 1,
            None => plan.work.push(Work { candidate: c, reason: "new" }),
            Some(e) => {
                let unchanged = e.src_len == c.len && e.src_mtime == c.mtime;
                let same_settings = e.params == PARAMS_VERSION && (e.format == Format::Bc1 || e.format == opts.alpha_format);
                if !dds_exists {
                    plan.work.push(Work { candidate: c, reason: "dds missing" });
                } else if !unchanged {
                    plan.work.push(Work { candidate: c, reason: "source changed" });
                } else if !same_settings {
                    plan.work.push(Work { candidate: c, reason: "settings changed" });
                } else {
                    plan.current += 1;
                    plan.current_dds_bytes += e.dds_len;
                }
            }
        }
    }
    plan
}

/// Write `bytes` next to the PNG: temp file first, validated content only, then an atomic
/// swap into place.
pub(super) fn put_in_place(dds: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = dds.with_extension("dds.circinus-tmp");
    std::fs::write(&tmp, bytes)?;
    if dds.exists() {
        // Windows will not rename over an existing file.
        std::fs::remove_file(dds)?;
    }
    if let Err(e) = std::fs::rename(&tmp, dds) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    Ok(())
}

/// Convert one PNG. Nothing is left behind on failure.
pub fn convert_one(c: &Candidate, opts: &Options, now: i64) -> Result<(Entry, Encoded)> {
    let png = std::fs::read(&c.png)?;
    let src_hash = hash(&png);
    let encoded = encode::encode_png(&png, opts)?;
    validate::check(&encoded).map_err(Error::Other)?;
    put_in_place(&c.dds, &encoded.bytes)?;
    let entry = Entry {
        rel: c.rel.clone(),
        src_len: c.len,
        src_mtime: c.mtime,
        src_hash,
        dds_len: encoded.bytes.len() as u64,
        dds_hash: hash(&encoded.bytes),
        format: encoded.format,
        params: PARAMS_VERSION,
        created_at: now,
        width: encoded.width,
        height: encoded.height,
        replaced: None,
    };
    Ok((entry, encoded))
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub total: usize,
    pub done: usize,
    pub converted: usize,
    pub failed: usize,
    pub png_bytes: u64,
    pub dds_bytes: u64,
    pub current: String,
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub uid: String,
    pub rel: String,
    pub result: std::result::Result<Entry, String>,
    pub png_len: u64,
}

/// Convert everything in `work` on a pool of `threads` threads (0 = all but one core).
/// `on_progress` is called from worker threads, throttled; `cancel` stops after the current
/// files.
pub fn run(work: Vec<(String, Candidate)>, opts: &Options, threads: usize, cancel: &AtomicBool, on_progress: &(dyn Fn(&Progress) + Sync)) -> Vec<Outcome> {
    let threads = if threads == 0 { std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).saturating_sub(1).max(1) } else { threads };
    let pool = rayon::ThreadPoolBuilder::new().num_threads(threads).thread_name(|i| format!("circinus-dds-{i}")).build();
    let total = work.len();
    let progress = Mutex::new(Progress { total, ..Progress::default() });
    let last_emit = Mutex::new(std::time::Instant::now());
    let done = AtomicUsize::new(0);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let job = |(uid, c): (String, Candidate)| -> Outcome {
        if cancel.load(Ordering::Relaxed) {
            return Outcome { uid, rel: c.rel.clone(), result: Err("cancelled".into()), png_len: c.len };
        }
        let result = convert_one(&c, opts, now);
        let n = done.fetch_add(1, Ordering::Relaxed) + 1;
        {
            let mut p = progress.lock().unwrap();
            p.done = n;
            p.current = c.rel.clone();
            match &result {
                Ok((e, _)) => {
                    p.converted += 1;
                    p.png_bytes += c.len;
                    p.dds_bytes += e.dds_len;
                }
                Err(_) => p.failed += 1,
            }
            let mut last = last_emit.lock().unwrap();
            if last.elapsed().as_millis() >= 200 || n == total {
                *last = std::time::Instant::now();
                on_progress(&p);
            }
        }
        Outcome { uid, rel: c.rel.clone(), result: result.map(|(e, _)| e).map_err(|e| e.to_string()), png_len: c.len }
    };
    match pool {
        Ok(pool) => pool.install(|| work.into_par_iter().map(job).collect()),
        Err(_) => work.into_iter().map(job).collect(),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Revalidation {
    /// Entries still good.
    pub keep: Vec<Entry>,
    /// Sources that changed or vanished; their DDS was deleted (if still ours).
    pub stale: Vec<String>,
    /// DDS files that are gone (Steam removed them, the user deleted them).
    pub missing: Vec<String>,
    /// DDS files that are not the ones Circinus wrote any more; left alone, dropped from the manifest.
    pub foreign: Vec<String>,
}

fn is_ours(dds: &Path, e: &Entry) -> bool {
    match std::fs::read(dds) {
        Ok(bytes) => bytes.len() as u64 == e.dds_len && hash(&bytes) == e.dds_hash,
        Err(_) => false,
    }
}

/// After a mod changed: drop DDS files whose PNG changed (the game would otherwise load
/// old art) and forget entries whose files are gone or no longer ours. Every PNG is hashed —
/// this runs for changed mods only, and a Workshop update can rewrite a file in place.
pub fn revalidate(mod_root: &Path, entries: &[Entry]) -> Revalidation {
    let mut r = Revalidation::default();
    for e in entries {
        let dds = mod_root.join(e.dds_rel());
        let backup = e.replaced.as_ref().map(|b| mod_root.join(b));
        if !dds.is_file() {
            r.missing.push(e.rel.clone());
            continue;
        }
        // The source is the PNG, or the original DDS we kept when there was no PNG.
        let source = if e.rel.eq_ignore_ascii_case(&e.dds_rel()) { backup.clone().unwrap_or_else(|| dds.clone()) } else { mod_root.join(&e.rel) };
        let source_ok = std::fs::read(&source).map(|b| hash(&b) == e.src_hash).unwrap_or(false);
        if source_ok && is_ours(&dds, e) {
            r.keep.push(e.clone());
            continue;
        }
        if is_ours(&dds, e) {
            // Source changed under us: restore the original if we kept one, else drop ours.
            match backup.filter(|b| b.is_file()) {
                Some(b) if std::fs::remove_file(&dds).is_ok() && std::fs::rename(&b, &dds).is_ok() => {}
                _ => {
                    let _ = std::fs::remove_file(&dds);
                }
            }
            r.stale.push(e.rel.clone());
        } else {
            // The mod's author (or Steam) rewrote the file: it is theirs again, and the backup
            // of their older version is of no use to anyone.
            if let Some(b) = backup {
                let _ = std::fs::remove_file(b);
            }
            r.foreign.push(e.rel.clone());
        }
    }
    r
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reverted {
    pub deleted: Vec<String>,
    /// Files that were not the ones Circinus wrote; left in place.
    pub kept: Vec<String>,
    /// Originals put back from their backups.
    #[serde(default)]
    pub restored: Vec<String>,
    pub bytes_freed: u64,
}

/// Delete the DDS files Circinus created for a mod — only those, and only if unchanged — and
/// put back any original it replaced.
pub fn revert(mod_root: &Path, entries: &[Entry]) -> Reverted {
    let mut r = Reverted::default();
    for e in entries {
        let dds = mod_root.join(e.dds_rel());
        if !dds.is_file() {
            continue;
        }
        if !is_ours(&dds, e) {
            r.kept.push(e.rel.clone());
            continue;
        }
        if std::fs::remove_file(&dds).is_err() {
            r.kept.push(e.rel.clone());
            continue;
        }
        r.bytes_freed += e.dds_len;
        match e.replaced.as_ref().map(|b| mod_root.join(b)).filter(|b| b.is_file()) {
            Some(b) if std::fs::rename(&b, &dds).is_ok() => r.restored.push(e.rel.clone()),
            _ => r.deleted.push(e.rel.clone()),
        }
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn png(w: u32, h: u32, alpha: bool) -> Vec<u8> {
        let img = RgbaImage::from_fn(w, h, |x, y| Rgba([(x * 7) as u8, (y * 5) as u8, 120, if alpha && x < w / 2 { 0 } else { 255 }]));
        let mut out = Vec::new();
        image::write_buffer_with_format(&mut std::io::Cursor::new(&mut out), img.as_raw(), w, h, image::ColorType::Rgba8, image::ImageFormat::Png).unwrap();
        out
    }

    fn fixture(dir: &Path) {
        let w = |p: &str, b: &[u8]| {
            let p = dir.join(p);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b).unwrap();
        };
        w("About/About.xml", b"<ModMetaData/>");
        w("About/Preview.png", &png(8, 8, false));
        w("Textures/Things/Wall.png", &png(32, 16, false));
        w("Textures/UI/Icon.png", &png(24, 24, true));
        w("1.6/Textures/Pawn/Body.png", &png(20, 40, true));
        w("Textures/Things/Shipped.png", &png(8, 8, false));
        w("Textures/Things/Shipped.dds", b"authors own file");
        w("Defs/Things.xml", b"<Defs/>");
    }

    #[test]
    fn finds_only_textures() {
        let tmp = tempfile::tempdir().unwrap();
        fixture(tmp.path());
        let c = find_pngs(tmp.path());
        let rels: Vec<&str> = c.iter().map(|c| c.rel.as_str()).collect();
        assert_eq!(rels, vec!["1.6/Textures/Pawn/Body.png", "Textures/Things/Shipped.png", "Textures/Things/Wall.png", "Textures/UI/Icon.png"]);
    }

    #[test]
    fn convert_revalidate_revert() {
        let tmp = tempfile::tempdir().unwrap();
        fixture(tmp.path());
        let opts = Options::default();
        let plan = plan(find_pngs(tmp.path()), &HashMap::new(), &opts);
        assert_eq!(plan.shipped, 1);
        assert_eq!(plan.work.len(), 3);
        let work: Vec<(String, Candidate)> = plan.work.into_iter().map(|w| ("mod".to_string(), w.candidate)).collect();
        let cancel = AtomicBool::new(false);
        let out = run(work, &opts, 2, &cancel, &|_| {});
        assert_eq!(out.len(), 3);
        let entries: Vec<Entry> = out.iter().map(|o| o.result.clone().unwrap()).collect();
        assert!(tmp.path().join("Textures/Things/Wall.dds").is_file());
        assert!(tmp.path().join("1.6/Textures/Pawn/Body.dds").is_file());
        assert!(!tmp.path().join("Textures/Things/Wall.dds.circinus-tmp").exists());
        assert_eq!(std::fs::read(tmp.path().join("Textures/Things/Shipped.dds")).unwrap(), b"authors own file");
        assert_eq!(entries.iter().find(|e| e.rel.ends_with("Wall.png")).unwrap().format, Format::Bc1);
        assert_eq!(entries.iter().find(|e| e.rel.ends_with("Icon.png")).unwrap().format, Format::Bc7);

        // Second pass: nothing to do.
        let map: HashMap<String, Entry> = entries.iter().map(|e| (e.rel.clone(), e.clone())).collect();
        let again = super::plan(find_pngs(tmp.path()), &map, &opts);
        assert!(again.work.is_empty());
        assert_eq!(again.current, 3);

        // Settings change forces alpha textures only.
        let bc3 = Options { alpha_format: Format::Bc3, ..Options::default() };
        let p3 = super::plan(find_pngs(tmp.path()), &map, &bc3);
        assert_eq!(p3.work.len(), 2);
        assert!(p3.work.iter().all(|w| w.reason == "settings changed"));

        // The author updates a PNG: its DDS must go.
        std::fs::write(tmp.path().join("Textures/Things/Wall.png"), png(32, 16, true)).unwrap();
        let r = revalidate(tmp.path(), &entries);
        assert_eq!(r.stale, vec!["Textures/Things/Wall.png"]);
        assert_eq!(r.keep.len(), 2);
        assert!(!tmp.path().join("Textures/Things/Wall.dds").exists());

        // Someone replaced a DDS with their own: it is not ours to delete.
        std::fs::write(tmp.path().join("Textures/UI/Icon.dds"), b"custom").unwrap();
        let rv = revert(tmp.path(), &r.keep);
        assert_eq!(rv.deleted, vec!["1.6/Textures/Pawn/Body.png"]);
        assert_eq!(rv.kept, vec!["Textures/UI/Icon.png"]);
        assert!(tmp.path().join("Textures/UI/Icon.dds").is_file());
        assert!(!tmp.path().join("1.6/Textures/Pawn/Body.dds").exists());
    }

    #[test]
    fn bad_png_leaves_nothing_behind() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("Textures/Bad.png");
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b"not a png").unwrap();
        let c = find_pngs(tmp.path());
        let r = convert_one(&c[0], &Options::default(), 0);
        assert!(r.is_err());
        assert!(!tmp.path().join("Textures/Bad.dds").exists());
        assert!(std::fs::read_dir(tmp.path().join("Textures")).unwrap().count() == 1);
    }
}
