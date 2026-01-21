//! Allocation profiler for webpx encode/decode operations.
//!
//! Run with:
//! ```
//! cargo run --release --all-features --example alloc_profile
//! ```
//!
//! This generates dhat-heap.json which can be viewed at:
//! https://nnethercote.github.io/dh_view/dh_view.html

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

use rgb::RGBA8;
use std::time::Instant;
use webpx::{
    decode, decode_append, decode_into, decode_rgba, decode_rgba_into, AnimationDecoder,
    AnimationEncoder, ColorMode, Decoder, Encoder, StreamingDecoder, StreamingEncoder, Unstoppable,
};

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
    bytes_allocated: u64,
    allocations: u64,
}

fn run_profiled<F, R>(name: &'static str, f: F) -> ProfileResult
where
    F: FnOnce() -> R,
{
    // Force a GC-like cleanup
    let stats_before = dhat::HeapStats::get();
    let start = Instant::now();
    let _result = f();
    let elapsed = start.elapsed();
    let stats_after = dhat::HeapStats::get();

    ProfileResult {
        name,
        time_us: elapsed.as_micros() as u64,
        bytes_allocated: stats_after.total_bytes - stats_before.total_bytes,
        allocations: stats_after.total_blocks - stats_before.total_blocks,
    }
}

fn print_result(r: &ProfileResult, pixels: u64) {
    let bytes_per_pixel = r.bytes_allocated as f64 / pixels as f64;
    let allocs_per_pixel = r.allocations as f64 / pixels as f64;
    let throughput = (pixels as f64) / (r.time_us as f64 / 1_000_000.0) / 1_000_000.0;

    println!(
        "  {:<40} {:>8} µs  {:>10} bytes ({:.2}/px)  {:>6} allocs ({:.4}/px)  {:.1} Mpx/s",
        r.name,
        r.time_us,
        r.bytes_allocated,
        bytes_per_pixel,
        r.allocations,
        allocs_per_pixel,
        throughput
    );
}

fn profile_encoding(width: u32, height: u32) {
    let pixels = (width * height) as u64;
    println!(
        "\n=== ENCODING {}x{} ({} pixels) ===",
        width, height, pixels
    );
    println!();

    let rgba = generate_gradient_rgba(width, height);
    let argb = generate_gradient_argb(width, height);

    // RGBA input -> Vec output
    let r = run_profiled("Encoder::new_rgba().encode()", || {
        Encoder::new_rgba(&rgba, width, height)
            .quality(85.0)
            .encode(Unstoppable)
            .unwrap()
    });
    print_result(&r, pixels);

    // RGBA input -> WebPData output (zero-copy)
    let r = run_profiled("Encoder::new_rgba().encode_owned()", || {
        Encoder::new_rgba(&rgba, width, height)
            .quality(85.0)
            .encode_owned(Unstoppable)
            .unwrap()
    });
    print_result(&r, pixels);

    // ARGB u32 input (zero-copy) -> Vec output
    let r = run_profiled("Encoder::new_argb().encode() [zero-copy in]", || {
        Encoder::new_argb(&argb, width, height)
            .quality(85.0)
            .encode(Unstoppable)
            .unwrap()
    });
    print_result(&r, pixels);

    // Pre-allocated output
    let mut output = Vec::with_capacity(width as usize * height as usize);
    let r = run_profiled("Encoder::new_rgba().encode_into() [prealloc]", || {
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
    println!(
        "\n=== DECODING {}x{} ({} pixels) ===",
        width, height, pixels
    );
    println!();

    let rgba = generate_gradient_rgba(width, height);
    let webp_lossy = Encoder::new_rgba(&rgba, width, height)
        .quality(85.0)
        .encode(Unstoppable)
        .unwrap();
    let webp_lossless = Encoder::new_rgba(&rgba, width, height)
        .lossless(true)
        .encode(Unstoppable)
        .unwrap();

    // Basic decode -> Vec
    let r = run_profiled("decode_rgba() [alloc new Vec]", || {
        decode_rgba(&webp_lossy).unwrap()
    });
    print_result(&r, pixels);

    // Typed decode -> Vec<RGBA8>
    let r = run_profiled("decode::<RGBA8>() [typed, alloc new Vec]", || {
        decode::<RGBA8>(&webp_lossy).unwrap()
    });
    print_result(&r, pixels);

    // Decode into pre-allocated buffer (zero-copy output)
    let stride = width * 4;
    let mut buffer = vec![0u8; (stride * height) as usize];
    let r = run_profiled("decode_rgba_into() [zero-copy out]", || {
        decode_rgba_into(&webp_lossy, &mut buffer, stride).unwrap()
    });
    print_result(&r, pixels);

    // Typed decode into pre-allocated slice
    let mut typed_buffer: Vec<RGBA8> = vec![RGBA8::default(); (width * height) as usize];
    let r = run_profiled("decode_into::<RGBA8>() [typed, zero-copy]", || {
        decode_into::<RGBA8>(&webp_lossy, &mut typed_buffer, width).unwrap()
    });
    print_result(&r, pixels);

    // Append to existing Vec
    let mut append_buffer: Vec<RGBA8> = Vec::with_capacity((width * height) as usize);
    let r = run_profiled("decode_append::<RGBA8>() [prealloc Vec]", || {
        append_buffer.clear();
        decode_append::<RGBA8>(&webp_lossy, &mut append_buffer).unwrap()
    });
    print_result(&r, pixels);

    // Lossless decode
    let r = run_profiled("decode_rgba() [lossless]", || {
        decode_rgba(&webp_lossless).unwrap()
    });
    print_result(&r, pixels);

    // Decoder builder (basic)
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
    println!(
        "\n=== DECODER TRANSFORMS {}x{} ({} pixels) ===",
        width, height, pixels
    );
    println!();

    let rgba = generate_gradient_rgba(width, height);
    let webp = Encoder::new_rgba(&rgba, width, height)
        .quality(85.0)
        .encode(Unstoppable)
        .unwrap();

    // Full decode
    let r = run_profiled("Decoder full decode", || {
        Decoder::new(&webp).unwrap().decode_rgba_raw().unwrap()
    });
    print_result(&r, pixels);

    // Scale to 50%
    let scaled_pixels = ((width / 2) * (height / 2)) as u64;
    let r = run_profiled("Decoder scale 50%", || {
        Decoder::new(&webp)
            .unwrap()
            .scale(width / 2, height / 2)
            .decode_rgba_raw()
            .unwrap()
    });
    print_result(&r, scaled_pixels);

    // Scale to 25%
    let scaled_pixels_25 = ((width / 4) * (height / 4)) as u64;
    let r = run_profiled("Decoder scale 25%", || {
        Decoder::new(&webp)
            .unwrap()
            .scale(width / 4, height / 4)
            .decode_rgba_raw()
            .unwrap()
    });
    print_result(&r, scaled_pixels_25);

    // Crop center
    let crop_w = width / 2;
    let crop_h = height / 2;
    let crop_pixels = (crop_w * crop_h) as u64;
    let r = run_profiled("Decoder crop center 50%", || {
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
    println!(
        "\n=== STREAMING {}x{} ({} pixels) ===",
        width, height, pixels
    );
    println!();

    let rgba = generate_gradient_rgba(width, height);
    let webp = Encoder::new_rgba(&rgba, width, height)
        .quality(85.0)
        .encode(Unstoppable)
        .unwrap();

    // Streaming decode - all at once
    let r = run_profiled("StreamingDecoder.update() [all at once]", || {
        let mut decoder = StreamingDecoder::new(ColorMode::Rgba).unwrap();
        decoder.update(&webp).unwrap();
        decoder.finish().unwrap()
    });
    print_result(&r, pixels);

    // Streaming decode - chunked
    let r = run_profiled("StreamingDecoder.append() [4k chunks]", || {
        let mut decoder = StreamingDecoder::new(ColorMode::Rgba).unwrap();
        for chunk in webp.chunks(4096) {
            let _ = decoder.append(chunk);
        }
        decoder.finish().unwrap()
    });
    print_result(&r, pixels);

    // Streaming encode
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
    println!(
        "\n=== ANIMATION {}x{} x {} frames ({} total pixels) ===",
        width, height, frame_count, total_pixels
    );
    println!();

    // Generate frames
    let frames: Vec<Vec<u8>> = (0..frame_count)
        .map(|_| generate_gradient_rgba(width, height))
        .collect();

    // Encode animation
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

    // Decode all frames
    let r = run_profiled("AnimationDecoder.decode_all()", || {
        let mut decoder = AnimationDecoder::new(&anim_data).unwrap();
        decoder.decode_all().unwrap()
    });
    print_result(&r, total_pixels);

    // Iterate frames
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
    let _profiler = dhat::Profiler::new_heap();

    println!("╔══════════════════════════════════════════════════════════════════════════════════════════════════════╗");
    println!("║                              WEBPX ALLOCATION PROFILER                                               ║");
    println!("╚══════════════════════════════════════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("Legend:");
    println!("  - bytes: Total heap bytes allocated during operation (lower = better for memory)");
    println!("  - allocs: Number of heap allocations (lower = better for latency)");
    println!("  - /px: Allocation overhead per pixel (useful for estimating resource needs)");
    println!("  - Mpx/s: Throughput in megapixels per second");
    println!();
    println!("Best practices for low allocation:");
    println!("  - Use encode_owned() instead of encode() to avoid extra copy");
    println!("  - Use decode_into() / decode_rgba_into() with pre-allocated buffers");
    println!("  - Use ARGB u32 input format for zero-copy encoding path");
    println!("  - Reuse buffers with encode_into() / decode_append()");

    // Test at different sizes
    for &(width, height) in &[(256, 256), (512, 512), (1024, 1024)] {
        profile_encoding(width, height);
        profile_decoding(width, height);
    }

    // Test transforms at larger size
    profile_decoder_transforms(1024, 1024);

    // Test streaming
    profile_streaming(512, 512);

    // Test animation
    profile_animation(256, 256, 5);

    println!("\n═══════════════════════════════════════════════════════════════════════════════════════════════════════");
    println!("SUMMARY");
    println!("═══════════════════════════════════════════════════════════════════════════════════════════════════════");
    println!();
    println!("Encoding Memory Heuristics:");
    println!("  - Base memory ≈ width × height × 4 bytes (RGBA working buffer)");
    println!("  - libwebp internal ≈ width × height × 2 bytes (VP8 encoder state)");
    println!("  - Output ≈ variable (typically 5-20% of input for lossy @ q85)");
    println!();
    println!("Decoding Memory Heuristics:");
    println!("  - Output ≈ width × height × bpp bytes (3 for RGB, 4 for RGBA)");
    println!("  - Zero-copy path: Only libwebp internal + output if using decode_into()");
    println!();
    println!("Best API Choices by Use Case:");
    println!("  - Minimum allocations: encode_owned() + decode_into()");
    println!("  - Buffer reuse: encode_into() + decode_append()");
    println!("  - Type safety: Encoder::from_pixels() + decode::<RGBA8>()");
    println!("  - Network streaming: StreamingEncoder/Decoder");
    println!();

    // dhat will automatically write dhat-heap.json on drop
    println!("Detailed allocation data written to: dhat-heap.json");
    println!("View at: https://nnethercote.github.io/dh_view/dh_view.html");
}
