//! The blob heap's encodings: compressed integers, method signatures and custom attribute
//! values (partition II.23).
//!
//! A custom attribute blob does not say what it contains; it is a bare sequence of values whose
//! types come from the constructor's signature. So reading `[HarmonyPatch(typeof(Pawn), "Tick")]`
//! means finding the constructor, decoding its parameter types from its signature blob, and
//! then reading the value blob one typed argument at a time. That is the whole reason the
//! signature decoder exists here; it goes no deeper than an attribute needs.

use super::metadata::{Coded, Metadata};
use super::pe::{slice_at, u16_at, u32_at, u8_at};

/// A compressed unsigned integer (II.23.2): one, two or four bytes, the top bits saying which.
pub(super) fn read_compressed(data: &[u8], at: &mut usize) -> Option<u32> {
    let b0 = u8_at(data, *at)?;
    if b0 & 0x80 == 0 {
        *at += 1;
        Some(u32::from(b0))
    } else if b0 & 0xC0 == 0x80 {
        let b1 = u8_at(data, *at + 1)?;
        *at += 2;
        Some((u32::from(b0 & 0x3F) << 8) | u32::from(b1))
    } else if b0 & 0xE0 == 0xC0 {
        let rest = slice_at(data, *at + 1, 3)?;
        *at += 4;
        Some((u32::from(b0 & 0x1F) << 24) | (u32::from(rest[0]) << 16) | (u32::from(rest[1]) << 8) | u32::from(rest[2]))
    } else {
        None
    }
}

/// A `TypeDefOrRefOrSpecEncoded` token: the row shifted left two, the table in the low bits.
fn read_type_token(data: &[u8], at: &mut usize) -> Option<(super::metadata::Table, u32)> {
    let raw = read_compressed(data, at)?;
    Coded::TypeDefOrRef.decode(raw)
}

/// A parameter type, kept only as precisely as attribute decoding needs. Anything an attribute
/// cannot carry (pointers, generics, by-reference parameters) is `Other`, which still has to be
/// parsed so the parameters after it are found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SigType {
    /// `System.Int32` and the other ELEMENT_TYPE_* names, as .NET prints them.
    Primitive(&'static str),
    /// A class or value type by full name. Whether it is an enum is not recorded anywhere
    /// reachable without loading the assembly; the attribute decoder assumes so.
    Named(String),
    SzArray(Box<SigType>),
    Other,
}

impl SigType {
    /// `System.Type` in any of the ways it can be spelled, including the assembly-qualified one.
    pub(super) fn is_system_type(&self) -> bool {
        match self {
            SigType::Named(n) => n == "System.Type" || n.starts_with("System.Type,"),
            _ => false,
        }
    }
}

fn primitive_name(code: u8) -> Option<&'static str> {
    Some(match code {
        0x01 => "System.Void",
        0x02 => "System.Boolean",
        0x03 => "System.Char",
        0x04 => "System.SByte",
        0x05 => "System.Byte",
        0x06 => "System.Int16",
        0x07 => "System.UInt16",
        0x08 => "System.Int32",
        0x09 => "System.UInt32",
        0x0A => "System.Int64",
        0x0B => "System.UInt64",
        0x0C => "System.Single",
        0x0D => "System.Double",
        0x0E => "System.String",
        0x16 => "System.TypedReference",
        0x18 => "System.IntPtr",
        0x19 => "System.UIntPtr",
        0x1C => "System.Object",
        _ => return None,
    })
}

/// Signatures nest (an array of a pointer to a generic of ...); real ones are shallow, and a
/// blob that nests past this is corrupt.
const MAX_DEPTH: u32 = 32;

/// Skip any custom modifiers (`modopt`/`modreq`) in front of a type.
fn skip_modifiers(data: &[u8], at: &mut usize) -> Option<()> {
    while matches!(u8_at(data, *at), Some(0x1F) | Some(0x20)) {
        *at += 1;
        read_type_token(data, at)?;
    }
    Some(())
}

