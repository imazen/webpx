# webpx

[![CI](https://github.com/imazen/webpx/actions/workflows/ci.yml/badge.svg)](https://github.com/imazen/webpx/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/webpx.svg)](https://crates.io/crates/webpx)
[![Docs.rs](https://docs.rs/webpx/badge.svg)](https://docs.rs/webpx)
[![codecov](https://codecov.io/gh/imazen/webpx/branch/main/graph/badge.svg)](https://codecov.io/gh/imazen/webpx)
[![License](https://img.shields.io/crates/l/webpx.svg)](https://github.com/imazen/webpx#license)

**Complete WebP encoding and decoding for Rust** - safe bindings to Google's libwebp with support for static images, animations, ICC profiles, streaming, and `no_std`.

## Why webpx?

- **Full libwebp features** - Lossy, lossless, animation, alpha, metadata
- **Safe & ergonomic API** - Builder patterns, strong types, comprehensive error handling
- **High performance** - Zero-copy where possible, direct FFI to optimized C code
- **Flexible** - Works with `no_std`, supports WebAssembly via emscripten
- **Migration-friendly** - Compatibility shims for `webp` and `webp-animation` crates

## Quick Start

```toml
[dependencies]
webpx = "0.1"
```

```rust
use webpx::{encode_rgba, decode_rgba, Unstoppable};

// Encode RGBA pixels to WebP
let webp = encode_rgba(&pixels, width, height, 85.0, Unstoppable)?;

// Decode WebP back to RGBA
let (pixels, w, h) = decode_rgba(&webp)?;
```

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
| **Cancellation** | Cooperative cancellation via [`enough`](https://docs.rs/enough) crate |

## Examples

### Basic Encoding

```rust
use webpx::{encode_rgba, encode_lossless, encode_rgb, Unstoppable};

// Lossy encoding (quality 0-100)
let webp = encode_rgba(&rgba_data, 640, 480, 85.0, Unstoppable)?;

// Lossless encoding (exact pixels)
let webp = encode_lossless(&rgba_data, 640, 480, Unstoppable)?;

// RGB without alpha
let webp = encode_rgb(&rgb_data, 640, 480, 85.0, Unstoppable)?;
```

### Builder API with Options

```rust
use webpx::{Encoder, Preset, Unstoppable};

let webp = Encoder::new(&rgba_data, 640, 480)
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
use webpx::{AnimationEncoder, AnimationDecoder};

// Create animated WebP
let mut encoder = AnimationEncoder::new(320, 240)?;
encoder.set_quality(80.0);
encoder.set_lossless(false);

encoder.add_frame(&frame1, 0)?;     // Start at 0ms
encoder.add_frame(&frame2, 100)?;   // Show at 100ms
encoder.add_frame(&frame3, 200)?;   // Show at 200ms
let webp = encoder.finish(300)?;    // Total duration

// Decode animation
let mut decoder = AnimationDecoder::new(&webp)?;
let info = decoder.info();
println!("{} frames, {}x{}", info.frame_count, info.width, info.height);

// Iterate frames
while let Some(frame) = decoder.next_frame()? {
    render(&frame.data, frame.timestamp_ms);
}

// Or get all at once
decoder.reset();
let frames = decoder.decode_all()?;
```

### ICC Profiles & Metadata

```rust
use webpx::{embed_icc, get_icc_profile, embed_exif, get_exif};

// Embed ICC profile
let webp_with_icc = embed_icc(&webp_data, &srgb_profile)?;

// Extract ICC profile
if let Some(icc) = get_icc_profile(&webp_data)? {
    println!("ICC profile: {} bytes", icc.len());
}

// EXIF data
let webp_with_exif = embed_exif(&webp_data, &exif_bytes)?;
if let Some(exif) = get_exif(&webp_data)? {
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
    }
}

let (pixels, width, height) = decoder.finish()?;
```

### Cooperative Cancellation

Encoding can be cancelled cooperatively using the [`enough`](https://docs.rs/enough) crate:

```rust
use webpx::{encode_rgba, Error, StopReason};
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

match encode_rgba(&data, width, height, 85.0, MyCanceller(cancelled)) {
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
use webpx::{Encoder, Preset};

let webp = Encoder::new(&data, w, h)
    .preset(Preset::Photo)
    .encode()?;
```

## Platform Support

| Platform | Status |
|----------|--------|
| Linux x64/ARM64 | ✅ Full support |
| macOS x64/ARM64 | ✅ Full support |
| Windows x64/ARM64 | ✅ Full support |
| WebAssembly (emscripten) | ✅ Supported |
| WebAssembly (wasm32-unknown-unknown) | ❌ Not supported* |

*libwebp requires C compilation. For pure-Rust WASM, see [image-webp](https://crates.io/crates/image-webp) (lossless only).

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

Contributions welcome! Please read our contributing guidelines and code of conduct.
