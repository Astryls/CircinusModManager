//! What loading a mod costs the game, estimated from its folder.
//!
//! RimWorld's loading bar is spent, in rough order, on XML — every Defs file parsed, every
//! PatchOperation's XPath run over the unified document, inheritance resolved, then every node
//! reflected into a Def — on mod assemblies (loading, JIT, static constructors, Harmony), and
//! on textures, where a PNG is decoded on the main thread and given mipmaps while a DDS is
//! read as it is. None of that can be measured from outside the game; it can be estimated
//! from what the folder holds, and the estimate ranks mods against one another well enough
//! to say which few of a thousand are worth looking at. The coefficients below are one
//! machine's worth of profiling folded into round numbers; `score` is the only place they
//! live, so a measured figure can replace them later without touching anything else.

use crate::model::Contents;
use std::path::Path;

/// Milliseconds per KB of Defs XML: parse, inherit, reflect.
const DEF_MS_PER_KB: f64 = 0.25;
/// Milliseconds per KB of Patches XML: parsing the patch files themselves.
const PATCH_XML_MS_PER_KB: f64 = 0.05;
/// Milliseconds per patch operation with a direct XPath (`Defs/ThingDef[defName="x"]`).
const PATCH_OP_MS: f64 = 0.6;
/// Milliseconds per operation whose XPath searches the whole document.
const HEAVY_OP_MS: f64 = 20.0;
/// Nanoseconds per pixel of PNG decoded, mipmapped and uploaded.
const PNG_NS_PER_PIXEL: f64 = 8.0;
/// Milliseconds per MB of DDS read and uploaded.
const DDS_MS_PER_MB: f64 = 2.0;
/// Per assembly: load and the first JIT work.
const DLL_MS_EACH: f64 = 15.0;
const DLL_MS_PER_MB: f64 = 60.0;
const SOUND_MS_PER_MB: f64 = 8.0;

/// The estimate for a folder's contents, in milliseconds.
/// What the game takes to load with no mods at all, when nothing has measured it here.
///
/// `score` sums *mods*, so a total built from it alone is short by however long RimWorld takes to
/// start on its own -- which is the larger half of the number for a small list. A real figure
/// arrives from the log whenever Prepatcher is installed (`playerlog::LoadRun::vanilla_secs`) and
/// is preferred over this the moment it does; this is the stand-in until then, from the same
/// machine the coefficients came from, and it is deliberately a round number so nobody mistakes
/// it for a measurement.
pub const VANILLA_SECS: f64 = 40.0;

pub fn score(c: &Contents) -> u64 {
    let l = &c.load;
    let kb = |b: u64| b as f64 / 1024.0;
    let mb = |b: u64| b as f64 / (1024.0 * 1024.0);
    let ms = kb(l.def_bytes) * DEF_MS_PER_KB
        + kb(l.patch_bytes) * PATCH_XML_MS_PER_KB
        + (l.patch_ops.saturating_sub(l.heavy_ops)) as f64 * PATCH_OP_MS
        + l.heavy_ops as f64 * HEAVY_OP_MS
        + l.png_pixels as f64 * PNG_NS_PER_PIXEL / 1e6
        + mb(l.dds_bytes) * DDS_MS_PER_MB
        + c.assemblies as f64 * DLL_MS_EACH
        + mb(l.dll_bytes) * DLL_MS_PER_MB
        + mb(l.sound_bytes) * SOUND_MS_PER_MB;
    ms.round() as u64
}

/// Whether an XPath makes the game search the whole document rather than walk to one node:
/// a descendant step, a wildcard step, or a predicate that has to test every candidate.
pub fn xpath_is_heavy(xpath: &str) -> bool {
    let x = xpath.trim();
    x.contains("//") || x.contains("/*") || x.starts_with('*') || x.contains("contains(") || x.contains("starts-with(") || x.contains("text()=") || x.contains("not(")
}

