//! Run the texture pipeline over some mods of a game folder, headless, with the same manifest
//! handling as the app. Usage:
//!   cargo run -p circinus --example dds_job -- <game dir> <data dir> convert <mod folder name>...
//!   cargo run -p circinus --example dds_job -- <game dir> <data dir> revert <mod folder name>...
//!   cargo run -p circinus --example dds_job -- <game dir> <data dir> revalidate <mod folder name>...
use circinus_core::dds::{job, Entry, Options};
use circinus_lib::state::{App, Settings};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    let game = PathBuf::from(args.get(1).expect("game dir"));
    let data = PathBuf::from(args.get(2).expect("data dir"));
    let verb = args.get(3).map(|s| s.as_str()).unwrap_or("convert");
    let names: Vec<&str> = args[4..].iter().map(|s| s.as_str()).collect();
    let mut settings = Settings::default();
    settings.locations.game_dir = Some(game.clone());
    settings.locations.config_dir = Some(game.join("Config"));
    settings.locations.workshop_dir = Some(game.join("workshop").join("content").join("294100"));
    let mut app = App::open_at(data, Some(settings))?;
    let shallow = app.scan_quick(false, &|_, _| {})?;
    app.apply_inspections(circinus_core::scan::inspect_mods(&shallow, &|_, _| {}))?;
    let mods: Vec<(String, String, PathBuf)> = app.mods.iter().filter(|m| names.is_empty() || names.iter().any(|n| m.path.file_name().map(|f| f == *n).unwrap_or(false))).map(|m| (m.uid.clone(), m.name.clone(), m.path.clone())).collect();
    let opts = Options { alpha_format: app.settings.dds.alpha_format, quality: app.settings.dds.quality, mipmaps: app.settings.dds.mipmaps, ..Options::default() };
    match verb {
        "convert" => {
            let t = std::time::Instant::now();
            let mut work = Vec::new();
            let (mut shipped, mut current) = (0, 0);
            for (uid, name, path) in &mods {
                let entries: HashMap<String, Entry> = app.cache.dds_entries::<Entry>(uid)?.into_iter().map(|e| (e.rel.clone(), e)).collect();
                let plan = job::plan(job::find_pngs(path), &entries, &opts);
                eprintln!("{name}: {} to convert, {} current, {} shipped", plan.work.len(), plan.current, plan.shipped);
                shipped += plan.shipped;
                current += plan.current;
                for w in plan.work {
                    work.push((uid.clone(), w.candidate));
                }
            }
            let cancel = AtomicBool::new(false);
            let outcomes = job::run(work, &opts, 0, &cancel, &|p| eprintln!("  {}/{} {}", p.done, p.total, p.current));
            let mut store = Vec::new();
            let (mut ok, mut failed, mut png_b, mut dds_b) = (0, 0, 0u64, 0u64);
            for o in outcomes {
                match o.result {
                    Ok(e) => {
                        ok += 1;
                        png_b += o.png_len;
                        dds_b += e.dds_len;
                        store.push((o.uid, o.rel, e));
                    }
                    Err(m) => {
                        failed += 1;
                        eprintln!("  FAILED {}: {m}", o.rel);
                    }
                }
            }
            app.cache.dds_store(&store)?;
            app.reload_dds_index();
            println!("converted {ok}, failed {failed}, current {current}, shipped {shipped}, png {png_b} B -> dds {dds_b} B, {} ms", t.elapsed().as_millis());
            for (uid, s) in &app.dds_index {
                println!("  {uid}: {} files, {} B", s.count, s.dds_bytes);
            }
        }
        "revalidate" => {
            for (uid, name, path) in &mods {
                let entries: Vec<Entry> = app.cache.dds_entries(uid)?;
                let r = job::revalidate(path, &entries);
                println!("{name}: keep {}, stale {:?}, missing {:?}, foreign {:?}", r.keep.len(), r.stale, r.missing, r.foreign);
                let drop: Vec<String> = r.stale.iter().chain(r.missing.iter()).chain(r.foreign.iter()).cloned().collect();
                app.cache.dds_delete(uid, &drop)?;
            }
        }
        "revert" => {
            for (uid, name, path) in &mods {
                let entries: Vec<Entry> = app.cache.dds_entries(uid)?;
                let r = job::revert(path, &entries);
                println!("{name}: deleted {}, kept {:?}, freed {} B", r.deleted.len(), r.kept, r.bytes_freed);
                app.cache.dds_delete_all(uid)?;
            }
        }
        other => eprintln!("unknown verb {other}"),
    }
    Ok(())
}
