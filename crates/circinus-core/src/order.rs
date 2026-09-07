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

/// The pre-patchers, in the order their own pages ask for, and the order they go in.
///
/// Harmony first: it is the patching library every one of the others is built on, and "load
/// Harmony first" has been the instruction on its page for as long as it has had one. Prepatcher
/// next, because it rewrites the game's assemblies before anything reads them and asks to sit as
/// high as it can. Then the mods built on those two.
///
/// This is a tiebreak, not a claim to know better. An author who declares an order in About.xml
/// is the authority on their own mod and those rules are already edges by the time this is read;
/// all this decides is two pre-patchers that say nothing about each other, where the alternative
/// is whatever order the player's list happened to be in.
const PREPATCH_ORDER: &[&str] = &["brrainz.harmony", "zetrith.prepatcher", "jikulopo.prepatcher", "bs.fishery", "brrainz.visualexceptions"];
/// The same mods as a set, for the classifier. One list, so the two cannot drift apart.
const PREPATCH_IDS: &[&str] = PREPATCH_ORDER;
/// Loading-screen and loader mods that belong above Core and ship no Defs; they do not always
/// say so in their About.xml.
const TOP_IDS: &[&str] = &["me.samboycoding.betterloading", "ilyvion.loadingprogress", "taranchuk.fastergameloading", "pirateby.harmony.optimizer", "automatic.startupimpact"];
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
/// The most Def files a mod may ship and still count as a library on the strength of its
/// dependents. HugsLib and Humanoid Alien Races ship a handful; a content mod ships hundreds.
const LIBRARY_MAX_DEFS: u32 = 40;
const OPTIMIZATION_IDS: &[&str] = &["krkr.rocketman", "bs.performance", "taranchuk.performanceoptimizer", "dubwise.dubsperformanceanalyzer", "telardo.graphicssettings", "notfood.performancefish", "user19990313.runtimegc", "mlie.runtimegc"];

/// Per-user adjustments HALO honours.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserOverrides {
    /// uid → phase the user assigned.
    pub phases: HashMap<String, Phase>,
    /// uid → the section (a group of the user's with its own place in the order) it sorts in.
    /// An entry in `phases` for the same uid wins: an explicit per-mod choice beats the group.
    #[serde(default)]
    pub sections: HashMap<String, String>,
    /// The sections, in the order they follow one another when several sit after one phase.
    #[serde(default)]
    pub section_defs: Vec<SectionDef>,
    /// uids whose position must not change.
    pub pinned: HashSet<String>,
    /// Sort alphabetically within a phase instead of keeping the current arrangement.
    pub alphabetical: bool,
    /// The user's own HALO rules: mods filed by package id or by name, built-in rules switched
    /// off or sent to another phase.
    #[serde(default)]
    pub halo: HaloRules,
}

/// Edits to HALO's classification a user makes on the HALO page. Everything here is a
/// portable statement about mods (package ids, names), not about folders, so it survives a
/// reinstall and can be shared.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HaloRules {
    /// packageId (lowercase) → phase: "file this mod as". A per-mod Sort it as still wins.
    pub package_phases: HashMap<String, Phase>,
    /// Name contains (case-insensitive) → phase, in order; the first match wins.
    pub name_phases: Vec<NamePhase>,
    /// Built-in rules switched off, by key (see `builtin_rules`).
    pub off: HashSet<String>,
    /// Built-in rules sent to another phase than their own, by key.
    pub retarget: HashMap<String, Phase>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NamePhase {
    pub needle: String,
    pub phase: Phase,
}

impl HaloRules {
    fn on(&self, key: &str) -> bool {
        !self.off.contains(key)
    }
    fn target(&self, key: &str, default: Phase) -> Phase {
        self.retarget.get(key).copied().unwrap_or(default)
    }
}

/// One of HALO's built-in classification rules, for the HALO page: what it looks at, where
/// it files a mod, and the package ids it knows by heart, in the order the rules are tried.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltinRule {
    pub key: String,
    /// What the rule looks at, one line.
    pub signal: String,
    /// The longer story: why this belongs where it goes.
    pub detail: String,
    pub phase: Phase,
    /// Package ids the rule knows outright, if it works from a list.
    pub ids: Vec<String>,
    /// Whether the user may switch it off or send it elsewhere (the game's own content is not up for debate).
    pub editable: bool,
}

/// The classification rules in the order `classify` tries them.
pub fn builtin_rules() -> Vec<BuiltinRule> {
    let r = |key: &str, signal: &str, detail: &str, phase: Phase, ids: &[&str], editable: bool| BuiltinRule { key: key.into(), signal: signal.into(), detail: detail.into(), phase, ids: ids.iter().map(|s| s.to_string()).collect(), editable };
    vec![
        r("official", "The game and its DLC", "Core, then the DLCs in release order. Everything that ships Defs must load after them, because a def can only inherit from mods above it; this cannot be switched off.", Phase::Core, &[], false),
        r("prepatch-ids", "Known pre-patchers", "Harmony, Prepatcher, Fishery, Visual Exceptions: they change the game before other mods load and ship no Defs of their own.", Phase::Prepatch, PREPATCH_IDS, true),
        r("top", "Asks to load before the game and has no Defs", "About.xml says it loads before Core (or it is a known loading-screen mod), and with no Defs there is nothing to lose its parents up there.", Phase::Prepatch, TOP_IDS, true),
        r("rule-top", "A rule says: load near the top", "A community or user rule marks it loadTop.", Phase::Framework, &[], true),
        r("optimizer", "Known performance mod, or code without Defs named like one", "RocketMan, Performance Fish and friends, or a code-only mod whose name says performance, optimiser or FPS: it has to see every other mod, so it loads last.", Phase::Optimization, OPTIMIZATION_IDS, true),
        r("rule-bottom", "A rule says: load near the bottom", "A community or user rule marks it loadBottom. Its add-ons follow it.", Phase::Late, &[], true),
        r("framework-ids", "Known frameworks", "Libraries many mods build on: HugsLib, Vanilla Expanded Framework, Vehicle Framework, XML Extensions, Combat Extended…", Phase::Framework, FRAMEWORK_IDS, true),
        r("dependents", "Code that three or more active mods need, with few Defs, not built on a framework", "Being depended on is not enough (VFE Empire has add-ons and is content); a library is mostly code, ships at most a few dozen Def files, and is not itself built on a known framework.", Phase::Framework, &[], true),
        r("name-library", "Named framework, library, lib or api, and has code", "The name says library and there is a DLL; a patch is not one however it is named.", Phase::Framework, &[], true),
        r("texture-pack", "Only textures, or a texture mod that replaces what another replaces", "No code, no Defs, just Textures; or named retexture. Later packs win, so they sort together where the order between them is visible.", Phase::Texture, &[], true),
        r("patch-only", "Only patches", "Patches and nothing else: it loads after what it changes.", Phase::Patch, &[], true),
        r("name-patch", "Named patch or compat", "Named like a patch and either has no code or joins two or more dependencies.", Phase::Patch, &[], true),
        r("content", "Everything else", "Things, pawns, biomes, rules: the ordinary content mod.", Phase::Content, &[], false),
    ]
}

/// A group the user gave its own place in the load order: its members sort together, right
/// after the ordinary members of `after`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SectionDef {
    pub id: String,
    pub name: String,
    pub after: Phase,
}

impl UserOverrides {
    /// Sort rank within a phase: 0 for its ordinary members, then the sections after it in
    /// the order the user keeps them.
    pub fn section_rank(&self, phase: Phase, section: Option<&str>) -> u16 {
        let Some(id) = section else { return 0 };
        self.section_defs.iter().filter(|d| d.after == phase).position(|d| d.id == id).map(|k| k as u16 + 1).unwrap_or(0)
    }
}

fn place(uid: &str, phase: Phase, reason: impl Into<String>) -> Placement {
    Placement { uid: uid.to_string(), phase, reason: reason.into(), section: None }
}

pub struct Context<'a> {
    pub mods: &'a [ModInfo],
    pub files: &'a HashMap<String, ModFiles>,
    pub rules: &'a [Rule],
    pub db: &'a Databases,
    pub major_minor: &'a str,
    pub overrides: &'a UserOverrides,
}

