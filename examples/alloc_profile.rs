//! Memory profiler for webpx encode/decode operations.
//!
//! Run with heaptrack to capture ALL allocations (including libwebp C code):
//! ```
//! heaptrack cargo run --release --all-features --example alloc_profile
//! heaptrack_print heaptrack.alloc_profile.*.zst
//! # Or for GUI: heaptrack_gui heaptrack.alloc_profile.*.zst
//! ```
//!
//! This profiles real memory usage including libwebp's internal allocations.

use rgb::RGBA8;
use std::io::{self, Write};
use std::time::Instant;
use webpx::{
    decode, decode_append, decode_into, decode_rgba, decode_rgba_into, AnimationDecoder,
    AnimationEncoder, ColorMode, Decoder, Encoder, StreamingDecoder, StreamingEncoder, Unstoppable,
};

/// Read peak RSS from /proc/self/status (Linux only)
#[cfg(target_os = "linux")]
fn get_peak_rss_kb() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find(|line| line.starts_with("VmHWM:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

#[cfg(not(target_os = "linux"))]
fn get_peak_rss_kb() -> Option<u64> {
    None
}

/// Read current RSS from /proc/self/status (Linux only)
#[cfg(target_os = "linux")]
fn get_current_rss_kb() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find(|line| line.starts_with("VmRSS:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

#[cfg(not(target_os = "linux"))]
fn get_current_rss_kb() -> Option<u64> {
    None
}

/// Generate a gradient RGBA image for benchmarking.
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

/// Generate ARGB u32 data (zero-copy fast path)
fn generate_gradient_argb(width: u32, height: u32) -> Vec<u32> {
    let mut data = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let r = (x * 255) / width.max(1);
            let g = (y * 255) / height.max(1);
            let b = ((x + y) * 127) / (width + height).max(1);
            let pixel = 0xFF000000 | (r << 16) | (g << 8) | b;
            data.push(pixel);
        }
    }
    data
}

struct ProfileResult {
    name: &'static str,
    time_us: u64,
    rss_before_kb: u64,
    rss_after_kb: u64,
}

fn run_profiled<F, R>(name: &'static str, f: F) -> ProfileResult
where
    F: FnOnce() -> R,
{
    // Force cleanup
    std::hint::black_box(());

    let rss_before = get_current_rss_kb().unwrap_or(0);
    let start = Instant::now();
    let _result = std::hint::black_box(f());
    let elapsed = start.elapsed();
    let rss_after = get_current_rss_kb().unwrap_or(0);

    ProfileResult {
        name,
        time_us: elapsed.as_micros() as u64,
        rss_before_kb: rss_before,
        rss_after_kb: rss_after,
    }
}

fn print_result(r: &ProfileResult, pixels: u64) {
    let rss_delta_kb = r.rss_after_kb.saturating_sub(r.rss_before_kb);
    let throughput = (pixels as f64) / (r.time_us as f64 / 1_000_000.0) / 1_000_000.0;

    println!(
        "  {:<45} {:>8} µs  RSS: {:>6} -> {:>6} KB (Δ {:>+6})  {:.1} Mpx/s",
        r.name, r.time_us, r.rss_before_kb, r.rss_after_kb, rss_delta_kb as i64, throughput
    );
}

fn section(title: &str) {
    println!("\n{}", "=".repeat(100));
    println!("{}", title);
    println!("{}", "=".repeat(100));
}

fn profile_encoding(width: u32, height: u32) {
    let pixels = (width * height) as u64;
    section(&format!(
        "ENCODING {}x{} ({} pixels, {} MB input)",
        width,
        height,
        pixels,
        (pixels * 4) / 1_000_000
    ));

    let rgba = generate_gradient_rgba(width, height);
    let argb = generate_gradient_argb(width, height);

    // Different encode output methods
    let r = run_profiled("Encoder::new_rgba().encode() -> Vec", || {
        Encoder::new_rgba(&rgba, width, height)
            .quality(85.0)
            .encode(Unstoppable)
            .unwrap()
    });
    print_result(&r, pixels);

    let r = run_profiled("Encoder::new_rgba().encode_owned() -> WebPData", || {
        Encoder::new_rgba(&rgba, width, height)
            .quality(85.0)
            .encode_owned(Unstoppable)
            .unwrap()
    });
    print_result(&r, pixels);

    let r = run_profiled("Encoder::new_argb().encode() [zero-copy input]", || {
        Encoder::new_argb(&argb, width, height)
            .quality(85.0)
            .encode(Unstoppable)
            .unwrap()
    });
    print_result(&r, pixels);

    let mut output = Vec::with_capacity(width as usize * height as usize);
    let r = run_profiled("Encoder::encode_into() [preallocated output]", || {
        output.clear();
        Encoder::new_rgba(&rgba, width, height)
            .quality(85.0)
            .encode_into(Unstoppable, &mut output)
            .unwrap()
    });
    print_result(&r, pixels);

    // Lossless
    let r = run_profiled("Encoder lossless", || {
        Encoder::new_rgba(&rgba, width, height)
            .lossless(true)
            .encode(Unstoppable)
            .unwrap()
    });
    print_result(&r, pixels);

    // Different methods
    for method in [0, 4, 6] {
        let name = Box::leak(format!("Encoder method={}", method).into_boxed_str());
        let r = run_profiled(name, || {
            Encoder::new_rgba(&rgba, width, height)
                .quality(85.0)
                .method(method)
                .encode(Unstoppable)
                .unwrap()
        });
        print_result(&r, pixels);
    }
}

fn profile_decoding(width: u32, height: u32) {
    let pixels = (width * height) as u64;
    section(&format!(
        "DECODING {}x{} ({} pixels, {} MB output)",
        width,
        height,
        pixels,
        (pixels * 4) / 1_000_000
    ));

    let rgba = generate_gradient_rgba(width, height);
    let webp_lossy = Encoder::new_rgba(&rgba, width, height)
        .quality(85.0)
        .encode(Unstoppable)
        .unwrap();
    let webp_lossless = Encoder::new_rgba(&rgba, width, height)
        .lossless(true)
        .encode(Unstoppable)
        .unwrap();

    println!(
        "  Encoded sizes: lossy={} bytes, lossless={} bytes",
        webp_lossy.len(),
        webp_lossless.len()
    );
    println!();

    // Basic decode -> Vec
    let r = run_profiled("decode_rgba() -> (Vec<u8>, w, h)", || {
        decode_rgba(&webp_lossy).unwrap()
    });
    print_result(&r, pixels);

    // Typed decode -> Vec<RGBA8>
    let r = run_profiled("decode::<RGBA8>() -> (Vec<RGBA8>, w, h)", || {
        decode::<RGBA8>(&webp_lossy).unwrap()
    });
    print_result(&r, pixels);

    // Decode into pre-allocated buffer
    let stride = width * 4;
    let mut buffer = vec![0u8; (stride * height) as usize];
    let r = run_profiled("decode_rgba_into() [preallocated buffer]", || {
        decode_rgba_into(&webp_lossy, &mut buffer, stride).unwrap()
    });
    print_result(&r, pixels);

    // Typed decode into pre-allocated slice
    let mut typed_buffer: Vec<RGBA8> = vec![RGBA8::default(); (width * height) as usize];
    let r = run_profiled("decode_into::<RGBA8>() [preallocated typed]", || {
        decode_into::<RGBA8>(&webp_lossy, &mut typed_buffer, width).unwrap()
    });
    print_result(&r, pixels);

    // Append to existing Vec
    let mut append_buffer: Vec<RGBA8> = Vec::with_capacity((width * height) as usize);
    let r = run_profiled("decode_append::<RGBA8>() [preallocated Vec]", || {
        append_buffer.clear();
        decode_append::<RGBA8>(&webp_lossy, &mut append_buffer).unwrap()
    });
    print_result(&r, pixels);

    // Lossless decode
    let r = run_profiled("decode_rgba() [lossless source]", || {
        decode_rgba(&webp_lossless).unwrap()
    });
    print_result(&r, pixels);

    // Decoder builder
    let r = run_profiled("Decoder::new().decode_rgba_raw()", || {
        Decoder::new(&webp_lossy)
            .unwrap()
            .decode_rgba_raw()
            .unwrap()
    });
    print_result(&r, pixels);
}

fn profile_decoder_transforms(width: u32, height: u32) {
    let pixels = (width * height) as u64;
    section(&format!("DECODER TRANSFORMS {}x{}", width, height));

    let rgba = generate_gradient_rgba(width, height);
    let webp = Encoder::new_rgba(&rgba, width, height)
        .quality(85.0)
        .encode(Unstoppable)
        .unwrap();

    let r = run_profiled("Full decode", || {
        Decoder::new(&webp).unwrap().decode_rgba_raw().unwrap()
    });
    print_result(&r, pixels);

    let scaled_pixels = ((width / 2) * (height / 2)) as u64;
    let r = run_profiled("Decode + scale 50%", || {
        Decoder::new(&webp)
            .unwrap()
            .scale(width / 2, height / 2)
            .decode_rgba_raw()
            .unwrap()
    });
    print_result(&r, scaled_pixels);

    let scaled_pixels_25 = ((width / 4) * (height / 4)) as u64;
    let r = run_profiled("Decode + scale 25%", || {
        Decoder::new(&webp)
            .unwrap()
            .scale(width / 4, height / 4)
            .decode_rgba_raw()
            .unwrap()
    });
    print_result(&r, scaled_pixels_25);

    let crop_w = width / 2;
    let crop_h = height / 2;
    let crop_pixels = (crop_w * crop_h) as u64;
    let r = run_profiled("Decode + crop center 50%", || {
        Decoder::new(&webp)
            .unwrap()
            .crop(width / 4, height / 4, crop_w, crop_h)
            .decode_rgba_raw()
            .unwrap()
    });
    print_result(&r, crop_pixels);
}

fn profile_streaming(width: u32, height: u32) {
    let pixels = (width * height) as u64;
    section(&format!("STREAMING {}x{}", width, height));

    let rgba = generate_gradient_rgba(width, height);
    let webp = Encoder::new_rgba(&rgba, width, height)
        .quality(85.0)
        .encode(Unstoppable)
        .unwrap();

    let r = run_profiled("StreamingDecoder.update() [all at once]", || {
        let mut decoder = StreamingDecoder::new(ColorMode::Rgba).unwrap();
        decoder.update(&webp).unwrap();
        decoder.finish().unwrap()
    });
    print_result(&r, pixels);

    let r = run_profiled("StreamingDecoder.append() [4k chunks]", || {
        let mut decoder = StreamingDecoder::new(ColorMode::Rgba).unwrap();
        for chunk in webp.chunks(4096) {
            let _ = decoder.append(chunk);
        }
        decoder.finish().unwrap()
    });
    print_result(&r, pixels);

    let r = run_profiled("StreamingEncoder.encode_rgba_with_callback()", || {
        let mut output = Vec::new();
        let encoder = StreamingEncoder::new(width, height).unwrap();
        encoder
            .encode_rgba_with_callback(&rgba, |chunk| {
                output.extend_from_slice(chunk);
                Ok(())
            })
            .unwrap();
        output
    });
    print_result(&r, pixels);
}

fn profile_animation(width: u32, height: u32, frame_count: usize) {
    let pixels = (width * height) as u64;
    let total_pixels = pixels * frame_count as u64;
    section(&format!(
        "ANIMATION {}x{} x {} frames ({} total pixels)",
        width, height, frame_count, total_pixels
    ));

    let frames: Vec<Vec<u8>> = (0..frame_count)
        .map(|_| generate_gradient_rgba(width, height))
        .collect();

    let r = run_profiled("AnimationEncoder (encode all frames)", || {
        let mut encoder = AnimationEncoder::new(width, height).unwrap();
        encoder.set_quality(85.0);
        for (i, frame) in frames.iter().enumerate() {
            encoder.add_frame_rgba(frame, (i * 100) as i32).unwrap();
        }
        encoder.finish((frame_count * 100) as i32).unwrap()
    });
    print_result(&r, total_pixels);

    // Create animation for decode test
    let mut encoder = AnimationEncoder::new(width, height).unwrap();
    encoder.set_quality(85.0);
    for (i, frame) in frames.iter().enumerate() {
        encoder.add_frame_rgba(frame, (i * 100) as i32).unwrap();
    }
    let anim_data = encoder.finish((frame_count * 100) as i32).unwrap();
    println!("  Animation size: {} bytes", anim_data.len());

    let r = run_profiled("AnimationDecoder.decode_all()", || {
        let mut decoder = AnimationDecoder::new(&anim_data).unwrap();
        decoder.decode_all().unwrap()
    });
    print_result(&r, total_pixels);

    let r = run_profiled("AnimationDecoder.next_frame() [iterate]", || {
        let mut decoder = AnimationDecoder::new(&anim_data).unwrap();
        let mut frames = Vec::new();
        while let Some(frame) = decoder.next_frame().unwrap() {
            frames.push(frame);
        }
        frames
    });
    print_result(&r, total_pixels);
}

fn main() {
    println!("╔════════════════════════════════════════════════════════════════════════════════════════════════════╗");
    println!("║                              WEBPX MEMORY PROFILER                                                 ║");
    println!("╚════════════════════════════════════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("For accurate allocation tracking, run with heaptrack:");
    println!("  heaptrack cargo run --release --all-features --example alloc_profile");
    println!("  heaptrack_print heaptrack.alloc_profile.*.zst");
    println!();
    println!("RSS values below are from /proc/self/status (approximate, not per-operation)");
    println!();

    if let Some(peak) = get_peak_rss_kb() {
        println!("Initial peak RSS: {} KB", peak);
    }

    // Warm up
    let _ = generate_gradient_rgba(64, 64);

    // Profile at different sizes
    for &(width, height) in &[(256, 256), (512, 512), (1024, 1024)] {
        profile_encoding(width, height);
        profile_decoding(width, height);
    }

    profile_decoder_transforms(1024, 1024);
    profile_streaming(512, 512);
    profile_animation(256, 256, 5);

    println!();
    section("SUMMARY");

    if let Some(peak) = get_peak_rss_kb() {
        println!(
            "Final peak RSS: {} KB ({:.1} MB)",
            peak,
            peak as f64 / 1024.0
        );
    }

    println!();
    println!("For detailed per-allocation data, analyze heaptrack output:");
    println!("  heaptrack_print heaptrack.alloc_profile.*.zst | less");
    println!("  heaptrack_gui heaptrack.alloc_profile.*.zst");

    // Flush to ensure heaptrack captures everything
    io::stdout().flush().unwrap();
}
