# webpx Project Instructions

## Overview

WebP encoding/decoding crate using FFI bindings to libwebp via `libwebp-sys`.

## Project Status

Initial implementation complete. All core features working:
- Static encode/decode (RGB, RGBA, YUV)
- Streaming encode/decode
- Animation encode/decode
- ICC/EXIF/XMP metadata
- Content presets

## Key Files

- `src/lib.rs` - Main entry, re-exports
- `src/encode.rs` - Static encoding, Encoder builder
- `src/decode.rs` - Static decoding, Decoder builder
- `src/streaming.rs` - StreamingDecoder, StreamingEncoder
- `src/animation.rs` - AnimationEncoder, AnimationDecoder
- `src/mux.rs` - ICC/EXIF/XMP metadata operations
- `src/config.rs` - Preset enum, EncoderConfig, DecoderConfig
- `src/types.rs` - ImageInfo, ColorMode, YuvPlanes
- `src/error.rs` - Error types

## Build Commands

Use justfile commands for common tasks:

```bash
just test      # Run all tests
just clippy    # Run clippy
just fmt       # Format code
just ci        # Full CI check (fmt, clippy, test)
just bench     # Run benchmarks
just coverage  # Generate coverage report
just doc       # Build and open docs
```

Or directly:

```bash
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt
```

## Test Fixtures

Test WebP files are in `tests/fixtures/`:
- `lossless_2color.webp` - 314 bytes, lossless
- `lossy_rgb.webp` - 2.2K, lossy RGB
- `lossy_alpha.webp` - 1.3K, lossy with alpha
- `animated.webp` - 11K, animated

Larger test files available at `~/work/codec-corpus/image-rs/test-images/webp/`.

## libwebp API Notes

- Use `WebPDemuxInternal` with `WEBP_DEMUX_ABI_VERSION` (not `WebPDemux`)
- Use `WebPMuxCreateInternal` with `WEBP_MUX_ABI_VERSION` (not `WebPMuxCreate`)
- Use `WebPAnimEncoderOptionsInitInternal` and `WebPAnimEncoderNewInternal`
- Animation timestamps are END times, not START times

## Test Coverage

124 tests total (88.92% line coverage):
- 12 unit tests in src/
- 112+ integration tests in tests/integration.rs

## Known Issues

None currently.

## User Feedback Log

See FEEDBACK.md (create if needed).
