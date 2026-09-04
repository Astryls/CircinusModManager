//! Convert one PNG to DDS with Circinus's pipeline and print what came out.
//! Usage: cargo run -p circinus-core --example dds_convert -- <in.png> [out.dds] [bc7|bc3] [quick|balanced|high|max]
use circinus_core::dds::{encode, validate, Format, Options, Quality};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input = args.get(1).expect("input png");
    let output = args.get(2).cloned().unwrap_or_else(|| input.replace(".png", ".dds"));
    let mut opts = Options::default();
    if let Some(f) = args.get(3).and_then(|s| Format::parse(s)) {
        opts.alpha_format = f;
    }
    if let Some(q) = args.get(4) {
        opts.quality = match q.as_str() { "quick" => Quality::Quick, "high" => Quality::High, "max" => Quality::Max, _ => Quality::Balanced };
    }
    let bytes = std::fs::read(input).expect("read");
    let t = std::time::Instant::now();
    let e = encode::encode_png(&bytes, &opts).expect("encode");
    let r = validate::check(&e).expect("validate");
    std::fs::write(&output, &e.bytes).expect("write");
    println!("{}x{} -> {}x{} {} {} levels, {} -> {} bytes, {:.1} dB, {:.1} ms", e.source_width, e.source_height, e.width, e.height, e.format.as_str(), e.levels, bytes.len(), e.bytes.len(), r.psnr_db, t.elapsed().as_secs_f64() * 1000.0);
}
