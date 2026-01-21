//! Comprehensive profiling for webpx encode/decode operations.
//!
//! This benchmark measures CPU time, throughput, and provides data points
//! for developing resource consumption heuristics.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use rgb::{RGB8, RGBA8};
use std::hint::black_box as bb;
use webpx::{
    decode, decode_append, decode_bgr, decode_bgra, decode_into, decode_rgb, decode_rgba,
    decode_rgba_into, decode_to_img, decode_yuv, AnimationDecoder, AnimationEncoder, ColorMode,
    Decoder, Encoder, Preset, StreamingDecoder, StreamingEncoder, Unstoppable,
};

/// Test image sizes: (width, height, description)
const SIZES: &[(u32, u32, &str)] = &[
    (64, 64, "tiny"),
    (256, 256, "small"),
    (512, 512, "medium"),
    (1024, 1024, "large"),
    (2048, 2048, "xlarge"),
];

/// Quality levels to test
const QUALITY_LEVELS: &[f32] = &[50.0, 75.0, 85.0, 95.0];

/// Compression methods (0=fast, 6=slow)
const METHODS: &[u8] = &[0, 2, 4, 6];

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

/// Generate RGB image data
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

/// Generate ARGB u32 data (zero-copy fast path)
fn generate_gradient_argb(width: u32, height: u32) -> Vec<u32> {
    let mut data = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let r = (x * 255) / width.max(1);
            let g = (y * 255) / height.max(1);
            let b = ((x + y) * 127) / (width + height).max(1);
            // ARGB: 0xAARRGGBB
            let pixel = 0xFF000000 | (r << 16) | (g << 8) | b;
            data.push(pixel);
        }
    }
    data
}

/// Generate typed RGBA8 pixels
fn generate_gradient_rgba8(width: u32, height: u32) -> Vec<RGBA8> {
    let mut data = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        for x in 0..width {
            let r = ((x * 255) / width.max(1)) as u8;
            let g = ((y * 255) / height.max(1)) as u8;
            let b = (((x + y) * 127) / (width + height).max(1)) as u8;
            data.push(RGBA8::new(r, g, b, 255));
        }
    }
    data
}

// =============================================================================
// ENCODER BENCHMARKS
// =============================================================================

/// Benchmark encoding with different input formats
fn bench_encode_formats(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode/formats");
    group.sample_size(30);

    let (width, height) = (512, 512);
    let rgba = generate_gradient_rgba(width, height);
    let rgb = generate_gradient_rgb(width, height);
    let argb = generate_gradient_argb(width, height);
    let rgba8_pixels = generate_gradient_rgba8(width, height);

    let pixels = (width * height) as u64;
    group.throughput(Throughput::Elements(pixels));

    // RGBA byte slice
    group.bench_function("rgba_bytes", |b| {
        b.iter(|| {
            Encoder::new_rgba(bb(&rgba), width, height)
                .quality(85.0)
                .encode(Unstoppable)
                .unwrap()
        });
    });

    // RGB byte slice
    group.bench_function("rgb_bytes", |b| {
        b.iter(|| {
            Encoder::new_rgb(bb(&rgb), width, height)
                .quality(85.0)
                .encode(Unstoppable)
                .unwrap()
        });
    });

    // ARGB u32 (zero-copy fast path)
    group.bench_function("argb_u32_zerocopy", |b| {
        b.iter(|| {
            Encoder::new_argb(bb(&argb), width, height)
                .quality(85.0)
                .encode(Unstoppable)
                .unwrap()
        });
    });

    // Typed pixels via from_pixels
    group.bench_function("typed_rgba8", |b| {
        b.iter(|| {
            Encoder::from_pixels(bb(&rgba8_pixels), width, height)
                .quality(85.0)
                .encode(Unstoppable)
                .unwrap()
        });
    });

    group.finish();
}

/// Benchmark encode output methods
fn bench_encode_outputs(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode/outputs");
    group.sample_size(30);

    let (width, height) = (512, 512);
    let rgba = generate_gradient_rgba(width, height);
    let pixels = (width * height) as u64;
    group.throughput(Throughput::Elements(pixels));

    // encode() - returns Vec<u8>
    group.bench_function("encode_to_vec", |b| {
        b.iter(|| {
            Encoder::new_rgba(bb(&rgba), width, height)
                .quality(85.0)
                .encode(Unstoppable)
                .unwrap()
        });
    });

    // encode_owned() - returns WebPData (no copy)
    group.bench_function("encode_owned", |b| {
        b.iter(|| {
            Encoder::new_rgba(bb(&rgba), width, height)
                .quality(85.0)
                .encode_owned(Unstoppable)
                .unwrap()
        });
    });

    // encode_into() - appends to existing Vec
    group.bench_function("encode_into", |b| {
        let mut output = Vec::with_capacity(1024 * 1024);
        b.iter(|| {
            output.clear();
            Encoder::new_rgba(bb(&rgba), width, height)
                .quality(85.0)
                .encode_into(Unstoppable, &mut output)
                .unwrap();
        });
    });

    group.finish();
}

