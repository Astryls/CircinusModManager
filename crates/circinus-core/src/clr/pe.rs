//! The Portable Executable wrapper around a .NET assembly: just enough of it to find the CLI
//! header, and through it the metadata, and to turn the RVAs the metadata uses into file
//! offsets so method bodies can be read.
//!
//! Every read here goes through the bounds-checked helpers at the bottom. A mod folder can hold
//! anything with a `.dll` extension — native libraries, half-downloaded files, a text file
//! someone renamed — and the reader's promise is that none of them can make it panic.

/// Little-endian reads that fail instead of slicing past the end.
pub(super) fn u8_at(data: &[u8], at: usize) -> Option<u8> {
    data.get(at).copied()
}

pub(super) fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    let bytes = data.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

pub(super) fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    let bytes = data.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

pub(super) fn u64_at(data: &[u8], at: usize) -> Option<u64> {
    let bytes = data.get(at..at.checked_add(8)?)?;
    let mut raw = [0u8; 8];
    raw.copy_from_slice(bytes);
    Some(u64::from_le_bytes(raw))
}

/// `data[at..at + len]`, or nothing if any part of that range is outside the buffer.
pub(super) fn slice_at(data: &[u8], at: usize, len: usize) -> Option<&[u8]> {
    data.get(at..at.checked_add(len)?)
}

/// One entry of the section table: the piece that maps a range of virtual addresses onto a
/// range of the file.
#[derive(Debug, Clone, Copy)]
struct Section {
    virtual_address: u32,
    virtual_size: u32,
    raw_offset: u32,
    raw_size: u32,
}

/// A parsed PE image with its CLI header located.
pub(super) struct Pe<'a> {
    data: &'a [u8],
    sections: Vec<Section>,
    /// The CLI header's location, from data directory 14.
    cli_rva: u32,
    cli_size: u32,
}

/// Where in the file the fields of the optional header live; PE32 and PE32+ differ only in
/// where the data directories start.
const DIRECTORIES_PE32: usize = 96;
const DIRECTORIES_PE32_PLUS: usize = 112;
const CLI_DIRECTORY: usize = 14;

/// Section tables in the wild have a dozen entries; a count in the thousands is a corrupt or
/// hostile header, and reading it would only waste time before failing anyway.
const MAX_SECTIONS: u16 = 96;

impl<'a> Pe<'a> {
    /// Parse the DOS stub, COFF header, optional header and section table. The error is a plain
    /// sentence naming what was missing, because it ends up in the Patches view beside the file.
    pub(super) fn parse(data: &'a [u8]) -> Result<Pe<'a>, String> {
        if u16_at(data, 0) != Some(0x5A4D) {
            return Err("not a .NET assembly (no PE header)".into());
        }
        let pe_offset = u32_at(data, 0x3C).map(|v| v as usize).ok_or("not a .NET assembly (no PE header)")?;
        if u32_at(data, pe_offset) != Some(0x0000_4550) {
            return Err("not a .NET assembly (no PE signature)".into());
        }
        let coff = pe_offset.checked_add(4).ok_or("not a .NET assembly (no PE signature)")?;
        let section_count = u16_at(data, coff + 2).ok_or("not a .NET assembly (truncated COFF header)")?;
        let optional_size = u16_at(data, coff + 16).ok_or("not a .NET assembly (truncated COFF header)")? as usize;
        let optional = coff + 20;
        let directories = match u16_at(data, optional) {
            Some(0x10B) => DIRECTORIES_PE32,
            Some(0x20B) => DIRECTORIES_PE32_PLUS,
            _ => return Err("not a .NET assembly (unknown optional header)".into()),
        };
        let directory_count = u32_at(data, optional + directories - 4).unwrap_or(0) as usize;
        if directory_count <= CLI_DIRECTORY || optional_size < directories + (CLI_DIRECTORY + 1) * 8 {
            return Err("not a managed assembly (no CLI header)".into());
        }
        let cli_entry = optional + directories + CLI_DIRECTORY * 8;
        let cli_rva = u32_at(data, cli_entry).ok_or("not a managed assembly (truncated data directories)")?;
        let cli_size = u32_at(data, cli_entry + 4).ok_or("not a managed assembly (truncated data directories)")?;
        if cli_rva == 0 || cli_size == 0 {
            return Err("not a managed assembly (no CLI header)".into());
        }
        if section_count > MAX_SECTIONS {
            return Err("unreadable image: the section table is implausibly large".into());
        }
        let table = optional.checked_add(optional_size).ok_or("unreadable image: bad optional header size")?;
        let mut sections = Vec::with_capacity(section_count as usize);
        for i in 0..section_count as usize {
            let entry = table + i * 40;
            let section = Section {
                virtual_size: u32_at(data, entry + 8).ok_or("unreadable image: truncated section table")?,
                virtual_address: u32_at(data, entry + 12).ok_or("unreadable image: truncated section table")?,
                raw_size: u32_at(data, entry + 16).ok_or("unreadable image: truncated section table")?,
                raw_offset: u32_at(data, entry + 20).ok_or("unreadable image: truncated section table")?,
            };
            sections.push(section);
        }
        Ok(Pe { data, sections, cli_rva, cli_size })
    }

    /// File offset of an RVA, if some section covers it. An RVA is valid within a section up to
    /// the larger of its virtual and raw sizes: the loader zero-fills the difference, and a
    /// metadata directory can legitimately sit in the zero-filled part of a padded section.
    pub(super) fn offset_of(&self, rva: u32) -> Option<usize> {
        for s in &self.sections {
            let extent = s.virtual_size.max(s.raw_size);
            if rva >= s.virtual_address && rva - s.virtual_address < extent {
                let within = rva - s.virtual_address;
                if within >= s.raw_size {
                    return None;
                }
                let offset = s.raw_offset.checked_add(within)? as usize;
                return if offset < self.data.len() { Some(offset) } else { None };
            }
        }
        None
    }

    /// The bytes at an RVA, bounded by the section's raw data and the file.
    pub(super) fn at_rva(&self, rva: u32, len: usize) -> Option<&'a [u8]> {
        let offset = self.offset_of(rva)?;
        slice_at(self.data, offset, len)
    }

