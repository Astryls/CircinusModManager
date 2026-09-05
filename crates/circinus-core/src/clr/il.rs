//! Just enough CIL to walk a method body and read the operands of five instructions.
//!
//! Nothing here is simulated. Stepping over instructions only needs their operand widths,
//! which is one 256-entry table (and a smaller one for the two-byte opcodes); the scanner then
//! pairs the strings and type tokens it saw with the calls that follow them. That is enough for
//! the shapes mods actually write — `new Harmony("id")`, `AccessTools.Method(typeof(Pawn),
//! "Tick")` — and anything more involved is reported as a computed target rather than guessed.

use super::pe::{u16_at, u32_at, u8_at, Pe};

pub(super) const CALL: u16 = 0x28;
pub(super) const CALLVIRT: u16 = 0x6F;
pub(super) const LDSTR: u16 = 0x72;
pub(super) const LDTOKEN: u16 = 0xD0;
pub(super) const NEWOBJ: u16 = 0x73;
pub(super) const NEWARR: u16 = 0x8D;

/// Operand width of an opcode, or one of the two special cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Width {
    Bytes(u8),
    /// `switch`: a count followed by that many 32-bit targets.
    Switch,
    /// An opcode ECMA-335 leaves unused: hitting one means the instruction stream has been
    /// lost, so the walk stops rather than reporting nonsense.
    Invalid,
}

/// Operand widths of the one-byte opcodes.
fn width_one_byte(op: u8) -> Width {
    use Width::*;
    match op {
        // Short-form inline operands: one byte.
        0x0E..=0x13 | 0x1F | 0x2B..=0x37 | 0xDE => Bytes(1),
        // Four-byte operands: tokens, int32 constants, long branch targets.
        0x20 | 0x22 | 0x27..=0x29 | 0x38..=0x44 | 0x6F..=0x75 | 0x79 | 0x7B..=0x81 | 0x8C | 0x8D | 0x8F | 0xA3..=0xA5 | 0xC2 | 0xC6 | 0xD0 | 0xDD => Bytes(4),
        // Eight-byte constants.
        0x21 | 0x23 => Bytes(8),
        0x45 => Switch,
        0x24 | 0x77 | 0x78 | 0xA6..=0xB2 | 0xBB..=0xC1 | 0xC4 | 0xC5 | 0xC7..=0xCF | 0xE1..=0xFD | 0xFF => Invalid,
        _ => Bytes(0),
    }
}

/// Operand widths of the opcodes behind the `0xFE` prefix.
fn width_two_byte(op: u8) -> Width {
    use Width::*;
    match op {
        0x06 | 0x07 | 0x15 | 0x16 | 0x1C => Bytes(4),
        0x09..=0x0E => Bytes(2),
        0x12 | 0x19 => Bytes(1),
        0x08 | 0x10 => Invalid,
        0x00..=0x1E => Bytes(0),
        _ => Invalid,
    }
}

/// Walk a method body, yielding `(opcode, operand)`: the operand is the value of a four-byte
/// operand and 0 for anything else. Two-byte opcodes come through as `0x100 | second`; nothing
/// here needs them, but they must still be stepped over. The walk stops silently at the first
/// byte it cannot interpret.
pub(super) fn instructions(il: &[u8]) -> impl Iterator<Item = (u16, u32)> + '_ {
    let mut at = 0usize;
    std::iter::from_fn(move || {
        let first = u8_at(il, at)?;
        at += 1;
        let (op, width) = if first == 0xFE {
            let second = u8_at(il, at)?;
            at += 1;
            (0x100 | u16::from(second), width_two_byte(second))
        } else {
            (u16::from(first), width_one_byte(first))
        };
        match width {
            Width::Invalid => None,
            Width::Switch => {
                let count = u32_at(il, at)? as usize;
                at += 4;
                // A bogus count would run the index off the end; stop instead of overflowing.
                if count > (il.len() - at) / 4 {
                    return None;
                }
                at += count * 4;
                Some((op, 0))
            }
            Width::Bytes(n) => {
                let n = n as usize;
                if at + n > il.len() {
                    return None;
                }
                let operand = if n == 4 { u32_at(il, at)? } else { 0 };
                at += n;
                Some((op, operand))
            }
        }
    })
}

/// The IL bytes of the method body at an RVA: a tiny header is one byte holding the length, a
/// fat header is twelve bytes with the length at offset four. `None` when the header is not
/// one of the two or the body runs past the end of the file.
pub(super) fn method_body<'a>(pe: &Pe<'a>, rva: u32) -> Option<&'a [u8]> {
    let body = pe.slice_at_rva(rva)?;
    let first = u8_at(body, 0)?;
    match first & 0x03 {
        0x02 => {
            let size = (first >> 2) as usize;
            body.get(1..1 + size)
        }
        0x03 => {
            let header_words = (u16_at(body, 0)? >> 12) as usize;
            let code_size = u32_at(body, 4)? as usize;
            let start = header_words.checked_mul(4)?;
            body.get(start..start.checked_add(code_size)?)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_body_walks_to_its_calls() {
        // ldstr 0x70000001; newobj 0x0A000002; ret
        let il = [0x72, 0x01, 0x00, 0x00, 0x70, 0x73, 0x02, 0x00, 0x00, 0x0A, 0x2A];
        let seen: Vec<_> = instructions(&il).collect();
        assert_eq!(seen, vec![(LDSTR, 0x7000_0001), (NEWOBJ, 0x0A00_0002), (0x2A, 0)]);
    }

    #[test]
    fn the_walk_stops_on_junk_and_truncation() {
        assert_eq!(instructions(&[0xFF]).count(), 0);
        assert_eq!(instructions(&[0x72, 0x01]).count(), 0, "truncated operand");
        assert_eq!(instructions(&[0xFE]).count(), 0, "prefix with nothing after it");
        // switch with a count that would run off the end
        assert_eq!(instructions(&[0x45, 0xFF, 0xFF, 0xFF, 0x7F]).count(), 0);
        // switch with two targets, then nop
        let il = [0x45, 0x02, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 0x00];
        assert_eq!(instructions(&il).collect::<Vec<_>>(), vec![(0x45, 0), (0x00, 0)]);
        // two-byte opcodes are stepped over: constrained. <token>; callvirt <token>
        let il = [0xFE, 0x16, 1, 0, 0, 0x1B, 0x6F, 3, 0, 0, 0x0A];
        assert_eq!(instructions(&il).collect::<Vec<_>>(), vec![(0x116, 0x1B00_0001), (CALLVIRT, 0x0A00_0003)]);
    }
}