/// Benchmark different quality levels
fn bench_encode_quality(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode/quality");
    group.sample_size(20);

    let (width, height) = (512, 512);
    let rgba = generate_gradient_rgba(width, height);
    let pixels = (width * height) as u64;
    group.throughput(Throughput::Elements(pixels));

    for &quality in QUALITY_LEVELS {
        group.bench_with_input(
            BenchmarkId::new("lossy", format!("q{}", quality as u32)),
            &rgba,
            |b, rgba| {
                b.iter(|| {
                    Encoder::new_rgba(bb(rgba), width, height)
                        .quality(quality)
                        .encode(Unstoppable)
                        .unwrap()
                });
            },
        );
    }

    // Lossless
    group.bench_with_input(BenchmarkId::new("lossless", "q100"), &rgba, |b, rgba| {
        b.iter(|| {
            Encoder::new_rgba(bb(rgba), width, height)
                .lossless(true)
                .encode(Unstoppable)
                .unwrap()
        });
    });

    group.finish();
}

/// Benchmark different compression methods
fn bench_encode_methods(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode/methods");
    group.sample_size(20);

    let (width, height) = (512, 512);
    let rgba = generate_gradient_rgba(width, height);
    let pixels = (width * height) as u64;
    group.throughput(Throughput::Elements(pixels));

    for &method in METHODS {
        group.bench_with_input(
            BenchmarkId::new("lossy_q85", format!("m{}", method)),
            &rgba,
            |b, rgba| {
                b.iter(|| {
                    Encoder::new_rgba(bb(rgba), width, height)
                        .quality(85.0)
                        .method(method)
                        .encode(Unstoppable)
                        .unwrap()
                });
            },
        );
    }

    group.finish();
}

/// Benchmark encoding at different sizes
fn bench_encode_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode/sizes");
    group.sample_size(15);

    for &(width, height, name) in SIZES {
        let rgba = generate_gradient_rgba(width, height);
        let pixels = (width * height) as u64;
        group.throughput(Throughput::Elements(pixels));

        group.bench_with_input(BenchmarkId::new("lossy_q85", name), &rgba, |b, rgba| {
            b.iter(|| {
                Encoder::new_rgba(bb(rgba), width, height)
                    .quality(85.0)
                    .encode(Unstoppable)
                    .unwrap()
            });
        });
    }

    group.finish();
}

/// Benchmark content presets
fn bench_encode_presets(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode/presets");
    group.sample_size(20);

    let (width, height) = (512, 512);
    let rgba = generate_gradient_rgba(width, height);
    let pixels = (width * height) as u64;
    group.throughput(Throughput::Elements(pixels));

    for preset in [
        Preset::Default,
        Preset::Photo,
        Preset::Picture,
        Preset::Drawing,
        Preset::Icon,
        Preset::Text,
    ] {
        group.bench_with_input(
            BenchmarkId::new("q85", format!("{:?}", preset)),
            &rgba,
            |b, rgba| {
                b.iter(|| {
                    Encoder::new_rgba(bb(rgba), width, height)
                        .preset(preset)
                        .quality(85.0)
                        .encode(Unstoppable)
                        .unwrap()
                });
            },
        );
    }

    group.finish();
}

// =============================================================================
// DECODER BENCHMARKS
// =============================================================================

/// Prepare encoded test data for decode benchmarks
fn prepare_encoded_data(width: u32, height: u32, lossy: bool, quality: f32) -> Vec<u8> {
    let rgba = generate_gradient_rgba(width, height);
    if lossy {
        Encoder::new_rgba(&rgba, width, height)
            .quality(quality)
            .encode(Unstoppable)
            .unwrap()
    } else {
        Encoder::new_rgba(&rgba, width, height)
            .lossless(true)
            .encode(Unstoppable)
            .unwrap()
    }
}