/// A packageId without the postfix RimWorld gives copies of the same mod from another source
/// (`brrainz.harmony_steam` and `brrainz.harmony` are the same mod, and RimWorld's own
/// dependency check ignores the postfix).
pub fn id_base(id: &str) -> &str {
    for suffix in ["_steam", "_copy", "_local"] {
        if let Some(base) = id.strip_suffix(suffix) {
            if !base.is_empty() {
                return base;
            }
        }
    }
    id
}

/// Is this dependency met by the active list? By package id (postfix ignored, alternatives
/// included) or by the Workshop item it points at.
fn dependency_met(d: &Dependency, active_bases: &HashSet<&str>, active_workshop: &HashSet<u64>) -> bool {
    if active_bases.contains(id_base(&d.package_id)) || d.alternatives.iter().any(|a| active_bases.contains(id_base(a))) {
        return true;
    }
    matches!(d.workshop_id(), Some(id) if active_workshop.contains(&id))
}

fn name_matches(name: &str, needles: &[&str]) -> bool {
    let n = name.to_ascii_lowercase();
    needles.iter().any(|x| n.contains(x))
}

/// Decide a phase for one active mod. `top` says the mod may sit above official content.
pub fn classify(m: &ModInfo, ctx: &Context, dependents: usize, texture_collides: bool, top: bool) -> Placement {
    let uid = m.uid.as_str();
    if let Some(p) = ctx.overrides.phases.get(&m.uid) {
        return place(uid, *p, "Set by you");
    }
    if let Some(def) = ctx.overrides.sections.get(&m.uid).and_then(|id| ctx.overrides.section_defs.iter().find(|d| d.id == *id)) {
        return Placement { uid: uid.to_string(), phase: def.after, reason: format!("In your group {}, which goes after {}", def.name, def.after.label().to_lowercase()), section: Some(def.id.clone()) };
    }
    let id = m.package_id.as_str();
    let c = &m.contents;
    if is_official(m) {
        return place(uid, Phase::Core, "The game itself");
    }
    // The user's own HALO rules come next: a mod filed by package id, then by name.
    let h = &ctx.overrides.halo;
    if let Some(p) = h.package_phases.get(id) {
        return place(uid, *p, "Your HALO rule for this package id");
    }
    let lower = m.name.to_ascii_lowercase();
    if let Some(np) = h.name_phases.iter().find(|np| !np.needle.trim().is_empty() && lower.contains(&np.needle.trim().to_ascii_lowercase())) {
        return place(uid, np.phase, format!("Your HALO rule: name contains \"{}\"", np.needle.trim()));
    }
    if h.on("prepatch-ids") && PREPATCH_IDS.contains(&id) {
        return place(uid, h.target("prepatch-ids", Phase::Prepatch), "Changes the game before other mods load");
    }
    if h.on("top") && top {
        return place(uid, h.target("top", Phase::Prepatch), "Asks to load before the game and has no Defs, so that is safe");
    }
    if h.on("rule-top") && ctx.rules.iter().any(|r| r.kind == RuleKind::LoadTop && r.subject == id) {
        return place(uid, h.target("rule-top", Phase::Framework), "A rule says: load near the top");
    }
    let optimizer = OPTIMIZATION_IDS.contains(&id) || (c.assemblies > 0 && c.defs == 0 && name_matches(&m.name, &["performance", "optimiz", "optimis", "rocketman", "fps boost"]));
    if h.on("optimizer") && optimizer {
        return place(uid, h.target("optimizer", Phase::Optimization), "Speeds up other mods, so it has to load after them");
    }
    if h.on("rule-bottom") && ctx.rules.iter().any(|r| r.kind == RuleKind::LoadBottom && r.subject == id) {
        return place(uid, h.target("rule-bottom", Phase::Late), "A rule says: load near the bottom");
    }
    if h.on("framework-ids") && FRAMEWORK_IDS.contains(&id) {
        return place(uid, h.target("framework-ids", Phase::Framework), "A library many mods use");
    }
    // Being depended on is not enough: content mods collect dependents too (VFE Empire has its
    // add-ons, Dubs Bad Hygiene its extensions). A library is mostly code — few Defs of its
    // own — and is not itself built on a framework.
    let built_on_framework = m.rules.dependencies.iter().any(|d| FRAMEWORK_IDS.contains(&d.package_id.as_str()));
    if h.on("dependents") && c.assemblies > 0 && dependents >= 3 && c.defs <= LIBRARY_MAX_DEFS && !built_on_framework {
        return place(uid, h.target("dependents", Phase::Framework), format!("{dependents} active mods need it, and it is mostly code"));
    }
    if h.on("name-library") && c.assemblies > 0 && name_matches(&m.name, &["framework", "library", " lib", "api"]) && !name_matches(&m.name, &["patch"]) {
        return place(uid, h.target("name-library", Phase::Framework), "Named like a library and has code");
    }
    if h.on("texture-pack") && (m.kind == ModKind::Textures || (c.textures + c.dds > 0 && c.assemblies == 0 && c.defs == 0 && (texture_collides || name_matches(&m.name, &["retexture", "texture", "textures"])))) {
        return place(uid, h.target("texture-pack", Phase::Texture), if texture_collides { "Replaces textures that other mods also replace" } else { "Only textures" });
    }
    if h.on("patch-only") && c.patches > 0 && c.defs == 0 && c.assemblies == 0 {
        return place(uid, h.target("patch-only", Phase::Patch), "Only patches, so it loads after what it changes");
    }
    if h.on("name-patch") && name_matches(&m.name, &["patch", "compat"]) && (c.assemblies == 0 || m.rules.dependencies.len() >= 2) {
        return place(uid, h.target("name-patch", Phase::Patch), "A patch that joins two mods");
    }
    place(uid, Phase::Content, "Adds content")
}

struct Edge {
    from: NodeIndex,
    to: NodeIndex,
    rule: Rule,
}

const OFFICIAL_ORDER: &str = "The game and its DLC load in release order";
const AFTER_OFFICIAL: &str = "Loads after the game and its DLC: a def can only inherit from mods above it";
const PREPATCH_ORDER_WHY: &str = "The pre-patchers load in the order their own pages ask for";
const PREPATCH_FIRST: &str = "A pre-patcher changes the game before anything else loads";

fn is_official(m: &ModInfo) -> bool {
    m.source == Source::Ludeon || official_rank(&m.package_id).is_some()
}

/// Position of an official packageId in release order (Core first); None for unknown ids.
fn official_rank(id: &str) -> Option<usize> {
    OFFICIAL.iter().position(|(_, p, _)| *p == id)
}

