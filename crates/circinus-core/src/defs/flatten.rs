//! Merging every active mod's Defs, running every patch, and keeping track of who won.
//!
//! The order is RimWorld's: each active mod's `Defs/` files are appended to one document in load
//! order, then each active mod's `Patches/` operations run against that whole document, again in
//! load order, and last the `Name`/`ParentName` inheritance is resolved. Every node carries the
//! origin that created it, so a value's owner is a fact rather than an inference, and every
//! change a patch makes to another mod's value is written down as it happens.

use super::tree::{import_children, root_tag, Doc, NodeId, Sym, DOCUMENT};
use super::xpath::{self, Expr, Item};
use crate::model::{LoadFolder, ModInfo};
use crate::xmlutil::read_text;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Who put something in the document. Index 0 is "nobody" — the arena's own scaffolding.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Origin {
    pub uid: String,
    pub package_id: String,
    /// Display name of the mod, for messages.
    pub name: String,
    /// The file inside the mod, relative to its folder.
    pub file: String,
    /// Load order position of the mod, so "later wins" is a comparison.
    pub index: u32,
    /// A patch operation rather than a Defs file.
    pub is_patch: bool,
}

/// A value a later mod took over from an earlier one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Overwrite {
    /// `ThingDef/Wall`, or `ThingDef/#3` for a def with no defName.
    pub def: String,
    /// `statBases/MaxHitPoints`, relative to the def, as it was when the change happened.
    pub path: String,
    pub from: u32,
    pub to: u32,
    pub old_value: String,
    pub new_value: String,
    /// The operation that did it (`PatchOperationReplace`), or `Defs` for a plain duplicate def.
    pub how: String,
    /// The node that was changed. A path is not an identity — twenty mods each removing
    /// `comps/li[3]` removed twenty different things — so chains are followed by node.
    #[serde(default)]
    pub node: u32,
    /// The node that took its place (`Replace`), the node itself (an attribute set, a rename),
    /// or `NONE` when it is simply gone (`Remove`).
    #[serde(default = "none")]
    pub next: u32,
}

fn none() -> u32 {
    super::tree::NONE
}

/// One value's history: the mod that shipped it, then every mod that changed it, in load order.
/// The last step is what the game sees.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chain {
    pub def: String,
    pub def_type: String,
    pub def_name: String,
    pub path: String,
    pub steps: Vec<ChainStep>,
    /// When several list items under one parent were removed by the same mod, they are one row:
    /// this holds the values that went, and `path` names the list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainStep {
    pub origin: u32,
    pub value: String,
    /// `Defs` for the value as shipped, else the operation.
    pub how: String,
}

/// A patch that did not do anything, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchProblem {
    pub origin: u32,
    pub xpath: String,
    pub class: String,
    pub reason: String,
    /// The operation says a miss is fine (`success="Always"`), so this is information, not a fault.
    pub tolerated: bool,
}

/// The same def type and name shipped by more than one mod: the last one wins outright.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Duplicate {
    pub def_type: String,
    pub def_name: String,
    pub origins: Vec<u32>,
    pub winner: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModStats {
    pub uid: String,
    pub name: String,
    /// Defs this mod contributes that survive to the end.
    pub defs: usize,
    /// Values in the finished document that came from this mod.
    pub values: usize,
    /// Values of other mods this mod's patches took over.
    pub wins: usize,
    /// Values of this mod's that another mod took over.
    pub losses: usize,
    pub operations: usize,
    pub failed_operations: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub origins: Vec<Origin>,
    pub defs: usize,
    pub values: usize,
    pub operations: usize,
    pub overwrites: Vec<Overwrite>,
    /// `overwrites` followed by node into the histories the view shows.
    #[serde(default)]
    pub chains: Vec<Chain>,
    pub problems: Vec<PatchProblem>,
    pub duplicates: Vec<Duplicate>,
    pub per_mod: Vec<ModStats>,
    /// Defs whose ParentName names a node nobody defines.
    pub missing_parents: Vec<String>,
    pub elapsed_ms: u64,
}

/// The flattened document plus everything worth saying about how it got that way.
pub struct Flattened {
    pub doc: Doc,
    pub report: Report,
}

// ---------------------------------------------------------------------------------------------
// Building the document
// ---------------------------------------------------------------------------------------------

pub struct Flattener {
    doc: Doc,
    root: NodeId,
    origins: Vec<Origin>,
    report: Report,
    /// Package ids of the active mods, lowercased: `MayRequire` and `PatchOperationFindMod`.
    active_ids: HashSet<String>,
    stats: HashMap<String, ModStats>,
    /// Compiled XPaths, shared across every mod: a patch expression repeats constantly.
    cache: HashMap<String, std::result::Result<Expr, String>>,
}

impl Default for Flattener {
    fn default() -> Self {
        Flattener::new(&[])
    }
}

impl Flattener {
    pub fn new(active_ids: &[String]) -> Flattener {
        let mut doc = Doc::new();
        let root = doc.new_element_named("Defs", 0);
        doc.append_child(DOCUMENT, root);
        Flattener {
            doc,
            root,
            origins: vec![Origin { name: "the game".into(), ..Default::default() }],
            report: Report::default(),
            active_ids: active_ids.iter().map(|s| s.to_ascii_lowercase()).collect(),
            stats: HashMap::new(),
            cache: HashMap::new(),
        }
    }

    fn origin(&mut self, m: &ModInfo, file: &str, index: u32, is_patch: bool) -> u32 {
        self.origins.push(Origin {
            uid: m.uid.clone(),
            package_id: m.package_id.clone(),
            name: m.name.clone(),
            file: file.to_string(),
            index,
            is_patch,
        });
        (self.origins.len() - 1) as u32
    }

    fn stats_for(&mut self, m: &ModInfo) -> &mut ModStats {
        self.stats.entry(m.uid.clone()).or_insert_with(|| ModStats { uid: m.uid.clone(), name: m.name.clone(), ..Default::default() })
    }

    fn compile(&mut self, xpath: &str) -> std::result::Result<Expr, String> {
        if let Some(hit) = self.cache.get(xpath) {
            return hit.clone();
        }
        let r = xpath::compile(xpath).map_err(|e| e.0);
        self.cache.insert(xpath.to_string(), r.clone());
        r
    }

