//! Texture optimisation as a background job: pick the mods, find their PNGs, convert on a
//! thread pool, keep the manifest in the cache, and re-check converted mods when they change.

use crate::commands::Shared;
use circinus_core::dds::{self, audit, job, Entry, Finding, Options, Progress};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub mods: usize,
    pub converted: usize,
    pub failed: usize,
    /// Already converted from an unchanged source.
    pub current: usize,
    /// Left alone because the author ships a DDS.
    pub shipped: usize,
    pub png_bytes: u64,
    pub dds_bytes: u64,
    pub seconds: u64,
    pub cancelled: bool,
    pub reverted: usize,
    /// Originals put back from their `.circinus-orig` backups.
    #[serde(default)]
    pub restored: usize,
    pub bytes_freed: u64,
    /// Foreign DDS files rebuilt by the audit's fix.
    #[serde(default)]
    pub fixed: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TexState {
    pub running: bool,
    /// idle | scanning | converting | reverting
    pub phase: String,
    pub progress: Progress,
    pub started_at: i64,
    pub finished_at: i64,
    /// (mod name, file, message), most recent last, capped.
    pub errors: Vec<(String, String, String)>,
    pub report: Option<Report>,
}

pub struct Textures {
    pub state: Mutex<TexState>,
    cancel: AtomicBool,
    handle: AppHandle,
    app: Shared,
    /// Mods already re-checked for the change they currently carry.
    revalidated: Mutex<HashSet<String>>,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

impl Textures {
    pub fn new(handle: AppHandle, app: Shared) -> Arc<Textures> {
        Arc::new(Textures { state: Mutex::new(TexState { phase: "idle".into(), ..TexState::default() }), cancel: AtomicBool::new(false), handle, app, revalidated: Mutex::new(HashSet::new()) })
    }

    pub fn snapshot(&self) -> TexState {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }

