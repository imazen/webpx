//! Memory formula derivation for webpx.
//!
//! Run with heaptrack to capture per-size memory usage:
//! ```
//! cargo build --release --all-features --example mem_formula
//!
//! # Single run:
//! heaptrack ./target/release/examples/mem_formula --size 1024 --mode lossy --quality 85 --method 4
//!
//! # Batch collection script:
//! for size in 128 256 512 1024 2048; do
//!   for mode in lossy lossless; do
//!     for method in 0 4 6; do
//!       for quality in 50 75 85 95; do
//!         echo "=== size=$size mode=$mode method=$method quality=$quality ==="
//!         heaptrack ./target/release/examples/mem_formula \
//!           --size $size --mode $mode --method $method --quality $quality 2>&1 | grep "peak heap"
//!       done
//!     done
//!   done
//! done
//!
//! # Quick sweep for formula fitting:
//! ./target/release/examples/mem_formula --sweep
//! ```

use std::env;
use std::io::{self, Write};
use webpx::{decode_rgba, Encoder, Unstoppable};

#[derive(Debug, Clone)]
struct Config {
    width: u32,
    height: u32,
    mode: String,
    quality: f32,
    method: u8,
    near_lossless: u8,
    bpp: u8, // 3 for RGB, 4 for RGBA
}

impl Default for Config {
    fn default() -> Self {
        Self {
            width: 512,
            height: 512,
            mode: "lossy".to_string(),
            quality: 85.0,
            method: 4,
            near_lossless: 100,
            bpp: 4,
        }
    }
}

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

fn generate_gradient_rgb(width: u32, height: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity((width * height * 3) as usize);
    for y in 0..height {
        for x in 0..width {
            let r = ((x * 255) / width.max(1)) as u8;
            let g = ((y * 255) / height.max(1)) as u8;
            let b = (((x + y) * 127) / (width + height).max(1)) as u8;
            data.push(r);
            data.push(g);
            data.push(b);
        }
    }
    data
}

fn run_encode(cfg: &Config) {
    let pixels = (cfg.width as u64) * (cfg.height as u64);
    let input_bytes = pixels * (cfg.bpp as u64);

    eprintln!(
        "Config: {}x{} mode={} q={} m={} nl={} bpp={}",
        cfg.width, cfg.height, cfg.mode, cfg.quality, cfg.method, cfg.near_lossless, cfg.bpp
    );
    eprintln!("Pixels: {} Input: {} bytes", pixels, input_bytes);

    let rgba = generate_gradient_rgba(cfg.width, cfg.height);
    let rgb = generate_gradient_rgb(cfg.width, cfg.height);

    match cfg.mode.as_str() {
        "lossy" => {
            let data = if cfg.bpp == 4 { &rgba } else { &rgb };
            let encoder = if cfg.bpp == 4 {
                Encoder::new_rgba(data, cfg.width, cfg.height)
            } else {
                Encoder::new_rgb(data, cfg.width, cfg.height)
            };
            let result = encoder
                .quality(cfg.quality)
                .method(cfg.method)
                .encode(Unstoppable)
                .unwrap();
            eprintln!(
                "Output: {} bytes ({:.2}% of input)",
                result.len(),
                (result.len() as f64 / input_bytes as f64) * 100.0
            );
        }
        "lossless" => {
            let encoder = Encoder::new_rgba(&rgba, cfg.width, cfg.height);
            let result = encoder
                .lossless(true)
                .method(cfg.method)
                .encode(Unstoppable)
                .unwrap();
            eprintln!(
                "Output: {} bytes ({:.2}% of input)",
                result.len(),
                (result.len() as f64 / input_bytes as f64) * 100.0
            );
        }
        "near-lossless" => {
            let encoder = Encoder::new_rgba(&rgba, cfg.width, cfg.height);
            let result = encoder
                .lossless(true)
                .near_lossless(cfg.near_lossless)
                .method(cfg.method)
                .encode(Unstoppable)
                .unwrap();
            eprintln!(
                "Output: {} bytes ({:.2}% of input)",
                result.len(),
                (result.len() as f64 / input_bytes as f64) * 100.0
            );
        }
        "decode" | "decode-lossy" => {
            let webp = Encoder::new_rgba(&rgba, cfg.width, cfg.height)
                .quality(cfg.quality)
                .encode(Unstoppable)
                .unwrap();
            drop(rgba);
            drop(rgb);
            let (decoded, w, h) = decode_rgba(&webp).unwrap();
            eprintln!("Decoded: {}x{}, {} bytes", w, h, decoded.len());
        }
        "decode-lossless" => {
            let webp = Encoder::new_rgba(&rgba, cfg.width, cfg.height)
                .lossless(true)
                .encode(Unstoppable)
                .unwrap();
            drop(rgba);
            drop(rgb);
            let (decoded, w, h) = decode_rgba(&webp).unwrap();
            eprintln!("Decoded: {}x{}, {} bytes", w, h, decoded.len());
        }
        _ => {
            eprintln!("Unknown mode: {}", cfg.mode);
        }
    }
}