/// One `Type` (II.23.2.12), consumed from the blob whether or not it is representable.
fn read_type(md: &Metadata, data: &[u8], at: &mut usize, depth: u32) -> Option<SigType> {
    if depth > MAX_DEPTH {
        return None;
    }
    skip_modifiers(data, at)?;
    let code = u8_at(data, *at)?;
    *at += 1;
    Some(match code {
        0x11 | 0x12 => {
            let (table, row) = read_type_token(data, at)?;
            match md.type_name(table, row) {
                Some(name) => SigType::Named(name),
                None => SigType::Other,
            }
        }
        0x0F | 0x10 | 0x45 => {
            // Pointer, by-reference and pinned: a prefix on another type.
            skip_modifiers(data, at)?;
            if code == 0x0F && u8_at(data, *at) == Some(0x01) {
                *at += 1;
            } else {
                read_type(md, data, at, depth + 1)?;
            }
            SigType::Other
        }
        0x13 | 0x1E => {
            read_compressed(data, at)?;
            SigType::Other
        }
        0x14 => {
            // Element type, rank, then the sizes and lower bounds. Each count is bounded by the
            // blob itself: every entry consumes at least a byte, so a corrupt count runs out of
            // bytes rather than time.
            read_type(md, data, at, depth + 1)?;
            read_compressed(data, at)?;
            let sizes = read_compressed(data, at)?;
            for _ in 0..sizes {
                read_compressed(data, at)?;
            }
            let bounds = read_compressed(data, at)?;
            for _ in 0..bounds {
                read_compressed(data, at)?;
            }
            SigType::Other
        }
        0x15 => {
            let kind = u8_at(data, *at)?;
            if kind != 0x11 && kind != 0x12 {
                return None;
            }
            *at += 1;
            read_type_token(data, at)?;
            let count = read_compressed(data, at)?;
            for _ in 0..count {
                read_type(md, data, at, depth + 1)?;
            }
            SigType::Other
        }
        0x1B => {
            read_method_signature_at(md, data, at, depth + 1)?;
            SigType::Other
        }
        0x1D => {
            let element = read_type(md, data, at, depth + 1)?;
            SigType::SzArray(Box::new(element))
        }
        _ => match primitive_name(code) {
            Some(name) => SigType::Primitive(name),
            None => return None,
        },
    })
}

/// A method signature (II.23.2.1): calling convention, optional generic arity, parameter
/// count, return type, parameters. Returns the parameter types.
fn read_method_signature_at(md: &Metadata, data: &[u8], at: &mut usize, depth: u32) -> Option<Vec<SigType>> {
    if depth > MAX_DEPTH {
        return None;
    }
    let convention = u8_at(data, *at)?;
    *at += 1;
    if convention & 0x10 != 0 {
        read_compressed(data, at)?;
    }
    let count = read_compressed(data, at)?;
    // The return type is read the same way as a parameter; its value is not needed.
    read_type(md, data, at, depth + 1)?;
    // A count larger than the blob could hold is a corrupt signature, not a long one.
    let mut params = Vec::with_capacity(count.min(64) as usize);
    for _ in 0..count {
        if *at >= data.len() {
            return None;
        }
        // A vararg call site marks where the fixed parameters end.
        if u8_at(data, *at) == Some(0x41) {
            break;
        }
        params.push(read_type(md, data, at, depth + 1)?);
    }
    Some(params)
}

/// The parameter types of a MethodDef or MemberRef signature blob.
pub(super) fn method_parameters(md: &Metadata, blob: &[u8]) -> Option<Vec<SigType>> {
    let mut at = 0usize;
    read_method_signature_at(md, blob, &mut at, 0)
}

// ---------------------------------------------------------------------------------------------
// Custom attribute values (II.23.3)
// ---------------------------------------------------------------------------------------------

/// A decoded attribute argument. Only the shapes the Harmony attributes use are told apart;
/// the rest is `Other`, present so positions still line up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Value {
    /// A `string` argument; `None` is a null string.
    Str(Option<String>),
    /// A `System.Type` argument, as the serialized (assembly-qualified) name; `None` is null.
    Type(Option<String>),
    /// An `int`, or any enum: every enum Harmony puts in an attribute has the default
    /// underlying type, and the blob has no way to say otherwise.
    Int32(i32),
    /// An array; `None` is a null array.
    Array(Option<Vec<Value>>),
    Other,
}