    /// Add one mod's Defs, in the folders RimWorld would read for this game version.
    pub fn add_defs(&mut self, m: &ModInfo, index: u32, folders: &[String]) {
        for folder in folders {
            let dir = join_folder(&mod_root(m), folder).join("Defs");
            for file in xml_files(&dir) {
                let rel = display_path(&mod_root(m), &file);
                let Ok(text) = read_text(&file) else {
                    self.report.problems.push(PatchProblem { origin: 0, xpath: rel, class: "Defs".into(), reason: "the file could not be read".into(), tolerated: false });
                    continue;
                };
                let origin = self.origin(m, &rel, index, false);
                let root = self.root;
                match import_children(&mut self.doc, &text, root, origin) {
                    Ok(added) => {
                        let n = added.len();
                        self.strip_unmet_requirements(&added);
                        self.stats_for(m).defs += n;
                    }
                    Err(e) => {
                        self.report.problems.push(PatchProblem { origin, xpath: String::new(), class: "Defs".into(), reason: format!("the XML could not be parsed: {e}"), tolerated: false });
                    }
                }
            }
        }
    }

    /// Nodes whose `MayRequire` names a mod that is not active never reach the game.
    fn strip_unmet_requirements(&mut self, roots: &[NodeId]) {
        let (may_require, may_any) = (self.doc.s.may_require, self.doc.s.may_require_any_of);
        let mut doomed: Vec<NodeId> = Vec::new();
        for r in roots {
            let mut stack = vec![*r];
            while let Some(n) = stack.pop() {
                let required = self.doc.attr(n, may_require).map(|v| v.to_string());
                let any_of = self.doc.attr(n, may_any).map(|v| v.to_string());
                let ok = required.map(|v| v.split(',').all(|id| self.active_ids.contains(id.trim().to_ascii_lowercase().as_str()))).unwrap_or(true)
                    && any_of.map(|v| v.split(',').any(|id| self.active_ids.contains(id.trim().to_ascii_lowercase().as_str()))).unwrap_or(true);
                if !ok {
                    doomed.push(n);
                    continue;
                }
                stack.extend(self.doc.element_children(n));
            }
        }
        for n in doomed {
            self.doc.remove(n);
        }
    }

    /// Run one mod's patches over everything added so far.
    pub fn add_patches(&mut self, m: &ModInfo, index: u32, folders: &[String]) {
        for folder in folders {
            let dir = join_folder(&mod_root(m), folder).join("Patches");
            for file in xml_files(&dir) {
                let rel = display_path(&mod_root(m), &file);
                let Ok(text) = read_text(&file) else { continue };
                let origin = self.origin(m, &rel, index, true);
                // A patch file's operations live in a scratch element outside the document, so
                // their own XML never shows up in a query.
                match root_tag(&text) {
                    // RimWorld ignores files whose root is not <Patch> — quietly, but Circinus
                    // says so, because a patch that never runs is exactly what people come here
                    // to find out about.
                    Ok(t) if !t.eq_ignore_ascii_case("Patch") => {
                        self.report.problems.push(PatchProblem { origin, xpath: String::new(), class: "Patch".into(), reason: format!("the file's root element is <{t}>, not <Patch>, so the game ignores it"), tolerated: false });
                        continue;
                    }
                    Err(e) => {
                        self.report.problems.push(PatchProblem { origin, xpath: String::new(), class: "Patch".into(), reason: format!("the XML could not be parsed, so the game ignores the whole file: {e}"), tolerated: false });
                        continue;
                    }
                    Ok(_) => {}
                }
                let holder = self.doc.new_element_named("PatchFile", origin);
                let ops = match import_children(&mut self.doc, &text, holder, origin) {
                    Ok(ops) => ops,
                    Err(e) => {
                        self.report.problems.push(PatchProblem { origin, xpath: String::new(), class: "Patch".into(), reason: format!("the XML could not be parsed: {e}"), tolerated: false });
                        continue;
                    }
                };
                for op in ops {
                    self.run_operation(op, origin, m);
                }
                self.doc.remove(holder);
            }
        }
    }

    /// One `<Operation Class="…">`. Returns whether it matched anything, which is what the
    /// sequence and conditional operations need to know.
    fn run_operation(&mut self, op: NodeId, origin: u32, m: &ModInfo) -> bool {
        let class = self.doc.attr(op, self.doc.s.class).unwrap_or("").to_string();
        let short = class.rsplit('.').next().unwrap_or("").to_string();
        self.report.operations += 1;
        self.stats_for(m).operations += 1;
        let success = self.doc.child_named_str(op, "success").and_then(|n| self.doc.value(n)).unwrap_or_default();
        let tolerated = success.eq_ignore_ascii_case("Always") || success.eq_ignore_ascii_case("Invert");
        let xpath = self.doc.child_named_str(op, "xpath").and_then(|n| self.doc.value(n)).unwrap_or_default();

        let matched = match short.as_str() {
            "PatchOperationSequence" => {
                let mut all = true;
                if let Some(list) = self.doc.child_named_str(op, "operations") {
                    for child in self.doc.element_children(list).collect::<Vec<_>>() {
                        // A sequence stops at its first failure, as RimWorld's does.
                        if !self.run_operation(child, origin, m) {
                            all = false;
                            break;
                        }
                    }
                }
                all
            }
            "PatchOperationConditional" => {
                let hit = !self.select(&xpath, origin, &short, tolerated).is_empty();
                let branch = if hit { "match" } else { "nomatch" };
                match self.doc.child_named_str(op, branch) {
                    None => true,
                    Some(b) => self.run_operation(b, origin, m),
                }
            }
            "PatchOperationFindMod" => {
                let wanted: Vec<String> = self
                    .doc
                    .child_named_str(op, "mods")
                    .map(|n| self.doc.element_children(n).filter_map(|li| self.doc.value(li)).collect())
                    .unwrap_or_default();
                // FindMod matches by display name in RimWorld; package ids are accepted too
                // because plenty of patches use them.
                let hit = wanted.iter().any(|w| {
                    let w = w.trim().to_ascii_lowercase();
                    self.active_ids.contains(&w) || self.origins.iter().any(|o| o.name.to_ascii_lowercase() == w)
                });
                let branch = if hit { "match" } else { "nomatch" };
                match self.doc.child_named_str(op, branch) {
                    None => true,
                    Some(b) => self.run_operation(b, origin, m),
                }
            }
            "PatchOperationTest" => !self.select(&xpath, origin, &short, true).is_empty(),
            "PatchOperationAdd" | "PatchOperationInsert" | "PatchOperationReplace" | "PatchOperationRemove" | "PatchOperationSetName" | "PatchOperationAttributeAdd" | "PatchOperationAttributeSet" | "PatchOperationAttributeRemove" => {
                self.run_edit(op, &short, &xpath, origin, tolerated, m)
            }
            "" => {
                self.problem(origin, &xpath, &short, "the operation has no Class", tolerated, m);
                false
            }
            _ => {
                // XML Extensions, VEF and others ship their own operations. Circinus does not
                // run them; it says so rather than pretending the document is complete.
                self.problem(origin, &xpath, &class, "Circinus does not know this operation, so its effect is not in this view", true, m);
                false
            }
        };
        matched
    }

