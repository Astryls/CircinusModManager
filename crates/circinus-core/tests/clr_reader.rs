//! The assembly reader against the C# scanner's answers.
//!
//! `tests/clr/<name>.expected.json` is what `tools/harmony-scan` printed for the assembly beside
//! it. The reader must say the same thing field by field; the lists may come in any order, the
//! path is the caller's business, and entries with `source: "accesstools"` are the one thing the
//! reader adds that the scanner never had.

use circinus_core::clr::scan_assembly;
use circinus_core::harmony::{AssemblyPatches, ScanOutput};
use std::path::{Path, PathBuf};
use std::time::Instant;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("clr")
}

fn expected(name: &str) -> AssemblyPatches {
    let text = std::fs::read_to_string(fixtures().join(format!("{name}.expected.json"))).expect("the oracle file is beside the assembly");
    let out: ScanOutput = serde_json::from_str(&text).expect("the oracle is the scanner's JSON");
    out.assemblies.into_iter().next().expect("one assembly per oracle file")
}

/// A canonical, order-insensitive form of each list, as sorted JSON strings so that every
/// field takes part in the comparison and the assertion message shows the whole row.
fn rows<T: serde::Serialize>(items: &[T]) -> Vec<String> {
    let mut rows: Vec<String> = items.iter().map(|i| serde_json::to_string(i).expect("serializable")).collect();
    rows.sort();
    rows
}

fn assert_matches_oracle(name: &str) -> AssemblyPatches {
    let path = fixtures().join(format!("{name}.dll"));
    let started = Instant::now();
    let got = scan_assembly(&path);
    let took = started.elapsed();
    let want = expected(name);
    assert_eq!(got.error, want.error, "{name}: error");
    assert_eq!(got.name, want.name, "{name}: assembly name");
    assert_eq!(got.mvid, want.mvid, "{name}: mvid");
    assert_eq!(rows(&got.harmony_ids), rows(&want.harmony_ids), "{name}: harmony ids");
    let patches: Vec<_> = got.patches.iter().filter(|p| p.source != "accesstools").cloned().collect();
    assert_eq!(rows(&patches), rows(&want.patches), "{name}: patches");
    for extra in got.patches.iter().filter(|p| p.source == "accesstools") {
        assert_eq!(extra.kind, "reaches", "{name}: an accesstools entry is a reach: {extra:?}");
        assert!(extra.target_type.is_some() && extra.target_method.is_some(), "{name}: a reach names its target: {extra:?}");
    }
    assert_eq!(rows(&got.manual_patches), rows(&want.manual_patches), "{name}: manual patches");
    assert_eq!(rows(&got.startup_classes), rows(&want.startup_classes), "{name}: startup classes");
    assert_eq!(rows(&got.mod_classes), rows(&want.mod_classes), "{name}: mod classes");
    // Scanning is on the path to the Patches view; even the biggest assembly a mod folder
    // holds must be quick in a dev build.
    assert!(took.as_secs() < 1, "{name}: scanning took {took:?}");
    got
}

#[test]
fn fixture_mod_matches_the_oracle() {
    assert_matches_oracle("FixtureMod");
}

#[test]
fn harmony_mod_matches_the_oracle() {
    assert_matches_oracle("HarmonyMod");
}

#[test]
fn harmony_itself_matches_the_oracle() {
    // 2.4 MB of generics, nested types and hundreds of method bodies.
    assert_matches_oracle("0Harmony");
}

