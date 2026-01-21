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
just test       # Run all tests
just clippy     # Run clippy
just fmt        # Format code
just semver     # Check semver compatibility
just ci         # Full CI check (fmt, clippy, test, semver)
just prepublish # Pre-publish check (ci + doc)
just bench      # Run benchmarks
just coverage   # Generate coverage report
just doc        # Build and open docs
```

Or directly:

```bash
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt
cargo semver-checks check-release --all-features
```

## Publishing Checklist

Before publishing a new version:

1. Run `just prepublish` to verify all checks pass
2. Update version in `Cargo.toml`
3. Commit version bump
4. Tag with `git tag vX.Y.Z`
5. Push: `git push && git push origin vX.Y.Z`
6. Verify CI passes on GitHub
7. Publish: `cargo publish`

**IMPORTANT:** `cargo-semver-checks` runs in CI and must pass before release. It detects breaking API changes that require a major version bump.

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

236 tests total:
- 25 unit tests in src/
- 179 integration tests in tests/integration.rs (including 20 real-data corpus tests)
- 32 doc tests

## Known Issues

None currently.

## Profiling Guidelines

When profiling memory or CPU time, always test with multiple content types:

1. **Synthetic test images:**
   - `gradient` - Smooth color transitions (best case for lossy)
   - `solid` - Single color (best case for lossless, also fast decode)
   - `noise` - Random pixels (worst case, stresses encoder/decoder)

2. **Real images:**
   - Use `~/work/codec-corpus/clic2025-1024/` for 1024×1024 photos
   - Real photos typically fall between gradient and noise

3. **Tools:**
   - Memory: `heaptrack` (not dhat - need to capture C library allocations)
   - CPU time: Simple timing with warmup iterations, or criterion benchmarks

4. **What to measure:**
   - Multiple sizes (256, 512, 1024, 2048)
   - Both lossy and lossless
   - Multiple methods (0, 4, 6 at minimum)
   - All content types above

5. **Reporting:**
   - Always report content type alongside measurements
   - Derive min/typ/max from content type variation
   - Validate formulas against real images before finalizing

Example measurement commands:
```bash
# Memory profiling
heaptrack ./target/release/examples/mem_formula --size 1024 --mode decode-only-lossy --content noise
heaptrack_print heaptrack.*.zst | grep "peak heap"

# CPU timing
./target/release/examples/mem_formula --size 1024 --mode time-decode-lossy --content gradient
./target/release/examples/mem_formula --size 1024 --mode time-encode-lossy --method 4 --content noise
```

## User Feedback Log

See FEEDBACK.md (create if needed).
