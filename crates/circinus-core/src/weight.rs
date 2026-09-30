//! Circinus weight: how much of the measured frame a mod costs, from circinus.sh
//! (public API, no key) and from local runs the Circinus profiler mod writes beside the saves.
//!
//! The site publishes cost as a *share of measured frame time* (median across clean runs),
//! bands it, and only ranks a mod past 25 clean runs and 10 independent installs.
//! `-1` anywhere means "not known" and is never treated as zero.

use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const API_BASE: &str = "https://circinus.sh/api/v1";
pub const USER_AGENT: &str = concat!("CircinusModManager/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Band {
    Negligible,
    Light,
    Moderate,
    Heavy,
    #[serde(rename = "veryheavy")]
    VeryHeavy,
    /// Measured, but below the ranking floors.
    Insufficient,
    Unknown,
}

impl Band {
    /// Absolute tiers of typical share (percent of frame), independent of mod count.
    pub fn for_share(share_pct: f64) -> Band {
        if share_pct < 0.5 {
            Band::Negligible
        } else if share_pct <= 2.0 {
            Band::Light
        } else if share_pct <= 5.0 {
            Band::Moderate
        } else if share_pct <= 15.0 {
            Band::Heavy
        } else {
            Band::VeryHeavy
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Band::Negligible => "Negligible",
            Band::Light => "Light",
            Band::Moderate => "Moderate",
            Band::Heavy => "Heavy",
            Band::VeryHeavy => "Very heavy",
            Band::Insufficient => "Not enough runs",
            Band::Unknown => "Not measured",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Weight {
    pub package_id: String,
    /// Median share of measured frame time, percent. None = not known.
    pub share: Option<f64>,
    pub band: Band,
    pub ranked: bool,
    /// Runs the mod was loaded in / profiled in / that produced rankable data.
    pub seen: Option<i64>,
    pub measured: Option<i64>,
    pub ranked_runs: Option<i64>,
    pub installs: Option<i64>,
    /// Net cost range for skip-capable patches: [gross − vanilla, gross].
    pub net_low: Option<f64>,
    pub net_high: Option<f64>,
    /// Author-restricted figures.
    pub withheld: bool,

    /// What this mod typically costs at *start-up*, in milliseconds: the median over the load
    /// runs the site has pooled, measured by Loading Progress on other people's machines.
    ///
    /// A different measurement of a different thing from `share`, and the two must never be
    /// added or compared. `share` is a fraction of frame time while the game is running;
    /// this is wall-clock milliseconds spent once, before anybody sees the main menu.
    ///
    /// None below the site's ranking floors, which it signals as `-1` and `num` drops for us
    /// -- the whole point of the convention is that a mod nobody has timed must never read as
    /// a mod that was timed and found to be free.
    pub load_ms_median: Option<f64>,
    /// How many pooled start-ups that median is over, and from how many separate installs.
    /// Shown beside the figure, because a median of three runs is a rumour.
    pub load_runs: Option<i64>,
    pub load_installs: Option<i64>,

    /// This machine's own share of frame time, from the Circinus profiler runs beside the
    /// saves. The same unit as `share` and measured the same way, but on one machine with one
    /// mod list, so it is a second reading of the same quantity rather than a substitute for
    /// the first.
    ///
    /// It lives beside `share` instead of replacing it because the interesting thing is the
    /// pair. A mod that costs everybody 0.1 % and costs you 2.7 % is a finding, and the old
    /// merge -- which dropped whichever of the two the other one already had -- made that
    /// comparison impossible to express, let alone to draw.
    #[serde(default)]
    pub local_share: Option<f64>,
    #[serde(default)]
    pub local_band: Option<Band>,
    /// How many of your own runs that median is over.
    #[serde(default)]
    pub local_runs: Option<i64>,

    /// Where `share` came from: "api", or "local" for a row that only your own runs produced.
    /// Kept for rows written before local figures had a column of their own.
    pub origin: String,
}

impl Weight {
    /// An empty row for `package_id`: known to exist, nothing measured about it yet. The
    /// starting point for a row that only local runs will fill.
    pub fn blank(package_id: String, origin: &str) -> Weight {
        Weight {
            package_id,
            share: None,
            band: Band::Unknown,
            ranked: false,
            seen: None,
            measured: None,
            ranked_runs: None,
            installs: None,
            net_low: None,
            net_high: None,
            withheld: false,
            load_ms_median: None,
            load_runs: None,
            load_installs: None,
            local_share: None,
            local_band: None,
            local_runs: None,
            origin: origin.into(),
        }
    }
}

fn num(v: Option<&Value>) -> Option<f64> {
    match v {
        Some(Value::Number(n)) => n.as_f64().filter(|x| *x >= 0.0),
        Some(Value::String(s)) => s.trim().trim_end_matches('%').parse::<f64>().ok().filter(|x| *x >= 0.0),
        _ => None,
    }
}

fn int(v: Option<&Value>) -> Option<i64> {
    num(v).map(|x| x.round() as i64)
}

fn first<'a>(obj: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|k| obj.get(*k)).filter(|v| !v.is_null())
}

/// Read a share of frame (percent) out of an API object. The API reports shares in percent
/// ("share = mod_ms / total_measured_frame_ms * 100"); a fraction ≤ 1 with a `fraction` hint is
/// scaled. Field names are tried in order so a schema tweak on the server only needs an edit here.
fn share_of(obj: &Value) -> Option<f64> {
    let direct = first(obj, &["share", "medianShare", "shareOfFrame", "frameShare", "median", "cost", "typicalShare", "sharePct", "percent"]);
    if let Some(v) = direct {
        if let Some(x) = num(Some(v)) {
            return Some(x);
        }
        if let Some(o) = v.as_object() {
            if let Some(x) = num(o.get("median").or_else(|| o.get("p50")).or_else(|| o.get("value")).or_else(|| o.get("share")).or_else(|| o.get("percent"))) {
                return Some(x);
            }
        }
    }
    // The schema may nest the figure ("stats.share.median", "cost.frameShare.p50", "summary.share_pct"):
    // look a few levels down for the first number under a share-like key.
    share_deep(obj, 0)
}

fn share_deep(v: &Value, depth: usize) -> Option<f64> {
    let obj = v.as_object()?;
    for (k, val) in obj {
        let key = k.to_ascii_lowercase();
        let share_like = key.contains("share") || key.contains("pct") || key == "percent";
        if share_like {
            if let Some(x) = num(Some(val)) {
                return Some(if key.contains("fraction") { x * 100.0 } else { x });
            }
            if let Some(o) = val.as_object() {
                for pick in ["median", "p50", "value", "typical", "mean"] {
                    if let Some(x) = num(o.get(pick)) {
                        return Some(x);
                    }
                }
            }
        }
    }
    if depth >= 3 {
        return None;
    }
    for (k, val) in obj {
        let key = k.to_ascii_lowercase();
        if val.is_object() && (key.contains("share") || key.contains("cost") || key.contains("frame") || key.contains("stat") || key.contains("summary") || key.contains("median") || key.contains("figure")) {
            if let Some(x) = share_deep(val, depth + 1) {
                return Some(x);
            }
        }
    }
    None
}

fn band_of(obj: &Value, share: Option<f64>, ranked: bool) -> Band {
    if let Some(b) = first(obj, &["band"]).and_then(|b| b.as_str()) {
        let b = b.to_ascii_lowercase().replace([' ', '_', '-'], "");
        return match b.as_str() {
            "negligible" => Band::Negligible,
            "light" => Band::Light,
            "moderate" => Band::Moderate,
            "heavy" => Band::Heavy,
            "veryheavy" => Band::VeryHeavy,
            "insufficient" | "unranked" | "unknown" => Band::Insufficient,
            _ => share.map(Band::for_share).unwrap_or(Band::Unknown),
        };
    }
    match share {
        Some(s) if ranked => Band::for_share(s),
        Some(_) => Band::Insufficient,
        None => Band::Unknown,
    }
}

/// Parse one mod object from `/api/v1/mods` or `/api/v1/mods/{packageId}`.
pub fn parse_mod(obj: &Value, origin: &str) -> Option<Weight> {
    let package_id = first(obj, &["packageId", "package_id", "id"])?.as_str()?.trim().to_ascii_lowercase();
    if package_id.is_empty() {
        return None;
    }
    let share = share_of(obj);
    let ranked = first(obj, &["ranked"]).and_then(|r| r.as_bool()).unwrap_or(false);
    let runs = obj.get("runs").filter(|r| r.is_object());
    let seen = runs.and_then(|r| int(r.get("seen"))).or_else(|| int(first(obj, &["seen", "runsSeen"])));
    let measured = runs.and_then(|r| int(r.get("measured"))).or_else(|| int(first(obj, &["measured", "runsMeasured", "runCount"])));
    let ranked_runs = runs.and_then(|r| int(r.get("ranked"))).or_else(|| int(first(obj, &["rankedRuns", "runsRanked"])));
    let installs = int(first(obj, &["installs", "installCount", "independentInstalls"]));
    let (net_low, net_high) = match first(obj, &["net", "netShare", "netCost"]) {
        Some(Value::Array(a)) if a.len() >= 2 => (num(a.first()), num(a.get(1))),
        Some(Value::Object(o)) => (num(o.get("low").or_else(|| o.get("min"))), num(o.get("high").or_else(|| o.get("max")))),
        _ => (None, None),
    };
    let withheld = first(obj, &["withheld"]).and_then(|w| w.as_bool()).unwrap_or(false);
    // `num` already refuses a negative, which is how the site says "below the floors": it sends
    // -1 rather than 0 precisely so an untimed mod cannot be read as a free one.
    let load_ms_median = num(first(obj, &["loadMsMedian", "load_ms_median"]));
    let load_runs = int(first(obj, &["loadRuns", "load_runs"]));
    let load_installs = int(first(obj, &["loadInstalls", "load_installs"]));
    Some(Weight {
        band: band_of(obj, share, ranked),
        package_id,
        share,
        ranked,
        seen,
        measured,
        ranked_runs,
        installs,
        net_low,
        net_high,
        withheld,
        load_ms_median,
        load_runs,
        load_installs,
        // The site pools other people's machines and has nothing to say about this one.
        local_share: None,
        local_band: None,
        local_runs: None,
        origin: origin.into(),
    })
}

/// The mod objects of a list response: a bare array, or an object whose first array-valued
/// field holds mods.
fn list_items(v: &Value) -> Vec<&Value> {
    match v {
        Value::Array(a) => a.iter().collect(),
        Value::Object(o) => o
            .get("mods")
            .or_else(|| o.get("items"))
            .or_else(|| o.get("data"))
            .or_else(|| o.get("results"))
            .or_else(|| o.values().find(|x| x.is_array()))
            .and_then(|x| x.as_array())
            .map(|a| a.iter().collect())
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Parse a list response.
pub fn parse_list(v: &Value) -> Vec<Weight> {
    list_items(v).iter().filter_map(|m| parse_mod(m, "api")).collect()
}

/// Fetch every mod the site has figures for, paging through the list endpoint. Also returns the
/// first raw record, so what the server actually sends can be shown when the figures look off.
pub async fn fetch_all(client: &reqwest::Client) -> Result<(Vec<Weight>, Option<Value>)> {
    let mut out: Vec<Weight> = Vec::new();
    let mut sample: Option<Value> = None;
    let limit = 500usize;
    let mut offset = 0usize;
    for _ in 0..200 {
        let url = format!("{API_BASE}/mods?limit={limit}&offset={offset}");
        let v: Value = client.get(&url).header(reqwest::header::USER_AGENT, USER_AGENT).send().await?.error_for_status()?.json().await?;
        if sample.is_none() {
            sample = list_items(&v).into_iter().find(|m| share_of(m).is_some()).or_else(|| list_items(&v).into_iter().next()).cloned();
        }
        let page = parse_list(&v);
        let n = page.len();
        out.extend(page);
        if n < limit {
            break;
        }
        offset += n;
    }
    Ok((out, sample))
}

/// Fetch one mod in full (per-patch medians are ignored here; the summary is what we show).
pub async fn fetch_one(client: &reqwest::Client, package_id: &str) -> Result<Option<Weight>> {
    let url = format!("{API_BASE}/mods/{}", package_id.to_ascii_lowercase());
    let resp = client.get(&url).header(reqwest::header::USER_AGENT, USER_AGENT).send().await?;
    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    let v: Value = resp.error_for_status()?.json().await?;
    Ok(parse_mod(v.get("mod").unwrap_or(&v), "api"))
}

/// Where the Circinus profiler mod writes runs: beside the game's config folder.
pub fn local_runs_dir(config_dir: &Path) -> PathBuf {
    config_dir.parent().map(|p| p.join("Circinus").join("Runs")).unwrap_or_else(|| config_dir.join("Circinus").join("Runs"))
}

/// Median share per packageId across local runs. Each run file is scanned for a per-mod cost
/// table (objects carrying a packageId and a ms or share figure) and normalized by the run's
/// `profilerWindowMs` when only milliseconds are given.
pub fn read_local_runs(dir: &Path) -> Result<HashMap<String, Weight>> {
    let mut samples: HashMap<String, Vec<f64>> = HashMap::new();
    let mut seen: HashMap<String, i64> = HashMap::new();
    let Ok(rd) = std::fs::read_dir(dir) else { return Ok(HashMap::new()) };
    for entry in rd.filter_map(|e| e.ok()) {
        let p = entry.path();
        if p.extension().map(|e| e != "json").unwrap_or(true) || p.file_name().map(|n| n == "index.json").unwrap_or(false) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&p) else { continue };
        let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
        let window_ms = v.pointer("/env/profilerWindowMs").and_then(|x| x.as_f64()).or_else(|| v.get("profilerWindowMs").and_then(|x| x.as_f64()));
        let mut stack: Vec<&Value> = vec![&v];
        while let Some(cur) = stack.pop() {
            match cur {
                Value::Array(a) => stack.extend(a.iter()),
                Value::Object(o) => {
                    if let Some(id) = o.get("packageId").and_then(|x| x.as_str()) {
                        let id = id.to_ascii_lowercase();
                        let share = share_of(cur).or_else(|| {
                            let ms = num(first(cur, &["ms", "medianMs", "msPerFrame", "cost_ms", "costMs"]))?;
                            let w = window_ms?;
                            if w > 0.0 { Some(ms / w * 100.0) } else { None }
                        });
                        if let Some(s) = share {
                            samples.entry(id.clone()).or_default().push(s);
                        }
                        *seen.entry(id).or_default() += 1;
                    }
                    stack.extend(o.values().filter(|x| x.is_array() || x.is_object()));
                }
                _ => {}
            }
        }
    }
    let mut out = HashMap::new();
    for (id, mut xs) in samples {
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = xs[xs.len() / 2];
        let n = xs.len() as i64;
        // Your runs fill the *local* fields and leave `share` alone. `share` is what everybody
        // else measured; nothing this machine did belongs in it, however little the pool has.
        // A local profiler run also times frames rather than start-up, so the load fields stay
        // empty here too.
        let mut w = Weight::blank(id.clone(), "local");
        w.seen = seen.get(&id).copied();
        w.local_share = Some(median);
        w.local_band = Some(Band::for_share(median));
        w.local_runs = Some(n);
        out.insert(id, w);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bands() {
        assert_eq!(Band::for_share(0.2), Band::Negligible);
        assert_eq!(Band::for_share(1.9), Band::Light);
        assert_eq!(Band::for_share(4.0), Band::Moderate);
        assert_eq!(Band::for_share(10.0), Band::Heavy);
        assert_eq!(Band::for_share(30.0), Band::VeryHeavy);
    }

    #[test]
    fn a_mod_nobody_has_timed_is_not_a_mod_that_costs_nothing() {
        // The site says "below the floors" as -1 rather than 0 for exactly this reason, and the
        // whole convention is worthless if it arrives here as a number. A zero would render as
        // "adds 0 s at start-up", which is a measurement nobody took.
        let list = serde_json::json!({"mods": [
            {"packageId": "a.timed", "loadMsMedian": 905.75, "loadRuns": 40, "loadInstalls": 14},
            {"packageId": "b.untimed", "loadMsMedian": -1.0, "loadRuns": 0, "loadInstalls": 0},
            {"packageId": "c.silent"}
        ]});
        let w = parse_list(&list);
        assert_eq!(w[0].load_ms_median, Some(905.75));
        assert_eq!(w[0].load_runs, Some(40));
        assert_eq!(w[0].load_installs, Some(14));
        assert_eq!(w[1].load_ms_median, None, "-1 is the site saying it does not know");
        assert_eq!(w[2].load_ms_median, None, "and a field that is simply absent is the same answer");
    }

    #[test]
    fn a_start_up_cost_is_not_a_frame_share() {
        // Two independent measurements in one row: a mod can have either, both or neither, and
        // reading one as the other would put milliseconds in a column of percentages.
        let list = serde_json::json!({"mods": [
            {"packageId": "only.load", "loadMsMedian": 1200.0},
            {"packageId": "only.frame", "share": 3.4, "ranked": true}
        ]});
        let w = parse_list(&list);
        assert_eq!(w[0].share, None);
        assert_eq!(w[0].load_ms_median, Some(1200.0));
        assert_eq!(w[1].share, Some(3.4));
        assert_eq!(w[1].load_ms_median, None);
    }

    #[test]
    fn parses_plausible_shapes() {
        let list = serde_json::json!({"mods": [
            {"packageId": "Krkr.RocketMan", "share": 3.4, "band": "moderate", "ranked": true, "runs": {"seen": 120, "measured": 100, "ranked": 80}, "installs": 40},
            {"packageId": "a.b", "share": -1, "ranked": false, "band": "insufficient"},
            {"packageId": "c.d", "median": {"share": 0.1}},
            {"packageId": "e.f", "band": "negligible", "ranked": true, "stats": {"frameShare": {"p50": 0.04, "p90": 0.2}}},
            {"packageId": "g.h", "ranked": true, "summary": {"share_pct": "0.75"}}
        ]});
        let w = parse_list(&list);
        assert_eq!(w.len(), 5);
        assert_eq!(w[3].share, Some(0.04), "nested share under stats.frameShare.p50");
        assert_eq!(w[3].band, Band::Negligible);
        assert_eq!(w[4].share, Some(0.75), "share_pct as a string two levels down");
        assert_eq!(w[0].package_id, "krkr.rocketman");
        assert_eq!(w[0].band, Band::Moderate);
        assert_eq!(w[0].measured, Some(100));
        assert_eq!(w[1].share, None);
        assert_eq!(w[1].band, Band::Insufficient);
        assert_eq!(w[2].share, Some(0.1));
        assert_eq!(w[2].band, Band::Insufficient);
    }

    #[test]
    fn local_runs_median() {
        let dir = tempfile::tempdir().unwrap();
        for (i, ms) in [2.0, 3.0, 10.0].iter().enumerate() {
            let run = serde_json::json!({"env": {"profilerWindowMs": 100.0}, "modCosts": [{"packageId": "x.y", "ms": ms}]});
            std::fs::write(dir.path().join(format!("run{i}.json")), run.to_string()).unwrap();
        }
        std::fs::write(dir.path().join("index.json"), "{}").unwrap();
        let w = read_local_runs(dir.path()).unwrap();
        // The local fields, and only those. `share` is what everybody else measured and this
        // function has no business filling it -- the merge relies on that being true, because
        // the pooled figure and this one have to be able to sit in one row at once.
        assert_eq!(w["x.y"].local_share, Some(3.0));
        assert_eq!(w["x.y"].local_runs, Some(3));
        assert_eq!(w["x.y"].local_band, Some(Band::Moderate));
        assert_eq!(w["x.y"].share, None, "a local run wrote itself into the pooled share");
        assert_eq!(w["x.y"].band, Band::Unknown);
    }
}
