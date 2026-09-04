//! Rentry.co mod lists: fetch a paste and pull package ids / workshop ids out of it.

use crate::import::{import_text, ImportedList};
use crate::steam::webapi::extract_workshop_ids;
use crate::{Error, Result};
use std::path::Path;

/// `https://rentry.co/abc123`, `https://rentry.org/abc123/raw`, or a bare paste id.
pub fn parse_rentry_id(text: &str) -> Option<String> {
    let t = text.trim();
    let lower = t.to_ascii_lowercase();
    if lower.contains("rentry.co/") || lower.contains("rentry.org/") {
        let after = t.splitn(2, "rentry.").nth(1)?;
        let path = after.splitn(2, '/').nth(1)?;
        let id: String = path.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-').collect();
        return (!id.is_empty()).then_some(id);
    }
    if !t.is_empty() && t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') && !t.contains('.') {
        return Some(t.to_string());
    }
    None
}

/// Fetch the raw text of a paste. Tries the JSON API first, then the raw page.
pub async fn fetch_rentry(client: &reqwest::Client, id: &str, auth: Option<&str>) -> Result<String> {
    let api = format!("https://rentry.co/api/raw/{id}");
    if let Ok(resp) = client.get(&api).send().await {
        if resp.status().is_success() {
            if let Ok(v) = resp.json::<serde_json::Value>().await {
                if let Some(content) = v.get("content").and_then(|c| c.as_str()) {
                    return Ok(content.to_string());
                }
            }
        }
    }
    let raw = format!("https://rentry.co/{id}/raw");
    let mut req = client.get(&raw);
    if let Some(a) = auth {
        req = req.header("rentry-auth", a);
    }
    let resp = req.send().await?;
    if !resp.status().is_success() {
        return Err(Error::Other(format!("Rentry answered {} for {id}", resp.status())));
    }
    Ok(resp.text().await?)
}

/// Parse a Rentry paste: package ids from `[package.id]` brackets or bare lines, plus workshop
/// ids from any Steam links, so entries without a package id can still be downloaded.
pub fn parse_rentry_text(text: &str) -> (ImportedList, Vec<u64>) {
    let workshop_ids = extract_workshop_ids(text);
    let list = import_text(text, Path::new("rentry.txt")).unwrap_or_else(|_| ImportedList { package_ids: Vec::new(), game_version: None, format: "Rentry".into() });
    (ImportedList { format: "Rentry list".into(), ..list }, workshop_ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_from_urls() {
        assert_eq!(parse_rentry_id("https://rentry.co/abc123").as_deref(), Some("abc123"));
        assert_eq!(parse_rentry_id("https://rentry.org/ab_c-1/raw").as_deref(), Some("ab_c-1"));
        assert_eq!(parse_rentry_id("abc123").as_deref(), Some("abc123"));
        assert_eq!(parse_rentry_id("brrainz.harmony"), None);
    }

    #[test]
    fn parses_rimsort_style_paste() {
        let text = "Created with RimSort\n\nMod list was created for game version: `1.6`\n\n- Harmony [brrainz.harmony][https://steamcommunity.com/sharedfiles/filedetails/?id=2009463077]\n- HugsLib [unlimitedhugs.hugslib][https://steamcommunity.com/sharedfiles/filedetails/?id=818773962]\n";
        let (list, ids) = parse_rentry_text(text);
        assert_eq!(list.package_ids, vec!["brrainz.harmony", "unlimitedhugs.hugslib"]);
        assert_eq!(ids, vec![2009463077, 818773962]);
    }
}
