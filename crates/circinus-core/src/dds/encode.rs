//! PNG → block-compressed DDS the way RimWorld wants it: BC1 for opaque textures, BC7 (or
//! BC3) for anything with alpha, a full mip chain down to 1×1, dimensions rounded up to
//! multiples of four, colour bled into transparent texels so neither the mips nor the blocks
//! pick up dark fringes. Encoding is Intel's ISPC texture compressor (`intel_tex_2`).
//!
//! Facts this relies on: Unity's `LoadRawTextureData` expects every mip level down to 1×1
//! when a texture is created with mipmaps; BC7 needs the DX10 header extension while BC1/BC3
//! use the legacy `DXT1`/`DXT5` fourCCs; block encoders work on whole 4×4 blocks, so levels
//! smaller than that are padded by replicating edge texels (the game only samples the real
//! texels).

use crate::{Error, Result};
use image::imageops::{resize, FilterType};
use image::{Rgba, RgbaImage};
use intel_tex_2::{bc1, bc3, bc7, RgbaSurface};
use serde::{Deserialize, Serialize};

/// Bump when the pipeline's output changes in a way that warrants re-converting.
pub const PARAMS_VERSION: u32 = 1;
/// Textures larger than this on either side are left alone (Unity's own limit is 16384).
pub const MAX_DIMENSION: u32 = 8192;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Bc1,
    Bc3,
    Bc7,
}

impl Format {
    pub fn block_bytes(self) -> usize {
        match self {
            Format::Bc1 => 8,
            Format::Bc3 | Format::Bc7 => 16,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Format::Bc1 => "bc1",
            Format::Bc3 => "bc3",
            Format::Bc7 => "bc7",
        }
    }
    pub fn parse(s: &str) -> Option<Format> {
        match s.to_ascii_lowercase().as_str() {
            "bc1" | "dxt1" => Some(Format::Bc1),
            "bc3" | "dxt5" => Some(Format::Bc3),
            "bc7" => Some(Format::Bc7),
            _ => None,
        }
    }
}

/// BC7 effort. BC1/BC3 have a single (fast) mode in the ISPC compressor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    Quick,
    #[default]
    Balanced,
    High,
    Max,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Options {
    /// Format for textures with an alpha channel (opaque ones always get BC1).
    pub alpha_format: Format,
    pub quality: Quality,
    pub mipmaps: bool,
    /// Bleed colour into fully transparent texels before filtering and encoding.
    pub dilate: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { alpha_format: Format::Bc7, quality: Quality::Balanced, mipmaps: true, dilate: true }
    }
}

/// A finished DDS file plus what went into it.
#[derive(Debug, Clone)]
pub struct Encoded {
    pub format: Format,
    /// Dimensions of the DDS (multiples of 4).
    pub width: u32,
    pub height: u32,
    pub source_width: u32,
    pub source_height: u32,
    pub levels: u32,
    pub opaque: bool,
    /// The complete file.
    pub bytes: Vec<u8>,
    /// The top level as encoded (after fix-size and dilation), for validation.
    pub top: RgbaImage,
}

pub fn round_up4(v: u32) -> u32 {
    v.div_ceil(4).max(1) * 4
}

/// Bytes of one block-compressed level.
pub fn level_bytes(format: Format, w: u32, h: u32) -> usize {
    (w.div_ceil(4) * h.div_ceil(4)) as usize * format.block_bytes()
}

/// Number of levels in a full chain from `w`×`h` down to 1×1.
pub fn mip_count(w: u32, h: u32) -> u32 {
    let mut n = 1;
    let (mut w, mut h) = (w, h);
    while w > 1 || h > 1 {
        w = (w / 2).max(1);
        h = (h / 2).max(1);
        n += 1;
    }
    n
}

pub fn decode_png(bytes: &[u8]) -> Result<RgbaImage> {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).map_err(|e| Error::Other(format!("PNG could not be decoded: {e}")))?;
    Ok(img.to_rgba8())
}

pub fn is_opaque(img: &RgbaImage) -> bool {
    img.pixels().all(|p| p[3] == 255)
}