    fn emit(&self) {
        let _ = self.handle.emit("dds-progress", self.snapshot());
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    fn options(&self) -> (Options, usize, bool) {
        let app = self.app.lock().unwrap();
        let d = &app.settings.dds;
        (Options { alpha_format: d.alpha_format, quality: d.quality, mipmaps: d.mipmaps, dilate: true }, d.threads, d.auto)
    }

    fn begin(&self, phase: &str) -> Result<(), String> {
        let mut s = self.state.lock().unwrap();
        if s.running {
            return Err("A texture job is already running".into());
        }
        self.cancel.store(false, Ordering::Relaxed);
        *s = TexState { running: true, phase: phase.into(), started_at: now(), ..TexState::default() };
        Ok(())
    }

    fn finish(&self, report: Report) {
        let mut s = self.state.lock().unwrap();
        s.running = false;
        s.phase = "idle".into();
        s.finished_at = now();
        s.report = Some(report);
    }

    /// Mods worth converting: valid, with PNG textures, not excluded.
    fn targets(&self, uids: &[String]) -> Vec<(String, String, PathBuf)> {
        let app = self.app.lock().unwrap();
        let want: HashSet<&str> = uids.iter().map(|s| s.as_str()).collect();
        app.mods
            .iter()
            .filter(|m| want.contains(m.uid.as_str()) && m.invalid.is_none() && m.source != circinus_core::Source::Ludeon && !app.user.dds_excluded.contains(&m.uid))
            .map(|m| (m.uid.clone(), m.name.clone(), m.path.clone()))
            .collect()
    }

    /// Convert the textures of `uids` (a blocking call; run it on a blocking thread).
    pub fn convert(self: &Arc<Self>, uids: Vec<String>) -> Result<Report, String> {
        self.begin("scanning")?;
        self.emit();
        let (opts, threads, _) = self.options();
        let targets = self.targets(&uids);
        let started = std::time::Instant::now();
        let manifest: HashMap<String, Vec<Entry>> = self.app.lock().unwrap().cache.dds_all().unwrap_or_default();
        // Find and plan per mod, in parallel: walking thousands of folders is I/O bound.
        use rayon::prelude::*;
        let plans: Vec<(String, String, job::Plan)> = targets
            .par_iter()
            .map(|(uid, name, path)| {
                let entries: HashMap<String, Entry> = manifest.get(uid).map(|v| v.iter().map(|e| (e.rel.clone(), e.clone())).collect()).unwrap_or_default();
                (uid.clone(), name.clone(), job::plan(job::find_pngs(path), &entries, &opts))
            })
            .collect();
        let mut report = Report { mods: targets.len(), ..Report::default() };
        let mut work: Vec<(String, job::Candidate)> = Vec::new();
        let names: HashMap<String, String> = targets.iter().map(|(u, n, _)| (u.clone(), n.clone())).collect();
        for (uid, _, plan) in plans {
            report.current += plan.current;
            report.shipped += plan.shipped;
            for w in plan.work {
                work.push((uid.clone(), w.candidate));
            }
        }
        {
            let mut s = self.state.lock().unwrap();
            s.phase = "converting".into();
            s.progress = Progress { total: work.len(), ..Progress::default() };
        }
        self.emit();
        let me = self.clone();
        let on_progress = move |p: &Progress| {
            if let Ok(mut s) = me.state.lock() {
                s.progress = p.clone();
            }
            me.emit();
        };
        let outcomes = job::run(work, &opts, threads, &self.cancel, &on_progress);
        // Persist what worked, remember what did not.
        let mut store: Vec<(String, String, Entry)> = Vec::new();
        let mut touched: HashSet<String> = HashSet::new();
        for o in outcomes {
            match o.result {
                Ok(e) => {
                    report.converted += 1;
                    report.png_bytes += o.png_len;
                    report.dds_bytes += e.dds_len;
                    touched.insert(o.uid.clone());
                    store.push((o.uid, o.rel, e));
                }
                Err(msg) => {
                    if msg == "cancelled" {
                        report.cancelled = true;
                    } else {
                        report.failed += 1;
                        let mut s = self.state.lock().unwrap();
                        if s.errors.len() < 300 {
                            s.errors.push((names.get(&o.uid).cloned().unwrap_or_else(|| o.uid.clone()), o.rel, msg));
                        }
                    }
                }
            }
        }
        {
            let mut app = self.app.lock().unwrap();
            if let Err(e) = app.cache.dds_store(&store) {
                tracing::warn!("could not store the DDS manifest: {e}");
            }
            app.reload_dds_index();
            let uids: Vec<String> = touched.iter().cloned().collect();
            let _ = app.cache.forget_mod_entries(&uids);
        }
        report.seconds = started.elapsed().as_secs();
        self.finish(report.clone());
        self.emit();
        if !touched.is_empty() {
            crate::run_scan(self.handle.clone(), self.app.clone(), false);
        }
        if report.seconds >= 60 {
            let _ = self.handle.notification().builder().title("Textures optimised").body(format!("{} converted, {} failed, {:.1} GB of DDS written.", report.converted, report.failed, report.dds_bytes as f64 / 1e9)).show();
        }
        Ok(report)
    }

    /// Delete the DDS files Circinus created for `uids`.
    pub fn revert(self: &Arc<Self>, uids: Vec<String>) -> Result<Report, String> {
        self.begin("reverting")?;
        self.emit();
        let started = std::time::Instant::now();
        let mods: Vec<(String, PathBuf)> = {
            let app = self.app.lock().unwrap();
            let want: HashSet<&str> = uids.iter().map(|s| s.as_str()).collect();
            app.mods.iter().filter(|m| want.contains(m.uid.as_str())).map(|m| (m.uid.clone(), m.path.clone())).collect()
        };
        let mut report = Report { mods: mods.len(), ..Report::default() };
        let mut touched = Vec::new();
        for (uid, path) in mods {
            let entries: Vec<Entry> = self.app.lock().unwrap().cache.dds_entries(&uid).unwrap_or_default();
            if entries.is_empty() {
                continue;
            }
            let r = job::revert(&path, &entries);
            report.reverted += r.deleted.len() + r.restored.len();
            report.restored += r.restored.len();
            report.bytes_freed += r.bytes_freed;
            let mut app = self.app.lock().unwrap();
            let _ = app.cache.dds_delete_all(&uid);
            if !r.kept.is_empty() {
                let mut s = self.state.lock().unwrap();
                for rel in r.kept.iter().take(50) {
                    s.errors.push((uid.clone(), rel.clone(), "not the file Circinus wrote — left in place".into()));
                }
            }
            touched.push(uid.clone());
            app.reload_dds_index();
        }
        {
            let app = self.app.lock().unwrap();
            let _ = app.cache.forget_mod_entries(&touched);
        }
        report.seconds = started.elapsed().as_secs();
        self.finish(report.clone());
        self.emit();
        if !touched.is_empty() {
            crate::run_scan(self.handle.clone(), self.app.clone(), false);
        }
        Ok(report)
    }

    /// After a scan: mods that changed and carry converted textures get re-checked, so a
    /// Workshop update never leaves the game loading yesterday's art. With the auto setting
    /// on, changed and new mods are converted again.
    pub fn after_scan(self: &Arc<Self>) {
        let (changed, auto): (Vec<(String, String, PathBuf, bool)>, bool) = {
            let app = self.app.lock().unwrap();
            let by_uid: HashMap<&str, &circinus_core::ModInfo> = app.mods.iter().map(|m| (m.uid.as_str(), m)).collect();
            let mut done = self.revalidated.lock().unwrap();
            let mut out = Vec::new();
            for c in &app.changes {
                if c.kind == circinus_core::changes::ChangeKind::Removed {
                    continue;
                }
                let key = format!("{}:{}", c.uid, c.when);
                if done.contains(&key) {
                    continue;
                }
                done.insert(key);
                if let Some(m) = by_uid.get(c.uid.as_str()) {
                    out.push((m.uid.clone(), m.name.clone(), m.path.clone(), app.dds_index.contains_key(&m.uid)));
                }
            }
            (out, app.settings.dds.auto)
        };
        if changed.is_empty() {
            return;
        }
        let mut to_convert = Vec::new();
        for (uid, name, path, has_entries) in changed {
            if has_entries {
                let entries: Vec<Entry> = self.app.lock().unwrap().cache.dds_entries(&uid).unwrap_or_default();
                let r = job::revalidate(&path, &entries);
                let drop: Vec<String> = r.stale.iter().chain(r.missing.iter()).chain(r.foreign.iter()).cloned().collect();
                if !drop.is_empty() {
                    tracing::info!(mod_ = %name, stale = r.stale.len(), missing = r.missing.len(), foreign = r.foreign.len(), "dds revalidate");
                    let mut app = self.app.lock().unwrap();
                    let _ = app.cache.dds_delete(&uid, &drop);
                    app.reload_dds_index();
                }
                if auto || (!r.stale.is_empty() || !r.missing.is_empty()) {
                    to_convert.push(uid);
                }
            } else if auto {
                to_convert.push(uid);
            }
        }
        if !to_convert.is_empty() && !self.snapshot().running {
            let me = self.clone();
            tauri::async_runtime::spawn_blocking(move || {
                let _ = me.convert(to_convert);
            });
        }
    }
}

/// One mod's foreign DDS files the game will refuse.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModAudit {
    pub uid: String,
    pub name: String,
    pub active: bool,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditReport {
    pub mods_checked: usize,
    pub mods: Vec<ModAudit>,
    pub files: usize,
    pub fixable: usize,
    pub seconds: u64,
}

impl Textures {
    /// Look at every DDS Circinus did not write in `uids` and report the ones Unity refuses.
    /// Header reads only; a large install takes a few seconds.
    pub fn audit(self: &Arc<Self>, uids: Vec<String>) -> Result<AuditReport, String> {
        self.begin("auditing")?;
        self.emit();
        let started = std::time::Instant::now();
        let (mods, manifest, active): (Vec<(String, String, PathBuf)>, HashMap<String, Vec<Entry>>, HashSet<String>) = {
            let app = self.app.lock().unwrap();
            let want: HashSet<&str> = uids.iter().map(|s| s.as_str()).collect();
            let mods = app.mods.iter().filter(|m| want.contains(m.uid.as_str()) && m.invalid.is_none() && m.source != circinus_core::Source::Ludeon).map(|m| (m.uid.clone(), m.name.clone(), m.path.clone())).collect();
            (mods, app.cache.dds_all().unwrap_or_default(), app.active.iter().cloned().collect())
        };
        use rayon::prelude::*;
        let mut out: Vec<ModAudit> = mods
            .par_iter()
            .filter_map(|(uid, name, path)| {
                if self.cancel.load(Ordering::Relaxed) {
                    return None;
                }
                let ours: HashSet<String> = manifest.get(uid).map(|v| v.iter().map(|e| e.dds_rel()).collect()).unwrap_or_default();
                let findings = audit::audit(path, &ours);
                (!findings.is_empty()).then(|| ModAudit { uid: uid.clone(), name: name.clone(), active: active.contains(uid), findings })
            })
            .collect();
        out.sort_by(|a, b| b.active.cmp(&a.active).then_with(|| b.findings.len().cmp(&a.findings.len())).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
        let report = AuditReport { mods_checked: mods.len(), files: out.iter().map(|m| m.findings.len()).sum(), fixable: out.iter().flat_map(|m| &m.findings).filter(|f| f.fixable).count(), mods: out, seconds: started.elapsed().as_secs() };
        self.finish(Report { mods: report.mods_checked, seconds: report.seconds, cancelled: self.cancel.load(Ordering::Relaxed), ..Report::default() });
        self.emit();
        Ok(report)
    }

    /// Rebuild the flagged files of `uids` (all of them when `rels` is empty), keeping the
    /// originals as backups; the manifest then treats them like any converted texture.
    pub fn fix(self: &Arc<Self>, targets: Vec<(String, Vec<String>)>) -> Result<Report, String> {
        self.begin("fixing")?;
        self.emit();
        let started = std::time::Instant::now();
        let (opts, threads, _) = self.options();
        let (mods, manifest): (HashMap<String, (String, PathBuf)>, HashMap<String, Vec<Entry>>) = {
            let app = self.app.lock().unwrap();
            (app.mods.iter().filter(|m| m.invalid.is_none() && m.source != circinus_core::Source::Ludeon).map(|m| (m.uid.clone(), (m.name.clone(), m.path.clone()))).collect(), app.cache.dds_all().unwrap_or_default())
        };
        // Audit again right now: the list the UI holds may be minutes old.
        let mut work: Vec<(String, PathBuf, Finding)> = Vec::new();
        for (uid, rels) in &targets {
            let Some((_, path)) = mods.get(uid) else { continue };
            let ours: HashSet<String> = manifest.get(uid).map(|v| v.iter().map(|e| e.dds_rel()).collect()).unwrap_or_default();
            for f in audit::audit(path, &ours) {
                if f.fixable && (rels.is_empty() || rels.contains(&f.rel)) {
                    work.push((uid.clone(), path.clone(), f));
                }
            }
        }
        let mut report = Report { mods: targets.len(), ..Report::default() };
        {
            let mut s = self.state.lock().unwrap();
            s.phase = "fixing".into();
            s.progress = Progress { total: work.len(), ..Progress::default() };
        }
        self.emit();
        let pool = rayon::ThreadPoolBuilder::new().num_threads(threads.max(1)).build().map_err(|e| e.to_string())?;
        let done = std::sync::atomic::AtomicUsize::new(0);
        let results: Vec<(String, String, std::result::Result<Entry, String>)> = pool.install(|| {
            use rayon::prelude::*;
            work.par_iter()
                .map(|(uid, path, f)| {
                    if self.cancel.load(Ordering::Relaxed) {
                        return (uid.clone(), f.rel.clone(), Err("cancelled".to_string()));
                    }
                    let r = audit::fix_one(path, f, &opts, now()).map_err(|e| e.to_string());
                    let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                    if let Ok(mut s) = self.state.lock() {
                        s.progress.done = n;
                        s.progress.current = f.rel.clone();
                        if r.is_ok() {
                            s.progress.converted += 1;
                        } else {
                            s.progress.failed += 1;
                        }
                    }
                    if n % 4 == 0 || n == work.len() {
                        self.emit();
                    }
                    (uid.clone(), f.rel.clone(), r)
                })
                .collect()
        });
        let mut store: Vec<(String, String, Entry)> = Vec::new();
        let mut touched: HashSet<String> = HashSet::new();
        for (uid, rel, r) in results {
            match r {
                Ok(e) => {
                    report.fixed += 1;
                    report.dds_bytes += e.dds_len;
                    touched.insert(uid.clone());
                    store.push((uid, e.rel.clone(), e));
                }
                Err(msg) if msg == "cancelled" => report.cancelled = true,
                Err(msg) => {
                    report.failed += 1;
                    let mut s = self.state.lock().unwrap();
                    if s.errors.len() < 300 {
                        s.errors.push((mods.get(&uid).map(|m| m.0.clone()).unwrap_or(uid), rel, msg));
                    }
                }
            }
        }
        {
            let mut app = self.app.lock().unwrap();
            if let Err(e) = app.cache.dds_store(&store) {
                tracing::warn!("could not store the DDS manifest: {e}");
            }
            app.reload_dds_index();
            let uids: Vec<String> = touched.iter().cloned().collect();
            let _ = app.cache.forget_mod_entries(&uids);
        }
        report.seconds = started.elapsed().as_secs();
        self.finish(report.clone());
        self.emit();
        if !touched.is_empty() {
            crate::run_scan(self.handle.clone(), self.app.clone(), false);
        }
        Ok(report)
    }
}

/// Per-mod figures for the Textures view, from the scan and the manifest — no walking.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModTextures {
    pub uid: String,
    pub name: String,
    pub active: bool,
    /// PNG/JPG textures the scan counted.
    pub pngs: u32,
    /// DDS files present (the author's plus ours).
    pub dds: u32,
    /// Converted by Circinus, per the manifest.
    pub converted: usize,
    pub dds_bytes: u64,
    pub png_bytes: u64,
    pub excluded: bool,
}

pub fn overview(app: &crate::state::App) -> Vec<ModTextures> {
    let active: HashSet<&str> = app.active.iter().map(|s| s.as_str()).collect();
    let mut out: Vec<ModTextures> = app
        .mods
        .iter()
        // Official content stays as shipped: writing into the game's Data folder would only
        // give Steam's integrity check something to complain about.
        .filter(|m| m.invalid.is_none() && m.source != circinus_core::Source::Ludeon && (m.contents.textures > 0 || m.contents.dds > 0 || app.dds_index.contains_key(&m.uid)))
        .map(|m| {
            let idx = app.dds_index.get(&m.uid);
            ModTextures {
                uid: m.uid.clone(),
                name: m.name.clone(),
                active: active.contains(m.uid.as_str()),
                pngs: m.contents.textures,
                dds: m.contents.dds,
                converted: idx.map(|i| i.count).unwrap_or(0),
                dds_bytes: idx.map(|i| i.dds_bytes).unwrap_or(0),
                png_bytes: idx.map(|i| i.png_bytes).unwrap_or(0),
                excluded: app.user.dds_excluded.contains(&m.uid),
            }
        })
        .collect();
    out.sort_by(|a, b| b.pngs.cmp(&a.pngs).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase())));
    out
}

pub fn default_options() -> dds::Options {
    dds::Options::default()
}
