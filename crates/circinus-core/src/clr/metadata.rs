//! ECMA-335 metadata: the streams, the tables and the heaps.
//!
//! The tables stream is one long run of fixed-width rows with no offsets in it; a table's
//! position is the sum of the sizes of every table before it, and a row's width depends on how
//! many rows the tables it points at have. So to read the one table we care about we have to
//! know the shape of all forty-five, which is what the schema below is. Everything else here is
//! bounds-checked cell access on top of that, and the string, blob, GUID and user-string heaps.

use super::pe::{slice_at, u16_at, u32_at, u64_at, u8_at};
use std::collections::HashMap;

/// The tables of partition II.22, numbered as the `Valid` bitmask numbers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(super) enum Table {
    Module = 0x00,
    TypeRef = 0x01,
    TypeDef = 0x02,
    FieldPtr = 0x03,
    Field = 0x04,
    MethodPtr = 0x05,
    MethodDef = 0x06,
    ParamPtr = 0x07,
    Param = 0x08,
    InterfaceImpl = 0x09,
    MemberRef = 0x0A,
    Constant = 0x0B,
    CustomAttribute = 0x0C,
    FieldMarshal = 0x0D,
    DeclSecurity = 0x0E,
    ClassLayout = 0x0F,
    FieldLayout = 0x10,
    StandAloneSig = 0x11,
    EventMap = 0x12,
    EventPtr = 0x13,
    Event = 0x14,
    PropertyMap = 0x15,
    PropertyPtr = 0x16,
    Property = 0x17,
    MethodSemantics = 0x18,
    MethodImpl = 0x19,
    ModuleRef = 0x1A,
    TypeSpec = 0x1B,
    ImplMap = 0x1C,
    FieldRva = 0x1D,
    EncLog = 0x1E,
    EncMap = 0x1F,
    Assembly = 0x20,
    AssemblyProcessor = 0x21,
    AssemblyOs = 0x22,
    AssemblyRef = 0x23,
    AssemblyRefProcessor = 0x24,
    AssemblyRefOs = 0x25,
    File = 0x26,
    ExportedType = 0x27,
    ManifestResource = 0x28,
    NestedClass = 0x29,
    GenericParam = 0x2A,
    MethodSpec = 0x2B,
    GenericParamConstraint = 0x2C,
}

const TABLE_COUNT: usize = 0x2D;

const ALL_TABLES: [Table; TABLE_COUNT] = [
    Table::Module,
    Table::TypeRef,
    Table::TypeDef,
    Table::FieldPtr,
    Table::Field,
    Table::MethodPtr,
    Table::MethodDef,
    Table::ParamPtr,
    Table::Param,
    Table::InterfaceImpl,
    Table::MemberRef,
    Table::Constant,
    Table::CustomAttribute,
    Table::FieldMarshal,
    Table::DeclSecurity,
    Table::ClassLayout,
    Table::FieldLayout,
    Table::StandAloneSig,
    Table::EventMap,
    Table::EventPtr,
    Table::Event,
    Table::PropertyMap,
    Table::PropertyPtr,
    Table::Property,
    Table::MethodSemantics,
    Table::MethodImpl,
    Table::ModuleRef,
    Table::TypeSpec,
    Table::ImplMap,
    Table::FieldRva,
    Table::EncLog,
    Table::EncMap,
    Table::Assembly,
    Table::AssemblyProcessor,
    Table::AssemblyOs,
    Table::AssemblyRef,
    Table::AssemblyRefProcessor,
    Table::AssemblyRefOs,
    Table::File,
    Table::ExportedType,
    Table::ManifestResource,
    Table::NestedClass,
    Table::GenericParam,
    Table::MethodSpec,
    Table::GenericParamConstraint,
];

/// The coded indexes of partition II.24.2.6: a row number and a tag saying which table it is
/// in, packed into one column whose width depends on the largest table it can point at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Coded {
    TypeDefOrRef,
    HasConstant,
    HasCustomAttribute,
    HasFieldMarshal,
    HasDeclSecurity,
    MemberRefParent,
    HasSemantics,
    MethodDefOrRef,
    MemberForwarded,
    Implementation,
    CustomAttributeType,
    ResolutionScope,
    TypeOrMethodDef,
}

