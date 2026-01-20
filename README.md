# webpx

[![CI](https://github.com/imazen/webpx/actions/workflows/ci.yml/badge.svg)](https://github.com/imazen/webpx/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/webpx.svg)](https://crates.io/crates/webpx)
[![Docs.rs](https://docs.rs/webpx/badge.svg)](https://docs.rs/webpx)
[![codecov](https://codecov.io/gh/imazen/webpx/branch/main/graph/badge.svg)](https://codecov.io/gh/imazen/webpx)
[![License](https://img.shields.io/crates/l/webpx.svg)](https://github.com/imazen/webpx#license)

Complete WebP encoding and decoding via FFI bindings to libwebp.

## Features

- **Static Images**: Encode and decode RGB, RGBA, and YUV formats
- **Animation**: Full animated WebP support with frame-by-frame or batch processing
- **Metadata**: ICC profile, EXIF, and XMP embedding and extraction
- **Streaming**: Incremental decoding as data arrives
- **Presets**: Content-aware optimization (Photo, Picture, Drawing, Icon, Text)
- **Builder APIs**: Ergonomic interfaces with sensible defaults

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
webpx = "0.1"
```

### Quick Start

```rust
use webpx::{encode_rgba, decode_rgba, encode_lossless};

// Encode RGBA data to lossy WebP
let rgba_data: &[u8] = &[/* ... */];
let webp = encode_rgba(rgba_data, 640, 480, 85.0)?;

// Decode WebP back to RGBA
let (pixels, width, height) = decode_rgba(&webp)?;

// Lossless encoding
let webp_lossless = encode_lossless(rgba_data, 640, 480)?;
```

### Builder API

```rust
use webpx::{Encoder, Decoder, Preset};

// Encode with options
let webp = Encoder::new(&rgba_data, 640, 480)
    .preset(Preset::Photo)
    .quality(85.0)
    .method(4)  // 0=fast, 6=better
    .encode()?;

// Decode with cropping/scaling
let decoder = Decoder::new(&webp)?;
let (cropped, w, h) = decoder
    .crop(10, 10, 100, 100)
    .decode_rgba_raw()?;
```

### ICC Profiles

```rust
use webpx::{Encoder, get_icc_profile, embed_icc};

// Embed ICC profile during encoding
let webp = Encoder::new(&rgba_data, 640, 480)
    .icc_profile(&srgb_icc)
    .encode()?;

// Extract ICC profile
if let Some(icc) = get_icc_profile(&webp)? {
    println!("Found ICC profile: {} bytes", icc.len());
}

// Embed into existing WebP
let webp_with_icc = embed_icc(&existing_webp, &icc_profile)?;
```

### Animation

```rust
use webpx::{AnimationEncoder, AnimationDecoder};

// Create animated WebP
let mut encoder = AnimationEncoder::new(640, 480)?;
encoder.set_quality(85.0);

encoder.add_frame(&frame1_rgba, 0)?;      // t=0ms
encoder.add_frame(&frame2_rgba, 100)?;    // t=100ms
encoder.add_frame(&frame3_rgba, 200)?;    // t=200ms
let webp = encoder.finish(300)?;          // total: 300ms

// Decode animation
let mut decoder = AnimationDecoder::new(&webp)?;
let info = decoder.info();
println!("{}x{}, {} frames", info.width, info.height, info.frame_count);

while let Some(frame) = decoder.next_frame()? {
    process_frame(&frame.data, frame.timestamp_ms);
}
```

### Streaming Decode

```rust
use webpx::{StreamingDecoder, DecodeStatus, ColorMode};

let mut decoder = StreamingDecoder::new(ColorMode::Rgba)?;

for chunk in network_chunks {
    match decoder.append(chunk)? {
        DecodeStatus::Complete => break,
        DecodeStatus::NeedMoreData => continue,
        DecodeStatus::Partial(rows) => {
            // Process partial data
            if let Some((data, w, h)) = decoder.get_partial() {
                display_rows(data, h);
            }
        }
    }
}

let (pixels, width, height) = decoder.finish()?;
```

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `decode` | Yes | Decoding support |
| `encode` | Yes | Encoding support |
| `std` | Yes | Standard library (disable for no_std) |
| `animation` | No | Animated WebP support |
| `icc` | No | ICC/EXIF/XMP metadata |
| `streaming` | No | Incremental processing |

Enable all features:

```toml
[dependencies]
webpx = { version = "0.1", features = ["animation", "icc", "streaming"] }
```

## Content Presets

| Preset | Use Case |
|--------|----------|
| `Default` | General purpose |
| `Photo` | Outdoor photos, landscapes |
| `Picture` | Indoor photos, portraits |
| `Drawing` | Line art, high contrast |
| `Icon` | Small colorful images |
| `Text` | Text-heavy images |

## Minimum Rust Version

Rust 1.80 or later.

## License

MIT OR Apache-2.0