/// Patch operations in one Patches file (every element whose `Class` is a PatchOperation,
/// nested ones included; containers such as Sequence, FindMod and Conditional are not
/// operations themselves), and how many of them carry a heavy XPath.
pub fn count_patch_ops(xml: &str) -> (u32, u32) {
    let Ok(doc) = roxmltree::Document::parse(xml) else { return count_patch_ops_loosely(xml) };
    let mut ops = 0u32;
    let mut heavy = 0u32;
    for n in doc.descendants().filter(|n| n.is_element()) {
        let Some(class) = n.attribute("Class") else { continue };
        let name = class.trim();
        if !name.starts_with("PatchOperation") && !name.contains(".PatchOperation") && !name.contains("PatchOp") {
            continue;
        }
        let short = name.rsplit('.').next().unwrap_or(name);
        if matches!(short, "PatchOperationSequence" | "PatchOperationFindMod" | "PatchOperationConditional" | "PatchOperationTest") {
            continue;
        }
        ops += 1;
        let xpath = n.children().find(|c| c.is_element() && c.tag_name().name().eq_ignore_ascii_case("xpath")).and_then(|c| c.text()).unwrap_or("");
        if xpath_is_heavy(xpath) {
            heavy += 1;
        }
    }
    (ops, heavy)
}

/// For a file the parser rejects: count `Class="PatchOperation…"` and heavy `<xpath>` texts by eye.
fn count_patch_ops_loosely(xml: &str) -> (u32, u32) {
    let ops = xml.matches("Class=\"PatchOperation").count().saturating_sub(xml.matches("PatchOperationSequence").count() + xml.matches("PatchOperationFindMod").count() + xml.matches("PatchOperationConditional").count()) as u32;
    let heavy = xml.split("<xpath>").skip(1).filter_map(|rest| rest.split("</xpath>").next()).filter(|x| xpath_is_heavy(x)).count() as u32;
    (ops, heavy.min(ops))
}

/// Width × height of a PNG (from IHDR) or JPEG (from its first SOF marker) without decoding
/// it. None for anything else, or a file too short to say.
pub fn image_pixels(path: &Path) -> Option<u64> {
    use std::io::Read;
    let mut f = std::fs::File::open(path).ok()?;
    let mut head = [0u8; 32];
    let n = f.read(&mut head).ok()?;
    if n >= 24 && head.starts_with(b"\x89PNG\r\n\x1a\n") && &head[12..16] == b"IHDR" {
        let w = u32::from_be_bytes([head[16], head[17], head[18], head[19]]) as u64;
        let h = u32::from_be_bytes([head[20], head[21], head[22], head[23]]) as u64;
        return Some(w * h);
    }
    if n >= 4 && head[0] == 0xFF && head[1] == 0xD8 {
        // JPEG: walk the segments to the first start-of-frame.
        let mut buf = head[..n].to_vec();
        let mut more = vec![0u8; 64 * 1024];
        let m = f.read(&mut more).ok()?;
        buf.extend_from_slice(&more[..m]);
        let mut i = 2usize;
        while i + 9 < buf.len() {
            if buf[i] != 0xFF {
                i += 1;
                continue;
            }
            let marker = buf[i + 1];
            if marker == 0xFF {
                i += 1;
                continue;
            }
            if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
                let h = u16::from_be_bytes([buf[i + 5], buf[i + 6]]) as u64;
                let w = u16::from_be_bytes([buf[i + 7], buf[i + 8]]) as u64;
                return Some(w * h);
            }
            let len = u16::from_be_bytes([buf[i + 2], buf[i + 3]]) as usize;
            i += 2 + len.max(2);
        }
    }
    None
}

/// A guess for a texture whose header could not be read: PNG compresses art to about a
/// third of a byte per pixel.
pub fn pixels_from_bytes(bytes: u64) -> u64 {
    bytes * 3
}

/// The finished figures for a folder: fills `score_ms` from the counts already gathered.
pub fn finish(c: &mut Contents) {
    c.load.score_ms = score(c);
}