/// Benchmark decoding with different output formats
fn bench_decode_formats(c: &mut Criterion) {
    let mut group = c.benchmark_group("decode/formats");
    group.sample_size(50);

    let (width, height) = (512, 512);
    let webp = prepare_encoded_data(width, height, true, 85.0);
    let pixels = (width * height) as u64;
    group.throughput(Throughput::Elements(pixels));

    // RGBA bytes
    group.bench_function("rgba_bytes", |b| {
        b.iter(|| decode_rgba(bb(&webp)).unwrap());
    });

    // RGB bytes
    group.bench_function("rgb_bytes", |b| {
        b.iter(|| decode_rgb(bb(&webp)).unwrap());
    });

    // BGRA bytes
    group.bench_function("bgra_bytes", |b| {
        b.iter(|| decode_bgra(bb(&webp)).unwrap());
    });

    // BGR bytes
    group.bench_function("bgr_bytes", |b| {
        b.iter(|| decode_bgr(bb(&webp)).unwrap());
    });

    // Typed RGBA8
    group.bench_function("typed_rgba8", |b| {
        b.iter(|| decode::<RGBA8>(bb(&webp)).unwrap());
    });

    // Typed RGB8
    group.bench_function("typed_rgb8", |b| {
        b.iter(|| decode::<RGB8>(bb(&webp)).unwrap());
    });

    // YUV
    group.bench_function("yuv420", |b| {
        b.iter(|| decode_yuv(bb(&webp)).unwrap());
    });

    group.finish();
}

/// Benchmark decode output methods (allocating vs zero-copy)
fn bench_decode_outputs(c: &mut Criterion) {
    let mut group = c.benchmark_group("decode/outputs");
    group.sample_size(50);

    let (width, height) = (512, 512);
    let webp = prepare_encoded_data(width, height, true, 85.0);
    let pixels = (width * height) as u64;
    group.throughput(Throughput::Elements(pixels));

    // decode_rgba() - allocates new Vec
    group.bench_function("decode_rgba_alloc", |b| {
        b.iter(|| decode_rgba(bb(&webp)).unwrap());
    });

    // decode_rgba_into() - pre-allocated buffer (zero-copy)
    group.bench_function("decode_rgba_into_zerocopy", |b| {
        let stride = width * 4;
        let mut buffer = vec![0u8; (stride * height) as usize];
        b.iter(|| decode_rgba_into(bb(&webp), &mut buffer, stride).unwrap());
    });

    // decode::<RGBA8>() - typed, allocates
    group.bench_function("decode_typed_alloc", |b| {
        b.iter(|| decode::<RGBA8>(bb(&webp)).unwrap());
    });

    // decode_into::<RGBA8>() - typed, pre-allocated
    group.bench_function("decode_typed_into", |b| {
        let mut buffer: Vec<RGBA8> = vec![RGBA8::default(); (width * height) as usize];
        b.iter(|| decode_into::<RGBA8>(bb(&webp), &mut buffer, width).unwrap());
    });

    // decode_append::<RGBA8>() - typed, append to existing
    group.bench_function("decode_typed_append", |b| {
        let mut buffer: Vec<RGBA8> = Vec::with_capacity((width * height) as usize);
        b.iter(|| {
            buffer.clear();
            decode_append::<RGBA8>(bb(&webp), &mut buffer).unwrap();
        });
    });

    // decode_to_img() - returns ImgVec
    group.bench_function("decode_to_img", |b| {
        b.iter(|| decode_to_img::<RGBA8>(bb(&webp)).unwrap());
    });

    group.finish();
}

/// Benchmark decoding at different sizes
fn bench_decode_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("decode/sizes");
    group.sample_size(30);

    for &(width, height, name) in SIZES {
        let webp = prepare_encoded_data(width, height, true, 85.0);
        let pixels = (width * height) as u64;
        group.throughput(Throughput::Elements(pixels));

        group.bench_with_input(BenchmarkId::new("lossy", name), &webp, |b, webp| {
            b.iter(|| decode_rgba(bb(webp)).unwrap());
        });
    }

    group.finish();
}

/// Benchmark lossy vs lossless decoding
fn bench_decode_compression(c: &mut Criterion) {
    let mut group = c.benchmark_group("decode/compression");
    group.sample_size(50);

    let (width, height) = (512, 512);
    let lossy = prepare_encoded_data(width, height, true, 85.0);
    let lossless = prepare_encoded_data(width, height, false, 100.0);
    let pixels = (width * height) as u64;
    group.throughput(Throughput::Elements(pixels));

    group.bench_with_input(BenchmarkId::new("rgba", "lossy"), &lossy, |b, webp| {
        b.iter(|| decode_rgba(bb(webp)).unwrap());
    });

    group.bench_with_input(
        BenchmarkId::new("rgba", "lossless"),
        &lossless,
        |b, webp| {
            b.iter(|| decode_rgba(bb(webp)).unwrap());
        },
    );

    group.finish();
}