/// Mods allowed above official content.
///
/// RimWorld's `XmlInheritance` resolves a def's `ParentName` only against mods loaded at or
/// before its own, so anything that ships Defs must sit below Core and every DLC — above them it
/// loses its parents (`BuildingBase`, `MoteBase`…), its category, and the game falls over in
/// `NewFrameDef_Thing`. What may sit above: the known pre-patchers, and mods without Defs that the
/// user filed under Prepatch or that declare (About, community, user) they load before official
/// content or before another such mod — Harmony, Prepatcher, Fishery, loading-screen mods.
/// Rule-derived membership needs an inspected folder; before that only the known ids qualify.
fn top_set(order: &[&ModInfo], ctx: &Context) -> HashSet<String> {
    let mut top: HashSet<&str> = HashSet::new();
    let mut eligible: HashSet<&str> = HashSet::new();
    let mut official_ids: HashSet<&str> = HashSet::new();
    for m in order {
        if m.package_id.is_empty() {
            continue;
        }
        if is_official(m) {
            official_ids.insert(m.package_id.as_str());
            continue;
        }
        if PREPATCH_IDS.contains(&m.package_id.as_str()) {
            top.insert(m.package_id.as_str());
            continue;
        }
        if m.contents.defs == 0 && m.kind != ModKind::Unknown {
            eligible.insert(m.package_id.as_str());
            if ctx.overrides.phases.get(&m.uid) == Some(&Phase::Prepatch) || TOP_IDS.contains(&m.package_id.as_str()) {
                top.insert(m.package_id.as_str());
            }
        }
    }
    // "x before t" comes from LoadBefore(x, t) or LoadAfter(t, x).
    let befores: Vec<(&str, &str)> = ctx
        .rules
        .iter()
        .filter_map(|r| match (r.kind, r.target.as_deref()) {
            (RuleKind::LoadBefore, Some(t)) => Some((r.subject.as_str(), t)),
            (RuleKind::LoadAfter, Some(t)) => Some((t, r.subject.as_str())),
            _ => None,
        })
        .collect();
    loop {
        let mut grew = false;
        for (x, t) in &befores {
            if !top.contains(x) && eligible.contains(x) && (official_ids.contains(t) || top.contains(t)) {
                top.insert(x);
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    order.iter().filter(|m| top.contains(m.package_id.as_str())).map(|m| m.uid.clone()).collect()
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
    // The official-content invariant: Core → DLCs in release order, then everything that is not
    // allowed on top loads after all of them. These outrank every declared rule.
    let top = top_set(order, ctx);
    let mut officials: Vec<usize> = (0..order.len()).filter(|i| is_official(order[*i])).collect();
    officials.sort_by_key(|i| (official_rank(&order[*i].package_id).unwrap_or(usize::MAX), *i));
    let mut invariant: HashSet<(NodeIndex, NodeIndex)> = HashSet::new();
    let halo = |subject: &str, target: &str, comment: &str| Rule { kind: RuleKind::LoadAfter, subject: subject.to_string(), target: Some(target.to_string()), source: RuleSource::Halo, comment: Some(comment.to_string()) };
    for w in officials.windows(2) {
        let (a, b) = (order[w[0]], order[w[1]]);
        let (from, to) = (idx_of[a.uid.as_str()], idx_of[b.uid.as_str()]);
        invariant.insert((from, to));
        cands.push(Edge { from, to, rule: halo(&b.package_id, &a.package_id, OFFICIAL_ORDER) });
    }
    for m in order {
        if is_official(m) || top.contains(&m.uid) {
            continue;
        }
        let to = idx_of[m.uid.as_str()];
        for o in &officials {
            let from = idx_of[order[*o].uid.as_str()];
            invariant.insert((from, to));
            cands.push(Edge { from, to, rule: halo(&m.package_id, &order[*o].package_id, AFTER_OFFICIAL) });
        }
    }
    // The pre-patch head. A pre-patcher changes the game before other mods load — that is what
    // the word means — so it goes above everything else allowed up there, and the known ones go
    // in the order their own pages ask for. Two of them that say nothing about each other used to
    // be left in whatever order the player's list happened to have them in, which for Harmony and
    // Prepatcher is not a coin worth tossing.
    let mut heads: Vec<usize> = (0..order.len()).filter(|i| PREPATCH_ORDER.contains(&order[*i].package_id.as_str())).collect();
    heads.sort_by_key(|i| (PREPATCH_ORDER.iter().position(|p| *p == order[*i].package_id).unwrap_or(usize::MAX), *i));
    for w in heads.windows(2) {
        let (a, b) = (order[w[0]], order[w[1]]);
        cands.push(Edge { from: idx_of[a.uid.as_str()], to: idx_of[b.uid.as_str()], rule: halo(&b.package_id, &a.package_id, PREPATCH_ORDER_WHY) });
    }
    for m in order.iter().filter(|m| top.contains(&m.uid) && !PREPATCH_ORDER.contains(&m.package_id.as_str())) {
        let to = idx_of[m.uid.as_str()];
        for h in &heads {
            cands.push(Edge { from: idx_of[order[*h].uid.as_str()], to, rule: halo(&m.package_id, &order[*h].package_id, PREPATCH_FIRST) });
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
    let mut issues = Vec::new();
    for (a, b) in keys {
        if a < b {
            if let (Some(x), Some(y)) = (best.get(&(a, b)), best.get(&(b, a))) {
                let (winner, loser) = if x.rule.source > y.rule.source {
                    ((a, b), (b, a))
                } else if y.rule.source > x.rule.source {
                    ((b, a), (a, b))
                } else {
                    continue;
                };
                let lost = best.remove(&loser).map(|e| e.rule).expect("edge present");
                if invariant.contains(&winner) {
                    // A declared rule wanted a Defs-bearing mod above Core: obeying it would break
                    // the game, so say why it was set aside instead of silently ignoring it.
                    // Edges point from the earlier mod to the later one; map back to the rule's subject/target.
                    let earlier = order[g[loser.0]].uid.clone();
                    let later = order[g[loser.1]].uid.clone();
                    let (uid, target_uid) = if lost.kind == RuleKind::LoadBefore { (earlier, later) } else { (later, earlier) };
                    issues.push(Issue::RuleIgnored { uid, target_uid, rule: lost.kind, source: lost.source, reason: "it has Defs, and their parents cannot be found above the game's own content".into() });
                }
            }
        }
    }
    for e in best.into_values() {
        g.add_edge(e.from, e.to, e.rule);
    }
    // Cut real cycles. One at a time, and each one reported as the path it actually is.
    //
    // This used to take the strongly connected component -- the set of mods that can all reach
    // one another -- and print its members joined by arrows, as though the set were a path. It is
    // not. Eight mods tangled together were shown as an eight-step loop in whatever order Tarjan
    // happened to return them, so the chain named steps that no rule had ever asked for, and the
    // one thing a player wants to know from a loop report ("which rule do I change") was the one
    // thing it could not tell them. Finding a real cycle inside the component costs a depth-first
    // walk and makes every step in the message true.
    //
    // The cut is one edge, not every edge of the lowest precedence, and the issue says which one
    // and where it came from. "The weakest rule was set aside" described neither.
    let mut guard = 0;
    loop {
        guard += 1;
        let sccs: Vec<Vec<NodeIndex>> = tarjan_scc(&g).into_iter().filter(|c| c.len() > 1).collect();
        if sccs.is_empty() || guard > 500 {
            break;
        }
        let mut cut_any = false;
        for comp in sccs {
            let set: HashSet<NodeIndex> = comp.iter().copied().collect();
            let Some(path) = find_cycle(&g, &set) else { continue };
            let rules: Vec<Rule> = path.iter().map(|e| g[*e].clone()).collect();
            let nodes: Vec<NodeIndex> = path.iter().filter_map(|e| g.edge_endpoints(*e).map(|(a, _)| a)).collect();
            let uids: Vec<String> = nodes.iter().map(|n| order[g[*n]].uid.clone()).collect();
            let names: Vec<String> = nodes.iter().map(|n| order[g[*n]].name.clone()).collect();
            let chain = match names.first() {
                Some(first) => format!("{} → {first}", names.join(" → ")),
                None => String::new(),
            };
            // The edge to drop: the weakest rule on this cycle. Ties go to the last one in the
            // walk rather than to whichever the graph happened to hand back first, so the same
            // list cut the same way twice.
            let weakest = rules.iter().map(|r| r.source).min().unwrap_or(RuleSource::About);
            let victim = path.iter().rposition(|e| g[*e].source == weakest).unwrap_or(0);
            let cut = rules.get(victim).cloned();
            issues.push(Issue::Cycle { uids, chain, rules, cut });
            g.remove_edge(path[victim]);
            cut_any = true;
        }
        // A component with no findable cycle cannot be helped by cutting; stop rather than spin.
        if !cut_any {
            break;
        }
    }
    (g, issues)
}

/// One real cycle inside a strongly connected component, as the edges along it.
///
/// A depth-first walk that stops the moment it reaches a node already on the path: everything
/// from that node onwards is a cycle, and every step of it is an edge that exists. Restricted to
/// `set`, because a walk that wanders out of the component may never come back.
fn find_cycle(g: &DiGraph<usize, Rule>, set: &HashSet<NodeIndex>) -> Option<Vec<petgraph::graph::EdgeIndex>> {
    let start = *set.iter().min_by_key(|n| n.index())?;
    // (node, edges taken to get here). The stack holds the path itself so the cycle can be read
    // straight off it rather than reconstructed from parents.
    let mut stack: Vec<(NodeIndex, Vec<petgraph::graph::EdgeIndex>)> = vec![(start, Vec::new())];
    let mut seen: HashSet<NodeIndex> = HashSet::new();
    while let Some((node, path)) = stack.pop() {
        if !seen.insert(node) {
            continue;
        }
        // Neighbours in a fixed order: two runs on the same list must report the same loop.
        let mut out: Vec<petgraph::graph::EdgeIndex> = g.edges(node).filter(|e| set.contains(&e.target())).map(|e| e.id()).collect();
        out.sort_by_key(|e| g.edge_endpoints(*e).map(|(_, b)| b.index()).unwrap_or(0));
        for e in out {
            let Some((_, next)) = g.edge_endpoints(e) else { continue };
            let mut here = path.clone();
            here.push(e);
            // Back to somewhere already on this path: the cycle is the tail from there.
            if let Some(at) = here.iter().position(|x| g.edge_endpoints(*x).map(|(a, _)| a) == Some(next)) {
                return Some(here[at..].to_vec());
            }
            stack.push((next, here));
        }
    }
    None
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

/// Classify every active mod, then lift phases along the hard edges: a mod that must load after
/// a Late or Performance mod belongs to that group too (RimJobWorld's add-ons follow RimJobWorld
/// to the bottom; a RocketMan patch follows RocketMan), so the groups stay contiguous and the
/// sort never has to interleave them.
pub fn placements(order: &[&ModInfo], ctx: &Context) -> Vec<Placement> {
    let (p, _, _) = placements_and_graph(order, ctx);
    p
}

fn placements_and_graph(order: &[&ModInfo], ctx: &Context) -> (Vec<Placement>, DiGraph<usize, Rule>, Vec<Issue>) {
    let deps = dependents_count(order);
    let collisions = textures::collisions(order, ctx.files, ctx.major_minor);
    let mut colliding: HashSet<String> = HashSet::new();
    for i in &collisions {
        if let Issue::TextureCollision { uids, .. } = i {
            colliding.extend(uids.iter().cloned());
        }
    }
    let top = top_set(order, ctx);
    let mut placements: Vec<Placement> = order.iter().map(|m| classify(m, ctx, deps.get(&m.package_id).copied().unwrap_or(0), colliding.contains(&m.uid), top.contains(&m.uid))).collect();
    let (g, issues) = build_graph(order, ctx);
    // Walk in topological order so each lift is final. Only the late phases pull mods down: a
    // content mod that follows a library or a patch stays where it is.
    if let Ok(topo) = petgraph::algo::toposort(&g, None) {
        for n in topo {
            let i = g[n];
            let from = placements[i].phase;
            if from < Phase::Late {
                continue;
            }
            for e in g.edges(n) {
                let j = g[e.target()];
                let chosen = ctx.overrides.phases.contains_key(&order[j].uid) || ctx.overrides.sections.contains_key(&order[j].uid);
                if placements[j].phase < from && !chosen && !is_official(order[j]) {
                    placements[j].phase = from;
                    placements[j].reason = format!("Must load after {}, so it goes with the {} group", order[i].name, from.label().to_lowercase());
                }
            }
        }
    }
    (placements, g, issues)
}

/// Compute the HALO order for `current` (uids). Rules are hard, phases soft, pins respected.
pub fn sort(current: &[String], ctx: &Context) -> SortResult {
    let by_uid: HashMap<&str, &ModInfo> = ctx.mods.iter().map(|m| (m.uid.as_str(), m)).collect();
    let order: Vec<&ModInfo> = current.iter().filter_map(|u| by_uid.get(u.as_str()).copied()).collect();
    let (placements, g, mut issues) = placements_and_graph(&order, ctx);
    let phase_of: HashMap<&str, (Phase, u16)> = placements.iter().map(|p| (p.uid.as_str(), (p.phase, ctx.overrides.section_rank(p.phase, p.section.as_deref())))).collect();

    // Priority Kahn: among ready nodes pick the lowest (phase, section, key).
    let key_of = |i: usize| -> (Phase, u16, String) {
        let m = order[i];
        let (phase, rank) = phase_of.get(m.uid.as_str()).copied().unwrap_or((Phase::Content, 0));
        let k = if ctx.overrides.alphabetical { m.name.to_lowercase() } else { format!("{i:06}") };
        (phase, rank, k)
    };
    let mut indeg: Vec<usize> = vec![0; g.node_count()];
    for e in g.edge_references() {
        indeg[e.target().index()] += 1;
    }
    let mut heap: BinaryHeap<Reverse<((Phase, u16, String), usize)>> = BinaryHeap::new();
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

    // Dependencies. Matched the way RimWorld matches them: the postfix Steam copies carry
    // (`_steam`) is ignored, and a dependency that names a Workshop item is satisfied by that
    // item whatever its About.xml calls itself.
    let active_bases: HashSet<&str> = active_ids.iter().map(|id| id_base(id)).collect();
    let active_workshop: HashSet<u64> = order.iter().filter_map(|m| m.published_file_id).collect();
    for m in &order {
        for d in &m.rules.dependencies {
            if dependency_met(d, &active_bases, &active_workshop) {
                continue;
            }
            // Installed but not active — or installed and unreadable, which is not the same as
            // absent and must not be reported as "not installed".
            let installed = ctx
                .mods
                .iter()
                .find(|x| {
                    (!x.package_id.is_empty() && (id_base(&x.package_id) == id_base(&d.package_id) || d.alternatives.iter().any(|a| id_base(a) == id_base(&x.package_id))))
                        || (x.published_file_id.is_some() && x.published_file_id == d.workshop_id())
                })
                .map(|x| x.uid.clone());
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
    // Nothing with Defs above official content, and official content in release order.
    let top = top_set(&order, ctx);
    let officials: Vec<usize> = (0..order.len()).filter(|i| is_official(order[*i])).collect();
    if let Some(&last_official) = officials.last() {
        for (i, m) in order.iter().enumerate().take(last_official) {
            // A Defs-less mod above Core is unusual but harmless; only Defs break there.
            if is_official(m) || top.contains(&m.uid) || m.contents.defs == 0 {
                continue;
            }
            let below = officials.iter().find(|o| **o > i).map(|o| order[*o].uid.clone()).expect("an official mod follows");
            issues.push(Issue::AboveOfficial { uid: m.uid.clone(), official_uid: below });
        }
        for w in officials.windows(2) {
            let (a, b) = (order[w[0]], order[w[1]]);
            if let (Some(ra), Some(rb)) = (official_rank(&a.package_id), official_rank(&b.package_id)) {
                if ra > rb {
                    issues.push(Issue::AboveOfficial { uid: a.uid.clone(), official_uid: b.uid.clone() });
                }
            }
        }
    }
    // Performance mods should be last, except for whatever a rule makes load after them.
    let (placements, g, cycle_issues) = placements_and_graph(&order, ctx);
    let phase_of: HashMap<&str, Phase> = placements.iter().map(|p| (p.uid.as_str(), p.phase)).collect();
    for (i, m) in order.iter().enumerate() {
        if phase_of.get(m.uid.as_str()) != Some(&Phase::Optimization) {
            continue;
        }
        let mut forced: HashSet<usize> = HashSet::new();
        let mut dfs = petgraph::visit::Dfs::new(&g, NodeIndex::new(i));
        while let Some(n) = dfs.next(&g) {
            forced.insert(g[n]);
        }
        let after: Vec<String> = order[i + 1..].iter().enumerate().filter(|(k, x)| phase_of.get(x.uid.as_str()) != Some(&Phase::Optimization) && !forced.contains(&(i + 1 + k))).map(|(_, x)| x.uid.clone()).collect();
        if !after.is_empty() {
            issues.push(Issue::MisplacedOptimization { uid: m.uid.clone(), after_uids: after });
        }
    }
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

    /// Harmony is its own mod now: nothing bundles it, so nearly every C# mod names it in
    /// modDependencies. It must be recognised however the copy in the folder spells itself —
    /// with RimWorld's `_steam` postfix, or under a packageId that does not match at all when
    /// the dependency points at the Workshop item by id.
    #[test]
    fn harmony_dependency_is_met_by_the_installed_copy() {
        let db = Databases::default();
        let files = HashMap::new();
        let ov = UserOverrides::default();
        let dep = |url: Option<&str>| Dependency {
            package_id: "brrainz.harmony".into(),
            display_name: Some("Harmony".into()),
            workshop_url: url.map(|u| u.to_string()),
            ..Default::default()
        };
        let missing = |mods: &[ModInfo], active: &[&str]| -> Vec<String> {
            let rules: Vec<Rule> = Vec::new();
            let ctx = Context { mods, files: &files, rules: &rules, db: &db, major_minor: "1.6", overrides: &ov };
            let order: Vec<String> = active.iter().map(|s| s.to_string()).collect();
            validate(&order, &ctx)
                .into_iter()
                .filter_map(|i| match i {
                    Issue::MissingDependency { uid, installed_uid, .. } => Some(format!("{uid}:{}", installed_uid.unwrap_or_else(|| "-".into()))),
                    _ => None,
                })
                .collect()
        };
        let core = m("core", "ludeon.rimworld", "RimWorld", Source::Ludeon);
        let mut user = m("lp", "ilyvion.loadingprogress", "Loading Progress", Source::Workshop);
        user.rules.dependencies = vec![dep(Some("steam://url/CommunityFilePage/2009463077"))];

        // the plain case
        let plain = vec![core.clone(), m("h", "brrainz.harmony", "Harmony", Source::Workshop), user.clone()];
        assert!(missing(&plain, &["h", "core", "lp"]).is_empty());

        // the Steam copy spells itself with the postfix RimWorld adds
        let postfixed = vec![core.clone(), m("h", "brrainz.harmony_steam", "Harmony", Source::Workshop), user.clone()];
        assert!(missing(&postfixed, &["h", "core", "lp"]).is_empty(), "the _steam postfix is not part of the identity");

        // …and the other way round: the dependency carries the postfix, the folder does not
        let mut asks_postfixed = user.clone();
        asks_postfixed.rules.dependencies = vec![Dependency { package_id: "brrainz.harmony_steam".into(), ..dep(None) }];
        let plain_installed = vec![core.clone(), m("h", "brrainz.harmony", "Harmony", Source::Workshop), asks_postfixed];
        assert!(missing(&plain_installed, &["h", "core", "lp"]).is_empty());

        // a repackaged copy whose About.xml says something else: the Workshop id still names it
        let mut repack = m("h", "someone.harmonyrepack", "Harmony", Source::Workshop);
        repack.published_file_id = Some(2009463077);
        assert!(missing(&[core.clone(), repack, user.clone()], &["h", "core", "lp"]).is_empty());

        // installed but not active: reported, and reported as installed
        let inactive = vec![core.clone(), m("h", "brrainz.harmony", "Harmony", Source::Workshop), user.clone()];
        assert_eq!(missing(&inactive, &["core", "lp"]), vec!["lp:h"]);

        // installed and unreadable is still installed, not absent
        let mut broken = m("h", "brrainz.harmony", "Harmony", Source::Workshop);
        broken.invalid = Some("About.xml could not be read".into());
        assert_eq!(missing(&[core.clone(), broken, user.clone()], &["core", "lp"]), vec!["lp:h"]);

        // genuinely absent
        assert_eq!(missing(&[core, user], &["core", "lp"]), vec!["lp:-"]);
    }

    #[test]
    fn workshop_id_is_read_from_either_url() {
        let d = |w: Option<&str>, dl: Option<&str>| Dependency { workshop_url: w.map(str::to_string), download_url: dl.map(str::to_string), ..Default::default() };
        assert_eq!(d(Some("steam://url/CommunityFilePage/2009463077"), None).workshop_id(), Some(2009463077));
        assert_eq!(d(Some("https://steamcommunity.com/sharedfiles/filedetails/?id=2009463077"), None).workshop_id(), Some(2009463077));
        assert_eq!(d(None, Some("https://steamcommunity.com/workshop/filedetails/?id=2009463077")).workshop_id(), Some(2009463077));
        // a release page is not a Workshop item
        assert_eq!(d(None, Some("https://github.com/pardeike/HarmonyRimWorld/releases/latest")).workshop_id(), None);
        assert_eq!(d(None, None).workshop_id(), None);
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
        assert_eq!(r.order, vec!["harmony", "core", "hugs", "bpc", "patch", "walls", "rocket"]);
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
        assert_eq!(r.order, vec!["harmony", "core", "hugs", "bpc"]);
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

    /// Every step in a reported loop must be a rule that exists.
    ///
    /// It did not used to be. The report was the strongly connected component -- the set of mods
    /// that can all reach one another -- printed in Tarjan's order with arrows between them, so a
    /// tangle of eight mods was shown as an eight-step loop naming steps nobody had written. A
    /// player reading it reasonably concluded the tool was making things up, and could not tell
    /// which rule to change because the message did not contain one.
    #[test]
    fn a_reported_loop_is_a_real_path_and_names_the_rule_it_dropped() {
        // Six mods in a ring, plus two more that hang off it: A -> B -> C -> D -> E -> F -> A,
        // with X and Y reachable from the ring and reaching back into it, so the component is
        // eight mods but the cycle is six.
        let names = ["A", "B", "C", "D", "E", "F", "X", "Y"];
        let mut mods: Vec<ModInfo> = names.iter().map(|n| m(&n.to_lowercase(), &format!("t.{}", n.to_lowercase()), n, Source::Workshop)).collect();
        for i in 0..6 {
            let prev = format!("t.{}", names[(i + 5) % 6].to_lowercase());
            mods[i].rules.load_after = vec![prev];
        }
        // X and Y join the same component without being on that ring.
        mods[6].rules.load_after = vec!["t.a".into()];
        mods[0].rules.load_after.push("t.y".into());
        mods[7].rules.load_after = vec!["t.x".into()];

        let files: HashMap<String, ModFiles> = HashMap::new();
        let db = Databases::default();
        let rules = compile_rules(&mods, &db);
        let ov = UserOverrides::default();
        let ctx = Context { mods: &mods, files: &files, rules: &rules, db: &db, major_minor: "1.6", overrides: &ov };
        let current: Vec<String> = mods.iter().map(|x| x.uid.clone()).collect();
        let r = sort(&current, &ctx);

        let cycles: Vec<&Issue> = r.issues.iter().filter(|i| matches!(i, Issue::Cycle { .. })).collect();
        assert!(!cycles.is_empty(), "the ring is a loop and has to be reported");
        let by_uid: HashMap<&str, &ModInfo> = mods.iter().map(|x| (x.uid.as_str(), x)).collect();

        for issue in &cycles {
            let Issue::Cycle { uids, rules: steps, chain, cut } = issue else { continue };
            assert_eq!(uids.len(), steps.len(), "one rule per step, or the message cannot name any of them");
            assert!(uids.len() >= 2, "a loop of one is not a loop: {chain}");
            // Every step names a rule that one of the two mods actually declared.
            for (i, rule) in steps.iter().enumerate() {
                let earlier = by_uid[uids[i].as_str()];
                let later = by_uid[uids[(i + 1) % uids.len()].as_str()];
                let subject = if rule.subject == later.package_id { later } else { earlier };
                let other = if std::ptr::eq(subject, later) { earlier } else { later };
                assert!(
                    rule.subject == subject.package_id,
                    "step {i} of {chain} names {} but the mods there are {} and {}",
                    rule.subject,
                    earlier.package_id,
                    later.package_id
                );
                assert_eq!(rule.target.as_deref(), Some(other.package_id.as_str()), "step {i} of {chain} points at a mod that is not the next one along");
            }
            // And every name in the chain is one of the mods on the loop, in the same order.
            let shown: Vec<&str> = chain.split(" → ").collect();
            assert_eq!(shown.len(), uids.len() + 1, "the chain has to close: {chain}");
            assert_eq!(shown[0], shown[shown.len() - 1], "and close on the mod it started from: {chain}");
            for (i, u) in uids.iter().enumerate() {
                assert_eq!(shown[i], by_uid[u.as_str()].name, "the chain and the mods disagree at step {i}");
            }
            // The message has to say which rule was dropped; "the weakest rule" named nothing.
            let cut = cut.as_ref().expect("a loop that was cut says what was cut");
            assert!(steps.iter().any(|r| r == cut), "the rule dropped has to be one of the ones on the loop");
        }

        // And the sort still finishes with every mod in it.
        assert_eq!(r.order.len(), mods.len());
    }

    /// The shape of the list that crashed: a loading-screen mod (no Defs) declares it loads before
    /// Core, which held Core back while the priority sort kept placing ready "framework" mods —
    /// Ancot Library, Dubs Bad Hygiene — above it. Above Core they cannot inherit `BuildingBase`,
    /// and RimWorld fails in `NewFrameDef_Thing`. Frameworks must stay below every official mod.
    #[test]
    fn nothing_with_defs_sorts_above_official_content() {
        let mut core = m("core", "ludeon.rimworld", "RimWorld", Source::Ludeon);
        core.contents.defs = 3000;
        core.kind = ModKind::Official;
        let mut royalty = m("royalty", "ludeon.rimworld.royalty", "Royalty", Source::Ludeon);
        royalty.contents.defs = 800;
        royalty.kind = ModKind::Official;
        let mut harmony = m("harmony", "brrainz.harmony", "Harmony", Source::Workshop);
        harmony.contents.assemblies = 1;
        harmony.kind = ModKind::Code;
        harmony.rules.load_before = vec!["ludeon.rimworld".into(), "ludeon.rimworld.royalty".into()];
        let mut loader = m("loader", "me.samboycoding.betterloading", "Better Loading", Source::Workshop);
        loader.contents.assemblies = 1;
        loader.kind = ModKind::Code;
        loader.rules.load_before = vec!["ludeon.rimworld".into()];
        let mut ancot = m("ancot", "ancot.ancotlibrary", "Ancot Library", Source::Workshop);
        ancot.contents.assemblies = 1;
        ancot.contents.defs = 120;
        ancot.kind = ModKind::Code;
        let mut dbh = m("dbh", "dubwise.dubsbadhygiene", "Dubs Bad Hygiene", Source::Workshop);
        dbh.contents.assemblies = 1;
        dbh.contents.defs = 300;
        dbh.kind = ModKind::Code;
        let mut addons = Vec::new();
        for i in 0..3 {
            let mut a = m(&format!("dbh{i}"), &format!("x.dbhaddon{i}"), &format!("DBH Addon {i}"), Source::Workshop);
            a.contents.defs = 5;
            a.kind = ModKind::Xml;
            a.rules.dependencies = vec![Dependency { package_id: "dubwise.dubsbadhygiene".into(), ..Default::default() }];
            addons.push(a);
        }
        // A mod with Defs whose About.xml wrongly asks to sit above Core.
        let mut wrong = m("wrong", "x.wrong", "Wrong Way Up", Source::Workshop);
        wrong.contents.defs = 12;
        wrong.kind = ModKind::Xml;
        wrong.rules.load_before = vec!["ludeon.rimworld".into()];
        let mut mods = vec![core, royalty, harmony, loader, ancot, dbh];
        mods.extend(addons);
        mods.push(wrong);
        let db = Databases::default();
        let rules = compile_rules(&mods, &db);
        let files = HashMap::new();
        let ov = UserOverrides::default();
        let ctx = Context { mods: &mods, files: &files, rules: &rules, db: &db, major_minor: "1.6", overrides: &ov };
        let current: Vec<String> = mods.iter().map(|m| m.uid.clone()).collect();
        let r = sort(&current, &ctx);
        let pos = |u: &str| r.order.iter().position(|x| x == u).unwrap();
        assert!(pos("harmony") < pos("core"), "{:?}", r.order);
        assert!(pos("loader") < pos("core"), "a Defs-less loader may stay above Core: {:?}", r.order);
        assert!(pos("core") < pos("royalty"), "{:?}", r.order);
        for u in ["ancot", "dbh", "dbh0", "dbh1", "dbh2", "wrong"] {
            assert!(pos(u) > pos("royalty"), "{u} must load after all official content: {:?}", r.order);
        }
        assert!(pos("dbh") < pos("dbh0"));
        let phase = |u: &str| r.placements.iter().find(|p| p.uid == u).unwrap().phase;
        assert_eq!(phase("ancot"), Phase::Framework);
        assert_eq!(phase("loader"), Phase::Prepatch, "declares it loads before Core and ships no Defs");
        assert!(r.issues.iter().any(|i| matches!(i, Issue::RuleIgnored { uid, .. } if uid == "wrong")), "{:?}", r.issues);
        assert!(!r.issues.iter().any(|i| matches!(i, Issue::AboveOfficial { .. } | Issue::Cycle { .. })), "{:?}", r.issues);
        // The order that crashed is flagged as an error before Play.
        let crashed: Vec<String> = ["harmony", "ancot", "dbh", "loader", "core", "royalty", "dbh0", "dbh1", "dbh2", "wrong"].iter().map(|s| s.to_string()).collect();
        let issues = validate(&crashed, &ctx);
        let above: Vec<&str> = issues.iter().filter_map(|i| match i { Issue::AboveOfficial { uid, .. } => Some(uid.as_str()), _ => None }).collect();
        assert_eq!(above, vec!["ancot", "dbh"]);
        assert!(issues.iter().any(|i| i.severity() == Severity::Error));
        // A DLC above Core is the same error.
        let dlc_first: Vec<String> = ["harmony", "loader", "royalty", "core", "ancot", "dbh", "dbh0", "dbh1", "dbh2", "wrong"].iter().map(|s| s.to_string()).collect();
        assert!(validate(&dlc_first, &ctx).iter().any(|i| matches!(i, Issue::AboveOfficial { uid, official_uid } if uid == "royalty" && official_uid == "core")));
    }

    /// A big content mod with a `loadBottom` rule and sixty add-ons: the add-ons follow it into
    /// the Late group, all of that sits before the performance mods, and no performance mod is
    /// flagged for having them after it.
    #[test]
    fn late_loaders_take_their_addons_with_them_and_stay_above_performance_mods() {
        let mut core = m("core", "ludeon.rimworld", "RimWorld", Source::Ludeon);
        core.contents.defs = 3000;
        core.kind = ModKind::Official;
        let mut hugs = m("hugs", "unlimitedhugs.hugslib", "HugsLib", Source::Workshop);
        hugs.contents.assemblies = 1;
        hugs.kind = ModKind::Code;
        let mut rjw = m("rjw", "rim.job.world", "RimJobWorld", Source::Local);
        rjw.contents.assemblies = 14;
        rjw.contents.defs = 1147;
        rjw.kind = ModKind::Code;
        rjw.rules.dependencies = vec![Dependency { package_id: "unlimitedhugs.hugslib".into(), ..Default::default() }];
        let mut addons = Vec::new();
        for i in 0..60 {
            let mut a = m(&format!("addon{i}"), &format!("x.rjwaddon{i}"), &format!("RJW Addon {i}"), Source::Workshop);
            a.contents.defs = 20;
            a.kind = ModKind::Xml;
            // Half say loadAfter, half only declare the dependency; both must count.
            if i % 2 == 0 {
                a.rules.load_after = vec!["rim.job.world".into()];
            } else {
                a.rules.dependencies = vec![Dependency { package_id: "rim.job.world".into(), ..Default::default() }];
            }
            addons.push(a);
        }
        let mut retex = m("retex", "x.rjwretex", "RJW ReTexture", Source::Workshop);
        retex.contents.textures = 40;
        retex.kind = ModKind::Textures;
        retex.rules.load_after = vec!["rim.job.world".into()];
        let mut furniture = m("furn", "x.furniture", "More Furniture", Source::Workshop);
        furniture.contents.defs = 50;
        furniture.kind = ModKind::Xml;
        let mut amo = m("amo", "mrk.architectmenuoptimizer", "Architect Menu Optimizer", Source::Workshop);
        amo.contents.assemblies = 1;
        amo.kind = ModKind::Code;
        let mut perfopt = m("perfopt", "taranchuk.performanceoptimizer", "Performance Optimizer", Source::Workshop);
        perfopt.contents.assemblies = 1;
        perfopt.kind = ModKind::Code;
        let mut rocket = m("rocket", "krkr.rocketman", "RocketMan", Source::Workshop);
        rocket.contents.assemblies = 1;
        rocket.kind = ModKind::Code;
        let mut rocketfix = m("rocketfix", "x.rocketfix", "RocketMan Compat", Source::Workshop);
        rocketfix.contents.patches = 3;
        rocketfix.kind = ModKind::Xml;
        rocketfix.rules.load_after = vec!["krkr.rocketman".into()];
        let mut mods = vec![core, hugs, furniture, rjw];
        mods.extend(addons);
        mods.extend([retex, amo, perfopt, rocket, rocketfix]);
        let db = Databases {
            community: parse_rules(r#"{"rules": {
                "rim.job.world": {"loadBottom": {"value": true}, "loadBefore": {"krkr.rocketman": {}}},
                "krkr.rocketman": {"loadBottom": {"value": true}},
                "taranchuk.performanceoptimizer": {"loadBottom": {"value": true}, "loadBefore": {"krkr.rocketman": {}}}
            }}"#, RuleSource::Community).unwrap(),
            ..Default::default()
        };
        let rules = compile_rules(&mods, &db);
        let files = HashMap::new();
        let ov = UserOverrides::default();
        let ctx = Context { mods: &mods, files: &files, rules: &rules, db: &db, major_minor: "1.6", overrides: &ov };
        // The order that raised the complaint: RimJobWorld among the performance mods, add-ons after them.
        let mut current: Vec<String> = vec!["core".into(), "hugs".into(), "furn".into(), "amo".into(), "rjw".into(), "perfopt".into()];
        current.extend((0..60).map(|i| format!("addon{i}")));
        current.extend(["retex".to_string(), "rocket".into(), "rocketfix".into()]);
        let r = sort(&current, &ctx);
        let pos = |u: &str| r.order.iter().position(|x| x == u).unwrap();
        let phase = |u: &str| r.placements.iter().find(|p| p.uid == u).unwrap().phase;
        assert_eq!(phase("rjw"), Phase::Late);
        assert_eq!(phase("addon1"), Phase::Late, "an add-on that only declares the dependency follows it");
        assert_eq!(phase("addon0"), Phase::Late);
        assert_eq!(phase("retex"), Phase::Late, "even a texture pack that must load after it");
        assert_eq!(phase("furn"), Phase::Content);
        assert_eq!(phase("amo"), Phase::Optimization);
        assert_eq!(phase("rocketfix"), Phase::Optimization, "a patch that must load after RocketMan stays with it");
        assert!(pos("furn") < pos("rjw"));
        for i in 0..60 {
            assert!(pos(&format!("addon{i}")) > pos("rjw"));
            assert!(pos(&format!("addon{i}")) < pos("amo"), "add-ons come before the performance mods: {:?}", &r.order[pos("rjw")..]);
        }
        assert!(pos("amo") < pos("perfopt") && pos("perfopt") < pos("rocket") && pos("rocket") < pos("rocketfix"));
        assert!(r.issues.iter().all(|i| !matches!(i, Issue::MisplacedOptimization { .. } | Issue::OrderViolation { .. } | Issue::Cycle { .. })), "{:?}", r.issues);
        // The complained-about order is flagged, but only for the mods no rule forces after.
        let before = validate(&current, &ctx);
        let flagged: Vec<&Issue> = before.iter().filter(|i| matches!(i, Issue::MisplacedOptimization { .. })).collect();
        assert!(flagged.iter().any(|i| matches!(i, Issue::MisplacedOptimization { uid, after_uids } if uid == "amo" && after_uids.len() == 62)), "{flagged:?}");
        assert!(!flagged.iter().any(|i| matches!(i, Issue::MisplacedOptimization { uid, .. } if uid == "rocket")), "RocketMan Compat must load after RocketMan, so it is not a complaint");
        // A dependency that loads after the mod that needs it is an order violation now.
        let mut wrong = current.clone();
        wrong.swap(1, 2); // furniture before HugsLib is fine; move HugsLib below RimJobWorld
        let h = wrong.iter().position(|u| u == "hugs").unwrap();
        let hugs_uid = wrong.remove(h);
        let at = wrong.iter().position(|u| u == "perfopt").unwrap();
        wrong.insert(at, hugs_uid);
        let v = validate(&wrong, &ctx);
        assert!(v.iter().any(|i| matches!(i, Issue::OrderViolation { uid, target_uid, comment, .. } if uid == "rjw" && target_uid == "hugs" && comment.as_deref() == Some(crate::rules::NEEDS))), "{v:?}");
    }

    /// A group with its own place in the order: its members sort together right after the
    /// ordinary members of the phase it follows, rules still hold, and a per-mod choice wins.
    #[test]
    fn sections_sort_after_their_phase_in_the_users_order() {
        let mut mods = fixture();
        for i in 0..4 {
            let mut c = m(&format!("c{i}"), &format!("x.content{i}"), &format!("Content {i}"), Source::Workshop);
            c.contents.defs = 3;
            mods.push(c);
        }
        let mut addon = m("addon", "x.addon", "Addon for Content 2", Source::Workshop);
        addon.contents.defs = 1;
        addon.rules.load_after = vec!["x.content2".into()];
        mods.push(addon);
        let db = Databases::default();
        let rules = compile_rules(&mods, &db);
        let files = HashMap::new();
        let mut ov = UserOverrides::default();
        ov.section_defs = vec![SectionDef { id: "g-late".into(), name: "My late content".into(), after: Phase::Content }, SectionDef { id: "g-later".into(), name: "Even later".into(), after: Phase::Content }];
        ov.sections.insert("c0".into(), "g-later".into());
        ov.sections.insert("c2".into(), "g-late".into());
        ov.sections.insert("c3".into(), "g-late".into());
        // c3 also has an explicit phase: that wins over the group
        ov.phases.insert("c3".into(), Phase::Patch);
        let ctx = Context { mods: &mods, files: &files, rules: &rules, db: &db, major_minor: "1.6", overrides: &ov };
        let current: Vec<String> = ["core", "harmony", "c0", "c1", "c2", "c3", "addon", "walls", "patch", "rocket"].iter().map(|s| s.to_string()).collect();
        let r = sort(&current, &ctx);
        let pos = |u: &str| r.order.iter().position(|x| x == u).unwrap();
        let pl = |u: &str| r.placements.iter().find(|p| p.uid == u).unwrap();
        assert_eq!(pl("c2").phase, Phase::Content);
        assert_eq!(pl("c2").section.as_deref(), Some("g-late"));
        assert!(pl("c2").reason.contains("My late content"), "{}", pl("c2").reason);
        assert_eq!(pl("c1").section, None);
        assert_eq!(pl("c3").phase, Phase::Patch, "the mod's own choice beats its group");
        assert_eq!(pl("c3").section, None);
        assert!(pos("c1") < pos("c2"), "ordinary content first, then the section: {:?}", r.order);
        assert!(pos("c2") < pos("c0"), "sections follow one another in the user's order: {:?}", r.order);
        assert!(pos("c0") < pos("walls") && pos("c0") < pos("patch"), "sections sit before the next phase: {:?}", r.order);
        assert!(pos("addon") > pos("c2"), "a rule still holds across the section: {:?}", r.order);
        assert_eq!(ov.section_rank(Phase::Content, Some("g-later")), 2);
        assert_eq!(ov.section_rank(Phase::Patch, Some("g-later")), 0, "a section is ranked only within the phase it follows");
    }

    /// Dependents alone do not make a library: VFE Empire (code, hundreds of Defs, built on the
    /// VE framework) and Dubs Bad Hygiene (code, hundreds of Defs) have add-ons that depend on
    /// them and are still content. HugsLib-sized code with a handful of Defs is one.
    #[test]
    fn content_mods_with_addons_are_not_libraries() {
        let mut mods = fixture();
        let mut vef = m("vef", "oskarpotocki.vanillafactionsexpanded.core", "Vanilla Expanded Framework", Source::Workshop);
        vef.contents.assemblies = 4;
        vef.contents.defs = 80;
        vef.kind = ModKind::Code;
        let mut empire = m("empire", "oskarpotocki.vfe.empire", "Vanilla Factions Expanded - Empire", Source::Workshop);
        empire.contents.assemblies = 1;
        empire.contents.defs = 260;
        empire.kind = ModKind::Code;
        empire.rules.dependencies = vec![Dependency { package_id: "oskarpotocki.vanillafactionsexpanded.core".into(), ..Default::default() }];
        let mut dbh = m("dbh", "dubwise.dubsbadhygiene", "Dubs Bad Hygiene", Source::Workshop);
        dbh.contents.assemblies = 1;
        dbh.contents.defs = 300;
        dbh.kind = ModKind::Code;
        let mut har = m("har", "erdelf.humanoidalienraces", "Humanoid Alien Races", Source::Workshop);
        har.contents.assemblies = 1;
        har.contents.defs = 12;
        har.kind = ModKind::Code;
        // A code mod with many Defs that nothing depends on: content, whatever it is named.
        let mut lone = m("lone", "x.lone", "Lone Systems", Source::Workshop);
        lone.contents.assemblies = 1;
        lone.contents.defs = 8;
        lone.kind = ModKind::Code;
        mods.extend([vef, empire, dbh, har, lone]);
        // Three add-ons each: addon0-2 need Empire, addon3-5 need DBH, addon6-8 need HAR.
        for (k, needs) in ["oskarpotocki.vfe.empire", "dubwise.dubsbadhygiene", "erdelf.humanoidalienraces"].iter().enumerate() {
            for j in 0..3 {
                let i = k * 3 + j;
                let mut a = m(&format!("addon{i}"), &format!("x.addon{i}"), &format!("Add-on {i}"), Source::Workshop);
                a.contents.defs = 5;
                a.kind = ModKind::Xml;
                a.rules.dependencies = vec![Dependency { package_id: (*needs).into(), ..Default::default() }];
                mods.push(a);
            }
        }
        let db = Databases::default();
        let rules = compile_rules(&mods, &db);
        let files = HashMap::new();
        let ov = UserOverrides::default();
        let ctx = Context { mods: &mods, files: &files, rules: &rules, db: &db, major_minor: "1.6", overrides: &ov };
        let current: Vec<String> = mods.iter().map(|x| x.uid.clone()).collect();
        let order: Vec<&ModInfo> = mods.iter().collect();
        let pl = placements(&order, &ctx);
        let phase = |u: &str| pl.iter().find(|p| p.uid == u).unwrap().phase;
        let reason = |u: &str| pl.iter().find(|p| p.uid == u).unwrap().reason.clone();
        assert_eq!(phase("vef"), Phase::Framework, "known framework");
        assert_eq!(phase("empire"), Phase::Content, "{}", reason("empire"));
        assert_eq!(phase("dbh"), Phase::Content, "{}", reason("dbh"));
        assert_eq!(phase("har"), Phase::Framework, "{}", reason("har"));
        assert!(reason("har").contains("3 active mods need it"));
        assert_eq!(phase("lone"), Phase::Content);
        // The add-ons still load after what they need.
        let r = sort(&current, &ctx);
        let pos = |u: &str| r.order.iter().position(|x| x == u).unwrap();
        assert!(pos("empire") < pos("addon0") && pos("dbh") < pos("addon3") && pos("har") < pos("addon6"), "{:?}", r.order);
        assert!(pos("vef") < pos("empire"));
    }

    /// The HALO page's overrides: a package id filed by hand, a name rule, a built-in rule
    /// switched off, another sent elsewhere; the game's own content is never moved.
    #[test]
    fn users_halo_rules_override_the_built_in_ones() {
        let mods = fixture();
        let db = Databases::default();
        let rules = compile_rules(&mods, &db);
        let files = HashMap::new();
        let mut ov = UserOverrides::default();
        ov.halo.package_phases.insert("voult.betterpawncontrol".into(), Phase::Late);
        ov.halo.name_phases.push(NamePhase { needle: "RETRO".into(), phase: Phase::Content });
        ov.halo.off.insert("prepatch-ids".into());
        ov.halo.off.insert("top".into());
        ov.halo.retarget.insert("patch-only".into(), Phase::Late);
        ov.halo.package_phases.insert("ludeon.rimworld".into(), Phase::Late);
        let ctx = Context { mods: &mods, files: &files, rules: &rules, db: &db, major_minor: "1.6", overrides: &ov };
        let order: Vec<&ModInfo> = mods.iter().collect();
        let pl = placements(&order, &ctx);
        let p = |u: &str| pl.iter().find(|p| p.uid == u).unwrap();
        assert_eq!(p("bpc").phase, Phase::Late);
        assert!(p("bpc").reason.contains("package id"));
        assert_eq!(p("walls").phase, Phase::Content, "{}", p("walls").reason);
        assert!(p("walls").reason.contains("RETRO"));
        assert_eq!(p("harmony").phase, Phase::Content, "with the pre-patcher rules off, Harmony is code with no other signal: {}", p("harmony").reason);
        assert_eq!(p("patch").phase, Phase::Late, "the patch-only rule now files under late loaders");
        assert_eq!(p("core").phase, Phase::Core, "official content is not up for debate");
        // The rule table the page shows matches what classify tries, and round-trips as JSON.
        let table = builtin_rules();
        assert_eq!(table.first().unwrap().key, "official");
        assert!(table.iter().any(|r| r.key == "framework-ids" && r.ids.iter().any(|i| i == "unlimitedhugs.hugslib")));
        assert!(table.iter().filter(|r| !r.editable).count() == 2);
        let json = serde_json::to_string(&ov.halo).unwrap();
        let back: HaloRules = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ov.halo);
        assert!(json.contains("\"packagePhases\""));
    }

    #[test]
    fn the_prepatchers_go_in_the_order_their_own_pages_ask_for() {
        // Harmony, Prepatcher and Fishery say nothing about one another in this fixture, which is
        // close enough to the real thing: Prepatcher's page asks to sit high, right under Harmony,
        // and until now two pre-patchers with nothing to separate them were left in whatever order
        // the player's list happened to have. That is not a coin worth tossing.
        let mut mods = fixture();
        let mut prep = m("prep", "zetrith.prepatcher", "Prepatcher", Source::Workshop);
        prep.contents.assemblies = 1;
        let mut fish = m("fish", "bs.fishery", "Fishery", Source::Workshop);
        fish.contents.assemblies = 1;
        // A loading-screen mod: allowed above the game, but not a pre-patcher.
        let mut loading = m("loading", "me.samboycoding.betterloading", "BetterLoading", Source::Workshop);
        loading.contents.assemblies = 1;
        loading.kind = ModKind::Code;
        mods.extend([prep, fish, loading]);

        let db = Databases::default();
        let rules = compile_rules(&mods, &db);
        let files = HashMap::new();
        let ov = UserOverrides::default();
        let ctx = Context { mods: &mods, files: &files, rules: &rules, db: &db, major_minor: "1.6", overrides: &ov };
        // Deliberately backwards, and with the loading-screen mod first.
        let current: Vec<String> = ["loading", "fish", "prep", "harmony", "core", "hugs", "bpc"].iter().map(|s| s.to_string()).collect();
        let r = sort(&current, &ctx);
        let at = |u: &str| r.order.iter().position(|x| x == u).unwrap_or_else(|| panic!("{u} is missing"));

        assert!(at("harmony") < at("prep"), "Harmony first: everything here is built on it — {:?}", r.order);
        assert!(at("prep") < at("fish"), "Fishery is built on Prepatcher — {:?}", r.order);
        assert!(at("fish") < at("loading"), "a loading screen is not a pre-patcher and waits its turn — {:?}", r.order);
        assert!(at("loading") < at("core"), "but it is still allowed above the game — {:?}", r.order);
        assert!(at("core") < at("hugs"), "and the game and its DLC come before anything with Defs — {:?}", r.order);
        assert!(r.issues.iter().all(|i| !matches!(i, Issue::Cycle { .. })), "no loop was invented to do it: {:?}", r.issues);

        // Sorting a list that is already right leaves it alone.
        let again = sort(&r.order, &ctx);
        assert_eq!(again.order, r.order, "the order it proposes is stable");
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
