# webpx ![CI](https://img.shields.io/github/actions/workflow/status/imazen/webpx/ci.yml?style=flat-square&label=CI) ![crates.io](https://img.shields.io/crates/v/webpx?style=flat-square) [![lib.rs](https://img.shields.io/crates/v/webpx?style=flat-square&label=lib.rs&color=blue)](https://lib.rs/crates/webpx) ![docs.rs](https://img.shields.io/docsrs/webpx?style=flat-square) ![license](https://img.shields.io/crates/l/webpx?style=flat-square)

Ergonomic Rust bindings to Google's [libwebp](https://chromium.googlesource.com/webm/libwebp), covering lossy and lossless encode/decode, animation, ICC/EXIF/XMP metadata, streaming, and `no_std`.

`webpx` wraps the upstream libwebp C library (via [`libwebp-sys`](https://crates.io/crates/libwebp-sys)) behind a typed, builder-style API. It adds a resource-`Limits` policy to bound what the decoder allocates on untrusted uploads, plus cooperative cancellation for interrupting long-running encodes, and it ships compatibility shims for drop-in migration from the `webp` and `webp-animation` crates. Reach for `webpx` when your application already links libwebp through another path (existing C/C++ code, a system package), when you need libwebp's hand-tuned DSP code paths, or when you've benchmarked your content and confirmed libwebp wins.

> Pure-Rust alternative: [`zenwebp`](https://github.com/imazen/zenwebp) is a `#![forbid(unsafe_code)]` reimplementation with native `wasm32-unknown-unknown` support. Its `zencodec` trait surface mirrors `webpx`'s, so the same caller code works against either crate — see the `zencodec` feature below.

## Background

`webpx` started as an LLM-authored test harness for [`zenwebp`](https://github.com/imazen/zenwebp). zenwebp is a pure-Rust, `#![forbid(unsafe_code)]` reimplementation of libwebp; `webpx` is a thin wrapper over the reference libwebp C library, built to check zenwebp's output against libwebp's across the whole encode/decode surface. The two share a `zencodec` trait surface, so the same caller code runs against either — which is what makes that parity check work.

For new projects, prefer zenwebp: it's the pure-Rust path, with no C dependency or FFI. `webpx` stays published and maintained for callers who specifically need libwebp.

## Install

```toml
[dependencies]
webpx = "0.4.0"
```

Opt into the non-default features you need:

```toml
[dependencies]
webpx = { version = "0.4.0", features = ["animation", "icc", "streaming"] }
```

## Quick start

`webpx` works in 8-bit interleaved channel order. Decode produces tightly packed `RGBA` (`R, G, B, A` per pixel, top-to-bottom); the `Encoder::new_rgba` constructor expects the same. `RGB`, `BGRA`, and `BGR` variants are available on both sides.

### Decode (WebP bytes -> RGBA)

```rust
use webpx::decode_rgba;

// `webp_bytes` is the contents of a .webp file.
let (pixels, width, height) = decode_rgba(&webp_bytes)?;
// `pixels.len() == width as usize * height as usize * 4`, in R,G,B,A order.
# Ok::<(), webpx::At<webpx::Error>>(())
```

`decode_rgba` enforces `Limits::default()` on the input — see [Decoding untrusted input](#decoding-untrusted-input). For metadata-stripping, cropping/scaling, BGRA, or zero-copy output, use the [`Decoder`](#decoding-with-cropping--scaling) builder.

### Encode (RGBA -> WebP bytes)

```rust
use webpx::{Encoder, Unstoppable};

// A 2x2 RGBA image: red, green, blue, white.
let rgba: Vec<u8> = vec![
    255, 0, 0, 255,
    0, 255, 0, 255,
    0, 0, 255, 255,
    255, 255, 255, 255,
];

// Lossy, quality 0.0..=100.0.
let webp = Encoder::new_rgba(&rgba, 2, 2)
    .quality(85.0)
    .encode(Unstoppable)?;
# Ok::<(), webpx::At<webpx::Error>>(())
```

`Unstoppable` opts out of cancellation; pass a real `Stop` value to make long encodes interruptible (see [Cooperative cancellation](#cooperative-cancellation)).

## Resource limits & cancellation

Decoding untrusted uploads needs bounded resource use, and `webpx` enforces a `Limits` policy on every decode path so a hostile file can't exhaust memory. That is denial-of-service protection, not a memory-safety guarantee — `webpx` decodes through libwebp's C code over FFI, so that exposure is libwebp's; for a pure-Rust decoder without it, use [`zenwebp`](https://github.com/imazen/zenwebp). The cancellation support below is encode-only.

### Decoding untrusted input

`Limits::default()` applies opinionated production caps suited to typical web / image-server use, and **every decode-side entry point enforces them unless you opt out** — the free functions (`decode_rgba`, `decode_yuv`, `get_icc_profile`, ...), the `Decoder` builder, the `AnimationDecoder`, and the `StreamingDecoder` alike. Defaults: 64 MP per frame, 256 MP cumulative, 16383×16383 (libwebp's intrinsic limit), 64 MiB input, 4096 frames, 5 min animation, 4 MiB metadata, 256 MiB output.

Override individual fields with the `with_*` builders on top of `Limits::default()`, or pass `Limits::none()` through the configurable paths (`DecoderConfig::limits`, `AnimationDecoder::with_options_limits`, the `get_*_with_limits` getters, `StreamingDecoder::limits`) to opt out entirely — only when the input source is fully trusted.

```rust
use webpx::{Decoder, DecoderConfig, Limits};

// Tighter than default: 16 MP per frame for a thumbnail decoder.
let limits = Limits::default().with_max_pixels(16 * 1024 * 1024);

let (pixels, w, h) = Decoder::new(&webp_data)?
    .config(DecoderConfig::new().limits(limits))
    .decode_rgba_raw()?;
# Ok::<(), webpx::At<webpx::Error>>(())
```

For animations, `max_total_pixels` bounds the cumulative `W × H × frame_count` (a 1000×1000 × 200-frame file is 200 MP cumulative even when each frame fits a per-frame `max_pixels` cap), and `max_animation_ms` bounds the summed frame timestamps. Field naming matches `zencodec::ResourceLimits`, so a single shared policy carries cleanly between Imazen codecs.

### Cooperative cancellation

Encoding accepts any [`enough::Stop`](https://docs.rs/enough) implementation; pass `Unstoppable` to disable it. libwebp calls back into the `Stop` during the encode, so a flagged cancellation aborts mid-work and surfaces as `Error::Stopped`.

```rust
use webpx::{Encoder, Error, StopReason};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

struct Canceller(Arc<AtomicBool>);
impl enough::Stop for Canceller {
    fn check(&self) -> Result<(), enough::StopReason> {
        if self.0.load(Ordering::Relaxed) {
            Err(enough::StopReason::Cancelled)
        } else {
            Ok(())
        }
    }
}

let cancelled = Arc::new(AtomicBool::new(false));
// ... another thread: cancelled.store(true, Ordering::Relaxed);

match Encoder::new_rgba(&data, width, height)
    .quality(85.0)
    .encode(Canceller(cancelled))
{
    Ok(webp) => { /* success */ }
    Err(Error::Stopped(StopReason::Cancelled)) => { /* aborted */ }
    Err(e) => { /* other error */ }
}
# Ok::<(), webpx::At<webpx::Error>>(())
```

For ready-made cancellation primitives (timeouts, channels), see [`almost-enough`](https://docs.rs/almost-enough).

## More examples

### Encoder builder

```rust
use webpx::{Encoder, Preset, Unstoppable};

// Lossless: exact pixels.
let webp = Encoder::new_rgba(&rgba, 640, 480)
    .lossless(true)
    .encode(Unstoppable)?;

// Tuned lossy with a content preset.
let webp = Encoder::new_rgba(&rgba, 640, 480)
    .preset(Preset::Photo)   // content-aware optimization
    .quality(90.0)
    .method(5)               // 0-6: higher = slower, smaller
    .alpha_quality(95)       // 0-100 alpha-plane quality
    .sharp_yuv(true)         // sharper RGB->YUV conversion
    .encode(Unstoppable)?;

// RGB input (no alpha).
let webp = Encoder::new_rgb(&rgb, 640, 480)
    .quality(85.0)
    .encode(Unstoppable)?;
# Ok::<(), webpx::At<webpx::Error>>(())
```

### EncoderConfig with statistics

```rust
use webpx::EncoderConfig;

// Presets for the common extremes.
let webp = EncoderConfig::max_compression().encode_rgba(&data, width, height)?;
let webp = EncoderConfig::max_compression_lossless().encode_rgba(&data, width, height)?;

// Fine-grained control, with PSNR/size stats.
let config = EncoderConfig::new()
    .quality(85.0)
    .method(6)
    .filter_strength(60)
    .sns_strength(80)
    .segments(4)
    .pass(6)
    .preprocessing(4);
let (webp, stats) = config.encode_rgba_with_stats(&data, width, height)?;
println!("PSNR {:.2} dB, {} bytes", stats.psnr[4], stats.coded_size);
# Ok::<(), webpx::At<webpx::Error>>(())
```

### Decoding with cropping & scaling

```rust
use webpx::Decoder;

let decoder = Decoder::new(&webp_data)?;

// Inspect dimensions without decoding pixels.
let info = decoder.info();
println!("{}x{}, alpha: {}", info.width, info.height, info.has_alpha);

// Extract a region, then resize it.
let (pixels, w, h) = decoder
    .crop(100, 100, 400, 300)
    .scale(200, 150)
    .decode_rgba_raw()?;
# Ok::<(), webpx::At<webpx::Error>>(())
```

### Animation (`animation` feature)

```rust
use webpx::{AnimationEncoder, AnimationDecoder, ColorMode, Limits};

// Encode an animated WebP. Timestamps are the show time of each frame in ms.
let mut encoder = AnimationEncoder::new(320, 240)?;
encoder.set_quality(80.0);
encoder.set_lossless(false);
encoder.add_frame_rgba(&frame0, 0)?;
encoder.add_frame_rgba(&frame1, 100)?;
encoder.add_frame_rgba(&frame2, 200)?;
let webp = encoder.finish(300)?; // end timestamp

// Decode with limits when the input is untrusted.
let limits = Limits::default()
    .with_max_frames(1024)
    .with_max_animation_ms(60_000);
let mut decoder =
    AnimationDecoder::with_options_limits(&webp, ColorMode::Rgba, true, &limits)?;
let total = decoder.info().frame_count;
while let Some(frame) = decoder.next_frame()? {
    render(&frame.data, frame.timestamp_ms);
}
# fn render(_: &[u8], _: i32) {}
# Ok::<(), webpx::At<webpx::Error>>(())
```

### ICC profiles & metadata (`icc` feature)

```rust
use webpx::{embed_icc, get_icc_profile_with_limits, embed_exif, Limits};

// Embed an ICC profile (returns a new, larger WebP).
let with_icc = embed_icc(&webp_data, &srgb_profile)?;

// Extract — bound the metadata chunk even if the bitstream declares it huge.
let limits = Limits::default().with_max_metadata_bytes(4 * 1024 * 1024);
if let Some(icc) = get_icc_profile_with_limits(&with_icc, &limits)? {
    println!("ICC profile: {} bytes", icc.len());
}

// EXIF works the same way (embed_exif / get_exif_with_limits), as does XMP.
let _with_exif = embed_exif(&webp_data, &exif_bytes)?;
# Ok::<(), webpx::At<webpx::Error>>(())
```

### Streaming decode (`streaming` feature)

```rust
use webpx::{StreamingDecoder, DecodeStatus, ColorMode};

let mut decoder = StreamingDecoder::new(ColorMode::Rgba)?;
for chunk in network_stream {
    match decoder.append(&chunk)? {
        DecodeStatus::Complete => break,
        DecodeStatus::NeedMoreData => continue,
        DecodeStatus::Partial(_rows) => {
            if let Some((data, w, h)) = decoder.get_partial() {
                display_partial(data, w, h); // progressive preview
            }
        }
        _ => {}
    }
}
let (pixels, width, height) = decoder.finish()?;
# fn display_partial(_: &[u8], _: u32, _: u32) {}
# let network_stream: Vec<Vec<u8>> = Vec::new();
# Ok::<(), webpx::At<webpx::Error>>(())
```

`StreamingDecoder` checks `max_input_bytes` against the cumulative fed bytes and the dimension/pixel caps against the canvas as soon as the header prefix arrives — before libwebp allocates the output.

## Key types

| Type | Role |
|------|------|
| `Encoder` | Lossy/lossless encode builder (`new_rgba` / `new_rgb` / `new_bgra` / `new_bgr` / `new_yuv` + `_stride` variants). |
| `EncoderConfig` | Reusable encode configuration with `encode_rgba` / `encode_rgba_with_stats` and `EncodeStats`. |
| `Decoder` | Decode builder with `crop` / `scale` / `config` and typed/raw/zero-copy output methods. |
| `decode_rgba` / `decode_rgb` / `decode_bgra` / `decode_bgr` | One-call decode free functions returning `(Vec<u8>, width, height)`. |
| `Limits` | Resource policy enforced on every decode path; `default()` (production caps) or `none()` (trusted input). |
| `Preset` | Content presets: `Default`, `Photo`, `Picture`, `Drawing`, `Icon`, `Text`. |
| `AnimationEncoder` / `AnimationDecoder` | Multi-frame WebP encode/decode (`animation` feature). |
| `StreamingDecoder` / `StreamingEncoder` | Incremental processing (`streaming` feature). |
| `embed_icc` / `get_icc_profile_with_limits` (+ EXIF/XMP) | Metadata mux/demux (`icc` feature). |
| `Error`, `At<Error>` | Error type with lightweight location tracking via [`whereat`](https://docs.rs/whereat). |
| `Stop`, `Unstoppable`, `StopReason` | Cooperative cancellation, re-exported from [`enough`](https://docs.rs/enough). |

Errors are returned as `Result<T, At<Error>>`, where `At` records the source location for traceability. Propagate with `.at()`, attach your crate's info with `at_crate!`, and recover the plain error with `.into_inner()`.

## Features

| Feature | Default | Description |
|---------|---------|-------------|
| `decode` | yes | WebP decoding. |
| `encode` | yes | WebP encoding. |
| `std` | yes | Standard library; disable for `no_std` + `alloc`. |
| `animation` | no | Animated WebP encode/decode. |
| `icc` | no | ICC / EXIF / XMP metadata mux. |
| `streaming` | no | Incremental decode/encode. |
| `zencodec` | no | [`zencodec`](https://docs.rs/zencodec) trait implementations (mirrors `zenwebp`'s surface for drop-in interchange). |

### `no_std`

`webpx` builds on `no_std` + `alloc`. Disable the `std` feature:

```toml
[dependencies]
webpx = { version = "0.4.0", default-features = false, features = ["decode", "encode"] }
```

(The underlying libwebp still requires a C toolchain at build time.)

## Content presets

| Preset | Best for |
|--------|----------|
| `Default` | General use, balanced settings. |
| `Photo` | Photographs, outdoor scenes. |
| `Picture` | Indoor shots and portraits. |
| `Drawing` | Line art, high-contrast edges. |
| `Icon` | Small images, color preservation. |
| `Text` | Screenshots, crisp text. |

## Migration shims

Drop-in compatibility modules ease porting from existing crates — change the import, keep your code:

```rust
// From the `webp` crate:
use webpx::compat::webp::{Encoder, Decoder};

// From the `webp-animation` crate (uses `finalize()` to match its API):
use webpx::compat::webp_animation::{Encoder, Decoder};
```

## Platform support

| Platform | Status |
|----------|--------|
| Linux, macOS, Windows (x64 / ARM64) | Supported. |
| WebAssembly (`wasm32-unknown-emscripten`) | Supported (requires emscripten, since libwebp is C). |
| MIPS / MIPS DSP | Inherits libwebp's hand-tuned DSP-R2 paths. |

For a native `wasm32-unknown-unknown` build without a C toolchain, use [`zenwebp`](https://github.com/imazen/zenwebp).

## Minimum supported Rust version

Rust 1.89 or later.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

## Contributing

Issues and pull requests are welcome on [GitHub](https://github.com/imazen/webpx).

## AI-generated code notice

This crate was developed with assistance from Claude (Anthropic). Not all code has been manually reviewed; review critical paths before production use.