impl Coded {
    /// The tables a tag value selects, in tag order; `None` for tag values the standard leaves
    /// unused (CustomAttributeType has three of them).
    fn tables(self) -> &'static [Option<Table>] {
        use Table::*;
        match self {
            Coded::TypeDefOrRef => &[Some(TypeDef), Some(TypeRef), Some(TypeSpec)],
            Coded::HasConstant => &[Some(Field), Some(Param), Some(Property)],
            Coded::HasCustomAttribute => &[
                Some(MethodDef),
                Some(Field),
                Some(TypeRef),
                Some(TypeDef),
                Some(Param),
                Some(InterfaceImpl),
                Some(MemberRef),
                Some(Module),
                Some(DeclSecurity),
                Some(Property),
                Some(Event),
                Some(StandAloneSig),
                Some(ModuleRef),
                Some(TypeSpec),
                Some(Assembly),
                Some(AssemblyRef),
                Some(File),
                Some(ExportedType),
                Some(ManifestResource),
                Some(GenericParam),
                Some(GenericParamConstraint),
                Some(MethodSpec),
            ],
            Coded::HasFieldMarshal => &[Some(Field), Some(Param)],
            Coded::HasDeclSecurity => &[Some(TypeDef), Some(MethodDef), Some(Assembly)],
            Coded::MemberRefParent => &[Some(TypeDef), Some(TypeRef), Some(ModuleRef), Some(MethodDef), Some(TypeSpec)],
            Coded::HasSemantics => &[Some(Event), Some(Property)],
            Coded::MethodDefOrRef => &[Some(MethodDef), Some(MemberRef)],
            Coded::MemberForwarded => &[Some(Field), Some(MethodDef)],
            Coded::Implementation => &[Some(File), Some(AssemblyRef), Some(ExportedType)],
            Coded::CustomAttributeType => &[None, None, Some(MethodDef), Some(MemberRef), None],
            Coded::ResolutionScope => &[Some(Module), Some(ModuleRef), Some(AssemblyRef), Some(TypeRef)],
            Coded::TypeOrMethodDef => &[Some(TypeDef), Some(MethodDef)],
        }
    }

    fn tag_bits(self) -> u32 {
        let n = self.tables().len();
        // Enough bits to hold every tag value: 2 tables need 1 bit, 3..4 need 2, 5..8 need 3.
        (usize::BITS - (n - 1).leading_zeros()).max(1)
    }

    /// Split a coded value into the table it names and the row within it.
    pub(super) fn decode(self, value: u32) -> Option<(Table, u32)> {
        let bits = self.tag_bits();
        let tag = (value & ((1 << bits) - 1)) as usize;
        let row = value >> bits;
        let table = (*self.tables().get(tag)?)?;
        if row == 0 {
            return None;
        }
        Some((table, row))
    }
}

/// One column's encoding, from which its width follows once the row counts are known.
#[derive(Debug, Clone, Copy)]
enum Col {
    U8,
    U16,
    U32,
    Str,
    Guid,
    Blob,
    Idx(Table),
    Coded(Coded),
}