    /// The operations that actually change the document.
    fn run_edit(&mut self, op: NodeId, short: &str, xpath: &str, origin: u32, tolerated: bool, m: &ModInfo) -> bool {
        let targets = self.select(xpath, origin, short, tolerated);
        if targets.is_empty() {
            self.problem(origin, xpath, short, "nothing in the list matches this xpath", tolerated, m);
            return false;
        }
        let value_nodes: Vec<NodeId> = self.doc.child_named_str(op, "value").map(|v| self.doc.children(v).collect()).unwrap_or_default();
        let order = self.doc.child_named_str(op, "order").and_then(|n| self.doc.value(n)).unwrap_or_default();
        let attribute = self.doc.child_named_str(op, "attribute").and_then(|n| self.doc.value(n)).unwrap_or_default();
        let plain_value = self.doc.child_named_str(op, "value").and_then(|n| self.doc.value(n)).unwrap_or_default();
        let new_name = self.doc.child_named_str(op, "name").and_then(|n| self.doc.value(n)).unwrap_or_default();

        for t in targets {
            match short {
                "PatchOperationAdd" => {
                    let Item::Node(target) = t else { continue };
                    for v in &value_nodes {
                        let copy = self.doc.clone_subtree(*v, Some(origin), 0);
                        if order.eq_ignore_ascii_case("Prepend") {
                            self.doc.prepend_child(target, copy);
                        } else {
                            self.doc.append_child(target, copy);
                        }
                    }
                }
                "PatchOperationInsert" => {
                    let Item::Node(target) = t else { continue };
                    // Prepend (the default) puts the value before the matched node.
                    let append = order.eq_ignore_ascii_case("Append");
                    let mut anchor = target;
                    for v in &value_nodes {
                        let copy = self.doc.clone_subtree(*v, Some(origin), 0);
                        if append {
                            self.doc.insert_after(anchor, copy);
                            anchor = copy;
                        } else {
                            self.doc.insert_before(target, copy);
                        }
                    }
                }
                "PatchOperationRemove" => {
                    if let Some(i) = self.note_change(t, origin, "PatchOperationRemove", String::new(), m) {
                        self.report.overwrites[i].next = super::tree::NONE;
                    }
                    match t {
                        Item::Node(n) => self.doc.remove(n),
                        Item::Attr { node, index } => {
                            let name = self.doc.attr_name_at(index);
                            self.doc.remove_attr(node, name);
                        }
                    }
                }
                "PatchOperationReplace" => match t {
                    Item::Node(target) => {
                        let after = value_nodes.first().map(|v| self.doc.string_value(*v).trim().to_string()).unwrap_or_default();
                        let noted = self.note_change(t, origin, "PatchOperationReplace", after, m);
                        let mut anchor = target;
                        let mut first: Option<NodeId> = None;
                        for v in &value_nodes {
                            let copy = self.doc.clone_subtree(*v, Some(origin), 0);
                            self.doc.insert_after(anchor, copy);
                            anchor = copy;
                            first.get_or_insert(copy);
                        }
                        if let Some(i) = noted {
                            self.report.overwrites[i].next = first.unwrap_or(super::tree::NONE);
                        }
                        self.doc.remove(target);
                    }
                    Item::Attr { node, index } => {
                        let name = self.doc.attr_name_at(index);
                        self.note_change(t, origin, "PatchOperationReplace", plain_value.clone(), m);
                        self.doc.set_attr(node, name, &plain_value);
                    }
                },
                "PatchOperationSetName" => {
                    let Item::Node(target) = t else { continue };
                    let sym = self.doc.syms.intern(&new_name);
                    self.note_change(t, origin, "PatchOperationSetName", new_name.clone(), m);
                    self.doc.rename(target, sym);
                    self.doc.nodes[target as usize].origin = origin;
                }
                "PatchOperationAttributeAdd" | "PatchOperationAttributeSet" => {
                    let Item::Node(target) = t else { continue };
                    let sym = self.doc.syms.intern(&attribute);
                    let exists = self.doc.attr(target, sym).is_some();
                    // Add only fills a gap; Set always writes.
                    if exists && short == "PatchOperationAttributeAdd" {
                        continue;
                    }
                    if exists {
                        self.note_change(t, origin, short, plain_value.clone(), m);
                    }
                    self.doc.set_attr(target, sym, &plain_value);
                }
                "PatchOperationAttributeRemove" => {
                    let Item::Node(target) = t else { continue };
                    let Some(sym) = self.doc.syms.lookup(&attribute) else { continue };
                    self.doc.remove_attr(target, sym);
                }
                _ => {}
            }
        }
        true
    }

    fn select(&mut self, xpath: &str, origin: u32, class: &str, tolerated: bool) -> Vec<Item> {
        if xpath.trim().is_empty() {
            return Vec::new();
        }
        match self.compile(xpath) {
            Err(e) => {
                self.report.problems.push(PatchProblem { origin, xpath: xpath.to_string(), class: class.to_string(), reason: format!("the xpath does not parse: {e}"), tolerated });
                Vec::new()
            }
            Ok(expr) => match xpath::select(&self.doc, DOCUMENT, &expr) {
                Ok(items) => items,
                Err(e) => {
                    self.report.problems.push(PatchProblem { origin, xpath: xpath.to_string(), class: class.to_string(), reason: e.0, tolerated });
                    Vec::new()
                }
            },
        }
    }

    fn problem(&mut self, origin: u32, xpath: &str, class: &str, reason: &str, tolerated: bool, m: &ModInfo) {
        if !tolerated {
            self.stats_for(m).failed_operations += 1;
        }
        self.report.problems.push(PatchProblem { origin, xpath: xpath.to_string(), class: class.to_string(), reason: reason.to_string(), tolerated });
    }