/// Human summary of where a mod's estimate comes from, one clause per part that matters.
pub fn explain(c: &Contents) -> Vec<String> {
    let l = &c.load;
    let mut out = Vec::new();
    let mb = |b: u64| format!("{:.1} MB", b as f64 / 1e6);
    if c.defs > 0 {
        out.push(format!("{} def file{} ({})", c.defs, if c.defs == 1 { "" } else { "s" }, mb(l.def_bytes)));
    }
    if l.patch_ops > 0 {
        out.push(format!("{} patch operation{}{}", l.patch_ops, if l.patch_ops == 1 { "" } else { "s" }, if l.heavy_ops > 0 { format!(", {} scanning the whole document", l.heavy_ops) } else { String::new() }));
    }
    if l.png_pixels > 0 {
        out.push(format!("{:.1} megapixels of PNG to decode ({})", l.png_pixels as f64 / 1e6, mb(l.png_bytes)));
    }
    if l.dds_bytes > 0 {
        out.push(format!("{} of DDS", mb(l.dds_bytes)));
    }
    if c.assemblies > 0 {
        out.push(format!("{} assembl{} ({})", c.assemblies, if c.assemblies == 1 { "y" } else { "ies" }, mb(l.dll_bytes)));
    }
    if l.sound_bytes > 0 {
        out.push(format!("{} of sound", mb(l.sound_bytes)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_operations_and_heavy_xpaths() {
        let xml = r#"<Patch>
  <Operation Class="PatchOperationAdd"><xpath>Defs/ThingDef[defName="Wall"]/comps</xpath><value><li/></value></Operation>
  <Operation Class="PatchOperationFindMod">
    <mods><li>Royalty</li></mods>
    <match Class="PatchOperationSequence">
      <operations>
        <li Class="PatchOperationReplace"><xpath>//ThingDef[defName="Bed"]/statBases</xpath><value/></li>
        <li Class="PatchOperationAddModExtension"><xpath>Defs/ThingDef[contains(defName,"Gun")]</xpath><value/></li>
        <li Class="XmlExtensions.PatchOperationSafeAdd"><xpath>/Defs/ThingDef[defName="Table"]</xpath><value/></li>
      </operations>
    </match>
  </Operation>
  <Operation Class="PatchOperationConditional"><xpath>Defs/*/comps</xpath><match Class="PatchOperationRemove"><xpath>Defs/ThingDef[defName="X"]</xpath></match></Operation>
</Patch>"#;
        assert_eq!(count_patch_ops(xml), (5, 2));
        assert!(xpath_is_heavy("//ThingDef"));
        assert!(xpath_is_heavy("Defs/*/comps"));
        assert!(!xpath_is_heavy("Defs/ThingDef[defName=\"Wall\"]/comps"));
        assert!(!xpath_is_heavy("/Defs/ThingDef[defName=\"Wall\"]"));
        // A broken file still gets a rough count.
        let broken = r#"<Patch><Operation Class="PatchOperationAdd"><xpath>//X</xpath></Operation><Operation Class="PatchOperationAdd"><xpath>Defs/A</xpath>"#;
        assert_eq!(count_patch_ops(broken), (2, 1));
    }

    #[test]
    fn reads_image_sizes_from_headers() {
        let tmp = tempfile::tempdir().unwrap();
        let png = tmp.path().join("a.png");
        let img = image::RgbaImage::from_pixel(64, 48, image::Rgba([1, 2, 3, 255]));
        img.save(&png).unwrap();
        assert_eq!(image_pixels(&png), Some(64 * 48));
        // A JPEG SOF0 by hand: FFD8, then an APP0 segment, then SOF0 with height 16, width 32.
        let mut jpg = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0x00, 0x00];
        jpg.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x10, 0x00, 0x20, 0x01, 0x01, 0x11, 0x00]);
        let jp = tmp.path().join("b.jpg");
        std::fs::write(&jp, jpg).unwrap();
        assert_eq!(image_pixels(&jp), Some(16 * 32));
        std::fs::write(tmp.path().join("c.png"), b"nope").unwrap();
        assert_eq!(image_pixels(&tmp.path().join("c.png")), None);
        assert_eq!(pixels_from_bytes(1000), 3000);
    }

    #[test]
    fn score_ranks_the_expensive_parts() {
        let mut xml_only = Contents { defs: 200, ..Default::default() };
        xml_only.load.def_bytes = 3 * 1024 * 1024;
        let mut patch_heavy = Contents { patches: 10, ..Default::default() };
        patch_heavy.load.patch_ops = 300;
        patch_heavy.load.heavy_ops = 100;
        let mut png_heavy = Contents { textures: 900, ..Default::default() };
        png_heavy.load.png_pixels = 900 * 512 * 512;
        let mut dds = png_heavy.clone();
        dds.load.png_pixels = 0;
        dds.load.dds_bytes = 900 * 512 * 512;
        let tiny = Contents { defs: 1, ..Default::default() };
        let s = |c: &Contents| score(c);
        assert!(s(&patch_heavy) > s(&xml_only), "a hundred document scans outweigh 3 MB of Defs: {} vs {}", s(&patch_heavy), s(&xml_only));
        assert!(s(&png_heavy) > 4 * s(&dds), "DDS takes most of the texture cost away: {} vs {}", s(&png_heavy), s(&dds));
        assert!(s(&tiny) < 5);
        finish(&mut xml_only);
        assert_eq!(xml_only.load.score_ms, s(&xml_only));
        let words = explain(&patch_heavy).join("; ");
        assert!(words.contains("300 patch operations, 100 scanning the whole document"), "{words}");
    }
}
