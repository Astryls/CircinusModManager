//! Def flattening as a background job: build the document the game builds, keep it in memory,
//! and answer questions about it.
//!
//! A thousand-mod list is tens of thousands of XML files, so the work runs on a thread and
//! reports progress the way the texture jobs do. The finished document stays in memory
//! afterwards: the inspector asks it for a def or an XPath, and re-reading every file to answer
//! "what is in ThingDef/Wall" would be minutes of work for a question that takes microseconds.

use crate::commands::Shared;
use circinus_core::defs::flatten::{self, Flattened, Flattener, Report};
use circinus_core::defs::tree::{Doc, NodeId, DOCUMENT};
use circinus_core::defs::xpath::{self, Item};
use circinus_core::model::ModInfo;
use serde::Serialize;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};

type CmdResult<T> = std::result::Result<T, String>;

/// Enough nodes for any real def; a runaway one is cut off rather than sent whole.
const MAX_TREE_NODES: usize = 4000;
/// The most matches one query returns, whatever the caller asks for.
const MAX_MATCHES: usize = 2000;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefsState {
    pub running: bool,
    /// idle | defs | patches | inheritance
    pub phase: String,
    pub done: usize,
    pub total: usize,
    /// The mod being read right now.
    pub current: String,
    pub started_at: i64,
    pub finished_at: i64,
    /// The last run was stopped part-way, so there is nothing to look at.
    pub stopped: bool,
    pub error: Option<String>,
    pub report: Option<Report>,
}

pub struct Defs {
    pub state: Mutex<DefsState>,
    /// The finished document, kept so the inspector can ask about it without flattening again.
    result: Mutex<Option<Flattened>>,
    stop: AtomicBool,
    handle: AppHandle,
    app: Shared,
    /// When the last progress event went out, so a thousand mods do not mean a thousand events.
    last_emit: Mutex<std::time::Instant>,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

impl Defs {
    pub fn new(handle: AppHandle, app: Shared) -> Arc<Defs> {
        Arc::new(Defs {
            state: Mutex::new(DefsState { phase: "idle".into(), ..DefsState::default() }),
            result: Mutex::new(None),
            stop: AtomicBool::new(false),
            handle,
            app,
            last_emit: Mutex::new(std::time::Instant::now()),
        })
    }

