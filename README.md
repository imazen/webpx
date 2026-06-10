# webpx

[![CI](https://github.com/imazen/webpx/actions/workflows/ci.yml/badge.svg)](https://github.com/imazen/webpx/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/webpx.svg)](https://crates.io/crates/webpx)
[![Docs.rs](https://docs.rs/webpx/badge.svg)](https://docs.rs/webpx)
[![codecov](https://codecov.io/gh/imazen/webpx/branch/main/graph/badge.svg)](https://codecov.io/gh/imazen/webpx)
[![License](https://img.shields.io/crates/l/webpx.svg)](https://github.com/imazen/webpx#license)

**Ergonomic FFI bindings to Google's libwebp**, with support for static images, animations, ICC profiles, streaming, and `no_std`.

## Use [`zenwebp`](https://github.com/imazen/zenwebp) instead

For any new project, reach for **[`zenwebp`](https://github.com/imazen/zenwebp)**. It is equally or more capable than `webpx` on every axis that matters:

- **Full feature parity** with libwebp: lossy and lossless encode and decode, animation, alpha, ICC / EXIF / XMP metadata, streaming, content presets, resource limits.
- **Native `wasm32-unknown-unknown` support** — pure Rust, no C compiler, no emscripten. `webpx` requires emscripten because libwebp is C.
- **`#![forbid(unsafe_code)]`** — pure Rust top to bottom. Zero FFI surface, zero `unsafe` blocks.

Performance and compression are essentially a wash:

- libwebp can be **up to 35 % faster on specific photos**, but it can also be **up to 2.5× slower** on others. Net wash unless you're tuned to specific content types.
- Encoded-size difference is at most **~0.02 %** — noise, not meaningful.

The security argument is concrete, not theoretical:

- **libwebp has a documented history of high-severity vulnerabilities.** CVE-2023-4863 was a heap buffer overflow in `BuildHuffmanTable` actively exploited in the wild against Chrome, Safari, Firefox, and Electron apps via a 0-click attack chain — patched out of band on every major platform. That is the failure mode an FFI wrapper inherits, not a hypothetical.
- **Every libwebp wrapper that has been audited has shipped soundness bugs**, `webpx` included. Versions 0.1.0–0.1.4 are yanked, and 0.2.0 + 0.2.1 fixed multiple stride-overflow / use-after-free / aliasing issues found across two parallel audit passes. If you adopt a libwebp wrapper, you are taking on that exposure.

`zenwebp`'s `#![forbid(unsafe_code)]` makes that whole class of bug structurally impossible. Use it.

`webpx` is maintained for users whose application already links libwebp through another path (existing C / C++ code, system package) and would prefer to share that codebase, or who specifically need libwebp's MIPS DSP code paths. If that's not you, switch.

## Why use webpx anyway?

- **Ergonomic Rust API** — Builder patterns, strong types, comprehensive error handling, `Limits` policy for untrusted-input decoding
- **Shares an existing libwebp link** — If your application already links libwebp via another path (C / C++ code, system package), `webpx` reuses that codebase rather than pulling in a second WebP implementation
- **MIPS DSP** — libwebp ships hand-written MIPS DSP-R2 / DSP-ASE assembly paths. If you target that hardware, `webpx` inherits them; `zenwebp` does not.
- **Up to 35 % faster on specific photos** — libwebp's hand-tuned VP8 path beats pure-Rust on some content. Note that it can also be up to 2.5× slower on other content; benchmark your actual workload.

## Quick Start

```toml
[dependencies]
webpx = "0.2"
```

```rust
use webpx::{Encoder, decode_rgba, Unstoppable};

// Encode RGBA pixels to WebP
let webp = Encoder::new_rgba(&pixels, width, height)
    .quality(85.0)
    .encode(Unstoppable)?;

// Decode WebP back to RGBA
let (pixels, w, h) = decode_rgba(&webp)?;
```

## Decoding untrusted input

`Limits::default()` applies **opinionated production caps** suited to
typical web / image-server use, and **every decode-side entry point
enforces them unless you opt out** — the free functions (`decode_rgba`,
`decode_yuv`, `get_icc_profile`, ...), the `Decoder` builder, the
`AnimationDecoder`, and the `StreamingDecoder` alike. Defaults: 64 MP
per frame, 256 MP cumulative, 16383×16383 (libwebp's intrinsic limit),
64 MiB input, 4096 frames, 5 min animation, 4 MiB metadata, 256 MiB
output. Override individual fields via the `with_*` builders on top of
`Limits::default()`, or pass `Limits::none()` through the configurable
paths (`DecoderConfig::limits`, `with_options_limits`, `_with_limits`,
`StreamingDecoder::limits`) to opt out entirely (only when you fully
trust the input).

```rust
use webpx::{Decoder, DecoderConfig, Limits};

// Tighter than default: 16 MP per frame for a thumbnail decoder.
let limits = Limits::default().with_max_pixels(16 * 1024 * 1024);

let img = Decoder::new(webp_data)?
    .config(DecoderConfig::new().limits(limits))
    .decode_rgba()?;
```

The same `Limits` value also wires into [`AnimationDecoder::with_options_limits`]
and [`mux::get_icc_profile_with_limits`] (and the `_with_limits` variants
for EXIF / XMP). Field naming matches `zencodec::ResourceLimits` so a
single shared policy carries cleanly between Imazen codecs.

[`Limits`]: https://docs.rs/webpx/latest/webpx/struct.Limits.html
[`AnimationDecoder::with_options_limits`]: https://docs.rs/webpx/latest/webpx/struct.AnimationDecoder.html#method.with_options_limits
[`mux::get_icc_profile_with_limits`]: https://docs.rs/webpx/latest/webpx/fn.get_icc_profile_with_limits.html

## Features at a Glance

| Feature | Description |
|---------|-------------|
| **Lossy Encoding** | VP8-based compression with quality 0-100 |
| **Lossless Encoding** | Exact pixel preservation |
| **Alpha Channel** | Full transparency support with separate quality control |
| **Animation** | Multi-frame WebP with timing control |
| **ICC Profiles** | Embed/extract color profiles |
| **EXIF/XMP** | Preserve camera metadata |
| **Streaming** | Decode as data arrives |
| **Cropping/Scaling** | Decode to any size |
| **YUV Support** | Direct YUV420 input/output |
| **Content Presets** | Optimized settings for photos, drawings, icons, text |
| **Resource Limits** | `Limits` policy: per-frame & cumulative pixel caps, frame count, metadata size, ... |
| **Cancellation** | Cooperative cancellation via [`enough`](https://docs.rs/enough) crate |

## Examples

### Basic Encoding

```rust
use webpx::{Encoder, Unstoppable};

// Lossy encoding (quality 0-100)
let webp = Encoder::new_rgba(&rgba_data, 640, 480)
    .quality(85.0)
    .encode(Unstoppable)?;

// Lossless encoding (exact pixels)
let webp = Encoder::new_rgba(&rgba_data, 640, 480)
    .lossless(true)
    .encode(Unstoppable)?;

// RGB without alpha
let webp = Encoder::new_rgb(&rgb_data, 640, 480)
    .quality(85.0)
    .encode(Unstoppable)?;
```

### Builder API with Options

```rust
use webpx::{Encoder, Preset, Unstoppable};

let webp = Encoder::new_rgba(&rgba_data, 640, 480)
    .preset(Preset::Photo)    // Content-aware optimization
    .quality(90.0)            // Higher quality
    .method(5)                // Better compression (slower)
    .alpha_quality(95)        // High-quality alpha
    .sharp_yuv(true)          // Better color accuracy
    .encode(Unstoppable)?;
```

### Advanced Configuration

```rust
use webpx::EncoderConfig;

// Maximum compression (slow but smallest files)
let config = EncoderConfig::max_compression();
let webp = config.encode_rgba(&data, width, height)?;

// Maximum quality lossless
let config = EncoderConfig::max_compression_lossless();
let webp = config.encode_rgba(&data, width, height)?;

// Fine-grained control
let config = EncoderConfig::new()
    .quality(85.0)
    .method(6)
    .filter_strength(60)
    .sns_strength(80)
    .segments(4)
    .pass(6)
    .preprocessing(4);
let (webp, stats) = config.encode_rgba_with_stats(&data, width, height)?;
println!("PSNR: {:.2} dB, size: {} bytes", stats.psnr[4], stats.coded_size);
```

### Decoding with Processing

```rust
use webpx::Decoder;

let decoder = Decoder::new(&webp_data)?;

// Get image info without decoding
let info = decoder.info();
println!("{}x{}, alpha: {}", info.width, info.height, info.has_alpha);

// Decode with cropping and scaling
let (pixels, w, h) = decoder
    .crop(100, 100, 400, 300)  // Extract region
    .scale(200, 150)           // Resize
    .decode_rgba_raw()?;
```

### Animation

```rust
use webpx::{AnimationEncoder, AnimationDecoder, ColorMode, Limits};

// Create animated WebP
let mut encoder = AnimationEncoder::new(320, 240)?;
encoder.set_quality(80.0);
encoder.set_lossless(false);

encoder.add_frame_rgba(&frame1_rgba, 0)?;     // Start at 0ms
encoder.add_frame_rgba(&frame2_rgba, 100)?;   // Show at 100ms
encoder.add_frame_rgba(&frame3_rgba, 200)?;   // Show at 200ms
let webp = encoder.finish(300)?;              // End timestamp

// Decode animation. Use `with_options_limits` if the input is
// untrusted — `max_total_pixels` covers the W × H × frame_count
// case (a 1000×1000 × 200-frame animation has 200 MP cumulative
// even when each frame fits a per-frame `max_pixels` cap).
let limits = Limits::none()
    .with_max_pixels(64 * 1024 * 1024)
    .with_max_total_pixels(256 * 1024 * 1024)
    .with_max_frames(1024)
    .with_max_animation_ms(60_000)            // 60 s of animation
    .with_max_input_bytes(64 * 1024 * 1024);  // 64 MB bitstream
let mut decoder = AnimationDecoder::with_options_limits(
    &webp, ColorMode::Rgba, true, &limits,
)?;
let info = decoder.info();
println!("{} frames, {}x{}", info.frame_count, info.width, info.height);

// Iterate frames
while let Some(frame) = decoder.next_frame()? {
    render(&frame.data, frame.timestamp_ms);
}

// Or get all at once (this also enforces `max_animation_ms` against
// the cumulative timestamp).
decoder.reset();
let frames = decoder.decode_all()?;
```

### ICC Profiles & Metadata

```rust
use webpx::{embed_icc, get_icc_profile_with_limits, embed_exif, get_exif_with_limits, Limits};

// Embed ICC profile
let webp_with_icc = embed_icc(&webp_data, &srgb_profile)?;

// Extract — apply `max_metadata_bytes` to bound the ICCP/EXIF/XMP
// chunk size even if the bitstream declares it as huge. Without
// limits, an internal 256 MiB hard cap still applies.
let limits = Limits::none().with_max_metadata_bytes(4 * 1024 * 1024);
if let Some(icc) = get_icc_profile_with_limits(&webp_data, &limits)? {
    println!("ICC profile: {} bytes", icc.len());
}

// EXIF data
let webp_with_exif = embed_exif(&webp_data, &exif_bytes)?;
if let Some(exif) = get_exif_with_limits(&webp_data, &limits)? {
    // Parse EXIF...
}
```

### Streaming Decode

```rust
use webpx::{StreamingDecoder, DecodeStatus, ColorMode};

let mut decoder = StreamingDecoder::new(ColorMode::Rgba)?;

// Feed data as it arrives
for chunk in network_stream {
    match decoder.append(&chunk)? {
        DecodeStatus::Complete => break,
        DecodeStatus::NeedMoreData => continue,
        DecodeStatus::Partial(rows) => {
            // Progressive display
            if let Some((data, w, h)) = decoder.get_partial() {
                display_partial(data, w, h);
            }
        }
        _ => {} // Handle future variants
    }
}

let (pixels, width, height) = decoder.finish()?;
```

### Cooperative Cancellation

Encoding can be cancelled cooperatively using the [`enough`](https://docs.rs/enough) crate:

```rust
use webpx::{Encoder, Error, StopReason};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// Create a cancellation flag
let cancelled = Arc::new(AtomicBool::new(false));
let flag = cancelled.clone();

// Custom Stop implementation
struct MyCanceller(Arc<AtomicBool>);
impl enough::Stop for MyCanceller {
    fn check(&self) -> Result<(), enough::StopReason> {
        if self.0.load(Ordering::Relaxed) {
            Err(enough::StopReason::Cancelled)
        } else {
            Ok(())
        }
    }
}

// In another thread: flag.store(true, Ordering::Relaxed);

match Encoder::new_rgba(&data, width, height)
    .quality(85.0)
    .encode(MyCanceller(cancelled))
{
    Ok(webp) => { /* success */ },
    Err(Error::Stopped(StopReason::Cancelled)) => { /* cancelled */ },
    Err(e) => { /* other error */ },
}
```

For ready-to-use cancellation primitives (timeouts, channels, etc.), see the [`almost-enough`](https://docs.rs/almost-enough) crate.

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `decode` | Yes | WebP decoding |
| `encode` | Yes | WebP encoding |
| `std` | Yes | Use std (disable for no_std + alloc) |
| `animation` | No | Animated WebP support |
| `icc` | No | ICC/EXIF/XMP metadata |
| `streaming` | No | Incremental decode/encode |

```toml
# All features
webpx = { version = "0.1", features = ["animation", "icc", "streaming"] }

# no_std
webpx = { version = "0.1", default-features = false, features = ["decode", "encode"] }
```

## Content Presets

Choose a preset to optimize for your content type:

| Preset | Best For | Characteristics |
|--------|----------|-----------------|
| `Default` | General use | Balanced settings |
| `Photo` | Photographs | Better color, outdoor scenes |
| `Picture` | Indoor/portraits | Skin tone optimization |
| `Drawing` | Line art | High contrast, sharp edges |
| `Icon` | Small images | Color preservation |
| `Text` | Screenshots | Crisp text rendering |

```rust
use webpx::{Encoder, Preset, Unstoppable};

let webp = Encoder::new_rgba(&data, w, h)
    .preset(Preset::Photo)
    .encode(Unstoppable)?;
```

## Platform Support

| Platform | Status |
|----------|--------|
| Linux x64/ARM64 | ✅ Full support |
| macOS x64/ARM64 | ✅ Full support |
| Windows x64/ARM64 | ✅ Full support |
| WebAssembly (emscripten) | ✅ Supported |
| WebAssembly (wasm32-unknown-unknown) | ❌ Not supported (use [`zenwebp`](https://github.com/imazen/zenwebp), which is native to this target) |
| MIPS / MIPS DSP | ✅ Inherits libwebp's hand-tuned DSP-R2 paths |

### Building for WebAssembly

```bash
# Install emscripten
git clone https://github.com/emscripten-core/emsdk.git ~/emsdk
cd ~/emsdk && ./emsdk install latest && ./emsdk activate latest

# Add target and build
rustup target add wasm32-unknown-emscripten
source ~/emsdk/emsdk_env.sh
cargo build --target wasm32-unknown-emscripten --release
```

## Migration from Other Crates

### From `webp` crate

```rust
// Before
use webp::{Encoder, Decoder};

// After - use compat shim
use webpx::compat::webp::{Encoder, Decoder};
// API is compatible, just change the import
```

### From `webp-animation` crate

```rust
// Before
use webp_animation::{Encoder, Decoder};

// After - use compat shim
use webpx::compat::webp_animation::{Encoder, Decoder};
// Uses finalize() instead of finish() to match original API
```

## Performance Tips

1. **Use appropriate `method`** - Higher values (4-6) give better compression but are slower
2. **Choose the right preset** - Presets tune internal parameters for content type
3. **Consider `sharp_yuv`** - Better color accuracy at slight speed cost
4. **Batch frames** - For animations, encode multiple frames before finalizing
5. **Pre-allocate buffers** - Use `StreamingDecoder::with_buffer()` to avoid allocations

## Minimum Supported Rust Version

Rust 1.80 or later.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contributing

Contributions welcome! Please open issues and pull requests on [GitHub](https://github.com/imazen/webpx).

## AI-Generated Code Notice

This crate was developed with assistance from Claude (Anthropic). Not all code has been manually reviewed. Please review critical paths before production use.
