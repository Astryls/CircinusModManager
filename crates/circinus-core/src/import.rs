//! Import an active-mod list from the formats people actually have lying around:
//! ModsConfig.xml, `.rml` mod lists, `.rws` saves (plain, gzip or zstd), RimSort/RimPy JSON,
//! and pasted text (one id per line, or `Name [package.id][url]` clipboard exports).

use crate::modsconfig;
use crate::xmlutil::{child, li_texts, with_document};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedList {
    pub package_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_version: Option<String>,
    /// Human description of what was parsed ("RimWorld save", "ModsConfig.xml", …).
    pub format: String,
}

const MAX_HEAD: usize = 8 * 1024 * 1024;

/// Read the first few MB of a possibly compressed file; the mod list lives in the header.
fn read_head(path: &Path) -> Result<String> {
    let mut f = std::fs::File::open(path)?;
    let mut magic = [0u8; 4];
    let n = f.read(&mut magic)?;
    let mut f = std::fs::File::open(path)?;
    let mut buf = Vec::with_capacity(MAX_HEAD.min(1 << 20));
    if n >= 2 && magic[0] == 0x1f && magic[1] == 0x8b {
        let mut dec = flate2::read::GzDecoder::new(&mut f);
        dec.by_ref().take(MAX_HEAD as u64).read_to_end(&mut buf)?;
    } else if n >= 4 && magic == [0x28, 0xb5, 0x2f, 0xfd] {
        let mut dec = zstd::stream::read::Decoder::new(&mut f).map_err(|e| Error::Other(e.to_string()))?;
        dec.by_ref().take(MAX_HEAD as u64).read_to_end(&mut buf)?;
    } else {
        f.by_ref().take(MAX_HEAD as u64).read_to_end(&mut buf)?;
    }
    Ok(String::from_utf8_lossy(&buf).trim_start_matches('\u{feff}').to_string())
}

fn ids_between(text: &str, open: &str, close: &str) -> Option<Vec<String>> {
    let start = text.find(open)?;
    let rest = &text[start + open.len()..];
    let end = rest.find(close)?;
    let block = &rest[..end];
    let mut ids = Vec::new();
    let mut cursor = block;
    while let Some(i) = cursor.find("<li>") {
        let after = &cursor[i + 4..];
        let Some(j) = after.find("</li>") else { break };
        let id = after[..j].trim().to_ascii_lowercase();
        if !id.is_empty() {
            ids.push(id);
        }
        cursor = &after[j + 5..];
    }
    Some(ids)
}

fn text_between(text: &str, open: &str, close: &str) -> Option<String> {
    let start = text.find(open)?;
    let rest = &text[start + open.len()..];
    let end = rest.find(close)?;
    Some(rest[..end].trim().to_string())
}

/// Import from a file, choosing the parser by content rather than extension.
pub fn import_file(path: &Path) -> Result<ImportedList> {
    let head = read_head(path)?;
    import_text(&head, path)
}

pub fn import_text(text: &str, path: &Path) -> Result<ImportedList> {
    let t = text.trim_start();
    if t.starts_with('{') || t.starts_with('[') {
        return import_json(t);
    }
    if t.starts_with('<') {
        // Saves are huge and only their <meta> block matters: scan rather than parse.
        if t.contains("<savegame") || t.contains("<savedModList") {
            let ids = ids_between(t, "<modIds>", "</modIds>").ok_or_else(|| Error::Other("No <modIds> block found".into()))?;
            let version = text_between(t, "<gameVersion>", "</gameVersion>");
            let format = if t.contains("<savegame") { "RimWorld save" } else { "Mod list (.rml)" };
            return Ok(ImportedList { package_ids: ids, game_version: version, format: format.into() });
        }
        if t.contains("<ModsConfigData") {
            let cfg = modsconfig::parse(t, path)?;
            return Ok(ImportedList { package_ids: cfg.active_mods, game_version: Some(cfg.version).filter(|v| !v.is_empty()), format: "ModsConfig.xml".into() });
        }
        // Any other XML with an activeMods or modIds list
        return with_document(t, path, |doc| {
            let root = doc.root_element();
            let node = child(root, "activeMods").or_else(|| child(root, "modIds")).or_else(|| child(root, "mods"));
            match node {
                Some(n) => Ok(ImportedList { package_ids: li_texts(n).into_iter().map(|s| s.to_ascii_lowercase()).collect(), game_version: None, format: "XML mod list".into() }),
                None => Err(Error::Other("XML file has no activeMods or modIds list".into())),
            }
        });
    }
    import_plain(text)
}