/// Row layouts of partition II.22, in column order.
fn schema(table: Table) -> &'static [Col] {
    use Col::*;
    use Table::*;
    match table {
        Module => &[U16, Str, Guid, Guid, Guid],
        TypeRef => &[Coded(self::Coded::ResolutionScope), Str, Str],
        TypeDef => &[U32, Str, Str, Coded(self::Coded::TypeDefOrRef), Idx(Field), Idx(MethodDef)],
        FieldPtr => &[Idx(Field)],
        Field => &[U16, Str, Blob],
        MethodPtr => &[Idx(MethodDef)],
        MethodDef => &[U32, U16, U16, Str, Blob, Idx(Param)],
        ParamPtr => &[Idx(Param)],
        Param => &[U16, U16, Str],
        InterfaceImpl => &[Idx(TypeDef), Coded(self::Coded::TypeDefOrRef)],
        MemberRef => &[Coded(self::Coded::MemberRefParent), Str, Blob],
        Constant => &[U8, U8, Coded(self::Coded::HasConstant), Blob],
        CustomAttribute => &[Coded(self::Coded::HasCustomAttribute), Coded(self::Coded::CustomAttributeType), Blob],
        FieldMarshal => &[Coded(self::Coded::HasFieldMarshal), Blob],
        DeclSecurity => &[U16, Coded(self::Coded::HasDeclSecurity), Blob],
        ClassLayout => &[U16, U32, Idx(TypeDef)],
        FieldLayout => &[U32, Idx(Field)],
        StandAloneSig => &[Blob],
        EventMap => &[Idx(TypeDef), Idx(Event)],
        EventPtr => &[Idx(Event)],
        Event => &[U16, Str, Coded(self::Coded::TypeDefOrRef)],
        PropertyMap => &[Idx(TypeDef), Idx(Property)],
        PropertyPtr => &[Idx(Property)],
        Property => &[U16, Str, Blob],
        MethodSemantics => &[U16, Idx(MethodDef), Coded(self::Coded::HasSemantics)],
        MethodImpl => &[Idx(TypeDef), Coded(self::Coded::MethodDefOrRef), Coded(self::Coded::MethodDefOrRef)],
        ModuleRef => &[Str],
        TypeSpec => &[Blob],
        ImplMap => &[U16, Coded(self::Coded::MemberForwarded), Str, Idx(ModuleRef)],
        FieldRva => &[U32, Idx(Field)],
        EncLog => &[U32, U32],
        EncMap => &[U32],
        Assembly => &[U32, U16, U16, U16, U16, U32, Blob, Str, Str],
        AssemblyProcessor => &[U32],
        AssemblyOs => &[U32, U32, U32],
        AssemblyRef => &[U16, U16, U16, U16, U32, Blob, Str, Str, Blob],
        AssemblyRefProcessor => &[U32, Idx(AssemblyRef)],
        AssemblyRefOs => &[U32, U32, U32, Idx(AssemblyRef)],
        File => &[U32, Str, Blob],
        ExportedType => &[U32, U32, Str, Str, Coded(self::Coded::Implementation)],
        ManifestResource => &[U32, U32, Str, Coded(self::Coded::Implementation)],
        NestedClass => &[Idx(TypeDef), Idx(TypeDef)],
        GenericParam => &[U16, U16, Coded(self::Coded::TypeOrMethodDef), Str],
        MethodSpec => &[Coded(self::Coded::MethodDefOrRef), Blob],
        GenericParamConstraint => &[Idx(GenericParam), Coded(self::Coded::TypeDefOrRef)],
    }
}

/// Where one table's rows sit in the tables stream and how its columns are laid out.
#[derive(Debug, Clone, Default)]
struct Layout {
    rows: u32,
    start: usize,
    row_size: usize,
    /// (offset within the row, width in bytes) per column.
    cols: Vec<(usize, usize)>,
}

/// A parsed metadata root: heaps plus the table layouts, with the two lookups the naming code
/// needs on every call precomputed (nested-class parents and which type owns each method).
pub(super) struct Metadata<'a> {
    strings: &'a [u8],
    user_strings: &'a [u8],
    blobs: &'a [u8],
    guids: &'a [u8],
    tables: &'a [u8],
    layouts: Vec<Layout>,
    nested_parent: HashMap<u32, u32>,
    /// TypeDef row that owns MethodDef row `i + 1`; 0 when no type claims it.
    method_owner: Vec<u32>,
}

/// A row count beyond this is not a real assembly: the largest tables in the wild are in the
/// low hundreds of thousands, and the token format caps rows at 2^24 anyway.
const MAX_ROWS: u32 = 1 << 24;

