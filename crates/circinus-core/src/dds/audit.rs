//! DDS files Circinus did not write — an author's, another tool's — that the game will refuse,
//! and how to mend them without losing the original.
//!
//! Unity 2022 creates a compressed `Texture2D` only when both sides are multiples of 4; a file
//! that is not logs "Compressed TextureFormat … requires a texture size that is a multiple of 4"
//! and then "Failed to create texture because of invalid parameters", and the mod's art comes
//! up pink. A truncated or unreadable file fails the same way. The fix regenerates the texture
//! from the PNG when one is beside it, or from the DDS's own top level when not, through the
//! same encoder and validator as everything else Circinus writes; the original is kept next to
//! it as `.dds.circinus-orig`, and Revert puts it back.

use super::encode::{self, Format, Options, PARAMS_VERSION};
use super::job::{hash, not_textures, put_in_place, Entry, BACKUP_SUFFIX};
use super::validate::{self, DdsInfo};
use crate::fsx::{real_root, rel_str, walk};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Problem {
    /// Block-compressed with a side that is not a multiple of 4: Unity refuses to create it.
    NotMultipleOf4,
    /// Shorter than its header says.
    Truncated { expected: u64, actual: u64 },
    /// Not a DDS at all, or a header nothing could load.
    Unreadable { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// The DDS, relative to the mod folder, forward slashes.
    pub rel: String,
    pub width: u32,
    pub height: u32,
    /// Human name of the pixel format ("BC7", "DXT1", "DXGI 83"…).
    pub format: String,
    pub levels: u32,
    pub bytes: u64,
    pub problem: Problem,
    /// A PNG with the same stem sits beside it — the best source for a rebuild.
    pub has_png: bool,
    /// Whether `fix_one` can rebuild it: from the PNG, or from a BC1/BC3/BC7 top level.
    pub fixable: bool,
}

/// What a lenient header read yields — enough to judge a file, not to trust it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub format_name: String,
    pub format: Option<Format>,
    pub width: u32,
    pub height: u32,
    pub levels: u32,
    pub block_compressed: bool,
    pub data_offset: usize,
    /// Bytes the declared levels need, when the format's block size is known.
    pub expected_len: Option<u64>,
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(b[i..i + 4].try_into().unwrap())
}

/// Read what the header claims. Unlike `validate::parse` this accepts partial mip chains and
/// formats Circinus does not write, because the question here is "will Unity take it".
pub fn inspect(bytes: &[u8]) -> std::result::Result<Header, String> {
    if bytes.len() < 128 || &bytes[0..4] != b"DDS " {
        return Err("not a DDS file".into());
    }
    if u32_at(bytes, 4) != 124 {
        return Err("header size is not 124".into());
    }
    let flags = u32_at(bytes, 8);
    let height = u32_at(bytes, 12);
    let width = u32_at(bytes, 16);
    if width == 0 || height == 0 {
        return Err("zero-sized texture".into());
    }
    let levels = if flags & 0x20000 != 0 { u32_at(bytes, 28).max(1) } else { 1 };
    let pf_flags = u32_at(bytes, 80);
    let (format_name, format, block_bytes, data_offset): (String, Option<Format>, Option<u64>, usize) = if pf_flags & 0x4 != 0 {
        match &bytes[84..88] {
            b"DXT1" => ("DXT1".into(), Some(Format::Bc1), Some(8), 128),
            b"DXT3" | b"DXT2" => ("DXT3".into(), None, Some(16), 128),
            b"DXT5" | b"DXT4" => ("DXT5".into(), Some(Format::Bc3), Some(16), 128),
            b"ATI1" | b"BC4U" | b"BC4S" => ("BC4".into(), None, Some(8), 128),
            b"ATI2" | b"BC5U" | b"BC5S" => ("BC5".into(), None, Some(16), 128),
            b"DX10" => {
                if bytes.len() < 148 {
                    return Err("DX10 header missing".into());
                }
                let dxgi = u32_at(bytes, 128);
                let (name, fmt, bb): (String, Option<Format>, Option<u64>) = match dxgi {
                    70..=72 => ("BC1".into(), Some(Format::Bc1), Some(8)),
                    73..=75 => ("BC2".into(), None, Some(16)),
                    76..=78 => ("BC3".into(), Some(Format::Bc3), Some(16)),
                    79..=81 => ("BC4".into(), None, Some(8)),
                    82..=84 => ("BC5".into(), None, Some(16)),
                    94..=96 => ("BC6H".into(), None, Some(16)),
                    97..=99 => ("BC7".into(), Some(Format::Bc7), Some(16)),
                    28 | 87 => ("RGBA8".into(), None, None),
                    other => (format!("DXGI {other}"), None, None),
                };
                (name, fmt, bb, 148)
            }
            other => (String::from_utf8_lossy(other).trim_end_matches('\0').to_string(), None, None, 128),
        }
    } else if pf_flags & 0x40 != 0 {
        (format!("RGB{}", u32_at(bytes, 88)), None, None, 128)
    } else {
        ("unknown".into(), None, None, 128)
    };
    let block_compressed = block_bytes.is_some();
    let expected_len = block_bytes.map(|bb| {
        let mut total = data_offset as u64;
        let (mut w, mut h) = (width, height);
        for _ in 0..levels {
            total += (w.div_ceil(4) as u64) * (h.div_ceil(4) as u64) * bb;
            w = (w / 2).max(1);
            h = (h / 2).max(1);
        }
        total
    });
    Ok(Header { format_name, format, width, height, levels, block_compressed, data_offset, expected_len })
}

