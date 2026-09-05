//! Reading .NET assemblies without .NET.
//!
//! A mod's code is a .NET assembly, and what it patches is written in the assembly's metadata
//! (custom attributes) and its IL (calls into Harmony). The format is ECMA-335, documented and
//! stable since 2005; reading it needs no runtime, so the Patches view works in every build of
//! Circinus with nothing to ship, download or explain. Nothing here loads or runs a mod's code.
//!
//! The reader is pure Rust over a byte slice. Every offset and length it takes from the file
//! is checked against the buffer before use, because a mod folder can hold anything with a
//! `.dll` extension and the one promise made here is that no input can make it panic.
//!
//! - `pe`: the PE wrapper — headers, sections, the CLI header, RVAs to file offsets.
//! - `metadata`: the metadata root, its streams, every table's layout, the heaps, and names.
//! - `blob`: signatures and custom attribute values.
//! - `il`: walking a method body.
//! - `scan`: what all of that means for Harmony, kept identical to the sidecar it replaces.

mod blob;
mod il;
mod metadata;
mod pe;
mod scan;

use crate::harmony::AssemblyPatches;
use std::path::Path;

/// Everything the Patches view wants to know about one assembly. Never fails: a file that is
/// not a .NET assembly, or is corrupt, comes back with `error` set and empty lists.
pub fn scan_assembly(path: &Path) -> AssemblyPatches {
    let mut result = AssemblyPatches { path: path.to_string_lossy().to_string(), ..Default::default() };
    let data = match std::fs::read(path) {
        Ok(data) => data,
        Err(e) => {
            result.error = Some(format!("cannot read file: {e}"));
            return result;
        }
    };
    if let Err(message) = scan_bytes(&data, &mut result) {
        // A half-read assembly is worse than none: the lists could name a patch that is not
        // there or miss one that is, so the answer is the error alone.
        result = AssemblyPatches { path: result.path, error: Some(message), ..Default::default() };
    }
    result
}

/// The reader proper, over bytes already in memory.
fn scan_bytes(data: &[u8], result: &mut AssemblyPatches) -> Result<(), String> {
    let pe = pe::Pe::parse(data)?;
    let root = pe.metadata()?;
    let md = metadata::Metadata::parse(root)?;
    scan::Scanner::new(&pe, &md, result).run();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_an_error_with_empty_lists() {
        let r = scan_assembly(Path::new("/no/such/place/Mod.dll"));
        assert!(r.error.as_deref().is_some_and(|e| e.starts_with("cannot read file")));
        assert!(r.patches.is_empty() && r.name.is_none());
    }

    #[test]
    fn bytes_that_are_not_an_assembly_are_refused_at_every_length() {
        let mut r = AssemblyPatches::default();
        assert!(scan_bytes(&[], &mut r).is_err());
        assert!(scan_bytes(b"\x89PNG\r\n\x1a\n", &mut r).unwrap_err().starts_with("not a .NET assembly"));
        assert!(scan_bytes(b"MZ", &mut r).is_err());
    }
}