/// A decoded custom attribute: the fixed arguments in constructor order, then the named ones.
#[derive(Debug, Clone, Default)]
pub(super) struct Attribute {
    pub(super) fixed: Vec<(SigType, Value)>,
    pub(super) named: Vec<(String, Value)>,
}

/// A `SerString`: 0xFF for null, otherwise a compressed length and UTF-8 bytes.
fn read_ser_string(data: &[u8], at: &mut usize) -> Option<Option<String>> {
    if u8_at(data, *at)? == 0xFF {
        *at += 1;
        return Some(None);
    }
    let len = read_compressed(data, at)? as usize;
    let bytes = slice_at(data, *at, len)?;
    *at += len;
    Some(Some(String::from_utf8_lossy(bytes).into_owned()))
}

/// A `FieldOrPropType` (II.23.3), which is how named arguments and boxed values say their type.
fn read_field_or_prop_type(data: &[u8], at: &mut usize, depth: u32) -> Option<SigType> {
    if depth > MAX_DEPTH {
        return None;
    }
    let code = u8_at(data, *at)?;
    *at += 1;
    Some(match code {
        0x50 => SigType::Named("System.Type".into()),
        0x51 => SigType::Primitive("System.Object"),
        0x55 => SigType::Named(read_ser_string(data, at)?.unwrap_or_default()),
        0x1D => SigType::SzArray(Box::new(read_field_or_prop_type(data, at, depth + 1)?)),
        _ => SigType::Primitive(primitive_name(code).filter(|n| *n != "System.Void")?),
    })
}

fn read_value(data: &[u8], at: &mut usize, ty: &SigType, depth: u32) -> Option<Value> {
    if depth > MAX_DEPTH {
        return None;
    }
    let take = |at: &mut usize, n: usize| -> Option<()> {
        slice_at(data, *at, n)?;
        *at += n;
        Some(())
    };
    Some(match ty {
        SigType::Primitive(name) => match *name {
            "System.Boolean" | "System.SByte" | "System.Byte" => {
                take(at, 1)?;
                Value::Other
            }
            "System.Char" | "System.Int16" | "System.UInt16" => {
                take(at, 2)?;
                Value::Other
            }
            "System.Int32" => {
                let v = u32_at(data, *at)? as i32;
                *at += 4;
                Value::Int32(v)
            }
            "System.UInt32" | "System.Single" => {
                take(at, 4)?;
                Value::Other
            }
            "System.Int64" | "System.UInt64" | "System.Double" => {
                take(at, 8)?;
                Value::Other
            }
            "System.String" => Value::Str(read_ser_string(data, at)?),
            "System.Object" => {
                // A boxed value carries its own type tag in front of it.
                let inner = read_field_or_prop_type(data, at, depth + 1)?;
                read_value(data, at, &inner, depth + 1)?
            }
            _ => return None,
        },
        SigType::Named(_) if ty.is_system_type() => Value::Type(read_ser_string(data, at)?),
        SigType::Named(_) => {
            let v = u32_at(data, *at)? as i32;
            *at += 4;
            Value::Int32(v)
        }
        SigType::SzArray(element) => {
            let count = u32_at(data, *at)?;
            *at += 4;
            if count == 0xFFFF_FFFF {
                Value::Array(None)
            } else {
                // Every element takes at least a byte, so a count past the blob's end is corrupt.
                if count as usize > data.len().saturating_sub(*at) {
                    return None;
                }
                let mut items = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    items.push(read_value(data, at, element, depth + 1)?);
                }
                Value::Array(Some(items))
            }
        }
        SigType::Other => return None,
    })
}