/// Give every fully transparent texel the colour of the nearest non-transparent one
/// (breadth-first from the visible edge), so filtering and block fitting never mix in black.
pub fn dilate(img: &mut RgbaImage) {
    let (w, h) = img.dimensions();
    let n = (w * h) as usize;
    let mut queue: std::collections::VecDeque<u32> = std::collections::VecDeque::new();
    let mut filled = vec![false; n];
    for i in 0..n {
        if img.as_raw()[i * 4 + 3] != 0 {
            filled[i] = true;
            queue.push_back(i as u32);
        }
    }
    if queue.is_empty() || queue.len() == n {
        return;
    }
    let raw = img.as_mut();
    while let Some(i) = queue.pop_front() {
        let x = i % w;
        let y = i / w;
        let (r, g, b) = (raw[(i * 4) as usize], raw[(i * 4 + 1) as usize], raw[(i * 4 + 2) as usize]);
        let mut push = |j: u32| {
            let idx = j as usize;
            if !filled[idx] {
                filled[idx] = true;
                raw[idx * 4] = r;
                raw[idx * 4 + 1] = g;
                raw[idx * 4 + 2] = b;
                queue.push_back(j);
            }
        };
        if x > 0 {
            push(i - 1);
        }
        if x + 1 < w {
            push(i + 1);
        }
        if y > 0 {
            push(i - w);
        }
        if y + 1 < h {
            push(i + w);
        }
    }
}

/// Halve a level with a 2×2 box filter computed in premultiplied space per pixel — no
/// full-size float buffer, and transparent texels never darken their neighbours.
fn downsample(img: &RgbaImage) -> RgbaImage {
    let (w, h) = img.dimensions();
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let src = img.as_raw();
    let mut out = RgbaImage::new(nw, nh);
    let at = |x: u32, y: u32| {
        let i = ((y.min(h - 1) * w + x.min(w - 1)) * 4) as usize;
        &src[i..i + 4]
    };
    for y in 0..nh {
        for x in 0..nw {
            let (sx, sy) = (x * 2, y * 2);
            let mut acc = [0.0f32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let p = at(sx + dx, sy + dy);
                let a = p[3] as f32;
                acc[0] += p[0] as f32 * a;
                acc[1] += p[1] as f32 * a;
                acc[2] += p[2] as f32 * a;
                acc[3] += a;
            }
            let a = acc[3] / 4.0;
            let px = if acc[3] > 0.0 { [(acc[0] / acc[3] + 0.5) as u8, (acc[1] / acc[3] + 0.5) as u8, (acc[2] / acc[3] + 0.5) as u8, (a + 0.5) as u8] } else { [0, 0, 0, 0] };
            out.put_pixel(x, y, Rgba(px));
        }
    }
    out
}

/// A level padded to whole 4×4 blocks by replicating edge texels.
fn padded(img: &RgbaImage) -> (Vec<u8>, u32, u32) {
    let (w, h) = img.dimensions();
    let (pw, ph) = (round_up4(w), round_up4(h));
    if pw == w && ph == h {
        return (img.as_raw().clone(), w, h);
    }
    let mut out = vec![0u8; (pw * ph * 4) as usize];
    for y in 0..ph {
        for x in 0..pw {
            let p = img.get_pixel(x.min(w - 1), y.min(h - 1));
            let i = ((y * pw + x) * 4) as usize;
            out[i..i + 4].copy_from_slice(&p.0);
        }
    }
    (out, pw, ph)
}

fn bc7_settings(q: Quality) -> bc7::EncodeSettings {
    match q {
        Quality::Quick => bc7::alpha_very_fast_settings(),
        Quality::Balanced => bc7::alpha_fast_settings(),
        Quality::High => bc7::alpha_basic_settings(),
        Quality::Max => bc7::alpha_slow_settings(),
    }
}

fn encode_level(format: Format, quality: Quality, img: &RgbaImage) -> Vec<u8> {
    let (data, pw, ph) = padded(img);
    let surf = RgbaSurface { data: &data, width: pw, height: ph, stride: pw * 4 };
    match format {
        Format::Bc1 => bc1::compress_blocks(&surf),
        Format::Bc3 => bc3::compress_blocks(&surf),
        Format::Bc7 => bc7::compress_blocks(&bc7_settings(quality), &surf),
    }
}

