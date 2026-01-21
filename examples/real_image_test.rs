//! Test memory estimation against real images.
//!
//! Usage:
//!   heaptrack cargo run --release --all-features --example real_image_test -- /path/to/image.png
//!
//! Or test against codec-corpus:
//!   for img in ~/work/codec-corpus/clic2025/final-test/*.png | head -5; do
//!     heaptrack cargo run --release --example real_image_test -- "$img" 2>&1 | grep -E "(peak heap|Estimate)"
//!   done

use std::env;
use std::path::Path;
use webpx::{heuristics::estimate_encode, Encoder, EncoderConfig, Unstoppable};

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: real_image_test <image_path> [lossy|lossless]");
        eprintln!("Example: heaptrack ./real_image_test photo.png lossy");
        return;
    }

    let path = Path::new(&args[1]);
    let mode = args.get(2).map(|s| s.as_str()).unwrap_or("lossy");

    // Load image using image crate
    let img = match image::open(path) {
        Ok(img) => img,
        Err(e) => {
            eprintln!("Failed to load image: {}", e);
            return;
        }
    };

    let rgba = img.to_rgba8();
    let width = rgba.width();
    let height = rgba.height();
    let pixels = (width as u64) * (height as u64);

    eprintln!(
        "Image: {} ({}x{}, {} pixels)",
        path.display(),
        width,
        height,
        pixels
    );

    // Get estimate
    let config = if mode == "lossless" {
        EncoderConfig::default().lossless(true).method(4)
    } else {
        EncoderConfig::default().quality(85.0).method(4)
    };

    let est = estimate_encode(width, height, 4, &config);

    eprintln!("Estimate ({}):", mode);
    eprintln!(
        "  min: {:.2} MB",
        est.peak_memory_bytes_min as f64 / 1_000_000.0
    );
    eprintln!(
        "  typ: {:.2} MB",
        est.peak_memory_bytes as f64 / 1_000_000.0
    );
    eprintln!(
        "  max: {:.2} MB",
        est.peak_memory_bytes_max as f64 / 1_000_000.0
    );

    // Actually encode to measure real memory
    eprintln!("\nEncoding...");
    let result = Encoder::new_rgba(rgba.as_raw(), width, height)
        .quality(85.0)
        .lossless(mode == "lossless")
        .method(4)
        .encode(Unstoppable)
        .unwrap();

    eprintln!(
        "Output: {} bytes ({:.2}% of input)",
        result.len(),
        (result.len() as f64 / (pixels * 4) as f64) * 100.0
    );
    eprintln!("\n(Run with heaptrack to see actual peak memory)");
}
