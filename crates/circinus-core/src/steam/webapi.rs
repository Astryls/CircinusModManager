//! Steam Web API, the parts that work without an API key.

use crate::model::RIMWORLD_APP_ID;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

const DETAILS_URL: &str = "https://api.steampowered.com/ISteamRemoteStorage/GetPublishedFileDetails/v1/";
const COLLECTION_URL: &str = "https://api.steampowered.com/ISteamRemoteStorage/GetCollectionDetails/v1/";
/// The endpoint accepts more, but 300 keeps responses small and retries cheap.
pub const DETAILS_CHUNK: usize = 300;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkshopItem {
    pub published_file_id: u64,
    pub title: String,
    pub creator: String,
    pub time_created: u64,
    pub time_updated: u64,
    pub file_size: u64,
    pub preview_url: Option<String>,
    pub tags: Vec<String>,
    pub consumer_app_id: u32,
    /// 1 = ok; anything else means the item is hidden, removed or not a workshop item.
    pub result: i64,
    /// 0 = item, 2 = collection.
    pub file_type: i64,
}

impl WorkshopItem {
    pub fn is_rimworld_mod(&self) -> bool {
        self.result == 1 && self.consumer_app_id == RIMWORLD_APP_ID && self.file_type == 0
    }
    pub fn url(&self) -> String {
        format!("https://steamcommunity.com/sharedfiles/filedetails/?id={}", self.published_file_id)
    }
}

fn num(v: Option<&Value>) -> u64 {
    match v {
        Some(Value::Number(n)) => n.as_u64().unwrap_or(0),
        Some(Value::String(s)) => s.parse().unwrap_or(0),
        _ => 0,
    }
}

/// Parse one `publishedfiledetails` entry.
pub fn parse_item(v: &Value) -> Option<WorkshopItem> {
    let id = num(v.get("publishedfileid"));
    if id == 0 {
        return None;
    }
    Some(WorkshopItem {
        published_file_id: id,
        title: v.get("title").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        creator: v.get("creator").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        time_created: num(v.get("time_created")),
        time_updated: num(v.get("time_updated")),
        file_size: num(v.get("file_size")),
        preview_url: v.get("preview_url").and_then(|x| x.as_str()).map(|s| s.to_string()),
        tags: v.get("tags").and_then(|t| t.as_array()).map(|a| a.iter().filter_map(|t| t.get("tag").and_then(|x| x.as_str()).map(|s| s.to_string())).collect()).unwrap_or_default(),
        consumer_app_id: num(v.get("consumer_app_id")) as u32,
        result: v.get("result").and_then(|x| x.as_i64()).unwrap_or(0),
        file_type: v.get("file_type").and_then(|x| x.as_i64()).unwrap_or(0),
    })
}

/// Parse a full GetPublishedFileDetails response.
pub fn parse_details(v: &Value) -> Vec<WorkshopItem> {
    v.pointer("/response/publishedfiledetails").and_then(|a| a.as_array()).map(|a| a.iter().filter_map(parse_item).collect()).unwrap_or_default()
}

/// Parse a GetCollectionDetails response into the child ids (items only, not sub-collections).
pub fn parse_collection(v: &Value) -> (Vec<u64>, Vec<u64>) {
    let mut items = Vec::new();
    let mut subcollections = Vec::new();
    if let Some(details) = v.pointer("/response/collectiondetails").and_then(|a| a.as_array()) {
        for coll in details {
            if let Some(children) = coll.get("children").and_then(|c| c.as_array()) {
                for child in children {
                    let id = num(child.get("publishedfileid"));
                    if id == 0 {
                        continue;
                    }
                    match child.get("filetype").and_then(|x| x.as_i64()).unwrap_or(0) {
                        0 => items.push(id),
                        2 => subcollections.push(id),
                        _ => {}
                    }
                }
            }
        }
    }
    (items, subcollections)
}

/// Workshop id from a URL, a `steam://` link, or a bare number.
pub fn parse_workshop_id(text: &str) -> Option<u64> {
    let t = text.trim();
    if let Ok(n) = t.parse::<u64>() {
        return Some(n);
    }
    let lower = t.to_ascii_lowercase();
    if lower.contains("steamcommunity.com") || lower.starts_with("steam://") {
        if let Some(pos) = lower.find("id=") {
            let digits: String = t[pos + 3..].chars().take_while(|c| c.is_ascii_digit()).collect();
            return digits.parse().ok();
        }
        if let Some(rest) = lower.strip_prefix("steam://url/communityfilepage/") {
            let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
            return digits.parse().ok();
        }
    }
    None
}

/// Every workshop id found in free text (one per line, URLs, `[id]`, etc.).
pub fn extract_workshop_ids(text: &str) -> Vec<u64> {
    let mut out = Vec::new();
    let re = regex::Regex::new(r"(?:[?&]id=|CommunityFilePage/)(\d{6,})").unwrap();
    for c in re.captures_iter(text) {
        if let Ok(n) = c[1].parse::<u64>() {
            if !out.contains(&n) {
                out.push(n);
            }
        }
    }
    for line in text.lines() {
        if let Ok(n) = line.trim().parse::<u64>() {
            if n > 100_000 && !out.contains(&n) {
                out.push(n);
            }
        }
    }
    out
}