/// Header + (for BC7) DX10 extension. Returns the bytes; the caller appends the levels.
pub fn header(format: Format, w: u32, h: u32, levels: u32) -> Vec<u8> {
    const DDSD_CAPS: u32 = 0x1;
    const DDSD_HEIGHT: u32 = 0x2;
    const DDSD_WIDTH: u32 = 0x4;
    const DDSD_PIXELFORMAT: u32 = 0x1000;
    const DDSD_MIPMAPCOUNT: u32 = 0x20000;
    const DDSD_LINEARSIZE: u32 = 0x80000;
    const DDPF_FOURCC: u32 = 0x4;
    const DDSCAPS_COMPLEX: u32 = 0x8;
    const DDSCAPS_TEXTURE: u32 = 0x1000;
    const DDSCAPS_MIPMAP: u32 = 0x400000;
    let mut out = Vec::with_capacity(148);
    let mut u = |v: u32| out.extend_from_slice(&v.to_le_bytes());
    u(u32::from_le_bytes(*b"DDS "));
    u(124);
    let mut flags = DDSD_CAPS | DDSD_HEIGHT | DDSD_WIDTH | DDSD_PIXELFORMAT | DDSD_LINEARSIZE;
    if levels > 1 {
        flags |= DDSD_MIPMAPCOUNT;
    }
    u(flags);
    u(h);
    u(w);
    u(level_bytes(format, w, h) as u32);
    u(0); // depth
    u(levels);
    for _ in 0..11 {
        u(0);
    }
    // DDS_PIXELFORMAT
    u(32);
    u(DDPF_FOURCC);
    u(u32::from_le_bytes(match format {
        Format::Bc1 => *b"DXT1",
        Format::Bc3 => *b"DXT5",
        Format::Bc7 => *b"DX10",
    }));
    for _ in 0..5 {
        u(0);
    }
    let mut caps = DDSCAPS_TEXTURE;
    if levels > 1 {
        caps |= DDSCAPS_MIPMAP | DDSCAPS_COMPLEX;
    }
    u(caps);
    u(0);
    u(0);
    u(0);
    u(0);
    if format == Format::Bc7 {
        u(98); // DXGI_FORMAT_BC7_UNORM
        u(3); // D3D10_RESOURCE_DIMENSION_TEXTURE2D
        u(0); // miscFlag
        u(1); // arraySize
        u(0); // miscFlags2: alpha mode unknown
    }
    out
}

/// The whole pipeline for one decoded image.
pub fn encode_rgba(mut img: RgbaImage, opts: &Options) -> Result<Encoded> {
    let (sw, sh) = img.dimensions();
    if sw == 0 || sh == 0 {
        return Err(Error::Other("empty image".into()));
    }
    if sw > MAX_DIMENSION || sh > MAX_DIMENSION {
        return Err(Error::Other(format!("{sw}×{sh} is larger than {MAX_DIMENSION} on a side; left as PNG")));
    }
    let opaque = is_opaque(&img);
    let format = if opaque { Format::Bc1 } else { opts.alpha_format };
    if !opaque && opts.dilate {
        dilate(&mut img);
    }
    let (w, h) = (round_up4(sw), round_up4(sh));
    let top = if (w, h) != (sw, sh) {
        // Resize rather than pad: padding would shift the art inside its draw quad. Colour
        // was bled into transparent texels above, so straight-alpha filtering is safe here.
        let mut t = resize(&img, w, h, FilterType::Lanczos3);
        if !opaque && opts.dilate {
            dilate(&mut t);
        }
        t
    } else {
        img
    };
    let mut levels: Vec<RgbaImage> = vec![top.clone()];
    if opts.mipmaps {
        // Each level is filtered premultiplied (transparent texels contribute nothing), then
        // dilated again: the GPU's bilinear filter mixes the stored colour of transparent
        // texels into visible edges, so it must not be black.
        while levels.last().map(|l| l.width() > 1 || l.height() > 1).unwrap_or(false) {
            let mut level = downsample(levels.last().unwrap());
            if !opaque && opts.dilate {
                dilate(&mut level);
            }
            levels.push(level);
        }
    }
    let mut bytes = header(format, w, h, levels.len() as u32);
    for l in &levels {
        bytes.extend_from_slice(&encode_level(format, opts.quality, l));
    }
    Ok(Encoded { format, width: w, height: h, source_width: sw, source_height: sh, levels: levels.len() as u32, opaque, bytes, top })
}