impl<'a> Metadata<'a> {
    pub(super) fn parse(root: &'a [u8]) -> Result<Metadata<'a>, String> {
        if u32_at(root, 0) != Some(0x424A_5342) {
            return Err("unreadable metadata: the metadata signature is missing".into());
        }
        let version_len = u32_at(root, 12).ok_or("unreadable metadata: truncated root")? as usize;
        // The version string is padded to a multiple of four; the stream count follows it.
        let after_version = 16usize.checked_add(version_len.div_ceil(4) * 4).ok_or("unreadable metadata: truncated root")?;
        let stream_count = u16_at(root, after_version + 2).ok_or("unreadable metadata: truncated root")? as usize;
        let mut at = after_version + 4;
        let (mut tables, mut strings, mut user_strings, mut blobs, mut guids) = (None, None, None, None, None);
        for _ in 0..stream_count.min(32) {
            let offset = u32_at(root, at).ok_or("unreadable metadata: truncated stream headers")? as usize;
            let size = u32_at(root, at + 4).ok_or("unreadable metadata: truncated stream headers")? as usize;
            let name_start = at + 8;
            let name_end = root.get(name_start..).and_then(|rest| rest.iter().position(|&b| b == 0)).map(|n| name_start + n).ok_or("unreadable metadata: truncated stream headers")?;
            let name = &root[name_start..name_end];
            // Names are NUL-terminated then padded to a four-byte boundary.
            at = name_end + 1;
            at = at.div_ceil(4) * 4;
            let end = offset.saturating_add(size).min(root.len());
            let bytes = root.get(offset..end).unwrap_or(&[]);
            match name {
                b"#~" | b"#-" => tables = Some(bytes),
                b"#Strings" => strings = Some(bytes),
                b"#US" => user_strings = Some(bytes),
                b"#Blob" => blobs = Some(bytes),
                b"#GUID" => guids = Some(bytes),
                _ => {}
            }
        }
        let tables = tables.ok_or("unreadable metadata: no tables stream")?;
        let mut md = Metadata {
            strings: strings.unwrap_or(&[]),
            user_strings: user_strings.unwrap_or(&[]),
            blobs: blobs.unwrap_or(&[]),
            guids: guids.unwrap_or(&[]),
            tables,
            layouts: Vec::new(),
            nested_parent: HashMap::new(),
            method_owner: Vec::new(),
        };
        md.lay_out_tables()?;
        md.index_nesting();
        md.index_method_owners();
        Ok(md)
    }

    /// Read the tables header, then walk the schema computing every table's start and width.
    fn lay_out_tables(&mut self) -> Result<(), String> {
        let t = self.tables;
        let heap_sizes = u8_at(t, 6).ok_or("unreadable metadata: truncated tables stream")?;
        let valid = u64_at(t, 8).ok_or("unreadable metadata: truncated tables stream")?;
        let mut at = 24usize;
        let mut rows = [0u32; 64];
        for (i, slot) in rows.iter_mut().enumerate() {
            if valid & (1u64 << i) != 0 {
                let n = u32_at(t, at).ok_or("unreadable metadata: truncated row counts")?;
                if n > MAX_ROWS {
                    return Err("unreadable metadata: a table claims an impossible number of rows".into());
                }
                *slot = n;
                at += 4;
            }
        }
        // HeapSizes bit 0x40 announces one extra 32-bit field before the rows.
        if heap_sizes & 0x40 != 0 {
            at += 4;
        }
        let str_w = if heap_sizes & 0x01 != 0 { 4 } else { 2 };
        let guid_w = if heap_sizes & 0x02 != 0 { 4 } else { 2 };
        let blob_w = if heap_sizes & 0x04 != 0 { 4 } else { 2 };
        let count = |table: Table| rows[table as usize];
        let index_width = |table: Table| if count(table) >= 1 << 16 { 4 } else { 2 };
        let coded_width = |c: Coded| {
            let largest = c.tables().iter().flatten().map(|&t| count(t)).max().unwrap_or(0);
            if largest >= 1u32 << (16 - c.tag_bits()) {
                4
            } else {
                2
            }
        };
        let mut layouts = Vec::with_capacity(TABLE_COUNT);
        for table in ALL_TABLES {
            let mut cols = Vec::new();
            let mut row_size = 0usize;
            for col in schema(table) {
                let w = match *col {
                    Col::U8 => 1,
                    Col::U16 => 2,
                    Col::U32 => 4,
                    Col::Str => str_w,
                    Col::Guid => guid_w,
                    Col::Blob => blob_w,
                    Col::Idx(target) => index_width(target),
                    Col::Coded(c) => coded_width(c),
                };
                cols.push((row_size, w));
                row_size += w;
            }
            let n = count(table);
            let layout = Layout { rows: n, start: at, row_size, cols };
            at = at.checked_add(row_size.checked_mul(n as usize).ok_or("unreadable metadata: tables overflow")?).ok_or("unreadable metadata: tables overflow")?;
            layouts.push(layout);
        }
        if at > t.len() {
            return Err("unreadable metadata: the tables run past the end of their stream".into());
        }
        self.layouts = layouts;
        Ok(())
    }

    fn index_nesting(&mut self) {
        for row in 1..=self.rows(Table::NestedClass) {
            if let (Some(nested), Some(enclosing)) = (self.cell(Table::NestedClass, row, 0), self.cell(Table::NestedClass, row, 1)) {
                if nested != 0 && enclosing != 0 && nested != enclosing {
                    self.nested_parent.insert(nested, enclosing);
                }
            }
        }
    }

    /// MethodDef rows are owned by ranges: type `i` owns from its MethodList up to the next
    /// type's MethodList. Turn that into a flat lookup so a call site's declaring type is one
    /// index away, however many methods the assembly has.
    fn index_method_owners(&mut self) {
        let methods = self.rows(Table::MethodDef);
        let types = self.rows(Table::TypeDef);
        self.method_owner = vec![0; methods as usize];
        for ty in 1..=types {
            let first = self.cell(Table::TypeDef, ty, 5).unwrap_or(0);
            let next = if ty < types { self.cell(Table::TypeDef, ty + 1, 5).unwrap_or(methods + 1) } else { methods + 1 };
            let (first, next) = (first.max(1), next.min(methods + 1));
            for m in first..next {
                if let Some(slot) = self.method_owner.get_mut(m as usize - 1) {
                    *slot = ty;
                }
            }
        }
    }

    pub(super) fn rows(&self, table: Table) -> u32 {
        self.layouts.get(table as usize).map(|l| l.rows).unwrap_or(0)
    }

    /// One cell, as the unsigned integer it encodes; rows are numbered from 1 as tokens do.
    pub(super) fn cell(&self, table: Table, row: u32, col: usize) -> Option<u32> {
        let layout = self.layouts.get(table as usize)?;
        if row == 0 || row > layout.rows {
            return None;
        }
        let (offset, width) = *layout.cols.get(col)?;
        let at = layout.start.checked_add((row as usize - 1).checked_mul(layout.row_size)?)?.checked_add(offset)?;
        match width {
            1 => u8_at(self.tables, at).map(u32::from),
            2 => u16_at(self.tables, at).map(u32::from),
            _ => u32_at(self.tables, at),
        }
    }

    pub(super) fn coded(&self, table: Table, row: u32, col: usize, kind: Coded) -> Option<(Table, u32)> {
        kind.decode(self.cell(table, row, col)?)
    }

    // -- heaps -----------------------------------------------------------------------------

    /// A `#Strings` entry: NUL-terminated UTF-8. Bad UTF-8 is replaced rather than refused, since
    /// a name is only ever printed.
    pub(super) fn string(&self, index: u32) -> Option<&'a str> {
        let rest = self.strings.get(index as usize..)?;
        let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
        std::str::from_utf8(&rest[..end]).ok()
    }

