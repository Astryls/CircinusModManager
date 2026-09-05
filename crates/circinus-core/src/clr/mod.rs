//! Reading .NET assemblies without .NET.
//!
//! A mod's code is a .NET assembly, and what it patches is written in the assembly's metadata
//! (custom attributes) and its IL (calls into Harmony). The format is ECMA-335, documented and
//! stable since 2005; reading it needs no runtime, so the Patches view works in every build of
//! Circinus with nothing to ship, download or explain. Nothing here loads or runs a mod's code.

use crate::harmony::AssemblyPatches;
use std::path::Path;

/// Everything the Patches view wants to know about one assembly. Never fails: a file that is
/// not a .NET assembly, or is corrupt, comes back with `error` set and empty lists.
pub fn scan_assembly(path: &Path) -> AssemblyPatches {
    AssemblyPatches { path: path.to_string_lossy().to_string(), error: Some("the assembly reader is not built yet".into()), ..Default::default() }
}
