//! HALO — Harmonized Automated Load Order.
//!
//! Rules are hard constraints (edges in a DAG); phases are soft preferences that decide the
//! order wherever the rules do not. Contradictory rules are resolved by source precedence, and
//! a real cycle is explained and cut rather than aborting the sort.

use crate::model::*;
use crate::rules::Databases;
use crate::scan::ModFiles;
use crate::textures;
use petgraph::algo::tarjan_scc;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

const PREPATCH_IDS: &[&str] = &["zetrith.prepatcher", "jikulopo.prepatcher", "brrainz.harmony", "brrainz.visualexceptions", "bs.fishery"];
const FRAMEWORK_IDS: &[&str] = &[
    "unlimitedhugs.hugslib",
    "oskarpotocki.vanillafactionsexpanded.core",
    "smashphil.vehicleframework",
    "imranfish.xmlextensions",
    "adaptive.storage.framework",
    "aoba.framework",
    "aoba.exosuit.framework",
    "ebsg.framework",
    "owlchemist.cherrypicker",
    "redmattis.betterprerequisites",
    "vanillaexpanded.backgrounds",
    "thesepeople.ritualattachableoutcomes",
    "ceteam.combatextended",
];
const OPTIMIZATION_IDS: &[&str] = &["krkr.rocketman", "bs.performance", "taranchuk.performanceoptimizer", "dubwise.dubsperformanceanalyzer", "telardo.graphicssettings", "notfood.performancefish", "user19990313.runtimegc", "mlie.runtimegc"];

/// Per-user adjustments HALO honours.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserOverrides {
    /// uid → phase the user assigned.
    pub phases: HashMap<String, Phase>,
    /// uids whose position must not change.
    pub pinned: HashSet<String>,
    /// Sort alphabetically within a phase instead of keeping the current arrangement.
    pub alphabetical: bool,
}

pub struct Context<'a> {
    pub mods: &'a [ModInfo],
    pub files: &'a HashMap<String, ModFiles>,
    pub rules: &'a [Rule],
    pub db: &'a Databases,
    pub major_minor: &'a str,
    pub overrides: &'a UserOverrides,
}

fn name_matches(name: &str, needles: &[&str]) -> bool {
    let n = name.to_ascii_lowercase();
    needles.iter().any(|x| n.contains(x))
}

/// Decide a phase for one active mod.
pub fn classify(m: &ModInfo, ctx: &Context, dependents: usize, texture_collides: bool) -> Placement {
    let uid = m.uid.clone();
    if let Some(p) = ctx.overrides.phases.get(&m.uid) {
        return Placement { uid, phase: *p, reason: "Set by you".into() };
    }
    let id = m.package_id.as_str();
    let c = &m.contents;
    if m.source == Source::Ludeon {
        return Placement { uid, phase: Phase::Core, reason: "Official content, pinned by RimWorld".into() };
    }
    if PREPATCH_IDS.contains(&id) {
        return Placement { uid, phase: Phase::Prepatch, reason: "Patches the game before other mods load".into() };
    }
    if ctx.rules.iter().any(|r| r.kind == RuleKind::LoadTop && r.subject == id) {
        return Placement { uid, phase: Phase::Framework, reason: "Rule: load at the top".into() };
    }
    if ctx.rules.iter().any(|r| r.kind == RuleKind::LoadBottom && r.subject == id) {
        return Placement { uid, phase: Phase::Optimization, reason: "Rule: load at the bottom".into() };
    }
    if OPTIMIZATION_IDS.contains(&id) || (c.assemblies > 0 && c.defs == 0 && name_matches(&m.name, &["performance", "optimiz", "optimis", "rocketman", "fps boost"])) {
        return Placement { uid, phase: Phase::Optimization, reason: "Optimizes other mods, so it must see them all".into() };
    }
    if FRAMEWORK_IDS.contains(&id) {
        return Placement { uid, phase: Phase::Framework, reason: "Known framework".into() };
    }
    if c.assemblies > 0 && dependents >= 3 {
        return Placement { uid, phase: Phase::Framework, reason: format!("{dependents} active mods depend on it") };
    }
    if c.assemblies > 0 && name_matches(&m.name, &["framework", "library", " lib", "api"]) && !name_matches(&m.name, &["patch"]) {
        return Placement { uid, phase: Phase::Framework, reason: "Named like a framework and ships code".into() };
    }
    if m.kind == ModKind::Textures || (c.textures + c.dds > 0 && c.assemblies == 0 && c.defs == 0 && (texture_collides || name_matches(&m.name, &["retexture", "texture", "textures"]))) {
        return Placement { uid, phase: Phase::Texture, reason: if texture_collides { "Replaces textures other mods also provide".into() } else { "Textures only".into() } };
    }
    if c.patches > 0 && c.defs == 0 && c.assemblies == 0 {
        return Placement { uid, phase: Phase::Patch, reason: "XML patches only; must see its targets first".into() };
    }
    if name_matches(&m.name, &["patch", "compat"]) && (c.assemblies == 0 || m.rules.dependencies.len() >= 2) {
        return Placement { uid, phase: Phase::Patch, reason: "Compatibility patch".into() };
    }
    Placement { uid, phase: Phase::Content, reason: "Adds content".into() }
}