/// Real mod assemblies are not ours to keep in the repository. Point `CIRCINUS_CLR_REAL` at a
/// folder of `<name>.dll` + `<name>.expected.json` pairs and run this with `--ignored` to see
/// how the reader compares on them, what it adds, and how long each takes.
#[test]
#[ignore]
fn real_assemblies_report() {
    let Some(dir) = std::env::var_os("CIRCINUS_CLR_REAL") else {
        eprintln!("set CIRCINUS_CLR_REAL to a folder of assemblies and oracle files");
        return;
    };
    let mut failures = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(&dir).expect("readable folder").flatten().map(|e| e.path()).collect();
    entries.sort();
    for oracle in entries.iter().filter(|p| p.to_string_lossy().ends_with(".expected.json")) {
        let name = oracle.file_name().and_then(|n| n.to_str()).and_then(|n| n.strip_suffix(".expected.json")).unwrap_or_default();
        let text = std::fs::read_to_string(oracle).expect("oracle");
        let want = serde_json::from_str::<ScanOutput>(&text).expect("oracle JSON").assemblies.into_iter().next().expect("one assembly");
        let started = Instant::now();
        let got = scan_assembly(&Path::new(&dir).join(format!("{name}.dll")));
        let took = started.elapsed();
        let patches: Vec<_> = got.patches.iter().filter(|p| p.source != "accesstools").cloned().collect();
        let extras: Vec<_> = got.patches.iter().filter(|p| p.source == "accesstools").collect();
        eprintln!("== {name}: {took:?}, {} patches, {} manual, {} startup, {} mod, {} accesstools extras", patches.len(), got.manual_patches.len(), got.startup_classes.len(), got.mod_classes.len(), extras.len());
        let checks: [(&str, Vec<String>, Vec<String>); 7] = [
            ("error", rows(&[got.error.clone()]), rows(&[want.error.clone()])),
            ("name", rows(&[got.name.clone()]), rows(&[want.name.clone()])),
            ("mvid", rows(&[got.mvid.clone()]), rows(&[want.mvid.clone()])),
            ("harmonyIds", rows(&got.harmony_ids), rows(&want.harmony_ids)),
            ("patches", rows(&patches), rows(&want.patches)),
            ("manualPatches", rows(&got.manual_patches), rows(&want.manual_patches)),
            ("startupClasses", rows(&got.startup_classes), rows(&want.startup_classes)),
        ];
        for (field, ours, theirs) in checks {
            if ours != theirs {
                failures.push(format!("{name}: {field}"));
                for row in ours.iter().filter(|r| !theirs.contains(r)) {
                    eprintln!("  only ours   {field}: {row}");
                }
                for row in theirs.iter().filter(|r| !ours.contains(r)) {
                    eprintln!("  only oracle {field}: {row}");
                }
            }
        }
        if rows(&got.mod_classes) != rows(&want.mod_classes) {
            failures.push(format!("{name}: modClasses"));
        }
        for extra in extras {
            eprintln!("  reaches {}::{} -> {}::{} ({})", extra.declaring_type, extra.method, extra.target_type.as_deref().unwrap_or("?"), extra.target_method.as_deref().unwrap_or("?"), extra.target_kind);
        }
    }
    assert!(failures.is_empty(), "differences from the oracle: {failures:?}");
}

#[test]
fn random_bytes_are_an_error_not_a_panic() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("Random.dll");
    // A fixed pseudo-random sequence, so a failure is reproducible.
    let mut state = 0x2545_F491_4F6C_DD1Du64;
    let bytes: Vec<u8> = (0..65_536)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 24) as u8
        })
        .collect();
    std::fs::write(&path, &bytes).expect("write");
    let r = scan_assembly(&path);
    assert!(r.error.is_some(), "random bytes are not an assembly");
    assert!(r.patches.is_empty() && r.harmony_ids.is_empty() && r.name.is_none());
}

#[test]
fn a_png_is_not_a_net_assembly() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("Picture.dll");
    std::fs::write(&path, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0").expect("write");
    let r = scan_assembly(&path);
    assert!(r.error.as_deref().is_some_and(|e| e.starts_with("not a .NET assembly")), "{:?}", r.error);
}

#[test]
fn every_truncation_of_the_fixture_is_an_error_or_a_partial_answer_never_a_panic() {
    let whole = std::fs::read(fixtures().join("FixtureMod.dll")).expect("fixture");
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("Cut.dll");
    // Every prefix of a 7 KB file: cheap, and it walks the reader through each header, table
    // and body being cut off at every possible point.
    for len in (0..whole.len()).step_by(7) {
        std::fs::write(&path, &whole[..len]).expect("write");
        let r = scan_assembly(&path);
        if r.error.is_none() {
            // Whatever it could read must at least be the right assembly.
            assert_eq!(r.name.as_deref(), Some("FixtureMod"), "at {len} bytes");
        }
    }
    // Half the file: the headers are there and the metadata is not, which is what a download
    // that stopped early looks like. That is an error, not a mod with no patches.
    std::fs::write(&path, &whole[..whole.len() / 2]).expect("write");
    let r = scan_assembly(&path);
    assert!(r.error.as_deref().is_some_and(|e| e.starts_with("unreadable metadata")), "{:?}", r.error);
    assert!(r.patches.is_empty() && r.startup_classes.is_empty() && r.name.is_none());
}

#[test]
fn corrupting_single_bytes_never_panics() {
    let whole = std::fs::read(fixtures().join("FixtureMod.dll")).expect("fixture");
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("Flipped.dll");
    // Flip bytes across the whole file, one at a time, so every header field and table cell
    // gets an impossible value at least once.
    for at in (0..whole.len()).step_by(3) {
        let mut copy = whole.clone();
        copy[at] ^= 0xFF;
        std::fs::write(&path, &copy).expect("write");
        let _ = scan_assembly(&path);
    }
}