/// Judge one file's bytes. None when Unity would take it.
pub fn judge(bytes: &[u8]) -> (Option<Header>, Option<Problem>) {
    match inspect(bytes) {
        Err(reason) => (None, Some(Problem::Unreadable { reason })),
        Ok(h) => {
            let problem = if let Some(expected) = h.expected_len.filter(|e| (bytes.len() as u64) < *e) {
                Some(Problem::Truncated { expected, actual: bytes.len() as u64 })
            } else if h.block_compressed && (h.width % 4 != 0 || h.height % 4 != 0) {
                Some(Problem::NotMultipleOf4)
            } else {
                None
            };
            (Some(h), problem)
        }
    }
}

/// Every DDS under a `Textures` directory that Circinus did not write, judged.
/// `ours` holds the DDS paths (relative, forward slashes) the manifest accounts for.
pub fn audit(mod_root: &Path, ours: &HashSet<String>) -> Vec<Finding> {
    let mut out = Vec::new();
    for entry in walk(mod_root, &not_textures) {
        let md = &entry.meta;
        if !md.is_file() {
            continue;
        }
        let path = entry.path.as_path();
        if !path.extension().map(|e| e.eq_ignore_ascii_case("dds")).unwrap_or(false) {
            continue;
        }
        let segs: Vec<String> = entry.rel.iter().map(|s| s.to_string_lossy().to_string()).collect();
        if segs.len() < 2 || !segs[..segs.len() - 1].iter().any(|s| s.eq_ignore_ascii_case("textures")) {
            continue;
        }
        let rel = rel_str(&entry.rel);
        if ours.contains(&rel) {
            continue;
        }
        // The header is all that is needed; whole files only when they are small anyway.
        let bytes = match read_head(path, md.len()) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let (header, problem) = judge_with_len(&bytes, md.len());
        let Some(problem) = problem else { continue };
        let has_png = path.with_extension("png").is_file();
        let h = header.unwrap_or(Header { format_name: "?".into(), format: None, width: 0, height: 0, levels: 0, block_compressed: false, data_offset: 0, expected_len: None });
        let fixable = has_png || (h.format.is_some() && !matches!(problem, Problem::Truncated { .. } | Problem::Unreadable { .. }));
        out.push(Finding { rel, width: h.width, height: h.height, format: h.format_name, levels: h.levels, bytes: md.len(), problem, has_png, fixable });
    }
    out.sort_by(|a, b| a.rel.cmp(&b.rel));
    out
}

fn read_head(path: &Path, len: u64) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut buf = vec![0u8; len.min(256) as usize];
    f.read_exact(&mut buf)?;
    Ok(buf)
}