    pub(super) fn string_cell(&self, table: Table, row: u32, col: usize) -> Option<&'a str> {
        self.string(self.cell(table, row, col)?)
    }

    /// A `#Blob` entry: compressed length, then that many bytes.
    pub(super) fn blob(&self, index: u32) -> Option<&'a [u8]> {
        if index == 0 {
            return None;
        }
        let mut at = index as usize;
        let len = super::blob::read_compressed(self.blobs, &mut at)? as usize;
        slice_at(self.blobs, at, len)
    }

    pub(super) fn blob_cell(&self, table: Table, row: u32, col: usize) -> Option<&'a [u8]> {
        self.blob(self.cell(table, row, col)?)
    }

    /// A `#US` entry by its heap offset: a blob of UTF-16LE with a trailing flag byte.
    pub(super) fn user_string(&self, offset: u32) -> Option<String> {
        let mut at = offset as usize;
        let len = super::blob::read_compressed(self.user_strings, &mut at)? as usize;
        let bytes = slice_at(self.user_strings, at, len)?;
        // The final byte, when the length is odd, only says whether the string has characters
        // that need special handling; it is not part of the text.
        let units: Vec<u16> = bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        Some(String::from_utf16_lossy(&units))
    }

    /// A `#GUID` entry (1-based), formatted the way .NET's `Guid.ToString("D")` prints it: the
    /// first three fields are little-endian integers, the rest is byte order.
    pub(super) fn guid(&self, index: u32) -> Option<String> {
        if index == 0 {
            return None;
        }
        let g = slice_at(self.guids, (index as usize - 1).checked_mul(16)?, 16)?;
        Some(format!(
            "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
            g[3], g[2], g[1], g[0], g[5], g[4], g[7], g[6], g[8], g[9], g[10], g[11], g[12], g[13], g[14], g[15]
        ))
    }

    // -- names -----------------------------------------------------------------------------

    /// Full name of a TypeDef row, nested types joined with `+` as the CLR writes them.
    pub(super) fn type_def_name(&self, row: u32) -> Option<String> {
        self.type_def_name_depth(row, 0)
    }

    fn type_def_name_depth(&self, row: u32, depth: u32) -> Option<String> {
        // Nesting in real code is a few levels deep; a chain longer than this is a corrupt
        // NestedClass table looping back on itself.
        if depth > 32 {
            return None;
        }
        let name = self.string_cell(Table::TypeDef, row, 1)?;
        if let Some(&outer) = self.nested_parent.get(&row) {
            if let Some(outer_name) = self.type_def_name_depth(outer, depth + 1) {
                return Some(format!("{outer_name}+{name}"));
            }
        }
        let ns = self.string_cell(Table::TypeDef, row, 2).unwrap_or("");
        Some(if ns.is_empty() { name.to_string() } else { format!("{ns}.{name}") })
    }

    /// Full name of a TypeRef row; a nested reference has another TypeRef as its scope.
    pub(super) fn type_ref_name(&self, row: u32) -> Option<String> {
        self.type_ref_name_depth(row, 0)
    }

    fn type_ref_name_depth(&self, row: u32, depth: u32) -> Option<String> {
        if depth > 32 {
            return None;
        }
        let name = self.string_cell(Table::TypeRef, row, 1)?;
        if let Some((Table::TypeRef, outer)) = self.coded(Table::TypeRef, row, 0, Coded::ResolutionScope) {
            if let Some(outer_name) = self.type_ref_name_depth(outer, depth + 1) {
                return Some(format!("{outer_name}+{name}"));
            }
        }
        let ns = self.string_cell(Table::TypeRef, row, 2).unwrap_or("");
        Some(if ns.is_empty() { name.to_string() } else { format!("{ns}.{name}") })
    }

    /// The name behind anything that can stand for a type. A TypeSpec (a constructed type such
    /// as `List<Foo>`) is deliberately unnamed: patch targets are never generic instantiations
    /// in practice, and the sidecar this replaces left them unnamed too, so its answers and
    /// ours stay the same.
    pub(super) fn type_name(&self, table: Table, row: u32) -> Option<String> {
        match table {
            Table::TypeDef => self.type_def_name(row),
            Table::TypeRef => self.type_ref_name(row),
            _ => None,
        }
    }

    /// The TypeDef row that declares a MethodDef row.
    pub(super) fn method_owner(&self, method: u32) -> Option<u32> {
        let owner = *self.method_owner.get(method.checked_sub(1)? as usize)?;
        if owner == 0 {
            None
        } else {
            Some(owner)
        }
    }

    // -- the tables the scanner reads, as typed rows ----------------------------------------

    pub(super) fn module_mvid(&self) -> Option<String> {
        self.guid(self.cell(Table::Module, 1, 2)?)
    }

    pub(super) fn assembly_name(&self) -> Option<&'a str> {
        self.string_cell(Table::Assembly, 1, 7)
    }

    pub(super) fn assembly_ref_names(&self) -> impl Iterator<Item = &'a str> + '_ {
        (1..=self.rows(Table::AssemblyRef)).filter_map(move |row| self.string_cell(Table::AssemblyRef, row, 6))
    }

    pub(super) fn type_ref_namespace(&self, row: u32) -> Option<&'a str> {
        self.string_cell(Table::TypeRef, row, 2)
    }

    pub(super) fn type_def_namespace(&self, row: u32) -> Option<&'a str> {
        self.string_cell(Table::TypeDef, row, 2)
    }

    pub(super) fn type_def_short_name(&self, row: u32) -> Option<&'a str> {
        self.string_cell(Table::TypeDef, row, 1)
    }

    pub(super) fn type_ref_short_name(&self, row: u32) -> Option<&'a str> {
        self.string_cell(Table::TypeRef, row, 1)
    }

    /// The name of a TypeDef's base type, through TypeDef or TypeRef.
    pub(super) fn type_def_base_name(&self, row: u32) -> Option<String> {
        let (table, base) = self.coded(Table::TypeDef, row, 3, Coded::TypeDefOrRef)?;
        self.type_name(table, base)
    }

    /// The MethodDef rows a TypeDef owns, as a range of 1-based rows.
    pub(super) fn type_def_methods(&self, row: u32) -> std::ops::Range<u32> {
        let methods = self.rows(Table::MethodDef);
        let types = self.rows(Table::TypeDef);
        let first = self.cell(Table::TypeDef, row, 5).unwrap_or(0).max(1);
        let next = if row < types { self.cell(Table::TypeDef, row + 1, 5).unwrap_or(methods + 1) } else { methods + 1 };
        first..next.min(methods + 1).max(first)
    }

    pub(super) fn method_rva(&self, row: u32) -> Option<u32> {
        self.cell(Table::MethodDef, row, 0)
    }

    pub(super) fn method_name(&self, row: u32) -> Option<&'a str> {
        self.string_cell(Table::MethodDef, row, 3)
    }

    pub(super) fn method_signature(&self, row: u32) -> Option<&'a [u8]> {
        self.blob_cell(Table::MethodDef, row, 4)
    }

    pub(super) fn member_ref_parent(&self, row: u32) -> Option<(Table, u32)> {
        self.coded(Table::MemberRef, row, 0, Coded::MemberRefParent)
    }

    pub(super) fn member_ref_name(&self, row: u32) -> Option<&'a str> {
        self.string_cell(Table::MemberRef, row, 1)
    }

    pub(super) fn member_ref_signature(&self, row: u32) -> Option<&'a [u8]> {
        self.blob_cell(Table::MemberRef, row, 2)
    }

    pub(super) fn method_spec_method(&self, row: u32) -> Option<(Table, u32)> {
        self.coded(Table::MethodSpec, row, 0, Coded::MethodDefOrRef)
    }

    pub(super) fn custom_attribute_parent(&self, row: u32) -> Option<(Table, u32)> {
        self.coded(Table::CustomAttribute, row, 0, Coded::HasCustomAttribute)
    }

    pub(super) fn custom_attribute_ctor(&self, row: u32) -> Option<(Table, u32)> {
        self.coded(Table::CustomAttribute, row, 1, Coded::CustomAttributeType)
    }

    pub(super) fn custom_attribute_value(&self, row: u32) -> Option<&'a [u8]> {
        self.blob_cell(Table::CustomAttribute, row, 2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coded_index_widths_follow_the_standard() {
        assert_eq!(Coded::TypeDefOrRef.tag_bits(), 2);
        assert_eq!(Coded::HasCustomAttribute.tag_bits(), 5);
        assert_eq!(Coded::MethodDefOrRef.tag_bits(), 1);
        assert_eq!(Coded::MemberRefParent.tag_bits(), 3);
        assert_eq!(Coded::CustomAttributeType.tag_bits(), 3);
        assert_eq!(Coded::ResolutionScope.tag_bits(), 2);
        assert_eq!(Coded::TypeDefOrRef.decode(0b101), Some((Table::TypeRef, 1)));
        assert_eq!(Coded::CustomAttributeType.decode(0b1011), Some((Table::MemberRef, 1)));
        assert_eq!(Coded::CustomAttributeType.decode(0b1000), None, "an unused tag");
        assert_eq!(Coded::TypeDefOrRef.decode(0), None, "row zero is null");
    }

    #[test]
    fn garbage_is_refused() {
        assert!(Metadata::parse(&[]).is_err());
        assert!(Metadata::parse(b"BSJB").is_err());
        let mut root = vec![0u8; 64];
        root[..4].copy_from_slice(b"BSJB");
        assert!(Metadata::parse(&root).is_err(), "no tables stream");
    }
}