    /// Everything from an RVA to the end of the file, for readers that find their own length
    /// (a method body's header says how long the body is).
    pub(super) fn slice_at_rva(&self, rva: u32) -> Option<&'a [u8]> {
        let offset = self.offset_of(rva)?;
        self.data.get(offset..)
    }

    /// The metadata root, located through the CLI header.
    pub(super) fn metadata(&self) -> Result<&'a [u8], String> {
        let header = self.at_rva(self.cli_rva, self.cli_size.min(72) as usize).ok_or("not a managed assembly (CLI header outside the image)")?;
        let metadata_rva = u32_at(header, 8).ok_or("not a managed assembly (truncated CLI header)")?;
        let metadata_size = u32_at(header, 12).ok_or("not a managed assembly (truncated CLI header)")?;
        if metadata_rva == 0 || metadata_size == 0 {
            return Err("not a managed assembly (no CLI metadata)".into());
        }
        let offset = self.offset_of(metadata_rva).ok_or("unreadable metadata: the metadata directory is outside the image")?;
        // Metadata that runs past the end of the file is refused, not clamped: the tables are
        // laid out end to end and the heaps come last, so a cut anywhere inside means names
        // and attributes are missing, and a partial list of patches is worse than an honest
        // error. A half-downloaded file is the usual way to get here.
        slice_at(self.data, offset, metadata_size as usize).ok_or_else(|| "unreadable metadata: the file ends before its metadata does (a truncated download?)".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_and_wrong_inputs_are_errors_not_panics() {
        assert!(Pe::parse(&[]).is_err());
        assert!(Pe::parse(b"MZ").is_err());
        assert!(Pe::parse(&[0x4D, 0x5A, 0, 0]).is_err());
        let mut fake = vec![0u8; 0x40];
        fake[0] = 0x4D;
        fake[1] = 0x5A;
        fake[0x3C] = 0x30;
        assert!(Pe::parse(&fake).is_err());
        fake[0x30..0x34].copy_from_slice(b"PE\0\0");
        assert!(Pe::parse(&fake).is_err());
    }

    #[test]
    fn reads_never_run_off_the_end() {
        let d = [1u8, 2, 3];
        assert_eq!(u16_at(&d, 2), None);
        assert_eq!(u32_at(&d, 0), None);
        assert_eq!(u32_at(&d, usize::MAX), None);
        assert_eq!(slice_at(&d, 1, 2), Some(&d[1..3]));
        assert_eq!(slice_at(&d, 1, 3), None);
    }
}