    /// Record that a patch took a value over from whoever owned it.
    /// Returns the journal index, so a Replace can record the node that took over.
    fn note_change(&mut self, target: Item, origin: u32, how: &str, new_value: String, m: &ModInfo) -> Option<usize> {
        let owner = target.owner();
        let old_origin = match target {
            Item::Node(n) => self.doc.nodes[n as usize].origin,
            Item::Attr { node, .. } => self.doc.nodes[node as usize].origin,
        };
        if old_origin == origin || old_origin == 0 {
            return None;
        }
        // A mod patching its own defs is not two mods disagreeing; nobody needs to read about it.
        if self.origins[old_origin as usize].uid == m.uid {
            return None;
        }
        let Some((def, path)) = self.locate(owner) else { return None };
        let old_value = match target {
            Item::Node(n) => self.doc.string_value(n).trim().to_string(),
            Item::Attr { index, .. } => self.doc.attr_value_at(index).to_string(),
        };
        let from_uid = self.origins[old_origin as usize].uid.clone();
        // Until told otherwise the node is changed in place; Remove and Replace say so after.
        self.report.overwrites.push(Overwrite { def, path, from: old_origin, to: origin, old_value, new_value, how: how.to_string(), node: owner, next: owner });
        self.stats_for(m).wins += 1;
        if let Some(s) = self.stats.get_mut(&from_uid) {
            s.losses += 1;
        }
        Some(self.report.overwrites.len() - 1)
    }

    /// The def a node belongs to, and the node's path inside it.
    fn locate(&self, n: NodeId) -> Option<(String, String)> {
        let mut cur = n;
        while let Some(p) = self.doc.parent(cur) {
            if p == self.root {
                return Some((self.def_label(cur), self.doc.path_within(cur, n)));
            }
            cur = p;
        }
        None
    }

    fn def_label(&self, def: NodeId) -> String {
        let ty = self.doc.name(def);
        let name = self
            .doc
            .child_named(def, self.doc.s.def_name)
            .and_then(|d| self.doc.value(d))
            .or_else(|| self.doc.attr(def, self.doc.s.name).map(|s| s.to_string()))
            .unwrap_or_else(|| "(unnamed)".into());
        format!("{ty}/{name}")
    }

    // ---- inheritance ----------------------------------------------------------------------

    /// Resolve `ParentName` against `Name`, the way RimWorld's XmlInheritance does: a child
    /// takes every node of its parent it does not define itself, `Inherit="False"` opts out, and
    /// abstract nodes are dropped at the end.
    pub fn resolve_inheritance(&mut self) {
        let (name_s, parent_s, inherit_s, abstract_s) = (self.doc.s.name, self.doc.s.parent_name, self.doc.s.inherit, self.doc.s.abstract_);
        let defs: Vec<NodeId> = self.doc.element_children(self.root).collect();
        // Named nodes, by (type, Name) — RimWorld keys inheritance within a def type.
        let mut named: HashMap<(Sym, String), NodeId> = HashMap::new();
        for d in &defs {
            if let Some(n) = self.doc.attr(*d, name_s) {
                named.insert((self.doc.tag(*d), n.to_string()), *d);
            }
        }
        // Resolve parents first: walk each chain from the top down, memoising.
        let mut done: HashSet<NodeId> = HashSet::new();
        for d in defs.iter().copied() {
            let mut chain: Vec<NodeId> = Vec::new();
            let mut cur = d;
            loop {
                if done.contains(&cur) {
                    break;
                }
                let Some(pn) = self.doc.attr(cur, parent_s).map(|s| s.to_string()) else { break };
                match named.get(&(self.doc.tag(cur), pn.clone())) {
                    None => {
                        let label = self.def_label(cur);
                        self.report.missing_parents.push(format!("{label} asks for a parent called {pn}, which nothing defines"));
                        break;
                    }
                    Some(&p) => {
                        if chain.contains(&p) || p == cur {
                            self.report.missing_parents.push(format!("{} inherits from itself", self.def_label(cur)));
                            break;
                        }
                        chain.push(cur);
                        cur = p;
                    }
                }
            }
            // `chain` runs child → … → the deepest unresolved ancestor; apply from the top.
            for node in chain.into_iter().rev() {
                if done.insert(node) {
                    if let Some(pn) = self.doc.attr(node, parent_s).map(|s| s.to_string()) {
                        if let Some(&p) = named.get(&(self.doc.tag(node), pn)) {
                            self.inherit_into(node, p, inherit_s);
                        }
                    }
                }
            }
            done.insert(d);
        }
        // Abstract nodes are scaffolding; the game never sees them.
        for d in defs {
            if self.doc.attr(d, abstract_s).map(|v| v.eq_ignore_ascii_case("true")).unwrap_or(false) {
                self.doc.remove(d);
            }
        }
    }

    /// Copy whatever `child` does not define from `parent`, recursively, marking every copied
    /// node with the parent it came from so the inspector can say "inherited from BuildingBase".
    fn inherit_into(&mut self, child: NodeId, parent: NodeId, inherit_s: Sym) {
        if self.doc.attr(child, inherit_s).map(|v| v.eq_ignore_ascii_case("false")).unwrap_or(false) {
            return;
        }
        let via = parent + 1; // 0 means "own"; store the parent's id offset by one
        let parent_kids: Vec<NodeId> = self.doc.element_children(parent).collect();
        for pk in parent_kids {
            let tag = self.doc.tag(pk);
            // A list the child also defines is replaced wholesale, not merged, as RimWorld does.
            match self.doc.child_named(child, tag) {
                Some(ck) => {
                    if self.doc.attr(ck, inherit_s).map(|v| v.eq_ignore_ascii_case("false")).unwrap_or(false) {
                        continue;
                    }
                    // Both sides have this element: merge their children unless the child holds
                    // a plain value, which simply wins.
                    if self.doc.has_element_children(pk) && self.doc.has_element_children(ck) && self.doc.child_named(ck, self.doc.s.li).is_none() {
                        self.inherit_into(ck, pk, inherit_s);
                    }
                }
                None => {
                    let copy = self.doc.clone_subtree(pk, None, via);
                    self.doc.append_child(child, copy);
                }
            }
        }
    }

    // ---- finishing --------------------------------------------------------------------------