/// Decode an attribute's value blob against its constructor's parameter types. `None` when the
/// blob does not fit the signature; the caller then treats the attribute as carrying nothing,
/// which is what the sidecar did when its decoder threw.
pub(super) fn decode_attribute(blob: &[u8], params: &[SigType]) -> Option<Attribute> {
    let mut at = 0usize;
    if u16_at(blob, at)? != 0x0001 {
        return None;
    }
    at += 2;
    let mut attribute = Attribute::default();
    for ty in params {
        let value = read_value(blob, &mut at, ty, 0)?;
        attribute.fixed.push((ty.clone(), value));
    }
    // An attribute with no named arguments may end right here; the count is then optional.
    let Some(named_count) = u16_at(blob, at) else {
        return Some(attribute);
    };
    at += 2;
    for _ in 0..named_count {
        let kind = u8_at(blob, at)?;
        if kind != 0x53 && kind != 0x54 {
            return None;
        }
        at += 1;
        let ty = read_field_or_prop_type(blob, &mut at, 0)?;
        let name = read_ser_string(blob, &mut at)??;
        let value = read_value(blob, &mut at, &ty, 0)?;
        attribute.named.push((name, value));
    }
    Some(attribute)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compressed_integers_in_all_three_widths() {
        let mut at = 0;
        assert_eq!(read_compressed(&[0x03], &mut at), Some(3));
        at = 0;
        assert_eq!(read_compressed(&[0x80, 0x80], &mut at), Some(0x80));
        assert_eq!(at, 2);
        at = 0;
        assert_eq!(read_compressed(&[0xC0, 0x00, 0x40, 0x00], &mut at), Some(0x4000));
        assert_eq!(at, 4);
        at = 0;
        assert_eq!(read_compressed(&[0xE0], &mut at), None);
        at = 0;
        assert_eq!(read_compressed(&[0x80], &mut at), None, "truncated");
    }

    #[test]
    fn an_attribute_blob_with_a_type_a_string_and_a_named_int() {
        // [Attr(typeof(Pawn), "Tick") { methodType = 3 }]
        let mut blob = vec![0x01, 0x00];
        let ty = b"RimWorld.Pawn, Assembly-CSharp";
        blob.push(ty.len() as u8);
        blob.extend_from_slice(ty);
        blob.push(4);
        blob.extend_from_slice(b"Tick");
        blob.extend_from_slice(&[0x01, 0x00]); // one named argument
        blob.push(0x54); // property
        blob.push(0x55); // enum
        let en = b"HarmonyLib.MethodType, 0Harmony";
        blob.push(en.len() as u8);
        blob.extend_from_slice(en);
        blob.push(10);
        blob.extend_from_slice(b"methodType");
        blob.extend_from_slice(&3i32.to_le_bytes());
        let params = [SigType::Named("System.Type".into()), SigType::Primitive("System.String")];
        let a = decode_attribute(&blob, &params).expect("decodes");
        assert_eq!(a.fixed[0].1, Value::Type(Some("RimWorld.Pawn, Assembly-CSharp".into())));
        assert_eq!(a.fixed[1].1, Value::Str(Some("Tick".into())));
        assert_eq!(a.named, vec![("methodType".to_string(), Value::Int32(3))]);
        // Chop it anywhere and the decoder says no rather than reading past the end.
        for n in 0..blob.len() {
            let _ = decode_attribute(&blob[..n], &params);
        }
    }

    #[test]
    fn arrays_and_nulls() {
        // [Attr(new[] { typeof(int), null })] as Type[]
        let mut blob = vec![0x01, 0x00, 0x02, 0x00, 0x00, 0x00];
        blob.push(12);
        blob.extend_from_slice(b"System.Int32");
        blob.push(0xFF);
        let params = [SigType::SzArray(Box::new(SigType::Named("System.Type".into())))];
        let a = decode_attribute(&blob, &params).expect("decodes");
        assert_eq!(a.fixed[0].1, Value::Array(Some(vec![Value::Type(Some("System.Int32".into())), Value::Type(None)])));
        let null_array = [0x01, 0x00, 0xFF, 0xFF, 0xFF, 0xFF];
        assert_eq!(decode_attribute(&null_array, &params).expect("decodes").fixed[0].1, Value::Array(None));
        let huge = [0x01, 0x00, 0xFF, 0xFF, 0xFF, 0x7F];
        assert!(decode_attribute(&huge, &params).is_none(), "a count past the end is refused");
    }
}
