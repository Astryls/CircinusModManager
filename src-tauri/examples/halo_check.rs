//! Run HALO on a game folder and check the official-content invariant on the result.
//! Usage: cargo run -p circinus --example halo_check -- <game dir> <data dir>
use circinus_lib::state::{App, Settings};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let game = PathBuf::from(args.get(1).expect("game dir"));
    let data = PathBuf::from(args.get(2).expect("data dir"));
    let mut settings = Settings::default();
    settings.locations.game_dir = Some(game.clone());
    settings.locations.config_dir = Some(game.join("Config"));
    settings.locations.workshop_dir = Some(game.join("workshop").join("content").join("294100"));
    let mut app = App::open_at(data, Some(settings))?;
    let shallow = app.scan_quick(false, &|_, _| {})?;
    let ins = circinus_core::scan::inspect_mods(&shallow, &|_, _| {});
    app.apply_inspections(ins)?;
    let before = app.active.clone();
    let t = std::time::Instant::now();
    let r = app.halo(false);
    eprintln!("halo: {} active, {} moves, {} issues in {} ms", r.order.len(), r.moves.len(), r.issues.len(), t.elapsed().as_millis());
    let by_uid: std::collections::HashMap<&str, &circinus_core::ModInfo> = app.mods.iter().map(|m| (m.uid.as_str(), m)).collect();
    let last_official = r.order.iter().rposition(|u| by_uid.get(u.as_str()).map(|m| m.is_official()).unwrap_or(false)).unwrap_or(0);
    for (i, u) in r.order.iter().enumerate().take(last_official + 1) {
        let m = by_uid[u.as_str()];
        println!("{i:4} {:<10} defs={:<4} {}  [{}]", format!("{:?}", r.placements.iter().find(|p| &p.uid == u).map(|p| p.phase).unwrap()), m.contents.defs, m.name, m.package_id);
    }
    let bad: Vec<&String> = r.order.iter().take(last_official).filter(|u| {
        let m = by_uid[u.as_str()];
        !m.is_official() && m.contents.defs > 0
    }).collect();
    println!("mods with Defs above the last official mod: {}", bad.len());
    for i in r.issues.iter().filter(|i| matches!(i, circinus_core::Issue::AboveOfficial { .. } | circinus_core::Issue::RuleIgnored { .. } | circinus_core::Issue::Cycle { .. })) {
        println!("issue: {i:?}");
    }
    let moved = before.iter().zip(r.order.iter()).filter(|(a, b)| a != b).count();
    println!("positions changed: {moved}");
    Ok(())
}