/// Benchmark Decoder builder with crop/scale
fn bench_decode_transforms(c: &mut Criterion) {
    let mut group = c.benchmark_group("decode/transforms");
    group.sample_size(30);

    let (width, height) = (1024, 1024);
    let webp = prepare_encoded_data(width, height, true, 85.0);

    // Full decode
    group.bench_function("full_1024x1024", |b| {
        b.iter(|| Decoder::new(bb(&webp)).unwrap().decode_rgba_raw().unwrap());
    });

    // Decode with scaling to 50%
    group.bench_function("scale_512x512", |b| {
        b.iter(|| {
            Decoder::new(bb(&webp))
                .unwrap()
                .scale(512, 512)
                .decode_rgba_raw()
                .unwrap()
        });
    });

    // Decode with scaling to 25%
    group.bench_function("scale_256x256", |b| {
        b.iter(|| {
            Decoder::new(bb(&webp))
                .unwrap()
                .scale(256, 256)
                .decode_rgba_raw()
                .unwrap()
        });
    });

    // Decode with cropping (center 512x512)
    group.bench_function("crop_512x512", |b| {
        b.iter(|| {
            Decoder::new(bb(&webp))
                .unwrap()
                .crop(256, 256, 512, 512)
                .decode_rgba_raw()
                .unwrap()
        });
    });

    group.finish();
}

// =============================================================================
// STREAMING BENCHMARKS
// =============================================================================

/// Benchmark streaming decoder
fn bench_streaming_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("streaming/decode");
    group.sample_size(30);

    let (width, height) = (512, 512);
    let webp = prepare_encoded_data(width, height, true, 85.0);
    let pixels = (width * height) as u64;
    group.throughput(Throughput::Elements(pixels));

    // Feed all data at once
    group.bench_function("all_at_once", |b| {
        b.iter(|| {
            let mut decoder = StreamingDecoder::new(ColorMode::Rgba).unwrap();
            decoder.update(bb(&webp)).unwrap();
            decoder.finish().unwrap()
        });
    });

    // Feed in chunks (simulating network stream)
    group.bench_function("chunked_4k", |b| {
        b.iter(|| {
            let mut decoder = StreamingDecoder::new(ColorMode::Rgba).unwrap();
            for chunk in webp.chunks(4096) {
                let _ = decoder.append(bb(chunk));
            }
            decoder.finish().unwrap()
        });
    });

    // Feed in small chunks
    group.bench_function("chunked_1k", |b| {
        b.iter(|| {
            let mut decoder = StreamingDecoder::new(ColorMode::Rgba).unwrap();
            for chunk in webp.chunks(1024) {
                let _ = decoder.append(bb(chunk));
            }
            decoder.finish().unwrap()
        });
    });

    group.finish();
}

/// Benchmark streaming encoder
fn bench_streaming_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("streaming/encode");
    group.sample_size(30);

    let (width, height) = (512, 512);
    let rgba = generate_gradient_rgba(width, height);
    let pixels = (width * height) as u64;
    group.throughput(Throughput::Elements(pixels));

    // Streaming encode with callback
    group.bench_function("callback", |b| {
        b.iter(|| {
            let mut output = Vec::new();
            let encoder = StreamingEncoder::new(width, height).unwrap();
            encoder
                .encode_rgba_with_callback(bb(&rgba), |chunk| {
                    output.extend_from_slice(chunk);
                    Ok(())
                })
                .unwrap();
            output
        });
    });

    group.finish();
}

// =============================================================================
// ANIMATION BENCHMARKS
// =============================================================================

/// Benchmark animation encoding
fn bench_animation_encode(c: &mut Criterion) {
    let mut group = c.benchmark_group("animation/encode");
    group.sample_size(20);

    let (width, height) = (256, 256);
    let frame1 = generate_gradient_rgba(width, height);
    let frame2 = generate_gradient_rgba(width, height);
    let frame3 = generate_gradient_rgba(width, height);

    // 3 frames
    group.bench_function("3_frames_256x256", |b| {
        b.iter(|| {
            let mut encoder = AnimationEncoder::new(width, height).unwrap();
            encoder.set_quality(85.0);
            encoder.add_frame_rgba(bb(&frame1), 0).unwrap();
            encoder.add_frame_rgba(bb(&frame2), 100).unwrap();
            encoder.add_frame_rgba(bb(&frame3), 200).unwrap();
            encoder.finish(300).unwrap()
        });
    });

    // Larger frames
    let (w2, h2) = (512, 512);
    let big_frame1 = generate_gradient_rgba(w2, h2);
    let big_frame2 = generate_gradient_rgba(w2, h2);
    let big_frame3 = generate_gradient_rgba(w2, h2);

    group.bench_function("3_frames_512x512", |b| {
        b.iter(|| {
            let mut encoder = AnimationEncoder::new(w2, h2).unwrap();
            encoder.set_quality(85.0);
            encoder.add_frame_rgba(bb(&big_frame1), 0).unwrap();
            encoder.add_frame_rgba(bb(&big_frame2), 100).unwrap();
            encoder.add_frame_rgba(bb(&big_frame3), 200).unwrap();
            encoder.finish(300).unwrap()
        });
    });

    group.finish();
}