/// `judge` for a header read from a longer file: the length check uses the file's size.
fn judge_with_len(head: &[u8], file_len: u64) -> (Option<Header>, Option<Problem>) {
    match inspect(head) {
        Err(reason) => (None, Some(Problem::Unreadable { reason })),
        Ok(h) => {
            let problem = if let Some(expected) = h.expected_len.filter(|e| file_len < *e) {
                Some(Problem::Truncated { expected, actual: file_len })
            } else if h.block_compressed && (h.width % 4 != 0 || h.height % 4 != 0) {
                Some(Problem::NotMultipleOf4)
            } else {
                None
            };
            (Some(h), problem)
        }
    }
}

/// Rebuild one flagged file. The original is renamed to `<file>.dds.circinus-orig` (an older
/// backup is kept over a newer one), the new file goes in its place, and the manifest entry
/// records both so revalidation and Revert know what happened.
pub fn fix_one(mod_root: &Path, f: &Finding, opts: &Options, now: i64) -> Result<Entry> {
    let dds = real_root(mod_root).join(&f.rel);
    let png = dds.with_extension("png");
    let original = std::fs::read(&dds)?;
    let (img, rel, src_len, src_mtime, src_hash) = if png.is_file() {
        let bytes = std::fs::read(&png)?;
        let md = std::fs::metadata(&png)?;
        let mtime = md.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
        let png_rel = f.rel[..f.rel.len() - 4].to_string() + &png.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        (encode::decode_png(&bytes)?, png_rel, md.len(), mtime, hash(&bytes))
    } else {
        let h = inspect(&original).map_err(Error::Other)?;
        let format = h.format.ok_or_else(|| Error::Other(format!("{} cannot be decoded by Circinus; no PNG beside it", h.format_name)))?;
        let info = DdsInfo { format, width: h.width, height: h.height, levels: h.levels, data_offset: h.data_offset };
        let top = h.data_offset as u64 + (h.width.div_ceil(4) as u64) * (h.height.div_ceil(4) as u64) * format.block_bytes() as u64;
        if (original.len() as u64) < top {
            return Err(Error::Other("top level is incomplete; no PNG beside it".into()));
        }
        (validate::decode_top(&original, &info), f.rel.clone(), original.len() as u64, 0, hash(&original))
    };
    let encoded = encode::encode_rgba(img, opts)?;
    validate::check(&encoded).map_err(Error::Other)?;
    let backup = backup_path(&dds);
    if !backup.exists() {
        std::fs::rename(&dds, &backup)?;
    } else {
        std::fs::remove_file(&dds)?;
    }
    if let Err(e) = put_in_place(&dds, &encoded.bytes) {
        // Put the original back rather than leave the mod without the texture.
        if !dds.exists() {
            let _ = std::fs::rename(&backup, &dds);
        }
        return Err(e);
    }
    Ok(Entry {
        rel,
        src_len,
        src_mtime,
        src_hash,
        dds_len: encoded.bytes.len() as u64,
        dds_hash: hash(&encoded.bytes),
        format: encoded.format,
        params: PARAMS_VERSION,
        created_at: now,
        width: encoded.width,
        height: encoded.height,
        replaced: Some(format!("{}{}", f.rel, BACKUP_SUFFIX)),
    })
}

