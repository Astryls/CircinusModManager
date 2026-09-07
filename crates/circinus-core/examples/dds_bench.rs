//! What each quality setting costs and what it buys.
//!
//! The four settings are BC7 effort levels and nothing else: BC1 and BC3 have one mode in the
//! ISPC compressor, so an opaque texture encodes identically at every setting and only the time
//! changes. This measures the part that does vary -- a texture with an alpha channel, encoded as
//! BC7 -- so the words in the Textures view can be checked rather than guessed at.
//!
//!   cargo run -p circinus-core --release --example dds_bench
//!
//! Release matters: the ISPC kernels are an order of magnitude slower in a debug build, and the
//! ratios between the settings are what this is for.

use circinus_core::dds::encode::{encode_rgba, Format, Options, Quality};
use circinus_core::dds::validate;
use image::{Rgba, RgbaImage};
use std::time::Instant;

/// Something with the qualities that make BC7 work hard: hard edges against transparency, a
/// smooth gradient, and noise. A flat image flatters every setting equally and says nothing.
fn sprite(w: u32, h: u32, seed: u32) -> RgbaImage {
    let mut n = seed.wrapping_mul(2654435761).wrapping_add(1);
    let mut rand = move || {
        n ^= n << 13;
        n ^= n >> 17;
        n ^= n << 5;
        n
    };
    let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
    RgbaImage::from_fn(w, h, |x, y| {
        let dx = (x as f32 - cx) / cx;
        let dy = (y as f32 - cy) / cy;
        let d = (dx * dx + dy * dy).sqrt();
        // A cut-out shape: opaque inside, hard edge, transparent outside.
        let a = if d < 0.78 { 255 } else { 0 };
        let noise = (rand() >> 24) as u8 / 6;
        let r = ((x * 200 / w.max(1)) as u8).saturating_add(noise);
        let g = ((y * 180 / h.max(1)) as u8).saturating_add(noise / 2);
        let b = ((1.0 - d).max(0.0) * 220.0) as u8;
        Rgba([r, g, b, a])
    })
}

fn main() {
    let cases: [(&str, u32, u32); 4] = [("item 128", 128, 128), ("pawn 256", 256, 256), ("building 512", 512, 512), ("atlas 2048", 2048, 2048)];
    let qualities = [Quality::Quick, Quality::Balanced];

    println!("BC7, with mipmaps. Time is the whole encode; dB is the decoded top level against the source.\n");
    println!("{:<14} {:>10} {:>10}", "", "Quick", "Balanced");

    let mut totals = [0f64; 2];
    for (name, w, h) in cases {
        let src = sprite(w, h, w * 7 + h);
        let mut times = Vec::new();
        let mut psnrs = Vec::new();
        for q in qualities {
            let opts = Options { alpha_format: Format::Bc7, quality: q, ..Options::default() };
            let t = Instant::now();
            let e = encode_rgba(src.clone(), &opts).expect("encodes");
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            let info = validate::parse(&e.bytes).expect("header");
            let decoded = validate::decode_top(&e.bytes, &info);
            psnrs.push(validate::compare(&e.top, &decoded).psnr_db);
            times.push(ms);
        }
        for (i, ms) in times.iter().enumerate() {
            totals[i] += ms;
        }
        println!("{:<14} {:>9.0}ms {:>9.0}ms", name, times[0], times[1]);
        println!("{:<14} {:>9.1}dB {:>9.1}dB", "", psnrs[0], psnrs[1]);
    }

    println!("\n{:<14} {:>9.0}ms {:>9.0}ms", "total", totals[0], totals[1]);
    println!("{:<14} {:>10} {:>9.1}x", "vs Quick", "1.0x", totals[1] / totals[0]);

    // The part the setting does not touch at all is asserted in the unit tests
    // (`quality_is_bc7_effort_and_nothing_else`) rather than measured here.
}
