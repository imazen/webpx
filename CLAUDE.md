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
- `WebPConfigInitInternal` takes `WEBP_ENCODER_ABI_VERSION` (NOT decoder)
- Animation timestamps are END times, not START times

## FFI soundness invariants (audit checklist)

These are the patterns that have bitten webpx; treat any new FFI call site as suspect until verified.

1. **Never use `libwebp_sys::Webp*::new()` bindgen helpers.** They're generated as `MaybeUninit::uninit() + WebP*Init + assume_init`, which is technically UB because the bindgen-generated structs expose libwebp's reserved `pad` arrays as ordinary Rust fields and libwebp's `*Init*` functions don't touch them. Use explicit `MaybeUninit::<libwebp_sys::T>::zeroed()` + the matching `*Init*` call, or webpx's `crate::ffi::picture::Picture` / `crate::ffi::mem_writer::MemWriter` RAII wrappers. Audited in 0.2.3, 0.3.3, PR #8 — every `WebPDecoderConfig::new`, `WebPConfig::new`, `WebPConfig::new_with_preset`, `WebPPicture::new` call site has been replaced.

2. **`WebPIUpdate` does NOT copy its input buffer.** It stashes the raw pointer and re-reads on subsequent calls. Never expose it through a Rust API that takes `&[u8]` without a lifetime tying the buffer to the decoder. webpx routes `update` through `append` (which uses `WebPIAppend`, copying); see 0.3.4 for the UAF fix.

3. **Any libwebp callback invoked from C frames** (progress hook, writer hook) must wrap the user-supplied Rust closure in `catch_unwind`. Letting a panic unwind through libwebp's C stack is UB. Capture the panic payload, signal abort, and re-raise from a Rust frame after the libwebp call returns. See `encode.rs::progress_hook` and `streaming.rs::write_callback` for the pattern.

4. **Strides cast to `i32` must go through `crate::ffi::validate::stride_fits_i32`.** A stride `>= 2^31` wraps negative; libwebp's row-pointer arithmetic walks backwards through process memory. Centralized in 0.2.1; never hand-roll this check.

5. **`width × bytes_per_pixel`, `stride × rows`, `slice.len() × bpp`** must use `saturating_mul`. On 32-bit `usize` (i686 in CI matrix), unchecked multiplications wrap and bypass downstream length guards.

6. **`MaybeUninit::<libwebp_sys::T>::uninit()` is forbidden.** Use `::zeroed()` so libwebp's reserved fields stay valid Rust state (zero) rather than uninit (UB).

7. **Resource cleanup goes through RAII** (`crate::ffi::picture::Picture`, `MemWriter`, `crate::ffi::demux::Demux` + `ChunkIter`). Never hand-roll `WebPPictureFree` / `WebPMemoryWriterClear` / `WebPDemuxDelete` / `WebPDemuxReleaseChunkIterator` — the wrappers handle every error path, panic path, and ownership-transfer case correctly.

8. **`slice::from_raw_parts(ptr, len)` on libwebp output**: always null-check `ptr` first. libwebp's documented contract is non-null on success but contract violations should produce a clean `BitstreamError`, not UB. See `animation.rs::next_frame`.

9. **Send/Sync on libwebp objects**: libwebp's per-instance state is Send-safe (you can move ownership across threads) but NOT Sync (no API takes `&self`). Webpx already opts out of Sync for `AnimationDecoder`/`Encoder`/`StreamingDecoder` — don't add it.

## Test Coverage

300+ tests:
- 28 unit tests in src/
- 199 integration tests in tests/integration.rs
- 19 soundness regression tests in tests/soundness.rs (each maps to a fixed bug)
- 7 zencodec integration tests
- 35 doc tests
- 12 fuzz targets (cron + ASan + Miri-incompatible-due-to-FFI)
- Memory leak harness via `examples/leak_test`

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
