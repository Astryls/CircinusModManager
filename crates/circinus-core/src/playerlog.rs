//! Read RimWorld's `Player.log` and say what happened: did the game load, did it reset the
//! mod list, did it crash, which mods' XML broke, which textures failed, what threw.
//!
//! The log is Unity's plain text: one message per line, exceptions followed by `  at …`
//! frames and Harmony's `    - PREFIX owner: …` lines, XML errors followed by a
//! `Possible Matches:` block naming the source mod and file. Nothing here depends on the
//! exact wording beyond what RimWorld 1.5/1.6 print today.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    /// No load failure, no crash marker.
    #[default]
    Ok,
    /// RimWorld caught an exception while loading play data, reset the list and tried again.
    LoadFailedReset,
    /// A native crash marker (`Crash!!!`) is in the log.
    Crashed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XmlProblem {
    pub message: String,
    /// `[Source: …]` from the Possible Matches block.
    pub source_mod: Option<String>,
    pub file: Option<String>,
    /// For "Could not find parent node": the parent that was missing.
    pub missing_parent: Option<String>,
    /// `<defName>` from the printed node, when there is one.
    pub def_name: Option<String>,
    pub line: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExceptionGroup {
    /// First line of the exception, `[Ref …]` stripped.
    pub message: String,
    /// First stack frame ("Namespace.Type.Method").
    pub top_frame: Option<String>,
    /// First frame outside the engine and the game (a mod's own code), if any.
    pub mod_frame: Option<String>,
    /// Harmony patch owners seen in the stack.
    pub patch_owners: Vec<String>,
    pub count: usize,
    pub line: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DdsFailure {
    pub path: String,
    pub reason: String,
    /// Workshop id from the path, when it is a Workshop file.
    pub workshop_id: Option<u64>,
    /// Folder name under Mods/, when it is a local mod.
    pub mod_folder: Option<String>,
    pub line: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashInfo {
    pub line: usize,
    /// The line right before `Crash!!!` when it looks like a reason.
    pub reason: Option<String>,
    /// Managed frames from the native stack trace, innermost first.
    pub frames: Vec<String>,
    /// First frame that belongs to a mod rather than the engine or the game.
    pub culprit_frame: Option<String>,
    /// True when the stack shows a LongEventHandler worker thread.
    pub off_main_thread: bool,
    pub quickstart: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Duplicate {
    pub package_id: String,
    pub folders: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Timing {
    pub label: String,
    pub seconds: f64,
    pub line: usize,
}

/// How long the game actually took to load, the last time it did.
///
/// RimWorld's own log has no timestamps and never says. What it does carry are lines other mods
/// print -- Prepatcher announces the vanilla load, and the def-cache mods announce their pipeline
/// -- so a measured figure is available to anybody running one of them, which on a list big
/// enough to care about load time is nearly everybody.
///
/// `source` is the line's own label and is shown beside the number. This is somebody else's
/// measurement of somebody else's stage, not a stopwatch Circinus held, and the screen says which
/// mod said it rather than presenting it as the app's own finding.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadRun {
    pub total_secs: f64,
    /// What in the log reported it.
    pub source: String,
    /// Vanilla's own load, when Prepatcher printed it: the floor under any list.
    pub vanilla_secs: Option<f64>,
    /// Mods active in that run, when a line happened to say. Lets the screen notice that the
    /// measurement was taken with a different list than the one on screen now.
    pub mods: Option<usize>,
}

/// The best measurement of a whole load in a parsed log, if it holds one.
///
/// "Best" is the longest, which is the honest choice: these lines report *stages*, the stages
/// nest, and the outermost is the one closest to what a player experiences as loading. Taking
/// the first, or the last, would report whichever stage happened to print.
pub fn load_run(r: &LogReport) -> Option<LoadRun> {
    pick(&r.timings, r.prepatcher_vanilla_load_secs)
}

/// The same, from raw text, without building the rest of the report.
///
/// The full parse gathers exceptions, textures, XML problems and stacks; this runs at every
/// launch on a log that can be tens of megabytes, and none of that is wanted. Two rules and a
/// linear pass.
pub fn load_run_of(text: &str) -> Option<LoadRun> {
    let mut timings: Vec<Timing> = Vec::new();
    let mut vanilla = None;
    for (i, line) in text.lines().enumerate() {
        if let Some(rest) = line.strip_prefix("Prepatcher: Starting... (vanilla load took ") {
            vanilla = parse_secs(rest.trim_end_matches(')'));
        } else if line.contains("Total pipeline time") || (line.starts_with('[') && (line.contains(" took ") || line.contains("Loaded in "))) {
            if let Some(secs) = timing_in(line) {
                if secs >= 1.0 {
                    timings.push(Timing { label: strip_ref(line), seconds: secs, line: i + 1 });
                }
            }
        }
    }
    pick(&timings, vanilla)
}

fn pick(timings: &[Timing], vanilla: Option<f64>) -> Option<LoadRun> {
    let biggest = timings.iter().max_by(|a, b| a.seconds.total_cmp(&b.seconds));
    // Prepatcher's vanilla figure is a real whole-load measurement too, and on a log with no
    // other timing it is the only one -- but it measures the game *without* the list, so it is
    // never the answer when a longer stage was also reported.
    match (biggest, vanilla) {
        (Some(t), v) if t.seconds >= v.unwrap_or(0.0) => Some(LoadRun { total_secs: t.seconds, source: label_of(&t.label), vanilla_secs: v, mods: mods_in(&t.label) }),
        (_, Some(v)) => Some(LoadRun { total_secs: v, source: "Prepatcher, before your mods loaded".into(), vanilla_secs: Some(v), mods: None }),
        _ => None,
    }
}

/// `[DefLoadCache] Cache saved, 83603 defs from 708 mods (12777 KB) in 1803ms. Total…` → the
/// bracketed name, which is the mod that measured it.
fn label_of(line: &str) -> String {
    let t = line.trim();
    if let Some(rest) = t.strip_prefix('[') {
        if let Some(i) = rest.find(']') {
            return rest[..i].trim().to_string();
        }
    }
    t.chars().take(40).collect()
}

/// "from 708 mods" anywhere on the line.
fn mods_in(line: &str) -> Option<usize> {
    let i = line.find(" mods")?;
    let head = &line[..i];
    let digits: String = head.chars().rev().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.chars().rev().collect::<String>().parse().ok()
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogReport {
    pub lines: usize,
    pub game_version: Option<String>,
    pub unity_version: Option<String>,
    pub gpu: Option<String>,
    pub vram_mb: Option<u64>,
    pub command_line: Option<String>,
    pub outcome: Outcome,
    /// "Resetting mods config and trying again" was logged.
    pub reset: bool,
    /// "Could not recover from errors loading play data. Giving up."
    pub gave_up: bool,
    /// The exception that made the first load fail, and its first frame.
    pub load_failure: Option<ExceptionGroup>,
    pub crash: Option<CrashInfo>,
    pub prepatcher_vanilla_load_secs: Option<f64>,
    pub prepatcher_restarted: bool,
    pub timings: Vec<Timing>,
    pub duplicates: Vec<Duplicate>,
    pub missing_parents: Vec<XmlProblem>,
    pub xml_errors: Vec<XmlProblem>,
    pub exceptions: Vec<ExceptionGroup>,
    pub dds_failures: Vec<DdsFailure>,
    /// "requires a texture size that is a multiple of 4" per format.
    pub multiple_of_4_warnings: HashMap<String, usize>,
    pub thread_texture_warnings: usize,
    /// "Could not load Texture2D at 'path'": path → count.
    pub textures_not_found: Vec<(String, usize)>,
    pub textures_not_found_total: usize,
    pub bad_texture_materials: usize,
    pub quickstart: bool,
}

const ENGINE_PREFIXES: &[&str] = &["UnityEngine", "System", "Verse", "RimWorld", "Mono", "HarmonyLib", "MonoMod", "LudeonTK", "Unity.", "mscorlib", "(wrapper", "object:", "<Module>"];
/// Mods that wrap the loading pipeline itself and therefore sit in every stack.
const INFRA_PREFIXES: &[&str] = &["ilyvion.LoadingProgress", "HugsLib.Patches", "HugsLib.Utils", "Prepatcher", "FasterGameLoading.LongEvent"];

fn is_engine_frame(frame: &str) -> bool {
    ENGINE_PREFIXES.iter().any(|p| frame.starts_with(p)) || INFRA_PREFIXES.iter().any(|p| frame.starts_with(p))
}

fn def_name_in(line: &str) -> Option<String> {
    let i = line.find("<defName>")?;
    let rest = &line[i + 9..];
    let j = rest.find("</defName>")?;
    Some(rest[..j].trim().to_string())
}

/// "  at Verse.ModDdsLoader.CreateTexture (RimWorld.IO.VirtualFile file, …) [0x00028] in <…>:0" → "Verse.ModDdsLoader.CreateTexture"
fn frame_of(line: &str) -> Option<String> {
    let t = line.trim_start();
    let rest = t.strip_prefix("at ")?;
    let end = rest.find(" (").or_else(|| rest.find(" [")).unwrap_or(rest.len());
    let f = rest[..end].trim();
    if f.is_empty() {
        None
    } else {
        Some(f.to_string())
    }
}

/// "0x0000018B5101543B (Mono JIT Code) WorkRoles.UI.WorkRolesTex:MakeCircle (int)" → "WorkRoles.UI.WorkRolesTex.MakeCircle"
fn native_frame_of(line: &str) -> Option<String> {
    let i = line.find("(Mono JIT Code)")?;
    let rest = line[i + "(Mono JIT Code)".len()..].trim();
    let rest = rest.strip_prefix("(wrapper managed-to-native)").or_else(|| rest.strip_prefix("(wrapper runtime-invoke)")).or_else(|| rest.strip_prefix("(wrapper dynamic-method)")).unwrap_or(rest).trim();
    let end = rest.find(" (").unwrap_or(rest.len());
    let f = rest[..end].replace(':', ".");
    if f.is_empty() {
        None
    } else {
        Some(f)
    }
}

fn strip_ref(line: &str) -> String {
    let mut s = line.to_string();
    if let Some(i) = s.find("[Ref ") {
        if let Some(j) = s[i..].find(']') {
            s.replace_range(i..i + j + 1, "");
        }
    }
    s.trim().to_string()
}

fn workshop_id_in(path: &str) -> Option<u64> {
    let lower = path.replace('\\', "/").to_ascii_lowercase();
    let i = lower.find("/workshop/content/294100/")?;
    let rest = &lower[i + "/workshop/content/294100/".len()..];
    rest.split('/').next()?.parse().ok()
}

fn mod_folder_in(path: &str) -> Option<String> {
    let norm = path.replace('\\', "/");
    let lower = norm.to_ascii_lowercase();
    let i = lower.find("/mods/")?;
    let rest = &norm[i + "/mods/".len()..];
    rest.split('/').next().map(|s| s.to_string())
}

fn parse_secs(s: &str) -> Option<f64> {
    let t = s.trim().trim_end_matches(')');
    if let Some(ms) = t.strip_suffix("ms") {
        return ms.trim().parse::<f64>().ok().map(|v| v / 1000.0);
    }
    if let Some(sec) = t.strip_suffix('s') {
        return sec.trim().parse::<f64>().ok();
    }
    None
}

/// Look for "took 12.3s" / "in 1803ms" / "time: 462986ms" on a line.
fn timing_in(line: &str) -> Option<f64> {
    for key in ["Total pipeline time:", "took ", " in "] {
        if let Some(i) = line.find(key) {
            let rest = line[i + key.len()..].trim_start();
            let tok: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == 'm' || *c == 's').collect();
            if let Some(v) = parse_secs(&tok) {
                return Some(v);
            }
        }
    }
    None
}

pub fn parse(text: &str) -> LogReport {
    let lines: Vec<&str> = text.lines().collect();
    let mut r = LogReport { lines: lines.len(), ..LogReport::default() };
    let mut refs: HashMap<String, usize> = HashMap::new(); // ref id → index in r.exceptions
    let mut pending_xml: Option<usize> = None; // index into missing_parents/xml_errors awaiting Possible Matches
    let mut pending_is_parent = false;
    let mut tex_counts: HashMap<String, usize> = HashMap::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let ln = i + 1;
        // ---- header facts ----
        if r.game_version.is_none() && line.starts_with("RimWorld ") && line.contains(" rev") {
            r.game_version = Some(line.trim().to_string());
        } else if let Some(v) = line.strip_prefix("Initialize engine version: ") {
            r.unity_version = Some(v.split(' ').next().unwrap_or(v).to_string());
        } else if let Some(v) = line.trim_start().strip_prefix("Renderer: ") {
            if r.gpu.is_none() {
                r.gpu = Some(v.trim().to_string());
            }
        } else if let Some(v) = line.trim_start().strip_prefix("VRAM: ") {
            r.vram_mb = v.trim().trim_end_matches(" MB").parse().ok();
        } else if let Some(v) = line.strip_prefix("Command line arguments: ") {
            r.command_line = Some(v.trim().to_string());
        }
        // ---- prepatcher ----
        if let Some(rest) = line.strip_prefix("Prepatcher: Starting... (vanilla load took ") {
            r.prepatcher_vanilla_load_secs = parse_secs(rest.trim_end_matches(')'));
        } else if line.starts_with("Prepatcher: Restarted with the patched assembly") {
            r.prepatcher_restarted = true;
        }
        // ---- timings worth showing ----
        if line.contains("Total pipeline time") || (line.starts_with('[') && (line.contains(" took ") || line.contains("Loaded in "))) {
            if let Some(secs) = timing_in(line) {
                if secs >= 1.0 {
                    r.timings.push(Timing { label: strip_ref(line), seconds: secs, line: ln });
                }
            }
        }
        // ---- duplicates ----
        if let Some(rest) = line.strip_prefix("Tried loading mod with the same packageId multiple times: ") {
            let id = rest.trim_end_matches(". Ignoring the duplicates.").trim().to_string();
            let mut folders = Vec::new();
            let mut j = i + 1;
            // The offending folders follow, one absolute path per line.
            while j < lines.len() && (lines[j].contains(":\\") || lines[j].starts_with('/')) && !lines[j].starts_with("Tried loading") {
                folders.push(lines[j].trim().to_string());
                j += 1;
            }
            r.duplicates.push(Duplicate { package_id: id, folders });
            i = j;
            continue;
        }
        // ---- load failure / reset / crash markers ----
        if line.starts_with("Caught exception while loading play data") && line.contains("Resetting mods config") {
            r.reset = true;
            if r.outcome == Outcome::Ok {
                r.outcome = Outcome::LoadFailedReset;
            }
            // "The exception was: …" follows, then frames.
            if i + 1 < lines.len() {
                if let Some(msg) = lines[i + 1].strip_prefix("The exception was: ") {
                    let (grp, next) = collect_exception(&lines, i + 1, msg, ln + 1);
                    r.load_failure = Some(grp);
                    i = next;
                    continue;
                }
            }
        } else if line.starts_with("Could not recover from errors loading play data") {
            r.gave_up = true;
        } else if line.trim() == "Crash!!!" {
            r.outcome = Outcome::Crashed;
            let reason = if i > 0 && !lines[i - 1].trim().is_empty() && lines[i - 1].len() < 120 && !lines[i - 1].starts_with("Total:") { Some(lines[i - 1].trim().to_string()) } else { None };
            let mut frames = Vec::new();
            let mut j = i + 1;
            while j < lines.len() && !lines[j].starts_with("========== END OF STACKTRACE") {
                if let Some(f) = native_frame_of(lines[j]) {
                    frames.push(f);
                }
                j += 1;
            }
            let culprit = frames.iter().find(|f| !is_engine_frame(f)).cloned();
            let off_main = frames.iter().any(|f| f.contains("LongEventHandler.RunEventFromAnotherThread") || f.contains("ThreadHelper.ThreadStart"));
            let quick = frames.iter().any(|f| f.contains("Quickstart"));
            r.quickstart |= quick;
            r.crash = Some(CrashInfo { line: ln, reason, frames, culprit_frame: culprit, off_main_thread: off_main, quickstart: quick });
            i = j;
            continue;
        }
        // ---- textures ----
        if let Some(rest) = line.strip_prefix("Compressed TextureFormat ") {
            if let Some(fmt) = rest.split(" requires a texture size").next() {
                *r.multiple_of_4_warnings.entry(fmt.trim().to_string()).or_default() += 1;
            }
        } else if line.starts_with("Tried to create a texture from a different thread") {
            r.thread_texture_warnings += 1;
        } else if line.starts_with("Exception loading UnityEngine.Texture2D from file.") {
            let path = lines.get(i + 1).and_then(|l| l.strip_prefix("absFilePath: ")).map(|s| s.trim().to_string()).unwrap_or_default();
            let reason = lines.get(i + 2).and_then(|l| l.strip_prefix("Exception: ")).map(|s| s.trim().to_string()).unwrap_or_default();
            r.dds_failures.push(DdsFailure { workshop_id: workshop_id_in(&path), mod_folder: mod_folder_in(&path), path, reason, line: ln });
        } else if let Some(rest) = line.strip_prefix("Could not load Texture2D at '") {
            if let Some(end) = rest.find('\'') {
                *tex_counts.entry(rest[..end].to_string()).or_default() += 1;
                r.textures_not_found_total += 1;
            }
        } else if line.contains("_BadTexture") {
            r.bad_texture_materials += 1;
        }
        // ---- XML problems ----
        if let Some(rest) = line.strip_prefix("XML error: ") {
            let msg = rest.split(" Full node:").next().unwrap_or(rest).trim().to_string();
            let missing = rest.strip_prefix("Could not find parent node named \"").and_then(|s| s.split('"').next()).map(|s| s.to_string());
            let p = XmlProblem { message: msg, missing_parent: missing.clone(), def_name: def_name_in(rest), line: ln, ..XmlProblem::default() };
            if missing.is_some() {
                r.missing_parents.push(p);
                pending_xml = Some(r.missing_parents.len() - 1);
                pending_is_parent = true;
            } else {
                r.xml_errors.push(p);
                pending_xml = Some(r.xml_errors.len() - 1);
                pending_is_parent = false;
            }
        } else if line.starts_with("Exception loading def from file ") || line.starts_with("Error loading def ") || line.starts_with("Could not load def ") {
            r.xml_errors.push(XmlProblem { message: strip_ref(line), line: ln, ..XmlProblem::default() });
            pending_xml = Some(r.xml_errors.len() - 1);
            pending_is_parent = false;
        } else if let Some(src) = line.strip_prefix("[Source: ") {
            let src = src.trim_end_matches(']').to_string();
            let file = lines.get(i + 1).and_then(|l| l.strip_prefix("[File: ")).map(|s| s.trim_end_matches(']').to_string());
            if let Some(idx) = pending_xml.take() {
                let target = if pending_is_parent { r.missing_parents.get_mut(idx) } else { r.xml_errors.get_mut(idx) };
                if let Some(p) = target {
                    p.source_mod = Some(src);
                    p.file = file;
                }
            }
        }
        // ---- exceptions ----
        if line.starts_with("Exception from asynchronous event: ") || line.starts_with("Could not execute post-long-event action. Exception: ") || line.starts_with("Exception filling window") || line.starts_with("Exception ticking") || line.starts_with("Exception in ") || (line.starts_with("Exception ") && line.contains("Exception:") && !line.starts_with("Exception loading UnityEngine.Texture2D") && !line.starts_with("Exception loading def")) {
            let msg = strip_ref(line);
            // Duplicate reference?
            let dup_ref = lines.get(i + 1).filter(|l| l.contains("Duplicate stacktrace")).and_then(|l| l.strip_prefix("[Ref ")).and_then(|l| l.split(']').next()).map(|s| s.to_string());
            if let Some(id) = dup_ref {
                if let Some(idx) = refs.get(&id) {
                    r.exceptions[*idx].count += 1;
                    i += 2;
                    continue;
                }
            }
            let (grp, next) = collect_exception(&lines, i, &msg, ln);
            if let Some(id) = lines.get(i + 1).and_then(|l| l.strip_prefix("[Ref ")).and_then(|l| l.split(']').next()).filter(|_| !lines[i + 1].contains("Duplicate")) {
                refs.insert(id.to_string(), r.exceptions.len());
            }
            r.exceptions.push(grp);
            i = next;
            continue;
        }
        if line.contains("Quickstart") {
            r.quickstart = true;
        }
        i += 1;
    }
    let mut tex: Vec<(String, usize)> = tex_counts.into_iter().collect();
    tex.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    r.textures_not_found = tex;
    r.exceptions.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.line.cmp(&b.line)));
    r
}

/// Gather the frames and patch owners that follow an exception line. Returns the group and the
/// index of the first line after the block.
fn collect_exception(lines: &[&str], start: usize, message: &str, ln: usize) -> (ExceptionGroup, usize) {
    let mut g = ExceptionGroup { message: message.to_string(), count: 1, line: ln, ..ExceptionGroup::default() };
    let mut j = start + 1;
    while j < lines.len() {
        let l = lines[j];
        let t = l.trim_start();
        if t.starts_with("at ") {
            if let Some(f) = frame_of(l) {
                if g.top_frame.is_none() {
                    g.top_frame = Some(f.clone());
                }
                if g.mod_frame.is_none() && !is_engine_frame(&f) {
                    g.mod_frame = Some(f);
                }
            }
        } else if let Some(rest) = t.strip_prefix("- PREFIX ").or_else(|| t.strip_prefix("- POSTFIX ")).or_else(|| t.strip_prefix("- TRANSPILER ")).or_else(|| t.strip_prefix("- FINALIZER ")) {
            if let Some(owner) = rest.split(':').next() {
                let owner = owner.trim().to_string();
                if !g.patch_owners.contains(&owner) {
                    g.patch_owners.push(owner);
                }
            }
        } else if l.starts_with("[Ref ") || l.starts_with("UnityEngine.") || l.starts_with("Parameter name") || l.starts_with("Rethrow as") || t.starts_with("--- End of") {
            // still part of the block
        } else if l.trim().is_empty() {
            break;
        } else {
            break;
        }
        j += 1;
    }
    (g, j)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"Initialize engine version: 2022.3.35f1 (011206c7a712)
Direct3D:
    Version:  Direct3D 11.0 [level 11.1]
    Renderer: NVIDIA GeForce RTX 4090 (ID=0x2684)
    VRAM:     24138 MB
Command line arguments: -disable-compute-shaders
RimWorld 1.6.4871 rev591
Tried loading mod with the same packageId multiple times: example.somemod. Ignoring the duplicates.
C:\Program Files (x86)\Steam\steamapps\common\RimWorld\Mods\54c9da5e4437
C:\Program Files (x86)\Steam\steamapps\common\RimWorld\Mods\247a67627879
Prepatcher: Starting... (vanilla load took 43.40687s)
[DefLoadCache] Cache saved, 83603 defs from 708 mods (12777 KB) in 1803ms. Total pipeline time: 462986ms
XML error: Could not find parent node named "Stun" for node "DamageDef". Full node: <DamageDef ParentName="Stun"><defName>Ancot_Stun</defName></DamageDef>

Possible Matches:
[Source: Ancot Library]
[File: C:\Program Files (x86)\Steam\steamapps\workshop\content\294100\2988801276\1.6\Defs\DamageDefs\Damages_Stun.xml]
Exception loading def from file GroupFeralDoggy.xml: System.ArgumentException: Could not find type named Rimworld_Animations.GroupAnimationContext_RJWSex
Parameter name: node
[Ref 58CD22BE] Duplicate stacktrace, see ref for original

Possible Matches:
[Source: Chief Cheese's Feral Fucker Animations]
[File: C:\Program Files (x86)\Steam\steamapps\common\RimWorld\Mods\ChiefsFeralFuckerAnims\1.6\Defs\GroupFeralDoggy.xml]
Caught exception while loading play data but there are active mods other than Core. Resetting mods config and trying again.
The exception was: System.NullReferenceException: Object reference not set to an instance of an object
[Ref 2F96CF]
  at RimWorld.ThingDefGenerator_Buildings.NewFrameDef_Thing (Verse.ThingDef def, System.Boolean hotReload) [0x00105] in <x>:0
    - POSTFIX Uuugggg.rimworld.Replace_Stuff.main: Void Replace_Stuff.OverMineable.FramesArentEdifices:Postfix(ThingDef __result)
  at RimWorld.DefGenerator.GenerateImpliedDefs_PreResolve (System.Boolean hotReload) [0x000af] in <x>:0
    - PREFIX RedMattis.GravShipSize: Void GravshipSize.HarmonyPatches:GenerateImpliedDefs_Prefix(Boolean hotReload)
  at Verse.PlayDataLoader.DoPlayLoad () [0x0014b] in <x>:0
Could not recover from errors loading play data. Giving up.
Exception from asynchronous event: System.NullReferenceException: Object reference not set to an instance of an object
[Ref 5D27CC67]
  at GravshipSize.GravshipSizeSettings.ApplySettingsNow () [0x0003c] in <y>:0
  at GravshipSize.HarmonyPatches.GenerateImpliedDefs_Prefix (System.Boolean hotReload) [0x00001] in <y>:0
  at RimWorld.DefGenerator.GenerateImpliedDefs_PreResolve (System.Boolean hotReload) [0x00015] in <x>:0
    - PREFIX RedMattis.GravShipSize: Void GravshipSize.HarmonyPatches:GenerateImpliedDefs_Prefix(Boolean hotReload)
Could not execute post-long-event action. Exception: System.InvalidOperationException: Sequence contains no elements
[Ref 90EFF30B]
  at System.Linq.Enumerable.First[TSource] (System.Collections.Generic.IEnumerable`1[T] source) [0x00013] in <z>:0
  at SomeMod.Patches.Thing:Postfix () [0x00013] in <z>:0
Could not execute post-long-event action. Exception: System.InvalidOperationException: Sequence contains no elements
[Ref 90EFF30B] Duplicate stacktrace, see ref for original
Could not execute post-long-event action. Exception: System.InvalidOperationException: Sequence contains no elements
[Ref 90EFF30B] Duplicate stacktrace, see ref for original
Compressed TextureFormat RGB Compressed DXT1|BC1 requires a texture size that is a multiple of 4
Compressed TextureFormat RGB(A) Compressed BC7 requires a texture size that is a multiple of 4
Exception loading UnityEngine.Texture2D from file.
absFilePath: C:\Program Files (x86)\Steam\steamapps\workshop\content\294100\2842502659\Textures\UI\Backgrounds\AnimatPath.dds
Exception: UnityEngine.UnityException: Failed to create texture because of invalid parameters.
[Ref 4D9DCEA]
  at UnityEngine.Texture2D.Internal_Create (UnityEngine.Texture2D mono) [0x00025] in <e>:0
Tried to create a texture from a different thread.
Could not load Texture2D at 'Empty/Empty' in any active mod or in base resources.
Could not load Texture2D at 'Empty/Empty' in any active mod or in base resources.
Could not load Texture2D at 'UI/Invisible' in any active mod or in base resources.
Material 'Custom/Cutout_BadTexture' with Shader 'Custom/Cutout' doesn't have a texture property '_MaskTex'
Graphics device is null.
Crash!!!
SymInit: Symbol-SearchPath: '.;C:\x'
0x00007FFBF7FBB64B (UnityPlayer) (function-name not available)
0x0000018AEBFB6AD7 (Mono JIT Code) (wrapper managed-to-native) UnityEngine.Texture2D:Internal_CreateImpl (UnityEngine.Texture2D,int,int,int)
0x0000018AEC9C8A13 (Mono JIT Code) UnityEngine.Texture2D:.ctor (int,int,UnityEngine.TextureFormat,bool)
0x0000018B5101543B (Mono JIT Code) WorkRoles.UI.WorkRolesTex:MakeCircle (int)
0x0000018B510150E3 (Mono JIT Code) WorkRoles.UI.WorkRolesTex:.cctor ()
0x0000018B5101108B (Mono JIT Code) WorkRoles.Patches.Patch_MemoryUtility_ClearAllMapsAndWorld:Postfix ()
0x0000018B50FC7A8B (Mono JIT Code) HugsLib.Quickstart.QuickstartController/<>c:<InitiateMapGeneration>b__7_0 ()
0x0000018B23249B7E (Mono JIT Code) Verse.LongEventHandler:RunEventFromAnotherThread (System.Action)
0x0000018AF89E5F06 (Mono JIT Code) System.Threading.ThreadHelper:ThreadStart_Context (object)
========== END OF STACKTRACE ===========
"#;

    #[test]
    fn reads_the_whole_story() {
        let r = parse(SAMPLE);
        assert_eq!(r.game_version.as_deref(), Some("RimWorld 1.6.4871 rev591"));
        assert_eq!(r.unity_version.as_deref(), Some("2022.3.35f1"));
        assert_eq!(r.gpu.as_deref(), Some("NVIDIA GeForce RTX 4090 (ID=0x2684)"));
        assert_eq!(r.vram_mb, Some(24138));
        assert_eq!(r.command_line.as_deref(), Some("-disable-compute-shaders"));
        assert_eq!(r.prepatcher_vanilla_load_secs, Some(43.40687));
        assert_eq!(r.duplicates.len(), 1);
        assert_eq!(r.duplicates[0].package_id, "example.somemod");
        assert_eq!(r.duplicates[0].folders.len(), 2);
        assert_eq!(r.timings.len(), 1);
        assert!((r.timings[0].seconds - 462.986).abs() < 0.01);

        assert_eq!(r.missing_parents.len(), 1);
        assert_eq!(r.missing_parents[0].missing_parent.as_deref(), Some("Stun"));
        assert_eq!(r.missing_parents[0].def_name.as_deref(), Some("Ancot_Stun"));
        assert_eq!(r.missing_parents[0].source_mod.as_deref(), Some("Ancot Library"));
        assert!(r.missing_parents[0].file.as_deref().unwrap().ends_with("Damages_Stun.xml"));
        assert_eq!(r.xml_errors.len(), 1);
        assert_eq!(r.xml_errors[0].source_mod.as_deref(), Some("Chief Cheese's Feral Fucker Animations"));

        assert!(r.reset);
        assert!(r.gave_up);
        let lf = r.load_failure.as_ref().unwrap();
        assert!(lf.message.contains("NullReferenceException"));
        assert_eq!(lf.top_frame.as_deref(), Some("RimWorld.ThingDefGenerator_Buildings.NewFrameDef_Thing"));
        assert_eq!(lf.patch_owners, vec!["Uuugggg.rimworld.Replace_Stuff.main", "RedMattis.GravShipSize"]);

        assert_eq!(r.exceptions.len(), 2);
        let seq = r.exceptions.iter().find(|e| e.message.contains("Sequence contains no elements")).unwrap();
        assert_eq!(seq.count, 3);
        assert_eq!(seq.mod_frame.as_deref(), Some("SomeMod.Patches.Thing:Postfix"));
        let grav = r.exceptions.iter().find(|e| e.message.contains("asynchronous")).unwrap();
        assert_eq!(grav.mod_frame.as_deref(), Some("GravshipSize.GravshipSizeSettings.ApplySettingsNow"));

        assert_eq!(r.dds_failures.len(), 1);
        assert_eq!(r.dds_failures[0].workshop_id, Some(2842502659));
        assert_eq!(r.multiple_of_4_warnings.len(), 2);
        assert_eq!(r.thread_texture_warnings, 1);
        assert_eq!(r.textures_not_found_total, 3);
        assert_eq!(r.textures_not_found[0], ("Empty/Empty".to_string(), 2));
        assert_eq!(r.bad_texture_materials, 1);

        assert_eq!(r.outcome, Outcome::Crashed);
        let c = r.crash.as_ref().unwrap();
        assert_eq!(c.reason.as_deref(), Some("Graphics device is null."));
        assert_eq!(c.culprit_frame.as_deref(), Some("WorkRoles.UI.WorkRolesTex.MakeCircle"));
        assert!(c.off_main_thread);
        assert!(c.quickstart);
        assert!(r.quickstart);
    }

    #[test]
    fn clean_log_is_ok() {
        let r = parse("RimWorld 1.6.4871 rev591\nLoading...\nDone.\n");
        assert_eq!(r.outcome, Outcome::Ok);
        assert!(!r.reset);
        assert!(r.crash.is_none());
        assert!(r.exceptions.is_empty());
    }

    #[test]
    fn helpers() {
        assert_eq!(workshop_id_in(r"C:\Steam\steamapps\workshop\content\294100\123\Textures\a.dds"), Some(123));
        assert_eq!(mod_folder_in(r"C:\Games\RimWorld\Mods\MyMod\Textures\a.dds").as_deref(), Some("MyMod"));
        assert_eq!(frame_of("  at Verse.X.Y (int a) [0x0] in <z>:0 ").as_deref(), Some("Verse.X.Y"));
        assert_eq!(native_frame_of("0x1 (Mono JIT Code) A.B:C (int)").as_deref(), Some("A.B.C"));
        assert_eq!(parse_secs("43.4s"), Some(43.4));
        assert_eq!(parse_secs("1803ms"), Some(1.803));
    }
}

#[cfg(test)]
mod load_run_tests {
    use super::*;

    const REAL: &str = "Prepatcher: Starting... (vanilla load took 43.40687s)\n[DefLoadCache] Cache saved, 83603 defs from 708 mods (12777 KB) in 1803ms. Total pipeline time: 462986ms\n";

    #[test]
    fn reads_a_real_load_from_a_real_log() {
        let run = load_run(&parse(REAL)).expect("a log with both lines has a load");
        assert!((run.total_secs - 462.986).abs() < 0.01, "{}", run.total_secs);
        assert_eq!(run.source, "DefLoadCache", "the mod that measured it is named");
        assert!((run.vanilla_secs.unwrap() - 43.40687).abs() < 0.001);
        assert_eq!(run.mods, Some(708));
    }

    #[test]
    fn the_longest_stage_wins_whatever_order_it_printed_in() {
        // Stages nest, so the outermost is the one closest to what a player calls loading.
        let text = "[A] step took 12s\n[Big] whole thing took 300s\n[B] step took 4s\n";
        let run = load_run(&parse(text)).unwrap();
        assert_eq!(run.source, "Big");
        assert!((run.total_secs - 300.0).abs() < 0.01);
    }

    #[test]
    fn prepatcher_alone_is_still_a_measurement() {
        let run = load_run(&parse("Prepatcher: Starting... (vanilla load took 41.5s)\n")).unwrap();
        assert!((run.total_secs - 41.5).abs() < 0.01);
        assert!(run.source.contains("Prepatcher"));
        assert_eq!(run.mods, None);
    }

    #[test]
    fn a_log_with_no_timings_has_no_load() {
        assert!(load_run(&parse("RimWorld 1.6.4530 rev1235\nsomething happened\n")).is_none());
    }

    #[test]
    fn the_light_scan_agrees_with_the_full_parse() {
        // Two readers of one log that disagreed would be worse than one that was wrong.
        assert_eq!(load_run_of(REAL), load_run(&parse(REAL)));
        assert_eq!(load_run_of(""), None);
    }

    #[test]
    fn a_line_without_a_mod_count_says_so_rather_than_guessing() {
        let run = load_run(&parse("[Slow] everything took 90s\n")).unwrap();
        assert_eq!(run.mods, None);
    }
}