    pub fn finish(mut self, elapsed_ms: u64) -> Flattened {
        let defs: Vec<NodeId> = self.doc.element_children(self.root).collect();
        self.report.defs = defs.len();
        self.report.chains = build_chains(&self.report.overwrites);

        // Duplicates: the same type and defName from more than one origin.
        let mut by_key: HashMap<(String, String), Vec<(NodeId, u32)>> = HashMap::new();
        for d in &defs {
            if let Some(name) = self.doc.child_named(*d, self.doc.s.def_name).and_then(|n| self.doc.value(n)) {
                by_key.entry((self.doc.name(*d).to_string(), name)).or_default().push((*d, self.doc.nodes[*d as usize].origin));
            }
        }
        for ((def_type, def_name), list) in by_key {
            if list.len() > 1 {
                let origins: Vec<u32> = list.iter().map(|(_, o)| *o).collect();
                let winner = *origins.last().unwrap();
                // Two folders of the same mod are not a conflict worth reporting.
                let uids: HashSet<&str> = origins.iter().map(|o| self.origins[*o as usize].uid.as_str()).collect();
                if uids.len() > 1 {
                    self.report.duplicates.push(Duplicate { def_type, def_name, origins, winner });
                }
            }
        }
        self.report.duplicates.sort_by(|a, b| (&a.def_type, &a.def_name).cmp(&(&b.def_type, &b.def_name)));

        // Values, and who owns them at the end.
        let mut per_origin: HashMap<u32, usize> = HashMap::new();
        let mut values = 0usize;
        let mut kept_defs: HashMap<String, usize> = HashMap::new();
        for d in &defs {
            let owner = self.origins[self.doc.nodes[*d as usize].origin as usize].uid.clone();
            *kept_defs.entry(owner).or_default() += 1;
            for n in self.doc.descendants(*d) {
                if self.doc.is_element(n) && !self.doc.has_element_children(n) {
                    values += 1;
                    *per_origin.entry(self.doc.nodes[n as usize].origin).or_default() += 1;
                }
            }
        }
        self.report.values = values;
        for (origin, n) in per_origin {
            let uid = self.origins[origin as usize].uid.clone();
            if let Some(s) = self.stats.get_mut(&uid) {
                s.values += n;
            }
        }
        for (uid, n) in kept_defs {
            if let Some(s) = self.stats.get_mut(&uid) {
                s.defs = n;
            }
        }
        let mut per_mod: Vec<ModStats> = self.stats.into_values().collect();
        per_mod.sort_by(|a, b| b.values.cmp(&a.values).then(a.name.cmp(&b.name)));
        self.report.per_mod = per_mod;
        self.report.origins = self.origins;
        self.report.elapsed_ms = elapsed_ms;
        Flattened { doc: self.doc, report: self.report }
    }

    /// The document as it stands, for the inspector.
    pub fn doc(&self) -> &Doc {
        &self.doc
    }
    pub fn root(&self) -> NodeId {
        self.root
    }
}

/// Flatten a whole active list: Defs first in load order, then patches in load order, then
/// inheritance. `folders_for` gives the load folders of a mod (see `textures::active_load_folders`).
pub fn flatten(order: &[&ModInfo], folders_for: &dyn Fn(&ModInfo) -> Vec<String>, progress: &dyn Fn(usize, usize)) -> Flattened {
    let started = std::time::Instant::now();
    let ids: Vec<String> = order.iter().map(|m| m.package_id.clone()).collect();
    let mut f = Flattener::new(&ids);
    let total = order.len() * 2;
    for (i, m) in order.iter().enumerate() {
        f.add_defs(m, i as u32, &folders_for(m));
        progress(i, total);
    }
    for (i, m) in order.iter().enumerate() {
        f.add_patches(m, i as u32, &folders_for(m));
        progress(order.len() + i, total);
    }
    f.resolve_inheritance();
    f.finish(started.elapsed().as_millis() as u64)
}

// ---------------------------------------------------------------------------------------------
// Histories
// ---------------------------------------------------------------------------------------------

/// Follow the journal by node: an entry whose `node` is an earlier entry's `next` continues that
/// history. Twenty mods each removing `comps/li[3]` are twenty histories of one step, not one of
/// twenty — and then, because twenty one-line rows saying "removed" are their own kind of noise,
/// removals of sibling list items by the same mod fold into one row that lists what went.
pub fn build_chains(overwrites: &[Overwrite]) -> Vec<Chain> {
    let none = super::tree::NONE;
    // Which journal entry continues from which node.
    let mut continues_at: HashMap<u32, usize> = HashMap::new();
    for (i, o) in overwrites.iter().enumerate() {
        continues_at.entry(o.node).or_insert(i);
    }
    let mut taken = vec![false; overwrites.len()];
    let mut chains: Vec<Chain> = Vec::new();
    for (i, first) in overwrites.iter().enumerate() {
        if taken[i] {
            continue;
        }
        taken[i] = true;
        let (def_type, def_name) = split_def(&first.def);
        let mut steps = vec![ChainStep { origin: first.from, value: first.old_value.clone(), how: "Defs".into() }, ChainStep { origin: first.to, value: first.new_value.clone(), how: first.how.clone() }];
        let mut cur = first;
        loop {
            if cur.next == none {
                break;
            }
            // The next entry on this node — or on the node that replaced it — must come later in
            // the journal, since the journal is in load order.
            let Some(&j) = continues_at.get(&cur.next).filter(|&&j| j > i && !taken[j]) else { break };
            taken[j] = true;
            cur = &overwrites[j];
            steps.push(ChainStep { origin: cur.to, value: cur.new_value.clone(), how: cur.how.clone() });
        }
        chains.push(Chain { def: first.def.clone(), def_type, def_name, path: first.path.clone(), steps, removed: Vec::new() });
    }

    // Fold "the same mod removed several items of the same list" into one row.
    let mut folded: Vec<Chain> = Vec::with_capacity(chains.len());
    let mut groups: HashMap<(String, String, u32, u32), Vec<usize>> = HashMap::new();
    for (i, c) in chains.iter().enumerate() {
        let is_removal = c.steps.len() == 2 && c.steps[1].how == "PatchOperationRemove";
        let Some(list) = list_of(&c.path).filter(|_| is_removal) else { continue };
        groups.entry((c.def.clone(), list.to_string(), c.steps[0].origin, c.steps[1].origin)).or_default().push(i);
    }
    let mut absorbed = vec![false; chains.len()];
    for ((def, list, from, to), members) in groups {
        if members.len() < 2 {
            continue;
        }
        let removed: Vec<String> = members.iter().map(|&i| chains[i].steps[0].value.clone()).collect();
        for &i in &members {
            absorbed[i] = true;
        }
        let (def_type, def_name) = split_def(&def);
        folded.push(Chain {
            def,
            def_type,
            def_name,
            path: list,
            steps: vec![ChainStep { origin: from, value: format!("{} items", removed.len()), how: "Defs".into() }, ChainStep { origin: to, value: String::new(), how: "PatchOperationRemove".into() }],
            removed,
        });
    }
    for (i, c) in chains.into_iter().enumerate() {
        if !absorbed[i] {
            folded.push(c);
        }
    }
    // Longest histories first: those are the values worth a look.
    folded.sort_by(|a, b| b.steps.len().cmp(&a.steps.len()).then(b.removed.len().cmp(&a.removed.len())).then(a.def.cmp(&b.def)).then(a.path.cmp(&b.path)));
    folded
}

