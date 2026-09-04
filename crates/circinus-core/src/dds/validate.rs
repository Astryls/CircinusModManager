//! Check a DDS file the way a strict loader would, then decode its top level and compare it
//! with the source. A file that fails here is deleted before the game can ever see it.

use super::encode::{level_bytes, mip_count, Encoded, Format};
use image::RgbaImage;

/// Below this the output is garbage (wrong stride, swapped channels, truncated data), not
/// merely lossy: real textures land at 35–50 dB, pathological synthetic ones around 25.
pub const MIN_PSNR_DB: f64 = 18.0;
/// Fraction of texels whose alpha flipped across the 50% line although the source was
/// clearly on one side of it (quantisation legitimately moves texels that sit near it).
pub const MAX_COVERAGE_DIFF: f64 = 0.02;
const COVERAGE_MARGIN: i32 = 24;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DdsInfo {
    pub format: Format,
    pub width: u32,
    pub height: u32,
    pub levels: u32,
    pub data_offset: usize,
}

fn u32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes(b[i..i + 4].try_into().unwrap())
}

/// Parse and check the header and the file length.
pub fn parse(bytes: &[u8]) -> Result<DdsInfo, String> {
    if bytes.len() < 128 || &bytes[0..4] != b"DDS " {
        return Err("not a DDS file".into());
    }
    if u32_at(bytes, 4) != 124 {
        return Err("header size is not 124".into());
    }
    let flags = u32_at(bytes, 8);
    for (bit, name) in [(0x1, "CAPS"), (0x2, "HEIGHT"), (0x4, "WIDTH"), (0x1000, "PIXELFORMAT")] {
        if flags & bit == 0 {
            return Err(format!("DDSD_{name} flag missing"));
        }
    }
    let height = u32_at(bytes, 12);
    let width = u32_at(bytes, 16);
    if width == 0 || height == 0 {
        return Err("zero-sized texture".into());
    }
    if u32_at(bytes, 24) > 1 {
        return Err("volume textures are not supported".into());
    }
    let levels = if flags & 0x20000 != 0 { u32_at(bytes, 28).max(1) } else { 1 };
    if u32_at(bytes, 76) != 32 || u32_at(bytes, 80) & 0x4 == 0 {
        return Err("pixel format is not a fourCC".into());
    }
    let fourcc = &bytes[84..88];
    let caps2 = u32_at(bytes, 112);
    if caps2 & 0x200 != 0 {
        return Err("cubemaps are not supported".into());
    }
    let (format, data_offset) = match fourcc {
        b"DXT1" => (Format::Bc1, 128),
        b"DXT5" => (Format::Bc3, 128),
        b"DX10" => {
            if bytes.len() < 148 {
                return Err("DX10 header missing".into());
            }
            let dxgi = u32_at(bytes, 128);
            let dim = u32_at(bytes, 132);
            let array = u32_at(bytes, 140);
            if dim != 3 {
                return Err(format!("resource dimension {dim} is not TEXTURE2D"));
            }
            if array != 1 {
                return Err(format!("array size {array}"));
            }
            match dxgi {
                98 | 99 => (Format::Bc7, 148),
                71 | 72 => (Format::Bc1, 148),
                77 | 78 => (Format::Bc3, 148),
                other => return Err(format!("DXGI format {other} is not BC1/BC3/BC7")),
            }
        }
        other => return Err(format!("fourCC {:?} is not DXT1/DXT5/DX10", String::from_utf8_lossy(other))),
    };
    if levels > 1 && levels != mip_count(width, height) {
        return Err(format!("{levels} mip levels; a full chain for {width}×{height} has {}", mip_count(width, height)));
    }
    let mut expected = data_offset;
    let (mut w, mut h) = (width, height);
    for _ in 0..levels {
        expected += level_bytes(format, w, h);
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    if bytes.len() != expected {
        return Err(format!("file is {} bytes, {expected} expected for {width}×{height} {} with {levels} levels", bytes.len(), format.as_str()));
    }
    Ok(DdsInfo { format, width, height, levels, data_offset })
}

/// Decode the top level (padded to whole blocks).
pub fn decode_top(bytes: &[u8], info: &DdsInfo) -> RgbaImage {
    let (pw, ph) = (info.width.div_ceil(4) * 4, info.height.div_ceil(4) * 4);
    let mut out = vec![0u8; (pw * ph * 4) as usize];
    let bsz = info.format.block_bytes();
    let bw = pw / 4;
    let pitch = (pw * 4) as usize;
    for by in 0..ph / 4 {
        for bx in 0..bw {
            let bi = info.data_offset + ((by * bw + bx) as usize) * bsz;
            let off = ((by * 4 * pw + bx * 4) * 4) as usize;
            let block = &bytes[bi..bi + bsz];
            match info.format {
                Format::Bc1 => bcdec_rs::bc1(block, &mut out[off..], pitch),
                Format::Bc3 => bcdec_rs::bc3(block, &mut out[off..], pitch),
                Format::Bc7 => bcdec_rs::bc7(block, &mut out[off..], pitch),
            }
        }
    }
    let mut img = RgbaImage::from_raw(pw, ph, out).expect("sized buffer");
    if (pw, ph) != (info.width, info.height) {
        img = image::imageops::crop_imm(&img, 0, 0, info.width, info.height).to_image();
    }
    img
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub psnr_db: f64,
    pub coverage_diff: f64,
}

/// PSNR in premultiplied space (invisible colour under alpha 0 does not count) and the share
/// of texels whose alpha crossed 50%.
pub fn compare(a: &RgbaImage, b: &RgbaImage) -> Report {
    let n = (a.width() * a.height()) as f64;
    let mut se = 0.0f64;
    let mut flipped = 0u64;
    for (p, q) in a.pixels().zip(b.pixels()) {
        let (pa, qa) = (p[3] as f64 / 255.0, q[3] as f64 / 255.0);
        for c in 0..3 {
            let d = p[c] as f64 * pa - q[c] as f64 * qa;
            se += d * d;
        }
        let d = p[3] as f64 - q[3] as f64;
        se += d * d;
        if (p[3] as i32 - 128).abs() > COVERAGE_MARGIN && (p[3] >= 128) != (q[3] >= 128) {
            flipped += 1;
        }
    }
    let mse = se / (n * 4.0);
    let psnr_db = if mse <= 0.0 { 99.0 } else { 10.0 * (255.0f64 * 255.0 / mse).log10() };
    Report { psnr_db, coverage_diff: flipped as f64 / n }
}

/// Everything at once: header, length, dimensions against what was encoded, then decode and
/// compare with the (fixed-size, dilated) source level.
pub fn check(encoded: &Encoded) -> Result<Report, String> {
    let info = parse(&encoded.bytes)?;
    if info.format != encoded.format || info.width != encoded.width || info.height != encoded.height || info.levels != encoded.levels {
        return Err(format!("header says {}×{} {} ×{} levels, encoder produced {}×{} {} ×{}", info.width, info.height, info.format.as_str(), info.levels, encoded.width, encoded.height, encoded.format.as_str(), encoded.levels));
    }
    let decoded = decode_top(&encoded.bytes, &info);
    if decoded.dimensions() != encoded.top.dimensions() {
        return Err("decoded size differs from the source".into());
    }
    let r = compare(&encoded.top, &decoded);
    if r.psnr_db < MIN_PSNR_DB {
        return Err(format!("decoded image differs from the source too much ({:.1} dB)", r.psnr_db));
    }
    if r.coverage_diff > MAX_COVERAGE_DIFF {
        return Err(format!("alpha coverage differs from the source ({:.1}% of texels)", r.coverage_diff * 100.0));
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dds::encode::{encode_rgba, Options};
    use image::Rgba;

    fn photo(w: u32, h: u32, alpha: bool) -> RgbaImage {
        // smooth content: what real textures look like to an encoder
        RgbaImage::from_fn(w, h, |x, y| {
            let fx = x as f32 / w as f32;
            let fy = y as f32 / h as f32;
            let r = (120.0 + 100.0 * (fx * 6.0).sin()) as u8;
            let g = (120.0 + 100.0 * (fy * 5.0).cos()) as u8;
            let b = (128.0 + 80.0 * ((fx + fy) * 4.0).sin()) as u8;
            let a = if !alpha { 255 } else if ((fx - 0.5).powi(2) + (fy - 0.5).powi(2)).sqrt() < 0.4 { 255 } else { 0 };
            Rgba([r, g, b, a])
        })
    }

    #[test]
    fn encoded_files_validate_with_high_psnr() {
        for (alpha, fmt) in [(false, Format::Bc1), (true, Format::Bc7), (true, Format::Bc3)] {
            let opts = Options { alpha_format: fmt, ..Options::default() };
            let e = encode_rgba(photo(96, 64, alpha), &opts).unwrap();
            let r = check(&e).unwrap();
            assert!(r.psnr_db > 30.0, "{fmt:?}: {:.1} dB", r.psnr_db);
            assert!(r.coverage_diff < 0.005, "{fmt:?}: coverage {}", r.coverage_diff);
        }
    }

    #[test]
    fn rejects_truncated_and_garbage() {
        let e = encode_rgba(photo(32, 32, true), &Options::default()).unwrap();
        let mut truncated = e.clone();
        truncated.bytes.truncate(truncated.bytes.len() - 5);
        assert!(check(&truncated).unwrap_err().contains("bytes"));
        let mut garbage = e.clone();
        for b in garbage.bytes[148..].iter_mut() {
            *b = 0x5a;
        }
        let err = check(&garbage).unwrap_err();
        assert!(err.contains("differs"), "{err}");
        assert!(parse(b"nope").is_err());
        let mut bad_levels = e.clone();
        bad_levels.bytes[28] = 2; // claims 2 levels
        assert!(parse(&bad_levels.bytes).unwrap_err().contains("mip levels"));
    }

    #[test]
    fn parse_reports_geometry() {
        let e = encode_rgba(photo(40, 24, false), &Options::default()).unwrap();
        let info = parse(&e.bytes).unwrap();
        assert_eq!(info, DdsInfo { format: Format::Bc1, width: 40, height: 24, levels: 6, data_offset: 128 });
    }
}