async fn post_form(client: &reqwest::Client, url: &str, form: &[(String, String)]) -> Result<Value> {
    let mut delay = Duration::from_secs(1);
    let mut last_err: Option<Error> = None;
    for attempt in 0..4 {
        let resp = client.post(url).form(form).send().await;
        match resp {
            Ok(r) if r.status().is_success() => return Ok(r.json::<Value>().await?),
            Ok(r) if r.status() == reqwest::StatusCode::TOO_MANY_REQUESTS || r.status().is_server_error() => {
                let retry_after = r.headers().get(reqwest::header::RETRY_AFTER).and_then(|v| v.to_str().ok()).and_then(|s| s.parse::<u64>().ok()).map(Duration::from_secs);
                last_err = Some(Error::Other(format!("Steam answered {}", r.status())));
                if attempt < 3 {
                    tokio::time::sleep(retry_after.unwrap_or(delay)).await;
                    delay *= 2;
                    continue;
                }
            }
            Ok(r) => return Err(Error::Other(format!("Steam answered {}", r.status()))),
            Err(e) => {
                last_err = Some(Error::Net(e));
                if attempt < 3 {
                    tokio::time::sleep(delay).await;
                    delay *= 2;
                    continue;
                }
            }
        }
    }
    Err(last_err.unwrap_or_else(|| Error::Other("Steam did not answer".into())))
}

/// Details for many items, in chunks. Unknown ids are simply absent from the result.
pub async fn published_file_details(client: &reqwest::Client, ids: &[u64]) -> Result<Vec<WorkshopItem>> {
    let mut out = Vec::new();
    for chunk in ids.chunks(DETAILS_CHUNK) {
        let mut form: Vec<(String, String)> = vec![("itemcount".into(), chunk.len().to_string())];
        for (i, id) in chunk.iter().enumerate() {
            form.push((format!("publishedfileids[{i}]"), id.to_string()));
        }
        let v = post_form(client, DETAILS_URL, &form).await?;
        out.extend(parse_details(&v));
    }
    Ok(out)
}

/// Expand a collection (recursively, one level of sub-collections) into item ids.
pub async fn collection_items(client: &reqwest::Client, collection_id: u64) -> Result<Vec<u64>> {
    let form = vec![("collectioncount".to_string(), "1".to_string()), ("publishedfileids[0]".to_string(), collection_id.to_string())];
    let v = post_form(client, COLLECTION_URL, &form).await?;
    let (mut items, subs) = parse_collection(&v);
    for sub in subs.into_iter().take(20) {
        let form = vec![("collectioncount".to_string(), "1".to_string()), ("publishedfileids[0]".to_string(), sub.to_string())];
        if let Ok(v) = post_form(client, COLLECTION_URL, &form).await {
            let (more, _) = parse_collection(&v);
            for id in more {
                if !items.contains(&id) {
                    items.push(id);
                }
            }
        }
    }
    if items.is_empty() {
        return Err(Error::Other("That collection is empty, private, or not a collection".into()));
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ids_from_many_forms() {
        assert_eq!(parse_workshop_id("2009463077"), Some(2009463077));
        assert_eq!(parse_workshop_id("https://steamcommunity.com/sharedfiles/filedetails/?id=2009463077&searchtext=x"), Some(2009463077));
        assert_eq!(parse_workshop_id("https://steamcommunity.com/workshop/filedetails/?id=818773962"), Some(818773962));
        assert_eq!(parse_workshop_id("steam://url/CommunityFilePage/2009463077"), Some(2009463077));
        assert_eq!(parse_workshop_id("not an id"), None);
        let ids = extract_workshop_ids("Harmony https://steamcommunity.com/sharedfiles/filedetails/?id=2009463077\n818773962\nsteam://url/CommunityFilePage/1541460369");
        assert_eq!(ids, vec![2009463077, 1541460369, 818773962]);
    }

    #[test]
    fn parses_responses() {
        let v: Value = serde_json::from_str(r#"{"response":{"result":1,"resultcount":2,"publishedfiledetails":[
            {"publishedfileid":"2009463077","result":1,"creator":"123","consumer_app_id":294100,"file_size":"123456","title":"Harmony","time_created":1580000000,"time_updated":1700000000,"preview_url":"https://x/p.png","tags":[{"tag":"Mod"},{"tag":"1.6"}]},
            {"publishedfileid":"1","result":9}]}}"#).unwrap();
        let items = parse_details(&v);
        assert_eq!(items.len(), 2);
        assert!(items[0].is_rimworld_mod());
        assert_eq!(items[0].tags, vec!["Mod", "1.6"]);
        assert!(!items[1].is_rimworld_mod());
        let c: Value = serde_json::from_str(r#"{"response":{"result":1,"resultcount":1,"collectiondetails":[{"publishedfileid":"5","result":1,"children":[{"publishedfileid":"10","sortorder":0,"filetype":0},{"publishedfileid":"11","sortorder":1,"filetype":2}]}]}}"#).unwrap();
        assert_eq!(parse_collection(&c), (vec![10], vec![11]));
    }
}