fn split_def(def: &str) -> (String, String) {
    match def.split_once('/') {
        Some((t, n)) => (t.to_string(), n.to_string()),
        None => (def.to_string(), String::new()),
    }
}

/// `comps/li[3]` → `comps`: the list an item belongs to, when the path ends in a list item.
fn list_of(path: &str) -> Option<&str> {
    let (parent, last) = path.rsplit_once('/')?;
    (last == "li" || last.starts_with("li[")).then_some(parent)
}

// ---------------------------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------------------------

fn mod_root(m: &ModInfo) -> PathBuf {
    m.link_target.clone().unwrap_or_else(|| m.path.clone())
}

fn join_folder(root: &Path, folder: &str) -> PathBuf {
    let f = folder.trim_matches(['/', '\\']);
    if f.is_empty() || f == "." {
        return root.to_path_buf();
    }
    let mut p = root.to_path_buf();
    for seg in f.split(['/', '\\']) {
        p.push(seg);
    }
    p
}

fn display_path(root: &Path, file: &Path) -> String {
    file.strip_prefix(root).unwrap_or(file).to_string_lossy().replace('\\', "/")
}

/// Every .xml under a folder, in the order RimWorld reads them (by path, case-insensitive).
fn xml_files(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = walkdir::WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.into_path())
        .filter(|p| p.extension().map(|x| x.eq_ignore_ascii_case("xml")).unwrap_or(false))
        .collect();
    out.sort_by_key(|p| p.to_string_lossy().to_lowercase());
    out
}