/// Benchmark animation decoding
fn bench_animation_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("animation/decode");
    group.sample_size(20);

    // Create test animation
    let (width, height) = (256, 256);
    let frame = generate_gradient_rgba(width, height);
    let mut enc = AnimationEncoder::new(width, height).unwrap();
    enc.set_quality(85.0);
    for i in 0..5 {
        enc.add_frame_rgba(&frame, i * 100).unwrap();
    }
    let anim_data = enc.finish(500).unwrap();

    // Decode all frames
    group.bench_function("decode_all_5_frames", |b| {
        b.iter(|| {
            let mut decoder = AnimationDecoder::new(bb(&anim_data)).unwrap();
            decoder.decode_all().unwrap()
        });
    });

    // Iterate frames one at a time
    group.bench_function("iterate_5_frames", |b| {
        b.iter(|| {
            let mut decoder = AnimationDecoder::new(bb(&anim_data)).unwrap();
            let mut frames = Vec::new();
            while let Some(frame) = decoder.next_frame().unwrap() {
                frames.push(frame);
            }
            frames
        });
    });

    group.finish();
}

// =============================================================================
// COMPREHENSIVE SIZE SCALING BENCHMARKS
// =============================================================================

/// Benchmark how encode time scales with image size
fn bench_encode_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("scaling/encode");
    group.sample_size(10);

    // Power of 2 sizes for consistent scaling analysis
    for exp in 6..=11 {
        // 64 to 2048
        let size = 1u32 << exp;
        let rgba = generate_gradient_rgba(size, size);
        let pixels = (size * size) as u64;
        group.throughput(Throughput::Elements(pixels));

        group.bench_with_input(
            BenchmarkId::new("lossy_q85", format!("{}x{}", size, size)),
            &rgba,
            |b, rgba| {
                b.iter(|| {
                    Encoder::new_rgba(bb(rgba), size, size)
                        .quality(85.0)
                        .encode(Unstoppable)
                        .unwrap()
                });
            },
        );
    }

    group.finish();
}

/// Benchmark how decode time scales with image size
fn bench_decode_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("scaling/decode");
    group.sample_size(20);

    for exp in 6..=11 {
        let size = 1u32 << exp;
        let webp = prepare_encoded_data(size, size, true, 85.0);
        let pixels = (size * size) as u64;
        group.throughput(Throughput::Elements(pixels));

        group.bench_with_input(
            BenchmarkId::new("lossy", format!("{}x{}", size, size)),
            &webp,
            |b, webp| {
                b.iter(|| decode_rgba(bb(webp)).unwrap());
            },
        );
    }

    group.finish();
}

criterion_group!(
    name = encoder_benches;
    config = Criterion::default().significance_level(0.05);
    targets =
        bench_encode_formats,
        bench_encode_outputs,
        bench_encode_quality,
        bench_encode_methods,
        bench_encode_sizes,
        bench_encode_presets,
);

criterion_group!(
    name = decoder_benches;
    config = Criterion::default().significance_level(0.05);
    targets =
        bench_decode_formats,
        bench_decode_outputs,
        bench_decode_sizes,
        bench_decode_compression,
        bench_decode_transforms,
);

criterion_group!(
    name = streaming_benches;
    config = Criterion::default().significance_level(0.05);
    targets =
        bench_streaming_decode,
        bench_streaming_encode,
);

criterion_group!(
    name = animation_benches;
    config = Criterion::default().significance_level(0.05);
    targets =
        bench_animation_encode,
        bench_animation_decode,
);

criterion_group!(
    name = scaling_benches;
    config = Criterion::default().significance_level(0.05);
    targets =
        bench_encode_scaling,
        bench_decode_scaling,
);

criterion_main!(
    encoder_benches,
    decoder_benches,
    streaming_benches,
    animation_benches,
    scaling_benches,
);