pub fn backup_path(dds: &Path) -> PathBuf {
    let mut s = dds.as_os_str().to_owned();
    s.push(BACKUP_SUFFIX);
    PathBuf::from(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::encode::header;
    use image::{Rgba, RgbaImage};

    fn png(w: u32, h: u32) -> Vec<u8> {
        let img = RgbaImage::from_fn(w, h, |x, y| Rgba([(x * 9) as u8, (y * 5) as u8, 90, 255]));
        let mut out = Vec::new();
        image::write_buffer_with_format(&mut std::io::Cursor::new(&mut out), img.as_raw(), w, h, image::ColorType::Rgba8, image::ImageFormat::Png).unwrap();
        out
    }

    /// A DXT1 file whose header claims `w`×`h` with one level, data zeroed.
    fn dxt1(w: u32, h: u32) -> Vec<u8> {
        let mut bytes = header(Format::Bc1, w, h, 1);
        bytes.extend(std::iter::repeat(0u8).take((w.div_ceil(4) * h.div_ceil(4) * 8) as usize));
        bytes
    }

    #[test]
    fn judges_headers_the_way_unity_does() {
        assert_eq!(judge(&dxt1(64, 32)).1, None);
        assert_eq!(judge(&dxt1(66, 32)).1, Some(Problem::NotMultipleOf4));
        let mut short = dxt1(64, 64);
        short.truncate(short.len() - 100);
        assert!(matches!(judge(&short).1, Some(Problem::Truncated { .. })));
        assert!(matches!(judge(b"PNG\r\n not dds at all, long enough to pass the length check ........................................................................").1, Some(Problem::Unreadable { .. })));
        // Formats Circinus does not write are still judged, not rejected.
        let mut bc5 = dxt1(64, 64);
        bc5[84..88].copy_from_slice(b"ATI2");
        bc5.extend(std::iter::repeat(0u8).take(16 * 16 * 8));
        let (h, p) = judge(&bc5);
        assert_eq!(h.unwrap().format_name, "BC5");
        assert_eq!(p, None);
    }

    #[test]
    fn audit_finds_and_fixes_foreign_files_and_keeps_the_original() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let w = |p: &str, b: &[u8]| {
            let p = root.join(p);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b).unwrap();
        };
        w("About/About.xml", b"<ModMetaData/>");
        w("Textures/Things/Good.dds", &dxt1(32, 32));
        w("Textures/Things/Odd.dds", &dxt1(30, 20));
        w("Textures/Things/Odd.png", &png(30, 20));
        w("Textures/UI/Lone.dds", &dxt1(18, 18));
        w("Textures/UI/Ours.dds", &dxt1(18, 18));
        w("Textures/UI/Ours.png", &png(18, 18));
        w("Source/Old.dds", &dxt1(6, 6));
        let ours: HashSet<String> = ["Textures/UI/Ours.dds".to_string()].into_iter().collect();
        let found = audit(root, &ours);
        let rels: Vec<&str> = found.iter().map(|f| f.rel.as_str()).collect();
        assert_eq!(rels, vec!["Textures/Things/Odd.dds", "Textures/UI/Lone.dds"]);
        assert!(found[0].has_png && found[0].fixable);
        assert!(!found[1].has_png && found[1].fixable, "a decodable DXT1 top level is a source too");
        let opts = Options::default();
        for f in &found {
            let e = fix_one(root, f, &opts, 7).unwrap();
            assert!(e.replaced.as_deref().unwrap().ends_with(BACKUP_SUFFIX));
            let fixed = std::fs::read(root.join(&f.rel)).unwrap();
            let info = validate::parse(&fixed).unwrap();
            assert_eq!(info.width % 4, 0);
            assert_eq!(info.height % 4, 0);
            assert!(root.join(e.replaced.as_ref().unwrap()).is_file(), "original kept as backup");
        }
        assert_eq!(found[0].width, 30);
        assert!(audit(root, &ours).is_empty() || audit(root, &ours).iter().all(|f| f.rel == "Textures/UI/Ours.dds"), "fixed files pass");
        // Revert restores the originals from the backups.
        let entries: Vec<Entry> = found.iter().map(|f| fix_entry_for(root, f)).collect();
        let r = super::super::job::revert(root, &entries);
        assert_eq!(r.restored.len(), 2, "{r:?}");
        assert_eq!(std::fs::read(root.join("Textures/Things/Odd.dds")).unwrap(), dxt1(30, 20));
        assert!(!backup_path(&root.join("Textures/Things/Odd.dds")).exists());
    }

    /// The entry a fix produced, re-read from the files (what the manifest would hold).
    fn fix_entry_for(root: &Path, f: &Finding) -> Entry {
        let dds = std::fs::read(root.join(&f.rel)).unwrap();
        let info = validate::parse(&dds).unwrap();
        let png = root.join(&f.rel).with_extension("png");
        let rel = if png.is_file() { f.rel.replace(".dds", ".png") } else { f.rel.clone() };
        Entry { rel, src_len: 0, src_mtime: 0, src_hash: 0, dds_len: dds.len() as u64, dds_hash: hash(&dds), format: info.format, params: PARAMS_VERSION, created_at: 7, width: info.width, height: info.height, replaced: Some(format!("{}{}", f.rel, BACKUP_SUFFIX)) }
    }
}