/// The load folders of a mod, defaulting the way RimWorld does. A thin wrapper so callers do not
/// need the scan's file lists when they only have `ModInfo`.
pub fn load_folders(m: &ModInfo, major_minor: &str, active: &HashSet<String>) -> Vec<String> {
    match &m.load_folders {
        Some(folders) => folders
            .iter()
            .filter(|f: &&LoadFolder| f.if_mod_active.is_empty() || f.if_mod_active.iter().any(|id| active.contains(id)))
            .filter(|f| f.if_mod_not_active.iter().all(|id| !active.contains(id)))
            .map(|f| f.path.clone())
            .collect(),
        None => {
            let root = mod_root(m);
            let mut out = Vec::new();
            if root.join(major_minor).is_dir() {
                out.push(major_minor.to_string());
            } else {
                // the newest version folder that is not above the game's
                let mut best: Option<String> = None;
                if let Ok(entries) = std::fs::read_dir(&root) {
                    for e in entries.flatten() {
                        let name = e.file_name().to_string_lossy().to_string();
                        let usable = crate::game::major_minor_of(&name).map(|mm| mm == name).unwrap_or(false) && crate::game::cmp_major_minor(&name, major_minor) != std::cmp::Ordering::Greater;
                        if usable && best.as_ref().map(|b| crate::game::cmp_major_minor(&name, b) == std::cmp::Ordering::Greater).unwrap_or(true) {
                            best = Some(name);
                        }
                    }
                }
                out.extend(best);
            }
            if root.join("Common").is_dir() {
                out.push("Common".into());
            }
            out.push(String::new());
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Source;
    use std::fs;

    fn write(p: &Path, text: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    fn a_mod(root: &Path, folder: &str, id: &str, name: &str) -> ModInfo {
        ModInfo { uid: folder.into(), path: root.join(folder), package_id: id.into(), name: name.into(), source: Source::Local, ..Default::default() }
    }

    /// Three mods: one ships the walls, one patches a value, one adds a comp.
    fn fixture() -> (tempfile::TempDir, Vec<ModInfo>) {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        write(
            &r.join("base/Defs/Things.xml"),
            r#"<Defs>
              <ThingDef Name="BuildingBase" Abstract="True"><statBases><MaxHitPoints>100</MaxHitPoints><Beauty>0</Beauty></statBases></ThingDef>
              <ThingDef ParentName="BuildingBase"><defName>Wall</defName><label>wall</label><statBases><MaxHitPoints>300</MaxHitPoints></statBases><comps><li Class="CompA"/></comps></ThingDef>
              <ThingDef ParentName="BuildingBase"><defName>Door</defName><label>door</label></ThingDef>
            </Defs>"#,
        );
        write(
            &r.join("tweak/Patches/Walls.xml"),
            r#"<Patch>
              <Operation Class="PatchOperationReplace">
                <xpath>Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints</xpath>
                <value><MaxHitPoints>500</MaxHitPoints></value>
              </Operation>
              <Operation Class="PatchOperationAdd">
                <xpath>Defs/ThingDef[defName="Wall"]/comps</xpath>
                <value><li Class="CompB"/></value>
              </Operation>
              <Operation Class="PatchOperationReplace">
                <xpath>Defs/ThingDef[defName="Nothing"]/label</xpath>
                <value><label>x</label></value>
              </Operation>
            </Patch>"#,
        );
        write(
            &r.join("late/Patches/Later.xml"),
            r#"<Patch>
              <Operation Class="PatchOperationReplace">
                <xpath>Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints</xpath>
                <value><MaxHitPoints>900</MaxHitPoints></value>
              </Operation>
              <Operation Class="PatchOperationAttributeSet">
                <xpath>Defs/ThingDef[defName="Door"]</xpath>
                <attribute>Name</attribute>
                <value>DoorBase</value>
              </Operation>
            </Patch>"#,
        );
        let mods = vec![a_mod(r, "base", "a.base", "Base"), a_mod(r, "tweak", "b.tweak", "Tweak"), a_mod(r, "late", "c.late", "Late")];
        (dir, mods)
    }

    fn run(mods: &[ModInfo]) -> Flattened {
        let refs: Vec<&ModInfo> = mods.iter().collect();
        flatten(&refs, &|_| vec![String::new()], &|_, _| {})
    }

    #[test]
    fn patches_run_in_load_order_and_the_last_one_wins() {
        let (_d, mods) = fixture();
        let f = run(&mods);
        let doc = &f.doc;
        let wall = xpath::select_str(doc, DOCUMENT, r#"Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints"#).unwrap();
        assert_eq!(wall.len(), 1);
        assert_eq!(doc.string_value(wall[0].owner()), "900");
        // the value belongs to the mod that set it last
        let origin = doc.nodes[wall[0].owner() as usize].origin;
        assert_eq!(f.report.origins[origin as usize].name, "Late");
        // and both take-overs are written down
        let takes: Vec<(&str, &str, &str)> = f.report.overwrites.iter().map(|o| (o.path.as_str(), f.report.origins[o.from as usize].name.as_str(), f.report.origins[o.to as usize].name.as_str())).collect();
        assert_eq!(takes, [("statBases/MaxHitPoints", "Base", "Tweak"), ("statBases/MaxHitPoints", "Tweak", "Late")]);
        assert_eq!(f.report.overwrites[0].old_value, "300");
        assert_eq!(f.report.overwrites[0].new_value, "500");
        // the added comp is there, after the original
        let comps = xpath::select_str(doc, DOCUMENT, r#"Defs/ThingDef[defName="Wall"]/comps/li"#).unwrap();
        let classes: Vec<&str> = comps.iter().map(|c| doc.attr_str(c.owner(), "Class").unwrap_or("")).collect();
        assert_eq!(classes, ["CompA", "CompB"]);
    }

    #[test]
    fn a_patch_that_matches_nothing_is_reported_against_its_mod() {
        let (_d, mods) = fixture();
        let f = run(&mods);
        let miss: Vec<&PatchProblem> = f.report.problems.iter().filter(|p| p.reason.contains("nothing in the list matches")).collect();
        assert_eq!(miss.len(), 1);
        assert_eq!(f.report.origins[miss[0].origin as usize].name, "Tweak");
        assert!(miss[0].xpath.contains("Nothing"));
        assert!(!miss[0].tolerated);
        let tweak = f.report.per_mod.iter().find(|s| s.name == "Tweak").unwrap();
        assert_eq!(tweak.operations, 3);
        assert_eq!(tweak.failed_operations, 1);
        assert_eq!(tweak.wins, 1, "it took MaxHitPoints from Base");
        let base = f.report.per_mod.iter().find(|s| s.name == "Base").unwrap();
        assert_eq!(base.losses, 1, "Tweak took MaxHitPoints; the Name Late adds to Door was not Base's to lose");
    }

    #[test]
    fn inheritance_fills_the_gaps_and_abstracts_disappear() {
        let (_d, mods) = fixture();
        let f = run(&mods);
        let doc = &f.doc;
        // the abstract base is gone
        assert!(xpath::select_str(doc, DOCUMENT, r#"Defs/ThingDef[@Name="BuildingBase"]"#).unwrap().is_empty());
        // Door had no statBases at all: it takes the parent's whole block
        let beauty = xpath::select_str(doc, DOCUMENT, r#"Defs/ThingDef[defName="Door"]/statBases/Beauty"#).unwrap();
        assert_eq!(doc.string_value(beauty[0].owner()), "0");
        // Wall defined MaxHitPoints itself and keeps it, but takes Beauty from the parent
        assert_eq!(doc.string_value(xpath::select_str(doc, DOCUMENT, r#"Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints"#).unwrap()[0].owner()), "900");
        let wall_beauty = xpath::select_str(doc, DOCUMENT, r#"Defs/ThingDef[defName="Wall"]/statBases/Beauty"#).unwrap();
        assert_eq!(wall_beauty.len(), 1);
        assert_ne!(doc.nodes[wall_beauty[0].owner() as usize].via, 0, "an inherited node says where it came from");
        assert_eq!(f.report.defs, 2);
    }

    #[test]
    fn duplicate_defs_from_two_mods_are_named() {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        write(&r.join("one/Defs/a.xml"), r#"<Defs><ThingDef><defName>Wall</defName><label>mine</label></ThingDef></Defs>"#);
        write(&r.join("two/Defs/a.xml"), r#"<Defs><ThingDef><defName>Wall</defName><label>theirs</label></ThingDef></Defs>"#);
        let mods = vec![a_mod(r, "one", "a.one", "One"), a_mod(r, "two", "b.two", "Two")];
        let f = run(&mods);
        assert_eq!(f.report.duplicates.len(), 1);
        let dup = &f.report.duplicates[0];
        assert_eq!((dup.def_type.as_str(), dup.def_name.as_str()), ("ThingDef", "Wall"));
        assert_eq!(f.report.origins[dup.winner as usize].name, "Two");
    }

    #[test]
    fn mayrequire_drops_what_is_not_installed_and_findmod_picks_a_branch() {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        write(
            &r.join("one/Defs/a.xml"),
            r#"<Defs><ThingDef><defName>Wall</defName><label>wall</label>
                 <comps><li Class="Plain"/><li MayRequire="not.installed" Class="Fancy"/></comps></ThingDef></Defs>"#,
        );
        write(
            &r.join("one/Patches/p.xml"),
            r#"<Patch>
                <Operation Class="PatchOperationFindMod">
                  <mods><li>a.one</li></mods>
                  <match Class="PatchOperationAdd"><xpath>Defs/ThingDef[defName="Wall"]</xpath><value><smeltable>true</smeltable></value></match>
                  <nomatch Class="PatchOperationAdd"><xpath>Defs/ThingDef[defName="Wall"]</xpath><value><smeltable>false</smeltable></value></nomatch>
                </Operation>
                <Operation Class="PatchOperationConditional">
                  <xpath>Defs/ThingDef[defName="Wall"]/nothingHere</xpath>
                  <nomatch Class="PatchOperationAdd"><xpath>Defs/ThingDef[defName="Wall"]</xpath><value><branch>nomatch</branch></value></nomatch>
                </Operation>
                <Operation Class="XmlExtensions.PatchOperationSomething"><xpath>Defs</xpath></Operation>
              </Patch>"#,
        );
        let mods = vec![a_mod(r, "one", "a.one", "One")];
        let f = run(&mods);
        let doc = &f.doc;
        let classes: Vec<&str> = xpath::select_str(doc, DOCUMENT, r#"Defs/ThingDef[defName="Wall"]/comps/li"#).unwrap().iter().map(|c| doc.attr_str(c.owner(), "Class").unwrap_or("")).collect();
        assert_eq!(classes, ["Plain"], "the li that needs an absent mod is not in the list");
        assert_eq!(doc.string_value(xpath::select_str(doc, DOCUMENT, r#"Defs/ThingDef[defName="Wall"]/smeltable"#).unwrap()[0].owner()), "true");
        assert_eq!(doc.string_value(xpath::select_str(doc, DOCUMENT, r#"Defs/ThingDef[defName="Wall"]/branch"#).unwrap()[0].owner()), "nomatch");
        // an operation from another mod's patch framework is named, not silently skipped
        let unknown: Vec<&PatchProblem> = f.report.problems.iter().filter(|p| p.class.starts_with("XmlExtensions")).collect();
        assert_eq!(unknown.len(), 1);
        assert!(unknown[0].tolerated);
    }

    /// The screenshot case: a list where every mod removes a different item. Those are separate
    /// histories, not one value changing hands twenty times, and they fold into one readable
    /// row per mod. A real chain — Base → Tweak → Late on one node — stays one chain.
    #[test]
    fn histories_follow_the_node_not_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        write(
            &r.join("base/Defs/a.xml"),
            r#"<Defs><ThingDef><defName>Wall</defName><statBases><MaxHitPoints>100</MaxHitPoints></statBases>
                 <comps><li>one</li><li>two</li><li>three</li><li>four</li></comps></ThingDef></Defs>"#,
        );
        // Removing li[1] three times removes three different items; the path is the same each time.
        write(
            &r.join("cull/Patches/p.xml"),
            r#"<Patch>
                <Operation Class="PatchOperationRemove"><xpath>Defs/ThingDef[defName="Wall"]/comps/li[1]</xpath></Operation>
                <Operation Class="PatchOperationRemove"><xpath>Defs/ThingDef[defName="Wall"]/comps/li[1]</xpath></Operation>
                <Operation Class="PatchOperationRemove"><xpath>Defs/ThingDef[defName="Wall"]/comps/li[1]</xpath></Operation>
                <Operation Class="PatchOperationReplace"><xpath>Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints</xpath><value><MaxHitPoints>500</MaxHitPoints></value></Operation>
              </Patch>"#,
        );
        write(
            &r.join("late/Patches/p.xml"),
            r#"<Patch>
                <Operation Class="PatchOperationReplace"><xpath>Defs/ThingDef[defName="Wall"]/statBases/MaxHitPoints</xpath><value><MaxHitPoints>900</MaxHitPoints></value></Operation>
                <Operation Class="PatchOperationRemove"><xpath>Defs/ThingDef[defName="Wall"]/comps/li[1]</xpath></Operation>
              </Patch>"#,
        );
        let mods = vec![a_mod(r, "base", "a.base", "Base"), a_mod(r, "cull", "b.cull", "Cull"), a_mod(r, "late", "c.late", "Late")];
        let f = run(&mods);
        let name = |o: u32| f.report.origins[o as usize].name.as_str();
        let chains = &f.report.chains;
        // The MaxHitPoints history is one chain of three steps, and the longest so it is first.
        let hp = chains.iter().find(|c| c.path == "statBases/MaxHitPoints").unwrap();
        let steps: Vec<(&str, &str)> = hp.steps.iter().map(|s| (name(s.origin), s.value.as_str())).collect();
        assert_eq!(steps, [("Base", "100"), ("Cull", "500"), ("Late", "900")]);
        assert_eq!(chains[0].path, "statBases/MaxHitPoints");
        // Cull's three removals from one list are one row naming what went …
        let cull = chains.iter().find(|c| c.path == "comps" && name(c.steps[1].origin) == "Cull").unwrap();
        assert_eq!(cull.removed, ["one", "two", "three"]);
        assert_eq!(cull.steps[0].value, "3 items");
        // … and Late's single removal stays its own row, on the item it actually removed.
        let late = chains.iter().find(|c| name(c.steps[1].origin) == "Late" && c.steps[1].how == "PatchOperationRemove").unwrap();
        // by then it is the only item left, so the path carries no index
        assert_eq!(late.path, "comps/li");
        assert_eq!(late.steps[0].value, "four");
        assert!(late.removed.is_empty());
        assert_eq!(chains.len(), 3, "{:?}", chains.iter().map(|c| (&c.path, c.steps.len())).collect::<Vec<_>>());
        // and every journal entry knows its node; a Remove says nothing follows
        assert!(f.report.overwrites.iter().all(|o| o.node != 0));
        assert!(f.report.overwrites.iter().filter(|o| o.how == "PatchOperationRemove").all(|o| o.next == super::super::tree::NONE));
    }

    /// A mod changing its own def is not a disagreement and never shows as one.
    #[test]
    fn a_mod_patching_itself_is_not_contested() {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        write(&r.join("one/Defs/a.xml"), r#"<Defs><ThingDef><defName>Wall</defName><label>wall</label></ThingDef></Defs>"#);
        write(&r.join("one/Patches/p.xml"), r#"<Patch><Operation Class="PatchOperationReplace"><xpath>Defs/ThingDef[defName="Wall"]/label</xpath><value><label>better wall</label></value></Operation></Patch>"#);
        let f = run(&[a_mod(r, "one", "a.one", "One")]);
        assert!(f.report.overwrites.is_empty());
        assert!(f.report.chains.is_empty());
        assert_eq!(f.report.per_mod[0].wins, 0);
    }

    #[test]
    fn insert_remove_and_setname() {
        let dir = tempfile::tempdir().unwrap();
        let r = dir.path();
        write(&r.join("one/Defs/a.xml"), r#"<Defs><ThingDef><defName>Wall</defName><a>1</a><b>2</b><c>3</c></ThingDef></Defs>"#);
        write(
            &r.join("two/Patches/p.xml"),
            r#"<Patch>
                <Operation Class="PatchOperationInsert"><xpath>Defs/ThingDef[defName="Wall"]/b</xpath><value><a2>x</a2></value></Operation>
                <Operation Class="PatchOperationRemove"><xpath>Defs/ThingDef[defName="Wall"]/c</xpath></Operation>
                <Operation Class="PatchOperationSetName"><xpath>Defs/ThingDef[defName="Wall"]/a</xpath><name>renamed</name></Operation>
              </Patch>"#,
        );
        let mods = vec![a_mod(r, "one", "a.one", "One"), a_mod(r, "two", "b.two", "Two")];
        let f = run(&mods);
        let doc = &f.doc;
        let wall = xpath::select_str(doc, DOCUMENT, r#"Defs/ThingDef[defName="Wall"]"#).unwrap()[0].owner();
        let kids: Vec<&str> = doc.element_children(wall).map(|k| doc.name(k)).collect();
        assert_eq!(kids, ["defName", "renamed", "a2", "b"]);
    }
}