struct Edge {
    from: NodeIndex,
    to: NodeIndex,
    rule: Rule,
}

/// Build the precedence graph for the active mods. Returns the graph plus the issues found
/// while building it (contradictions resolved, cycles cut).
fn build_graph(order: &[&ModInfo], ctx: &Context) -> (DiGraph<usize, Rule>, Vec<Issue>) {
    let mut g: DiGraph<usize, Rule> = DiGraph::new();
    let mut idx_of: HashMap<&str, NodeIndex> = HashMap::new();
    let mut by_pkg: HashMap<&str, NodeIndex> = HashMap::new();
    for (i, m) in order.iter().enumerate() {
        let n = g.add_node(i);
        idx_of.insert(m.uid.as_str(), n);
        if !m.package_id.is_empty() {
            by_pkg.entry(m.package_id.as_str()).or_insert(n);
        }
    }
    // Candidate edges: (from precedes to)
    let mut cands: Vec<Edge> = Vec::new();
    for r in ctx.rules {
        let (Some(s), Some(t)) = (by_pkg.get(r.subject.as_str()), r.target.as_deref().and_then(|t| by_pkg.get(t))) else { continue };
        if s == t {
            continue;
        }
        match r.kind {
            RuleKind::LoadAfter => cands.push(Edge { from: *t, to: *s, rule: r.clone() }),
            RuleKind::LoadBefore => cands.push(Edge { from: *s, to: *t, rule: r.clone() }),
            _ => {}
        }
    }
    // Dependencies imply loadAfter unless an explicit rule says otherwise.
    let explicit: HashSet<(NodeIndex, NodeIndex)> = cands.iter().map(|e| (e.from, e.to)).collect();
    for m in order {
        let Some(s) = by_pkg.get(m.package_id.as_str()) else { continue };
        for d in &m.rules.dependencies {
            let target = by_pkg.get(d.package_id.as_str()).or_else(|| d.alternatives.iter().find_map(|a| by_pkg.get(a.as_str())));
            let Some(t) = target else { continue };
            if t == s || explicit.contains(&(*s, *t)) || explicit.contains(&(*t, *s)) {
                continue;
            }
            cands.push(Edge { from: *t, to: *s, rule: Rule { kind: RuleKind::LoadAfter, subject: m.package_id.clone(), target: Some(d.package_id.clone()), source: RuleSource::Halo, comment: Some("Dependency".into()) } });
        }
    }
    // Resolve direct contradictions by precedence; equal precedence stays and becomes a cycle.
    let mut best: HashMap<(NodeIndex, NodeIndex), Edge> = HashMap::new();
    for e in cands {
        let key = (e.from, e.to);
        match best.get(&key) {
            Some(x) if x.rule.source >= e.rule.source => {}
            _ => {
                best.insert(key, e);
            }
        }
    }
    let keys: Vec<(NodeIndex, NodeIndex)> = best.keys().copied().collect();
    let mut dropped: Vec<Rule> = Vec::new();
    for (a, b) in keys {
        if a < b {
            if let (Some(x), Some(y)) = (best.get(&(a, b)), best.get(&(b, a))) {
                if x.rule.source > y.rule.source {
                    dropped.push(y.rule.clone());
                    best.remove(&(b, a));
                } else if y.rule.source > x.rule.source {
                    dropped.push(x.rule.clone());
                    best.remove(&(a, b));
                }
            }
        }
    }
    for e in best.into_values() {
        g.add_edge(e.from, e.to, e.rule);
    }
    let mut issues = Vec::new();
    // Cut real cycles: report each strongly connected component, then remove its
    // lowest-precedence internal edges until it is acyclic.
    let mut guard = 0;
    loop {
        guard += 1;
        let sccs: Vec<Vec<NodeIndex>> = tarjan_scc(&g).into_iter().filter(|c| c.len() > 1).collect();
        if sccs.is_empty() || guard > 50 {
            break;
        }
        for comp in sccs {
            let set: HashSet<NodeIndex> = comp.iter().copied().collect();
            let internal: Vec<petgraph::graph::EdgeIndex> = g.edge_references().filter(|e| set.contains(&e.source()) && set.contains(&e.target())).map(|e| e.id()).collect();
            let rules: Vec<Rule> = internal.iter().map(|e| g[*e].clone()).collect();
            let uids: Vec<String> = comp.iter().map(|n| order[g[*n]].uid.clone()).collect();
            let chain = comp.iter().map(|n| order[g[*n]].name.clone()).collect::<Vec<_>>().join(" → ");
            issues.push(Issue::Cycle { uids, chain: format!("{chain} → {}", order[g[comp[0]]].name), rules: rules.clone() });
            let min_src = rules.iter().map(|r| r.source).min().unwrap_or(RuleSource::About);
            for e in internal.into_iter().rev() {
                if g[e].source == min_src {
                    g.remove_edge(e);
                }
            }
        }
    }
    let _ = dropped;
    (g, issues)
}