    pub fn snapshot(&self) -> DefsState {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    fn emit(&self) {
        let _ = self.handle.emit("defs-progress", self.snapshot());
    }

    /// The mod list moved under us (a scan, a different active list): what was flattened
    /// describes a list that no longer exists, so it goes. A run in flight keeps its own state.
    pub fn forget(&self) {
        let mut s = match self.state.lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        if s.running {
            return;
        }
        if s.report.is_none() && self.result.lock().map(|r| r.is_none()).unwrap_or(true) {
            return;
        }
        *s = DefsState { phase: "idle".into(), ..DefsState::default() };
        if let Ok(mut r) = self.result.lock() {
            *r = None;
        }
        drop(s);
        self.emit();
    }

    fn step(&self, phase: &str, done: usize, total: usize, current: &str) {
        if let Ok(mut s) = self.state.lock() {
            s.phase = phase.into();
            s.done = done;
            s.total = total;
            s.current = current.to_string();
        }
        let due = self.last_emit.lock().map(|t| t.elapsed().as_millis() >= 100).unwrap_or(true);
        if due || done == total {
            if let Ok(mut t) = self.last_emit.lock() {
                *t = std::time::Instant::now();
            }
            self.emit();
        }
    }

    /// Flatten the active list (a blocking call; run it on a blocking thread).
    pub fn run(self: &Arc<Self>) -> CmdResult<()> {
        {
            let mut s = self.state.lock().map_err(|_| "state lock poisoned".to_string())?;
            if s.running {
                return Err("Circinus is already working out the merged defs".into());
            }
            self.stop.store(false, Ordering::Relaxed);
            *s = DefsState { running: true, phase: "defs".into(), started_at: now(), ..DefsState::default() };
        }
        if let Ok(mut r) = self.result.lock() {
            *r = None;
        }
        self.emit();
        // The list as it stands, without the descriptions: the flattener only wants folders.
        let (mods, major_minor): (Vec<ModInfo>, String) = {
            let app = self.app.lock().map_err(|_| "state lock poisoned".to_string())?;
            let by_uid: std::collections::HashMap<&str, &ModInfo> = app.mods.iter().map(|m| (m.uid.as_str(), m)).collect();
            let mods = app.active.iter().filter_map(|u| by_uid.get(u.as_str()).map(|m| ModInfo { description: String::new(), ..(*m).clone() })).collect();
            (mods, app.game_version.major_minor.clone())
        };
        if mods.is_empty() {
            self.fail("There are no active mods to merge");
            return Ok(());
        }
        let ids: Vec<String> = mods.iter().map(|m| m.package_id.clone()).collect();
        let active: HashSet<String> = ids.iter().cloned().collect();
        let folders: Vec<Vec<String>> = mods.iter().map(|m| flatten::load_folders(m, &major_minor, &active)).collect();

        let started = std::time::Instant::now();
        let total = mods.len() * 2;
        let mut f = Flattener::new(&ids);
        // Defs first, in load order, then every mod's patches over the whole document: RimWorld's
        // own order, and the reason a patch can only see what loaded before the patching starts.
        for (i, m) in mods.iter().enumerate() {
            if self.stop.load(Ordering::Relaxed) {
                return self.abandon();
            }
            self.step("defs", i, total, &m.name);
            f.add_defs(m, i as u32, &folders[i]);
        }
        for (i, m) in mods.iter().enumerate() {
            if self.stop.load(Ordering::Relaxed) {
                return self.abandon();
            }
            self.step("patches", mods.len() + i, total, &m.name);
            f.add_patches(m, i as u32, &folders[i]);
        }
        self.step("inheritance", total, total, "");
        f.resolve_inheritance();
        let flat = f.finish(started.elapsed().as_millis() as u64);
        tracing::info!(mods = mods.len(), defs = flat.report.defs, values = flat.report.values, ops = flat.report.operations, ms = flat.report.elapsed_ms, "defs flattened");
        let report = flat.report.clone();
        if let Ok(mut r) = self.result.lock() {
            *r = Some(flat);
        }
        if let Ok(mut s) = self.state.lock() {
            s.running = false;
            s.phase = "idle".into();
            s.current = String::new();
            s.finished_at = now();
            s.report = Some(report);
        }
        self.emit();
        Ok(())
    }

    fn abandon(&self) -> CmdResult<()> {
        if let Ok(mut s) = self.state.lock() {
            *s = DefsState { phase: "idle".into(), stopped: true, finished_at: now(), ..DefsState::default() };
        }
        if let Ok(mut r) = self.result.lock() {
            *r = None;
        }
        self.emit();
        Ok(())
    }

    fn fail(&self, why: &str) {
        if let Ok(mut s) = self.state.lock() {
            *s = DefsState { phase: "idle".into(), error: Some(why.to_string()), finished_at: now(), ..DefsState::default() };
        }
        self.emit();
    }

    /// Run an XPath over the flattened document.
    pub fn query(&self, xpath: &str, limit: usize) -> CmdResult<QueryResult> {
        let guard = self.result.lock().map_err(|_| "state lock poisoned".to_string())?;
        let flat = guard.as_ref().ok_or_else(|| NOTHING_YET.to_string())?;
        query_doc(&flat.doc, xpath, limit)
    }

    /// One def as a tree: every node with its owner, so the inspector can colour them.
    pub fn def(&self, def_type: &str, def_name: &str) -> CmdResult<Option<DefTree>> {
        let guard = self.result.lock().map_err(|_| "state lock poisoned".to_string())?;
        let flat = guard.as_ref().ok_or_else(|| NOTHING_YET.to_string())?;
        Ok(def_tree(&flat.doc, def_type, def_name))
    }
}

const NOTHING_YET: &str = "Nothing has been merged yet. Run it first, then ask.";

fn query_doc(doc: &Doc, xpath: &str, limit: usize) -> CmdResult<QueryResult> {
    if xpath.trim().is_empty() {
        return Ok(QueryResult::default());
    }
    let started = std::time::Instant::now();
    let items = xpath::select_str(doc, DOCUMENT, xpath).map_err(|e| e.0)?;
    let matches: Vec<Match> = items.iter().take(limit.min(MAX_MATCHES)).filter_map(|it| describe(doc, *it)).collect();
    Ok(QueryResult { total: items.len(), matches, elapsed_ms: started.elapsed().as_millis() as u64 })
}

fn def_tree(doc: &Doc, def_type: &str, def_name: &str) -> Option<DefTree> {
    let tag = doc.syms.lookup(def_type)?;
    // The last def with this name is the one the game keeps; a def with no defName is found by
    // its Name instead, which is how abstract bases and patched-in defs are named.
    let mut found = doc.defs_named(tag, def_name);
    if found.is_empty() {
        found = doc.defs_with_name(tag, def_name);
    }
    let &def = found.last()?;
    let base = doc.depth(def);
    let mut nodes: Vec<DefNode> = Vec::new();
    let mut truncated = 0usize;
    for n in doc.descendants(def) {
        if !doc.is_element(n) {
            continue;
        }
        if nodes.len() >= MAX_TREE_NODES {
            truncated += 1;
            continue;
        }
        let leaf = !doc.has_element_children(n);
        let via = doc.nodes[n as usize].via;
        nodes.push(DefNode {
            tag: doc.name(n).to_string(),
            path: doc.path_within(def, n),
            value: if leaf { doc.string_value(n).trim().to_string() } else { String::new() },
            leaf,
            depth: doc.depth(n).saturating_sub(base + 1),
            origin: doc.nodes[n as usize].origin,
            inherited: via != 0,
            // `via` names the node this one was copied from, offset by one so 0 means "own".
            inherited_from: if via != 0 { ident(doc, owning_def(doc, via - 1)).1 } else { String::new() },
            attrs: doc.attrs(n).map(|i| (doc.syms.get(doc.attr_name_at(i)).to_string(), doc.attr_value_at(i).to_string())).collect(),
        });
    }
    let (ty, name) = ident(doc, def);
    Some(DefTree { def_type: ty, def_name: name, origin: doc.nodes[def as usize].origin, nodes, truncated })
}

// ---------------------------------------------------------------------------------------------
// What a query answers with
// ---------------------------------------------------------------------------------------------

/// One node an XPath selected, placed: which def it is in, where inside it, and whose it is.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Match {
    /// `ThingDef/Wall`, the same label the report's overwrites use.
    pub def: String,
    pub def_type: String,
    pub def_name: String,
    /// `statBases/MaxHitPoints`, relative to the def.
    pub path: String,
    pub value: String,
    pub origin: u32,
    pub inherited: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResult {
    /// How many the xpath matched, which can be more than were returned.
    pub total: usize,
    pub matches: Vec<Match>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefNode {
    pub tag: String,
    pub path: String,
    /// The value, when this node holds one rather than more nodes.
    pub value: String,
    pub leaf: bool,
    /// 0 for the def's own children.
    pub depth: usize,
    pub origin: u32,
    /// Taken from a parent def rather than written here.
    pub inherited: bool,
    /// The parent def it came from, when it was inherited.
    pub inherited_from: String,
    pub attrs: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DefTree {
    pub def_type: String,
    pub def_name: String,
    /// Who shipped the def itself; individual nodes carry their own owner.
    pub origin: u32,
    pub nodes: Vec<DefNode>,
    /// Nodes left out because the def is enormous.
    pub truncated: usize,
}

/// The def a node lives in: the child of `<Defs>` above it.
fn def_containing(doc: &Doc, mut n: NodeId) -> Option<NodeId> {
    let root = doc.root()?;
    if n == root {
        return None;
    }
    loop {
        let p = doc.parent(n)?;
        if p == root {
            return Some(n);
        }
        n = p;
    }
}

/// The def a node belongs to, including one the flattener dropped at the end: an abstract
/// parent is detached from the document but its nodes stay readable, and it is the thing an
/// inherited value came from, so it still needs a name.
fn owning_def(doc: &Doc, mut n: NodeId) -> NodeId {
    let root = doc.root();
    loop {
        match doc.parent(n) {
            Some(p) if Some(p) == root => return n,
            Some(p) => n = p,
            None => return n,
        }
    }
}

/// A def's type and name, the way the report labels them.
fn ident(doc: &Doc, def: NodeId) -> (String, String) {
    let ty = doc.name(def).to_string();
    let name = doc
        .child_named(def, doc.s.def_name)
        .and_then(|d| doc.value(d))
        .or_else(|| doc.attr(def, doc.s.name).map(|s| s.to_string()))
        .unwrap_or_else(|| "(unnamed)".into());
    (ty, name)
}

fn describe(doc: &Doc, item: Item) -> Option<Match> {
    let owner = item.owner();
    let def = def_containing(doc, owner)?;
    let (def_type, def_name) = ident(doc, def);
    let mut path = doc.path_within(def, owner);
    let value = match item {
        Item::Node(n) => doc.string_value(n).trim().to_string(),
        Item::Attr { index, .. } => {
            let name = doc.syms.get(doc.attr_name_at(index)).to_string();
            path = if path.is_empty() { format!("@{name}") } else { format!("{path}/@{name}") };
            doc.attr_value_at(index).to_string()
        }
    };
    Some(Match {
        def: format!("{def_type}/{def_name}"),
        def_type,
        def_name,
        path,
        value,
        origin: doc.nodes[owner as usize].origin,
        inherited: doc.nodes[owner as usize].via != 0,
    })
}

// ---------------------------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------------------------

type St<'a> = State<'a, Arc<Defs>>;

/// Merge every active mod's defs in the background; progress arrives as `defs-progress`.
#[tauri::command]
pub async fn defs_start(defs: St<'_>) -> CmdResult<()> {
    if defs.snapshot().running {
        return Err("Circinus is already working out the merged defs".into());
    }
    let d = defs.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Err(e) = d.run() {
            tracing::warn!("defs flatten: {e}");
        }
    });
    Ok(())
}

#[tauri::command]
pub fn defs_status(defs: St<'_>) -> DefsState {
    defs.snapshot()
}

#[tauri::command]
pub fn defs_stop(defs: St<'_>) {
    defs.stop();
}

/// Run an XPath over the merged document, the way RimWorld hands one to a patch.
#[tauri::command]
pub async fn defs_query(defs: St<'_>, xpath: String, limit: usize) -> CmdResult<QueryResult> {
    let d = defs.inner().clone();
    tauri::async_runtime::spawn_blocking(move || d.query(&xpath, limit)).await.map_err(|e| e.to_string())?
}

/// One def as the game would have it, node by node, with the owner of each.
#[tauri::command]
pub async fn defs_def(defs: St<'_>, def_type: String, def_name: String) -> CmdResult<Option<DefTree>> {
    let d = defs.inner().clone();
    tauri::async_runtime::spawn_blocking(move || d.def(&def_type, &def_name)).await.map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use circinus_core::model::Source;
    use std::path::Path;

    fn write(p: &Path, text: &str) {
        std::fs::create_dir_all(p.parent().expect("a parent")).expect("the folder");
        std::fs::write(p, text).expect("the file");
    }

    /// One mod ships a wall, a later one patches it: the same shape the view is built around.
    fn fixture() -> (tempfile::TempDir, Flattened) {
        let dir = tempfile::tempdir().expect("a temp folder");
        let r = dir.path();
        write(
            &r.join("base/Defs/Things.xml"),
            r#"<Defs>
                 <ThingDef Name="BuildingBase" Abstract="True"><statBases><Flammability>1.0</Flammability></statBases></ThingDef>
                 <ThingDef ParentName="BuildingBase"><defName>Wall</defName><label>wall</label>
                   <statBases><MaxHitPoints>300</MaxHitPoints></statBases><comps><li Class="CompA"/></comps></ThingDef>
               </Defs>"#,
        );
        write(
            &r.join("tweak/Patches/Walls.xml"),
            r#"<Patch><Operation Class="PatchOperationReplace">
                 <xpath>Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints</xpath>
                 <value><MaxHitPoints>500</MaxHitPoints></value>
               </Operation></Patch>"#,
        );
        let mods = [
            ModInfo { uid: "base".into(), path: r.join("base"), package_id: "a.base".into(), name: "Base".into(), source: Source::Local, ..Default::default() },
            ModInfo { uid: "tweak".into(), path: r.join("tweak"), package_id: "b.tweak".into(), name: "Tweak".into(), source: Source::Local, ..Default::default() },
        ];
        let refs: Vec<&ModInfo> = mods.iter().collect();
        let flat = circinus_core::defs::flatten::flatten(&refs, &|_| vec![String::new()], &|_, _| {});
        (dir, flat)
    }

    #[test]
    fn a_query_places_every_hit_in_its_def() {
        let (_d, flat) = fixture();
        let r = query_doc(&flat.doc, "Defs/ThingDef/statBases/*", 50).expect("the query runs");
        let seen: Vec<(&str, &str, &str)> = r.matches.iter().map(|m| (m.def.as_str(), m.path.as_str(), m.value.as_str())).collect();
        assert_eq!(seen, [("ThingDef/Wall", "statBases/MaxHitPoints", "500"), ("ThingDef/Wall", "statBases/Flammability", "1.0")]);
        assert_eq!(r.total, 2);
        // the patched value belongs to the mod that set it, the inherited one to the parent def
        assert_eq!(flat.report.origins[r.matches[0].origin as usize].name, "Tweak");
        assert!(!r.matches[0].inherited);
        assert!(r.matches[1].inherited);
        // an attribute is named as one
        let attrs = query_doc(&flat.doc, "Defs/ThingDef/comps/li/@Class", 50).expect("the query runs");
        assert_eq!(attrs.matches[0].path, "comps/li/@Class");
        assert_eq!(attrs.matches[0].value, "CompA");
        // and a broken xpath says so rather than returning nothing
        assert!(query_doc(&flat.doc, "Defs/ThingDef[", 50).is_err());
    }

    #[test]
    fn a_def_tree_carries_depth_owner_and_inheritance() {
        let (_d, flat) = fixture();
        let t = def_tree(&flat.doc, "ThingDef", "Wall").expect("the wall is in the document");
        let rows: Vec<(&str, usize, bool, &str)> = t.nodes.iter().map(|n| (n.tag.as_str(), n.depth, n.leaf, n.value.as_str())).collect();
        // the inherited stat lands in the statBases the def already had, after its own value
        assert_eq!(rows, [("defName", 0, true, "Wall"), ("label", 0, true, "wall"), ("statBases", 0, false, ""), ("MaxHitPoints", 1, true, "500"), ("Flammability", 1, true, "1.0"), ("comps", 0, false, ""), ("li", 1, true, "")]);
        let hp = &t.nodes[3];
        assert_eq!(flat.report.origins[hp.origin as usize].name, "Tweak");
        assert_eq!(hp.path, "statBases/MaxHitPoints");
        assert!(!hp.inherited);
        let flam = &t.nodes[4];
        assert!(flam.inherited);
        assert_eq!(flam.inherited_from, "BuildingBase");
        assert_eq!(t.nodes[6].attrs, [("Class".to_string(), "CompA".to_string())]);
        assert!(def_tree(&flat.doc, "ThingDef", "NoSuchThing").is_none());
        assert!(def_tree(&flat.doc, "NoSuchDefType", "Wall").is_none());
    }
}
