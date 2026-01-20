//! Benchmarks for webpx encode/decode operations.

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::hint::black_box;
use webpx::{
    decode_rgba, encode_lossless, encode_rgba, AnimationEncoder, Encoder, Preset, Unstoppable,
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

fn bench_encode_lossy(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_lossy");

    for &(width, height) in &[(64, 64), (256, 256), (512, 512)] {
        let rgba = generate_gradient_rgba(width, height);
        let pixels = (width * height) as u64;
        group.throughput(Throughput::Elements(pixels));

        group.bench_with_input(
            BenchmarkId::new("q85", format!("{}x{}", width, height)),
            &rgba,
            |b, rgba| {
                b.iter(|| encode_rgba(black_box(rgba), width, height, 85.0, Unstoppable).unwrap());
            },
        );
    }

    group.finish();
}

fn bench_encode_lossless(c: &mut Criterion) {
    let mut group = c.benchmark_group("encode_lossless");

    for &(width, height) in &[(64, 64), (256, 256), (512, 512)] {
        let rgba = generate_gradient_rgba(width, height);
        let pixels = (width * height) as u64;
        group.throughput(Throughput::Elements(pixels));

        group.bench_with_input(
            BenchmarkId::new("lossless", format!("{}x{}", width, height)),
            &rgba,
            |b, rgba| {
                b.iter(|| encode_lossless(black_box(rgba), width, height, Unstoppable).unwrap());
            },
        );
    }

    group.finish();
}

fn bench_decode(c: &mut Criterion) {
    let mut group = c.benchmark_group("decode");

    for &(width, height) in &[(64, 64), (256, 256), (512, 512)] {
        let rgba = generate_gradient_rgba(width, height);
        let webp_lossy = encode_rgba(&rgba, width, height, 85.0, Unstoppable).unwrap();
        let webp_lossless = encode_lossless(&rgba, width, height, Unstoppable).unwrap();
        let pixels = (width * height) as u64;
        group.throughput(Throughput::Elements(pixels));

        group.bench_with_input(
            BenchmarkId::new("lossy", format!("{}x{}", width, height)),
            &webp_lossy,
            |b, webp| {
                b.iter(|| decode_rgba(black_box(webp)).unwrap());
            },
        );

        group.bench_with_input(
            BenchmarkId::new("lossless", format!("{}x{}", width, height)),
            &webp_lossless,
            |b, webp| {
                b.iter(|| decode_rgba(black_box(webp)).unwrap());
            },
        );
    }

    group.finish();
}

fn bench_presets(c: &mut Criterion) {
    let mut group = c.benchmark_group("presets");

    let width = 256;
    let height = 256;
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
            BenchmarkId::new("encode", format!("{:?}", preset)),
            &rgba,
            |b, rgba| {
                b.iter(|| {
                    Encoder::new(black_box(rgba), width, height)
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

fn bench_animation(c: &mut Criterion) {
    let mut group = c.benchmark_group("animation");

    let width = 128;
    let height = 128;
    let frame1 = generate_gradient_rgba(width, height);
    let frame2 = generate_gradient_rgba(width, height);
    let frame3 = generate_gradient_rgba(width, height);

    group.bench_function("encode_3_frames", |b| {
        b.iter(|| {
            let mut encoder = AnimationEncoder::new(width, height).unwrap();
            encoder.set_quality(85.0);
            encoder.add_frame(black_box(&frame1), 0).unwrap();
            encoder.add_frame(black_box(&frame2), 100).unwrap();
            encoder.add_frame(black_box(&frame3), 200).unwrap();
            encoder.finish(300).unwrap()
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_encode_lossy,
    bench_encode_lossless,
    bench_decode,
    bench_presets,
    bench_animation,
);
criterion_main!(benches);