fn import_json(t: &str) -> Result<ImportedList> {
    let v: serde_json::Value = serde_json::from_str(t)?;
    let arr = if v.is_array() {
        v.clone()
    } else {
        v.get("activeMods").cloned().or_else(|| v.get("mods").cloned()).unwrap_or(serde_json::Value::Null)
    };
    let items: Vec<serde_json::Value> = match arr {
        serde_json::Value::Array(a) => a,
        serde_json::Value::Object(o) => o.get("li").and_then(|l| l.as_array().cloned()).unwrap_or_default(),
        _ => return Err(Error::Other("JSON has no activeMods array".into())),
    };
    let ids = items
        .into_iter()
        .filter_map(|x| match x {
            serde_json::Value::String(s) => Some(s),
            serde_json::Value::Object(o) => o.get("packageId").and_then(|p| p.as_str()).map(|s| s.to_string()),
            _ => None,
        })
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    Ok(ImportedList { package_ids: ids, game_version: v.get("version").and_then(|x| x.as_str()).map(|s| s.to_string()), format: "JSON mod list".into() })
}

/// Plain text: `[package.id]` brackets win when present; otherwise one id per line.
fn import_plain(text: &str) -> Result<ImportedList> {
    let re = regex::Regex::new(r"\[([A-Za-z0-9][A-Za-z0-9._\-]*\.[A-Za-z0-9._\-]+)\]").unwrap();
    let mut ids: Vec<String> = re.captures_iter(text).map(|c| c[1].to_ascii_lowercase()).collect();
    if ids.is_empty() {
        ids = text
            .lines()
            .map(|l| l.trim().trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ')' || c == '-').trim())
            .filter(|l| !l.is_empty() && !l.starts_with('#') && l.contains('.') && !l.contains(' '))
            .map(|l| l.to_ascii_lowercase())
            .collect();
    }
    if ids.is_empty() {
        return Err(Error::Other("No package ids found in the text".into()));
    }
    Ok(ImportedList { package_ids: ids, game_version: None, format: "Text list".into() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn save_header_plain_and_gzip() {
        let save = "<?xml version=\"1.0\"?><savegame><meta><gameVersion>1.6.4530 rev1235</gameVersion><modIds><li>ludeon.rimworld</li><li>brrainz.harmony</li></modIds><modSteamIds><li>0</li></modSteamIds></meta><game>...</game></savegame>";
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.rws");
        std::fs::write(&p, save).unwrap();
        let l = import_file(&p).unwrap();
        assert_eq!(l.package_ids, vec!["ludeon.rimworld", "brrainz.harmony"]);
        assert_eq!(l.game_version.as_deref(), Some("1.6.4530 rev1235"));
        let gz = dir.path().join("b.rws");
        let mut enc = flate2::write::GzEncoder::new(std::fs::File::create(&gz).unwrap(), flate2::Compression::default());
        enc.write_all(save.as_bytes()).unwrap();
        enc.finish().unwrap();
        assert_eq!(import_file(&gz).unwrap().package_ids.len(), 2);
    }

    #[test]
    fn json_and_text() {
        let j = import_text(r#"{"version":"1.6","activeMods":["Ludeon.RimWorld","a.b"]}"#, Path::new("x.json")).unwrap();
        assert_eq!(j.package_ids, vec!["ludeon.rimworld", "a.b"]);
        let j2 = import_text(r#"{"activeMods":{"li":["a.b"]}}"#, Path::new("x.json")).unwrap();
        assert_eq!(j2.package_ids, vec!["a.b"]);
        let t = import_text("Created with RimSort\nHarmony [brrainz.harmony][https://x]\nHugsLib [UnlimitedHugs.HugsLib][No url specified]", Path::new("x.txt")).unwrap();
        assert_eq!(t.package_ids, vec!["brrainz.harmony", "unlimitedhugs.hugslib"]);
        let lines = import_text("brrainz.harmony\n# comment\nunlimitedhugs.hugslib\n", Path::new("x.txt")).unwrap();
        assert_eq!(lines.package_ids.len(), 2);
    }
}