pub fn encode_png(bytes: &[u8], opts: &Options) -> Result<Encoded> {
    encode_rgba(decode_png(bytes)?, opts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synth(w: u32, h: u32, alpha: bool) -> RgbaImage {
        RgbaImage::from_fn(w, h, |x, y| {
            let r = (x * 255 / w.max(1)) as u8;
            let g = (y * 255 / h.max(1)) as u8;
            let cx = w as f32 / 2.0;
            let cy = h as f32 / 2.0;
            let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
            let a = if !alpha { 255 } else if d < cx.min(cy) * 0.6 { 255 } else { 0 };
            Rgba([r, g, 90, a])
        })
    }

    #[test]
    fn opaque_becomes_bc1_with_full_mips() {
        let e = encode_rgba(synth(37, 29, false), &Options::default()).unwrap();
        assert_eq!(e.format, Format::Bc1);
        assert!(e.opaque);
        assert_eq!((e.width, e.height), (40, 32));
        assert_eq!(e.levels, mip_count(40, 32));
        assert_eq!(e.levels, 6);
        let expected: usize = (0..e.levels).map(|i| level_bytes(Format::Bc1, (40 >> i).max(1), (32 >> i).max(1))).sum();
        assert_eq!(e.bytes.len(), 128 + expected);
        assert_eq!(&e.bytes[0..4], b"DDS ");
        assert_eq!(&e.bytes[84..88], b"DXT1");
    }

    #[test]
    fn alpha_becomes_bc7_with_dx10_header() {
        let e = encode_rgba(synth(64, 64, true), &Options::default()).unwrap();
        assert_eq!(e.format, Format::Bc7);
        assert!(!e.opaque);
        assert_eq!(&e.bytes[84..88], b"DX10");
        assert_eq!(u32::from_le_bytes(e.bytes[128..132].try_into().unwrap()), 98);
        let expected: usize = (0..7).map(|i| level_bytes(Format::Bc7, (64 >> i).max(1), (64 >> i).max(1))).sum();
        assert_eq!(e.bytes.len(), 148 + expected);
        // transparent texels got a colour, not black
        let corner = e.top.get_pixel(0, 0);
        assert_eq!(corner[3], 0);
        assert!(corner[0] > 0 || corner[1] > 0);
    }

    #[test]
    fn bc3_option_and_no_mipmaps() {
        let opts = Options { alpha_format: Format::Bc3, mipmaps: false, ..Options::default() };
        let e = encode_rgba(synth(16, 8, true), &opts).unwrap();
        assert_eq!(e.format, Format::Bc3);
        assert_eq!(e.levels, 1);
        assert_eq!(&e.bytes[84..88], b"DXT5");
        assert_eq!(e.bytes.len(), 128 + level_bytes(Format::Bc3, 16, 8));
        // no mipmap flag / caps
        let flags = u32::from_le_bytes(e.bytes[8..12].try_into().unwrap());
        assert_eq!(flags & 0x20000, 0);
    }

    #[test]
    fn tiny_textures_are_padded_up() {
        let e = encode_rgba(synth(1, 1, false), &Options::default()).unwrap();
        assert_eq!((e.width, e.height), (4, 4));
        assert_eq!(e.levels, 3);
    }

    #[test]
    fn png_round_trip() {
        let img = synth(20, 12, true);
        let mut png = Vec::new();
        image::write_buffer_with_format(&mut std::io::Cursor::new(&mut png), img.as_raw(), 20, 12, image::ColorType::Rgba8, image::ImageFormat::Png).unwrap();
        let e = encode_png(&png, &Options::default()).unwrap();
        assert_eq!((e.source_width, e.source_height), (20, 12));
        assert_eq!((e.width, e.height), (20, 12));
    }
}