/// Count, for every active packageId, how many active mods declare it as a dependency.
fn dependents_count(order: &[&ModInfo]) -> HashMap<String, usize> {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for m in order {
        for d in &m.rules.dependencies {
            *counts.entry(d.package_id.clone()).or_default() += 1;
        }
        for t in &m.rules.load_after {
            *counts.entry(t.clone()).or_default() += 0;
        }
    }
    counts
}

/// Classify every active mod.
pub fn placements(order: &[&ModInfo], ctx: &Context) -> Vec<Placement> {
    let deps = dependents_count(order);
    let collisions = textures::collisions(order, ctx.files, ctx.major_minor);
    let mut colliding: HashSet<String> = HashSet::new();
    for i in &collisions {
        if let Issue::TextureCollision { uids, .. } = i {
            colliding.extend(uids.iter().cloned());
        }
    }
    order.iter().map(|m| classify(m, ctx, deps.get(&m.package_id).copied().unwrap_or(0), colliding.contains(&m.uid))).collect()
}

/// Compute the HALO order for `current` (uids). Rules are hard, phases soft, pins respected.
pub fn sort(current: &[String], ctx: &Context) -> SortResult {
    let by_uid: HashMap<&str, &ModInfo> = ctx.mods.iter().map(|m| (m.uid.as_str(), m)).collect();
    let order: Vec<&ModInfo> = current.iter().filter_map(|u| by_uid.get(u.as_str()).copied()).collect();
    let placements = placements(&order, ctx);
    let phase_of: HashMap<&str, Phase> = placements.iter().map(|p| (p.uid.as_str(), p.phase)).collect();
    let (g, mut issues) = build_graph(&order, ctx);

    // Priority Kahn: among ready nodes pick the lowest (phase, key).
    let key_of = |i: usize| -> (Phase, String) {
        let m = order[i];
        let phase = phase_of.get(m.uid.as_str()).copied().unwrap_or(Phase::Content);
        let k = if ctx.overrides.alphabetical { m.name.to_lowercase() } else { format!("{i:06}") };
        (phase, k)
    };
    let mut indeg: Vec<usize> = vec![0; g.node_count()];
    for e in g.edge_references() {
        indeg[e.target().index()] += 1;
    }
    let mut heap: BinaryHeap<Reverse<((Phase, String), usize)>> = BinaryHeap::new();
    for n in g.node_indices() {
        if indeg[n.index()] == 0 {
            heap.push(Reverse((key_of(g[n]), n.index())));
        }
    }
    let mut result: Vec<usize> = Vec::with_capacity(order.len());
    while let Some(Reverse((_, ni))) = heap.pop() {
        let n = NodeIndex::new(ni);
        result.push(g[n]);
        for e in g.edges(n) {
            let t = e.target();
            indeg[t.index()] -= 1;
            if indeg[t.index()] == 0 {
                heap.push(Reverse((key_of(g[t]), t.index())));
            }
        }
    }
    if result.len() < order.len() {
        // Should not happen after cycle cutting; keep anything unreachable in original order.
        let seen: HashSet<usize> = result.iter().copied().collect();
        result.extend((0..order.len()).filter(|i| !seen.contains(i)));
    }
    // Pins keep their index.
    let mut final_idx: Vec<usize> = Vec::with_capacity(order.len());
    let pinned: Vec<usize> = (0..order.len()).filter(|i| ctx.overrides.pinned.contains(&order[*i].uid)).collect();
    let mut unpinned = result.into_iter().filter(|i| !pinned.contains(i)).collect::<Vec<_>>().into_iter();
    for pos in 0..order.len() {
        if let Some(p) = pinned.iter().find(|p| **p == pos) {
            final_idx.push(*p);
        } else if let Some(u) = unpinned.next() {
            final_idx.push(u);
        }
    }
    final_idx.extend(unpinned);
    let new_order: Vec<String> = final_idx.iter().map(|i| order[*i].uid.clone()).collect();
    let moves: Vec<(String, usize, usize)> = final_idx.iter().enumerate().filter(|(to, from)| to != *from).map(|(to, from)| (order[*from].uid.clone(), *from, to)).collect();
    let mut after = validate(&new_order, ctx);
    after.retain(|i| !matches!(i, Issue::Cycle { .. }));
    issues.extend(after);
    SortResult { order: new_order, placements, moves, issues }
}

