//! Memory formula derivation for webpx.
//!
//! Run with heaptrack to capture per-size memory usage:
//! ```
//! cargo build --release --all-features --example mem_formula
//! for size in 128 256 512 1024 2048; do
//!   echo "=== ${size}x${size} ==="
//!   heaptrack ./target/release/examples/mem_formula $size 2>&1 | grep "peak heap"
//! done
//! ```

use std::env;
use std::io::{self, Write};
use webpx::{decode_rgba, Encoder, Unstoppable};

fn generate_gradient_rgba(width: u32, height: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let r = ((x * 255) / width.max(1)) as u8;
            let g = ((y * 255) / height.max(1)) as u8;
            let b = (((x + y) * 127) / (width + height).max(1)) as u8;
            data.push(r);
            data.push(g);
            data.push(b);
            data.push(255);
        }
    }
    data
}

fn main() {
    let args: Vec<String> = env::args().collect();

    // Parse mode from args
    let (size, mode) = if args.len() >= 2 {
        let size: u32 = args[1].parse().unwrap_or(512);
        let mode = args.get(2).map(|s| s.as_str()).unwrap_or("all");
        (size, mode)
    } else {
        eprintln!("Usage: mem_formula <size> [lossy|lossless|decode|all]");
        eprintln!("Example: heaptrack ./target/release/examples/mem_formula 1024 lossy");
        return;
    };

    let width = size;
    let height = size;
    let pixels = (width as u64) * (height as u64);
    let input_bytes = pixels * 4;

    eprintln!(
        "Size: {}x{} ({} pixels, {} bytes input)",
        width, height, pixels, input_bytes
    );

    let rgba = generate_gradient_rgba(width, height);

    match mode {
        "lossy" => {
            eprintln!("Mode: lossy encode (q85, method 4)");
            let result = Encoder::new_rgba(&rgba, width, height)
                .quality(85.0)
                .method(4)
                .encode(Unstoppable)
                .unwrap();
            eprintln!(
                "Output: {} bytes ({:.2}% of input)",
                result.len(),
                (result.len() as f64 / input_bytes as f64) * 100.0
            );
        }
        "lossless" => {
            eprintln!("Mode: lossless encode");
            let result = Encoder::new_rgba(&rgba, width, height)
                .lossless(true)
                .encode(Unstoppable)
                .unwrap();
            eprintln!(
                "Output: {} bytes ({:.2}% of input)",
                result.len(),
                (result.len() as f64 / input_bytes as f64) * 100.0
            );
        }
        "decode" => {
            eprintln!("Mode: decode (from lossy source)");
            let webp = Encoder::new_rgba(&rgba, width, height)
                .quality(85.0)
                .encode(Unstoppable)
                .unwrap();
            // Drop the source to not count it
            drop(rgba);
            let (decoded, w, h) = decode_rgba(&webp).unwrap();
            eprintln!("Decoded: {}x{}, {} bytes", w, h, decoded.len());
        }
        "lossy-m0" => {
            eprintln!("Mode: lossy encode (q85, method 0 - fastest)");
            let result = Encoder::new_rgba(&rgba, width, height)
                .quality(85.0)
                .method(0)
                .encode(Unstoppable)
                .unwrap();
            eprintln!("Output: {} bytes", result.len());
        }
        "lossy-m6" => {
            eprintln!("Mode: lossy encode (q85, method 6 - slowest)");
            let result = Encoder::new_rgba(&rgba, width, height)
                .quality(85.0)
                .method(6)
                .encode(Unstoppable)
                .unwrap();
            eprintln!("Output: {} bytes", result.len());
        }
        "all" | _ => {
            eprintln!("Mode: all (lossy + lossless + decode)");

            // Lossy
            let lossy = Encoder::new_rgba(&rgba, width, height)
                .quality(85.0)
                .method(4)
                .encode(Unstoppable)
                .unwrap();
            eprintln!("Lossy output: {} bytes", lossy.len());

            // Lossless
            let lossless = Encoder::new_rgba(&rgba, width, height)
                .lossless(true)
                .encode(Unstoppable)
                .unwrap();
            eprintln!("Lossless output: {} bytes", lossless.len());

            // Decode
            let (decoded, _, _) = decode_rgba(&lossy).unwrap();
            eprintln!("Decoded: {} bytes", decoded.len());
        }
    }

    io::stdout().flush().unwrap();
    io::stderr().flush().unwrap();
}