fn run_sweep() {
    // Quick sweep to collect data for formula fitting
    // Outputs CSV-style data for analysis
    println!("mode,width,height,pixels,method,quality,near_lossless,bpp");
    println!("# Run each line with heaptrack and record peak heap memory");

    let sizes = [128, 256, 384, 512, 768, 1024, 1536, 2048];
    let methods = [0, 2, 4, 6];
    let qualities = [50.0, 75.0, 85.0, 95.0, 100.0];

    for &size in &sizes {
        // Lossy: vary method and quality
        for &method in &methods {
            for &quality in &qualities {
                let pixels = (size as u64) * (size as u64);
                println!(
                    "lossy,{},{},{},{},{},100,4",
                    size, size, pixels, method, quality
                );
            }
        }

        // Lossless: vary method only (quality doesn't apply)
        for &method in &methods {
            let pixels = (size as u64) * (size as u64);
            println!("lossless,{},{},{},{},100,100,4", size, size, pixels, method);
        }

        // Decode: from lossy and lossless sources
        let pixels = (size as u64) * (size as u64);
        println!("decode-lossy,{},{},{},4,85,100,4", size, size, pixels);
        println!("decode-lossless,{},{},{},4,100,100,4", size, size, pixels);
    }
}

fn run_batch() {
    // Run a batch of configurations and print results
    // Use with: ./mem_formula --batch 2>&1 | tee results.txt
    // Then analyze with heaptrack separately

    let sizes = [256, 512, 1024, 2048];
    let methods = [0, 4, 6];

    eprintln!("=== LOSSY ENCODING ===");
    for &size in &sizes {
        for &method in &methods {
            let cfg = Config {
                width: size,
                height: size,
                mode: "lossy".to_string(),
                quality: 85.0,
                method,
                ..Default::default()
            };
            eprintln!("\n--- {}x{} method={} ---", size, size, method);
            run_encode(&cfg);
        }
    }

    eprintln!("\n=== LOSSLESS ENCODING ===");
    for &size in &sizes {
        for &method in &methods {
            let cfg = Config {
                width: size,
                height: size,
                mode: "lossless".to_string(),
                method,
                ..Default::default()
            };
            eprintln!("\n--- {}x{} method={} ---", size, size, method);
            run_encode(&cfg);
        }
    }

    eprintln!("\n=== DECODING ===");
    for &size in &sizes {
        let cfg = Config {
            width: size,
            height: size,
            mode: "decode-lossy".to_string(),
            ..Default::default()
        };
        eprintln!("\n--- {}x{} decode-lossy ---", size, size);
        run_encode(&cfg);

        let cfg = Config {
            width: size,
            height: size,
            mode: "decode-lossless".to_string(),
            ..Default::default()
        };
        eprintln!("\n--- {}x{} decode-lossless ---", size, size);
        run_encode(&cfg);
    }
}

fn print_usage() {
    eprintln!("Usage: mem_formula [OPTIONS]");
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --size <N>           Image size (NxN), default: 512");
    eprintln!("  --width <N>          Image width, default: 512");
    eprintln!("  --height <N>         Image height, default: 512");
    eprintln!("  --mode <MODE>        Mode: lossy, lossless, near-lossless,");
    eprintln!("                       decode-lossy, decode-lossless");
    eprintln!("  --quality <Q>        Quality 0-100, default: 85");
    eprintln!("  --method <M>         Method 0-6, default: 4");
    eprintln!("  --near-lossless <N>  Near-lossless 0-100, default: 100");
    eprintln!("  --bpp <N>            Bytes per pixel (3=RGB, 4=RGBA), default: 4");
    eprintln!("  --sweep              Print CSV of all configs for batch testing");
    eprintln!("  --batch              Run batch of common configurations");
    eprintln!();
    eprintln!("Examples:");
    eprintln!("  heaptrack ./mem_formula --size 1024 --mode lossy --quality 85");
    eprintln!("  heaptrack ./mem_formula --width 1920 --height 1080 --mode lossless");
    eprintln!("  ./mem_formula --sweep > configs.csv");
    eprintln!("  ./mem_formula --batch 2>&1 | tee batch_results.txt");
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        return;
    }

    // Check for special modes
    if args.contains(&"--sweep".to_string()) {
        run_sweep();
        return;
    }
    if args.contains(&"--batch".to_string()) {
        run_batch();
        io::stderr().flush().unwrap();
        return;
    }
    if args.contains(&"--help".to_string()) || args.contains(&"-h".to_string()) {
        print_usage();
        return;
    }

    // Parse arguments
    let mut cfg = Config::default();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--size" => {
                i += 1;
                let size: u32 = args[i].parse().unwrap_or(512);
                cfg.width = size;
                cfg.height = size;
            }
            "--width" => {
                i += 1;
                cfg.width = args[i].parse().unwrap_or(512);
            }
            "--height" => {
                i += 1;
                cfg.height = args[i].parse().unwrap_or(512);
            }
            "--mode" => {
                i += 1;
                cfg.mode = args[i].clone();
            }
            "--quality" | "-q" => {
                i += 1;
                cfg.quality = args[i].parse().unwrap_or(85.0);
            }
            "--method" | "-m" => {
                i += 1;
                cfg.method = args[i].parse().unwrap_or(4);
            }
            "--near-lossless" | "--nl" => {
                i += 1;
                cfg.near_lossless = args[i].parse().unwrap_or(100);
            }
            "--bpp" => {
                i += 1;
                cfg.bpp = args[i].parse().unwrap_or(4);
            }
            // Legacy positional args support
            arg if !arg.starts_with('-') => {
                if cfg.width == 512 && cfg.height == 512 {
                    // First positional = size
                    let size: u32 = arg.parse().unwrap_or(512);
                    cfg.width = size;
                    cfg.height = size;
                } else if cfg.mode == "lossy" {
                    // Second positional = mode
                    cfg.mode = arg.to_string();
                }
            }
            _ => {}
        }
        i += 1;
    }

    run_encode(&cfg);

    io::stdout().flush().unwrap();
    io::stderr().flush().unwrap();
}