/// Check the given order as it is. Cycles are reported from the rule graph regardless of order.
pub fn validate(current: &[String], ctx: &Context) -> Vec<Issue> {
    let by_uid: HashMap<&str, &ModInfo> = ctx.mods.iter().map(|m| (m.uid.as_str(), m)).collect();
    let order: Vec<&ModInfo> = current.iter().filter_map(|u| by_uid.get(u.as_str()).copied()).collect();
    let index_of: HashMap<&str, usize> = order.iter().enumerate().filter(|(_, m)| !m.package_id.is_empty()).map(|(i, m)| (m.package_id.as_str(), i)).collect();
    let active_ids: HashSet<&str> = index_of.keys().copied().collect();
    let mut issues: Vec<Issue> = Vec::new();

    // Dependencies
    for m in &order {
        for d in &m.rules.dependencies {
            let satisfied = active_ids.contains(d.package_id.as_str()) || d.alternatives.iter().any(|a| active_ids.contains(a.as_str()));
            if satisfied {
                continue;
            }
            let installed = ctx.mods.iter().find(|x| x.invalid.is_none() && (x.package_id == d.package_id || d.alternatives.contains(&x.package_id))).map(|x| x.uid.clone());
            let workshop_url = d
                .workshop_url
                .clone()
                .or_else(|| ctx.db.steam.workshop_ids_for(&d.package_id).first().map(|id| format!("https://steamcommunity.com/sharedfiles/filedetails/?id={id}")));
            let display_name = d.display_name.clone().or_else(|| ctx.db.steam.name_for(&d.package_id));
            issues.push(Issue::MissingDependency { uid: m.uid.clone(), dependency: d.package_id.clone(), display_name, installed_uid: installed, workshop_url });
        }
    }
    // Rules against the current order
    let mut seen_pairs: HashSet<(String, String)> = HashSet::new();
    for r in ctx.rules {
        let (Some(&si), Some(&ti)) = (index_of.get(r.subject.as_str()), r.target.as_deref().and_then(|t| index_of.get(t))) else { continue };
        let (s, t) = (order[si], order[ti]);
        match r.kind {
            RuleKind::LoadAfter if si < ti => issues.push(Issue::OrderViolation { uid: s.uid.clone(), target_uid: t.uid.clone(), rule: r.kind, source: r.source, comment: r.comment.clone() }),
            RuleKind::LoadBefore if si > ti => issues.push(Issue::OrderViolation { uid: s.uid.clone(), target_uid: t.uid.clone(), rule: r.kind, source: r.source, comment: r.comment.clone() }),
            RuleKind::Incompatible => {
                let key = if s.uid < t.uid { (s.uid.clone(), t.uid.clone()) } else { (t.uid.clone(), s.uid.clone()) };
                if seen_pairs.insert(key) {
                    issues.push(Issue::Incompatible { uid: s.uid.clone(), other_uid: t.uid.clone(), source: r.source });
                }
            }
            _ => {}
        }
    }
    // Versions
    for m in &order {
        if m.source != Source::Ludeon && !m.supports(ctx.major_minor) && !ctx.db.no_version_warning.contains(&m.package_id) {
            issues.push(Issue::VersionMismatch { uid: m.uid.clone(), supported: m.supported_versions.clone() });
        }
        if m.package_id.is_empty() {
            issues.push(Issue::MissingPackageId { uid: m.uid.clone() });
        }
        if let Some(reason) = &m.invalid {
            issues.push(Issue::Invalid { uid: m.uid.clone(), reason: reason.clone() });
        }
    }
    // Duplicates among installed copies of an active id
    let mut by_pkg: HashMap<&str, Vec<&ModInfo>> = HashMap::new();
    for m in ctx.mods.iter().filter(|m| m.invalid.is_none() && !m.package_id.is_empty()) {
        by_pkg.entry(m.package_id.as_str()).or_default().push(m);
    }
    for (pkg, copies) in by_pkg {
        if copies.len() > 1 && active_ids.contains(pkg) {
            issues.push(Issue::DuplicatePackageId { package_id: pkg.to_string(), uids: copies.iter().map(|m| m.uid.clone()).collect() });
        }
    }
    // Optimization mods should be last
    let placements = placements(&order, ctx);
    let phase_of: HashMap<&str, Phase> = placements.iter().map(|p| (p.uid.as_str(), p.phase)).collect();
    for (i, m) in order.iter().enumerate() {
        if phase_of.get(m.uid.as_str()) == Some(&Phase::Optimization) {
            let after: Vec<String> = order[i + 1..].iter().filter(|x| phase_of.get(x.uid.as_str()) != Some(&Phase::Optimization)).map(|x| x.uid.clone()).collect();
            if !after.is_empty() {
                issues.push(Issue::MisplacedOptimization { uid: m.uid.clone(), after_uids: after });
            }
        }
    }
    // Cycles
    let (_, cycle_issues) = build_graph(&order, ctx);
    issues.extend(cycle_issues);
    // Textures
    issues.extend(textures::collisions(&order, ctx.files, ctx.major_minor));
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{compile_rules, parse_rules};

    fn m(uid: &str, id: &str, name: &str, source: Source) -> ModInfo {
        ModInfo { uid: uid.into(), package_id: id.into(), name: name.into(), source, supported_versions: vec!["1.6".into()], ..Default::default() }
    }

    fn fixture() -> Vec<ModInfo> {
        let mut core = m("core", "ludeon.rimworld", "RimWorld", Source::Ludeon);
        core.contents.defs = 100;
        let mut harmony = m("harmony", "brrainz.harmony", "Harmony", Source::Workshop);
        harmony.contents.assemblies = 1;
        let mut hugs = m("hugs", "unlimitedhugs.hugslib", "HugsLib", Source::Workshop);
        hugs.contents.assemblies = 1;
        hugs.rules.load_after = vec!["brrainz.harmony".into()];
        let mut bpc = m("bpc", "voult.betterpawncontrol", "Better Pawn Control", Source::Workshop);
        bpc.contents.assemblies = 1;
        bpc.rules.dependencies = vec![Dependency { package_id: "brrainz.harmony".into(), ..Default::default() }];
        let mut rocket = m("rocket", "krkr.rocketman", "RocketMan", Source::Workshop);
        rocket.contents.assemblies = 1;
        let mut walls = m("walls", "nyx.retrowalls", "Retro Wall Textures", Source::Workshop);
        walls.contents.textures = 5;
        walls.kind = ModKind::Textures;
        let mut patch = m("patch", "x.patch", "Some Compat Patch", Source::Local);
        patch.contents.patches = 2;
        vec![core, harmony, hugs, bpc, rocket, walls, patch]
    }

    #[test]
    fn halo_orders_by_rules_then_phase() {
        let mods = fixture();
        let db = Databases { community: parse_rules(r#"{"rules": {"voult.betterpawncontrol": {"loadAfter": {"unlimitedhugs.hugslib": {}}}}}"#, RuleSource::Community).unwrap(), ..Default::default() };
        let rules = compile_rules(&mods, &db);
        let files = HashMap::new();
        let ov = UserOverrides::default();
        let ctx = Context { mods: &mods, files: &files, rules: &rules, db: &db, major_minor: "1.6", overrides: &ov };
        // A deliberately scrambled current order
        let current: Vec<String> = ["rocket", "walls", "bpc", "hugs", "harmony", "patch", "core"].iter().map(|s| s.to_string()).collect();
        let r = sort(&current, &ctx);
        assert_eq!(r.order, vec!["core", "harmony", "hugs", "bpc", "patch", "walls", "rocket"]);
        assert!(r.issues.iter().all(|i| !matches!(i, Issue::OrderViolation { .. } | Issue::MisplacedOptimization { .. })));
        let before = validate(&current, &ctx);
        assert!(before.iter().any(|i| matches!(i, Issue::OrderViolation { .. })));
        assert!(before.iter().any(|i| matches!(i, Issue::MisplacedOptimization { .. })));
    }

    #[test]
    fn contradiction_resolved_by_precedence_and_cycles_explained() {
        let mods = fixture();
        // Community says bpc before hugs; the user says bpc after hugs → user wins, no cycle.
        let db = Databases {
            community: parse_rules(r#"{"rules": {"voult.betterpawncontrol": {"loadBefore": {"unlimitedhugs.hugslib": {}}}}}"#, RuleSource::Community).unwrap(),
            user: parse_rules(r#"{"rules": {"voult.betterpawncontrol": {"loadAfter": {"unlimitedhugs.hugslib": {}}}}}"#, RuleSource::User).unwrap(),
            ..Default::default()
        };
        let rules = compile_rules(&mods, &db);
        let files = HashMap::new();
        let ov = UserOverrides::default();
        let ctx = Context { mods: &mods, files: &files, rules: &rules, db: &db, major_minor: "1.6", overrides: &ov };
        let current: Vec<String> = ["core", "harmony", "hugs", "bpc"].iter().map(|s| s.to_string()).collect();
        let r = sort(&current, &ctx);
        assert!(r.issues.iter().all(|i| !matches!(i, Issue::Cycle { .. })));
        assert_eq!(r.order, vec!["core", "harmony", "hugs", "bpc"]);
        // Two About rules that contradict each other form a real cycle: reported, then cut.
        let mut mods2 = fixture();
        mods2[2].rules.load_after = vec!["brrainz.harmony".into(), "voult.betterpawncontrol".into()];
        mods2[3].rules.load_after = vec!["unlimitedhugs.hugslib".into()];
        let db2 = Databases::default();
        let rules2 = compile_rules(&mods2, &db2);
        let ctx2 = Context { mods: &mods2, files: &files, rules: &rules2, db: &db2, major_minor: "1.6", overrides: &ov };
        let r2 = sort(&current, &ctx2);
        assert!(r2.issues.iter().any(|i| matches!(i, Issue::Cycle { chain, .. } if chain.contains("HugsLib"))));
        assert_eq!(r2.order.len(), 4);
    }

    #[test]
    fn pins_hold_position_and_phases_classify() {
        let mods = fixture();
        let db = Databases::default();
        let rules = compile_rules(&mods, &db);
        let files = HashMap::new();
        let mut ov = UserOverrides::default();
        ov.pinned.insert("walls".into());
        let ctx = Context { mods: &mods, files: &files, rules: &rules, db: &db, major_minor: "1.6", overrides: &ov };
        let current: Vec<String> = ["core", "walls", "rocket", "harmony", "hugs", "bpc", "patch"].iter().map(|s| s.to_string()).collect();
        let r = sort(&current, &ctx);
        assert_eq!(r.order[1], "walls");
        let phase = |u: &str| r.placements.iter().find(|p| p.uid == u).unwrap().phase;
        assert_eq!(phase("core"), Phase::Core);
        assert_eq!(phase("harmony"), Phase::Prepatch);
        assert_eq!(phase("hugs"), Phase::Framework);
        assert_eq!(phase("rocket"), Phase::Optimization);
        assert_eq!(phase("walls"), Phase::Texture);
        assert_eq!(phase("patch"), Phase::Patch);
        assert_eq!(phase("bpc"), Phase::Content);
    }
}
