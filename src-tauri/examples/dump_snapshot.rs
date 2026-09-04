//! Dump the UI snapshot for a game folder as JSON — for load testing the frontend with real
//! backend output. Usage: cargo run -p circinus --example dump_snapshot -- <game dir> <data dir>
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
    let t = std::time::Instant::now();
    let shallow = app.scan_quick(false, &|_, _| {})?;
    eprintln!("quick: {} mods, {} to inspect, {} ms", app.mods.len(), shallow.len(), t.elapsed().as_millis());
    let t = std::time::Instant::now();
    let ins = circinus_core::scan::inspect_mods(&shallow, &|_, _| {});
    app.apply_inspections(ins)?;
    eprintln!("inspect: {} ms", t.elapsed().as_millis());
    let t = std::time::Instant::now();
    let snap = app.snapshot();
    let json = serde_json::to_string(&snap)?;
    eprintln!("snapshot: active={} issues={} rules={} bytes={} in {} ms", snap.active.len(), snap.issues.len(), snap.rules.len(), json.len(), t.elapsed().as_millis());
    println!("{json}");
    Ok(())
}
