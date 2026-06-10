# Changelog

## [Unreleased]

### Changed — BREAKING (0.4.0)

- **Every decode-side entry point now applies `Limits::default()`
  unless explicitly opted out.** Previously only the `Decoder` builder
  (via `DecoderConfig::default()`) and `AnimationDecoder::
  with_options_limits` enforced caps; the free functions
  (`decode_rgba`, `decode_yuv`, `decode_*_into`, `decode::<P>`,
  `decode_append`, `decode_to_img`, `decode_into::<P>`), the metadata
  getters (`get_icc_profile`, `get_exif`, `get_xmp`),
  `AnimationDecoder::new` / `with_options`, and `StreamingDecoder`
  decoded unbounded. Inputs that exceed the default caps (64 MP per
  frame, 16383×16383, 64 MiB input, 4 MiB metadata chunk, 4096 frames,
  5 min animation) now return `Error::LimitExceeded` where 0.3.4
  decoded them. Opt out for trusted input via the configurable paths:
  `DecoderConfig::new().limits(Limits::none())`,
  `AnimationDecoder::with_options_limits(..)`, `get_*_with_limits(..)`,
  or the new `StreamingDecoder::limits(Limits::none())`.
- `StreamingDecoder` enforces its `Limits` incrementally:
  `max_input_bytes` against the cumulative bytes fed through
  `append`/`update`, and the dimension/pixel caps against the declared
  canvas as soon as the header prefix arrives — before libwebp
  allocates the output canvas. VP8X canvases are read directly from
  the first 30 bytes, so files whose image chunk sits megabytes behind
  ICCP/EXIF metadata are still bounded from the prefix.

### Added

- `StreamingDecoder::limits(Limits)` — replace the streaming decoder's
  resource policy (default: `Limits::default()`).
- `[package.metadata.docs.rs] all-features = true` so docs.rs resolves
  feature-gated items referenced by the enforcement-matrix docs.

### Fixed
- **`Decoder::decode_yuv` and `Decoder::decode_*_into` now enforce
  `DecoderConfig::limits`.** These paths use libwebp's simple API and
  bypassed `decode_advanced`, where the budget gates live — so a config
  carrying `max_pixels` / `max_total_pixels` / `max_input_bytes` decoded
  unbounded. The enforcement-matrix docs promised "Auto" for all
  `DecoderConfig::limits` decode paths; now they deliver it.
- **`Decoder::decode_yuv` rejects crop/scale configs instead of
  silently ignoring them.** Previously `.scale(50, 50).decode_yuv()`
  returned full-size planes. The `_into` methods already rejected
  crop/scale; YUV now matches. All simple-API paths (`decode_yuv`,
  `decode_*_into`) also reject the output-affecting advanced options
  `flip`, `bypass_filtering`, `no_fancy_upsampling`, and
  `alpha_dithering` rather than silently producing pixels the caller
  didn't ask for. `use_threads` remains accepted (perf hint only).

### Testing
- `fuzz_regression` harness gained a `run_limits_boundaries` runner:
  `limits_boundaries-*` seeds are `Arbitrary`-encoded structs, and the
  harness previously never exercised them — they passed without testing
  anything. Seeds now decode with the fuzz target's exact layout and
  drive the static, YUV, animation, and mux limit paths, with
  success-implies-budget asserts (`arbitrary` added as a dev-dependency).
- `limits_boundaries` fuzz target now intersects arbitrary limits with a
  16 MiPx allocation budget for end-to-end decodes (raw boundary values
  still cover the pure `check_*` math). Audit of five weekly OOM seeds
  (`9947b87f07e9`, `36a0851d8893`, `7b78d8c0a5f5`, `8ead9eb0c7cd`,
  `8e6fbe148cef`) showed every one carried absent-or-astronomical limits
  (≥ 4.9e18) — the "OOMs" were allocations the configured limits
  legitimately permitted, not enforcement bypasses. With the budget,
  any future OOM from this target is a real bug. The target also
  covers `decode_yuv` now.

### Changed
- Excluded `.github/`, `.gitignore`, `CLAUDE.md`, `justfile`, `docs/`, `tests/`, and `benches/` from the published crate tarball; `src/`, `examples/`, `README`, `CHANGELOG`, and `LICENSE` files still ship.

## [0.3.4] - 2026-05-05

### Security

- **`StreamingDecoder::update` no longer holds a dangling pointer to
  the caller's buffer.** Earlier versions (≤ 0.3.3) called
  `libwebp_sys::WebPIUpdate`, which is documented by libwebp as "data
  buffer is not copied to the internal memory" — libwebp stores the
  raw `data.as_ptr()` and re-reads it on subsequent calls. Because
  the `update(&mut self, data: &[u8])` signature did not tie `data`'s
  lifetime to the decoder, a caller could drop the buffer between
  calls; the next `update` / `finish` / `get_partial` would
  dereference the dangling pointer. **Use-after-free reachable from
  safe Rust without an `unsafe` block.**

  Fix: route `update` through `append` (which copies internally via
  `WebPIAppend`). Functional behavior for the documented single-call
  use is unchanged; the cost is one `memcpy` of the input. The
  function signature is unchanged so no downstream code needs to
  update.

### Testing

- 19 soundness regression tests (was 18). New
  `streaming_decoder_update_does_not_dangle_input_buffer` exercises
  the canonical UAF shape (drop the input buffer between `update`
  and `finish`). Under ASan the prior code would trip a
  heap-buffer-overflow report; under the fix it must succeed
  because the data was copied during `update`.

## [0.3.3] - 2026-05-03

### Security

- **`progress_hook` (encode.rs) and `write_callback` (streaming.rs)
  now wrap the user-supplied callback in `catch_unwind`.** Both are
  invoked by libwebp from C frames; a panic from the user's
  `Stop::should_stop()` impl or `StreamingEncoder::*_with_callback`
  body would have unwound through libwebp's C stack — undefined
  behavior. The capture now stashes the panic payload in the encode
  context and re-raises it from a Rust frame after `WebPEncode`
  returns control. `#[cfg(feature = "std")]`; `no_std` builds
  typically configure `panic = "abort"` and don't reach the unwinding
  path.
- **`AnimationDecoder::next_frame` now guards against a null `buf`
  pointer** before constructing a Rust slice. libwebp's contract is
  that `WebPAnimDecoderGetNext` returns a non-null pointer on
  success; the guard turns a potential UB into a clean
  `DecodingError::BitstreamError` if libwebp ever violates the
  contract.
- **`decode.rs::decode_into` now uses `output.len().saturating_mul(bpp)`**
  for the buffer-size computation. On 32-bit `usize` (i686 in CI),
  the unchecked multiplication could wrap to 0 for caller-owned
  typed-pixel slices large enough that `len × bpp` exceeds
  `u32::MAX`; the wrapped size was passed to libwebp as the buffer
  byte count, mismatching the actual extent.
- **`config.rs::EncoderConfig::to_libwebp` now passes
  `WEBP_ENCODER_ABI_VERSION`** to `WebPConfigInitInternal` instead of
  the decoder constant. Both equal `528` in libwebp-sys 0.14.2 so
  the bug is harmless today; the fix prevents silent breakage if
  libwebp ever bumps the constants asymmetrically.
- **The `WebPConfig::new_with_preset()` helper from libwebp-sys uses
  `MaybeUninit::uninit()` internally**, the same latent-UB pattern
  fixed in earlier sweeps and PR #8 for the other three bindgen
  helpers. `EncoderConfig::to_libwebp` now goes through explicit
  `MaybeUninit::zeroed()` + `WebPConfigInitInternal`. This was the
  fourth and final bindgen `*::new()` helper webpx still routed
  through; an `unsafe`-block sweep confirms zero remaining sites.

### Added

- **`zencodec` Cargo feature with full trait integration.** webpx now
  exposes `pub mod zencodec` with the same wrapper-type names and
  method signatures as `zenwebp::zencodec`, so source code that
  consumes the trait surface compiles unchanged when swapping crates.
  All four executor traits are wired:
  - `zencodec::encode::Encoder` on `WebpEncoder` — RGBA / BGRA / RGB
    input, ICC / EXIF / XMP metadata, generic-quality and
    generic-effort calibration, `Limits` propagation.
  - `zencodec::encode::AnimationFrameEncoder` on
    `WebpAnimationFrameEncoder` — push / finalize lifecycle, with
    canvas-dimension validation across frames.
  - `zencodec::decode::Decode` + `push_decoder` on `WebpDecoder` —
    one-shot and sink-based single-image decoding.
  - `zencodec::decode::AnimationFrameDecoder` on
    `WebpAnimationFrameDecoder` — frame-by-frame and sink-based
    rendering, with `frame_count` / `loop_count` accessors.
  - `zencodec::decode::StreamingDecode` on `WebpStreamingDecoder` —
    buffered shape (single-batch); a row-batch path against
    libwebp's incremental decoder is a future iteration.
- **`From<webpx::Limits> for zencodec::ResourceLimits`** and the
  reverse impl. Field-for-field mirror minus `max_metadata_bytes`
  (no zencodec counterpart, drops on round-trip).
- **`examples-crates/zencodec-swap-demo/`** — workspace-member crate
  demonstrating the cfg-gated swap pattern with `--features
  use-webpx` ↔ `use-zenwebp`. The demo's `main.rs` body is
  identical between backends; only the `use` import is cfg-gated.
  Both feature combos compile, run, and produce valid WebP via the
  same `zencodec` trait calls.
- **`pub mod zencodec`** re-export with `WebpAnimationFrameDecoder,
  WebpAnimationFrameEncoder, WebpDecodeJob, WebpDecoder,
  WebpDecoderConfig, WebpEncodeJob, WebpEncoder, WebpEncoderConfig,
  WebpStreamingDecoder` — names match `zenwebp::zencodec` exactly.

### Changed (internal — no public API change)

- **`WebpAnimationFrameEncoder` validates canvas dimensions across
  frames.** The first `push_frame` captures the canvas size; later
  frames with mismatched dimensions are rejected with
  `Error::InvalidInput` instead of being silently passed to
  libwebp's fixed-canvas animation encoder.
- **Saturating multiplications added to `width × bytes_per_pixel`
  sites** in `codec.rs:947`, `codec.rs:1021`, `animation.rs:551`.
  Within libwebp's 16383-cap dimensions these can't overflow today,
  but the change matches the rest of the codebase's discipline and
  prevents bitrot if libwebp ever raises its dimension cap.

### Merged (PR #8 from Shnatsel)

- **`AnimationEncoder::add_frame_internal`** — RAII `Picture::new()`
  + `picture.as_mut_ptr()` instead of the bindgen-generated
  `WebPPicture::new()` helper.
- **`Decoder::decode_advanced`** — explicit `MaybeUninit::zeroed()` +
  `WebPInitDecoderConfig` instead of `WebPDecoderConfig::new()`. This
  closes the third of four bindgen `*::new()` helpers using
  `MaybeUninit::uninit` internally.
- **`StreamingEncoder` callback + RGB paths** — `Picture::new()` +
  `MemWriter::new()` RAII; defensive null/zero-size guard on
  `slice::from_raw_parts(data, data_size)` in the writer callback.

### Testing

- 18 soundness regression tests (was 17). New
  `progress_hook_panic_is_caught_and_replayed` exercises the
  panic-through-C fix.
- 7 zencodec integration tests covering encoder roundtrip, lossless
  preservation, animation roundtrip, streaming decode, limits
  propagation, capabilities table, and `Limits` ↔ `ResourceLimits`
  round-trip.
- 12-agent parallel soundness audit run during this release;
  findings drove the Security entries above.

## [0.3.2] - 2026-05-02

### Added

- **`zencodec` feature flag** with a new `pub mod zencodec` exposing
  `WebpEncoderConfig`, `WebpEncodeJob`, `WebpEncoder`,
  `WebpAnimationFrameEncoder`, `WebpDecoderConfig`, `WebpDecodeJob`,
  `WebpDecoder`, `WebpStreamingDecoder`, `WebpAnimationFrameDecoder`.
  Type names and method signatures mirror `zenwebp::zencodec` exactly,
  so the same caller code compiles unchanged when swapping crates —
  the only line that changes is the `use` import. See
  `examples/zencodec_swap.rs` for the swap pattern.
- All four executor traits are wired:
  - `zencodec::encode::Encoder` on `WebpEncoder` — RGBA / BGRA / RGB
    input, ICC / EXIF / XMP metadata, generic-quality / generic-effort
    calibration, `Limits` propagation.
  - `zencodec::encode::AnimationFrameEncoder` on
    `WebpAnimationFrameEncoder` — accepts per-frame `PixelSlice`s,
    converts cumulative-timestamp on the trait side to webpx's
    timestamp-per-frame model, embeds metadata after assembly.
  - `zencodec::decode::Decode` on `WebpDecoder` — full single-image
    decode with caller-preferred descriptor selection.
  - `zencodec::decode::AnimationFrameDecoder` on
    `WebpAnimationFrameDecoder` — frame-by-frame and sink-based
    rendering, with `frame_count` / `loop_count` accessors.
  - `zencodec::decode::StreamingDecode` on `WebpStreamingDecoder` —
    buffered approach (full input → single batch). A row-batch
    implementation against libwebp's true incremental decoder is a
    future iteration.
- `From<webpx::Limits> for zencodec::ResourceLimits` and reverse.
  The two types were designed to mirror each other field-for-field;
  webpx's `max_metadata_bytes` is the only field without a zencodec
  counterpart and drops on the round trip.

### Documentation

- README points at the new example; lib-level rustdoc explains the
  swap pattern.

## [0.3.0] - 2026-05-02

### Security

- `Encoder::new_rgba` / `new_bgra` / `new_rgb` / `new_bgr` no longer
  panic with an arithmetic-overflow trap when constructed with
  `width >= 2^30` (RGBA/BGRA) or `width >= 2^30 + change` (RGB/BGR) —
  `width * 4` / `width * 3` overflowed `u32` *inside the constructor*,
  before the caller had a chance to validate dimensions. The fuzz
  campaign added in 0.2.3 (`dim_extremes`) caught it within seconds
  on its first run. Fix: `saturating_mul` on the byte stride; the
  oversize stride now flows through `validate_buffer_size_stride`
  and produces a clean `Error::InvalidInput` rather than a panic.

### Changed (breaking — caller may need to opt out of caps)

- **`Limits::default()` now applies opinionated production caps.**
  Previously `Limits::default()` returned `Limits::none()` (unbounded);
  callers who construct `DecoderConfig::default()` or
  `AnimationDecoder::with_options(...)` against untrusted input were
  silently relying on libwebp's intrinsic caps only. New default:
  `max_pixels = 64 MiP`, `max_total_pixels = 256 MiP`,
  `max_width = max_height = 16383`, `max_input_bytes = 64 MiB`,
  `max_frames = 4096`, `max_animation_ms = 5 min`,
  `max_metadata_bytes = 4 MiB`, `max_output_bytes = 256 MiB`.
  Code that needs the unbounded behavior must switch to
  `Limits::none()` explicitly.

### Testing

- New `examples/leak_test` harness exercises every webpx surface
  (encode, decode, streaming, animation, mux) in a tight loop. Used
  with `heaptrack` (`just leak-test`) or AddressSanitizer with
  `detect_leaks=1` (`just leak-test-asan`) to verify the FFI RAII
  layer cleans up libwebp's internal allocations on every path,
  including error returns. Wired into the ASan CI job. Local runs
  show zero webpx-attributable leaks; the sole reported leak is
  std's per-thread `stack_overflow::thread_info` allocation,
  released by OS process teardown but not visible to heaptrack
  (suppression added to the LSan list).
- `tests/soundness.rs` gained reproduction tests covering bug-history
  fixes that lacked direct repros: encoder constructor overflow
  panics on huge widths, `from_pixels_stride` truncation,
  `YuvPlanes::new_checked` rejection of out-of-range dims,
  `max_metadata_bytes` enforcement, `max_frames` enforcement against
  declared frame counts, `max_animation_ms` enforcement on
  cumulative timestamps, and `max_pixels` enforcement on
  oversize canvases. 17 soundness tests total.

### Documentation

- README and lib-level rustdoc explain that `Limits::default()` is
  now the safe baseline for untrusted input — callers no longer need
  to remember to apply caps explicitly. The "decoding untrusted
  input" example shows the new pattern (`Limits::default().with_*`).

## [0.2.3] - 2026-05-02

### Testing

- Four new fuzz targets focused on numeric input boundaries:
  `stride_extremes` (every encoder / decoder / streaming entry point
  with strides drawn from `i32::MAX ± N`, `u32::MAX`, and arbitrary),
  `dim_extremes` (width / height at libwebp's 16383 cap and beyond),
  `limits_boundaries` (`Limits` values at u32 / u64 boundaries with
  monotonicity probe), and `yuv_planes` (Y/U/V/A plane sizes /
  strides for `Encoder::new_yuv`). Wired into the cron fuzz matrix.

### Changed (internal — no public API change)

- Introduced an internal `crate::ffi` module with RAII wrappers for
  every libwebp resource that webpx owns. `Demux<'a>` and
  `ChunkIter<'_>` (used by the mux metadata path), `Picture` (used by
  every encode entry point), and `MemWriter` (the encode output
  buffer) auto-release on drop, replacing the bespoke
  `WebPPictureFree` / `WebPMemoryWriterClear` /
  `WebPDemuxReleaseChunkIterator` calls that used to be duplicated at
  every error branch — six sites in `encode.rs` alone, two in `mux.rs`.
  The audit surface for resource cleanup shrinks to one wrapper per
  resource. The wrappers are `pub(crate)`; this is purely an internal
  refactor and the public API is unchanged.
- Stride bounds checks for libwebp's `i32` parameters now live in one
  place: `crate::ffi::validate::stride_fits_i32`. Replaces five
  hand-rolled `if stride > i32::MAX` checks in `encode.rs` and
  `streaming.rs`.

### Documentation

- README, lib-level rustdoc, and the deprecation notes on
  `compat::webp` and `compat::webp_animation` now characterize
  webpx-vs-zenwebp performance honestly: libwebp can be up to ~35 %
  faster on specific photos but up to ~2.5× slower on others, and
  encoded-size differences are at most ~0.02 % (noise). Replaces the
  prior "comparable and sometimes faster" framing.
- Added native `wasm32-unknown-unknown` support to zenwebp's pros
  (webpx requires emscripten because libwebp is C).
- Added libwebp's MIPS DSP-R2 / DSP-ASE assembly paths to webpx's
  narrow set of remaining advantages.

## [0.2.2] - 2026-05-01

### Fixed

- `tests/integration.rs`, `tests/soundness.rs`, `tests/fuzz_regression.rs`,
  and several examples (`quick_encode`, `quick_encode_lossless`,
  `real_image_test`, `wasm_demo`, `mem_formula`) failed to compile
  under `--no-default-features --features decode` (or the symmetric
  `encode`-only build). Tests are now gated on the features they
  exercise; examples carry `required-features` entries; the
  feature-combo CI matrix gained `--tests --examples` builds and
  more single-feature combos so this kind of regression fails fast.
- `src/types.rs` and `src/streaming.rs` exposed encode-side helpers
  (`EncodePixel`, `WebPData::from_raw`, `StreamingEncoder`,
  `AnimationEncoder`) without an `encode` cfg and decode-side
  helpers (`DecodePixel`, `StreamingDecoder`,
  `DecoderConfig::check_still_image`) without a `decode` cfg.
  `cargo build --no-default-features --features <single>` is now
  warning-free.
- `StreamingDecoder::new` rejects `ColorMode::Yuv420` and
  `ColorMode::Yuva420` with `Error::InvalidInput` upfront. The
  prior code mapped them to `MODE_YUV` / `MODE_YUVA` and called
  `WebPINewRGB`, which returns NULL because that FFI only
  constructs RGB-family decoders — callers saw a misleading
  `Error::OutOfMemory`. `with_buffer` already had this rejection;
  `new` now matches.
- `mux::get_chunk` always calls `WebPDemuxReleaseChunkIterator` when
  `WebPDemuxGetChunk` returned non-zero, including the
  empty-chunk / null-bytes branch where the iterator was previously
  leaked. Refactored into a small `inspect_chunk` helper so the
  release path is single-pass.

### Documentation

- README, lib-level rustdoc, and the deprecation notes on
  `compat::webp` and `compat::webp_animation` now position
  **[`zenwebp`](https://github.com/imazen/zenwebp)** as the
  recommended default for any new project. `zenwebp` is equally or
  more capable than `webpx` on every axis (full feature parity:
  lossy / lossless encode and decode, animation, alpha, ICC / EXIF /
  XMP metadata, streaming, presets, limits), with comparable and
  sometimes faster runtime performance and compression, built with
  `#![forbid(unsafe_code)]`. The security argument is concrete:
  libwebp has a documented high-severity CVE history (CVE-2023-4863
  was an actively-exploited 0-click heap overflow patched out of
  band in every major browser), and every libwebp wrapper that has
  been audited — `webpx` included — has shipped soundness bugs
  (0.1.x is yanked; 0.2.0 + 0.2.1 closed multiple FFI issues
  across two audit passes). `zenwebp`'s pure-safe-Rust
  implementation does not have that exposure.
- Dropped "safe Rust" / "safe bindings" framing from the webpx
  positioning (this crate is FFI to libwebp; it inherits whatever
  soundness properties the underlying C provides). Replaced with
  "ergonomic Rust API".

## [0.2.1] - 2026-05-01

### Security

Closes a class of OOB issues found in a second-pass audit: caller-supplied
`u32` / `usize` strides cast to `i32` for libwebp without an upper-bound
check. A stride `>= 2^31` wrapped to a negative `i32`, and libwebp's row
pointer arithmetic walked backwards through process memory. Reachable
from safe Rust without an `unsafe` block by constructing a `&[u8]` of
≥ 2 GiB on 64-bit and passing a stride above `i32::MAX`. Affected sites:
`Encoder::new_rgba_stride` / `new_bgra_stride` / `new_rgb_stride` /
`new_bgr_stride` / `new_argb_stride` / `new_yuv` (every plane stride),
and `StreamingDecoder::with_buffer`.

Also fixed: `Encoder::from_img` and `Encoder::from_pixels_stride`
truncated `(stride * bpp) as u32` when the source `usize` stride
exceeded `u32::MAX / bpp`. Truncation produced a stride smaller than
the actual byte stride, which `validate_buffer_size_stride` could not
catch — libwebp then encoded with a wrong row layout. Not OOB on the
input slice (which still covered the buffer) but a silent
wrong-output bug; saturating cast added so oversized strides hit the
`i32::MAX` upper-bound check instead.

### Changed

- All `MaybeUninit::<libwebp_sys::*>::uninit()` sites switched to
  `MaybeUninit::<libwebp_sys::*>::zeroed()`. libwebp's `*Init*`
  functions only set their documented fields; the `pad` /
  `padding` arrays they leave alone are now initialized to zero
  rather than uninitialized. Resolves a latent UB pattern (reading
  uninit bytes during `assume_init` is technically UB even when
  those bytes are never observed).

### Deprecated

- The compatibility shim modules `compat::webp` and
  `compat::webp_animation` are now `#[deprecated]`. Both swallow
  error variants the main API surfaces (compat `Encoder::encode`
  returns an empty buffer on failure; compat `Decoder::decode`
  collapses every error to `None`) and neither exposes
  [`Limits`](crate::Limits) for untrusted-input decoding. They will
  be retained for at least one minor release.

### Documentation

- `Encoder::new_argb` rustdoc now spells out the
  little-endian-only assumption: the `0xAARRGGBB` numeric layout
  matches libwebp's expected `[B, G, R, A]` byte order on
  little-endian targets (every CI target). Big-endian callers
  should treat this path as unsupported.

### Testing

- `tests/soundness.rs` gains a `stride_overflow` module with four
  regression tests covering ARGB, RGBA, YUV, and streaming
  `with_buffer` stride bounds.


### Added

- `webpx::Limits` — opt-in resource policy modeled on
  `zencodec::ResourceLimits`. Fields: `max_pixels` (per-frame),
  `max_total_pixels` (across all animation frames),
  `max_width`, `max_height`, `max_input_bytes`, `max_frames`,
  `max_animation_ms`, `max_metadata_bytes`, `max_output_bytes`.
  All `Option<T>`; `None` means no webpx-side cap.
  Auto-enforced fields (`max_pixels`, `max_total_pixels`,
  `max_width`, `max_height`, `max_frames`, `max_input_bytes`,
  `max_animation_ms`, `max_metadata_bytes`) are checked at
  parse time on the `_with_limits` decoder / animation /
  mux entry points; see the [`Limits`] docs for the per-field
  enforcement matrix.
- `webpx::LimitExceeded` — error variant carrying the actual value
  and the limit that was exceeded. Returned via the new
  `Error::LimitExceeded` variant, also implements
  `core::error::Error` standalone.
- `DecoderConfig::limits(Limits)` and `::get_limits()` — apply the
  policy at parse time. `max_width`, `max_height`, `max_pixels`, and
  `max_total_pixels` are checked via `WebPGetFeatures` before
  libwebp allocates the output buffer; if `scale()` is also set,
  the post-scale dimensions are checked too.
- `AnimationDecoder::with_options_limits(data, mode, threads, &Limits)`
  + `AnimationDecoder::get_limits()` — applies limits against the
  canvas dimensions, declared `frame_count`, and input size before
  any decode work. `max_total_pixels` catches the W × H × N
  animation-bomb case that per-frame `max_pixels` misses; the
  decoder also enforces `max_animation_ms` against the cumulative
  frame timestamp inside `decode_all`. Closes [#4].
- `mux::get_icc_profile_with_limits` / `get_exif_with_limits` /
  `get_xmp_with_limits` — apply `Limits::max_metadata_bytes` on
  top of the existing 256 MiB internal hard cap.

### Changed (breaking)

- `EncoderConfig` and `DecoderConfig` are now `#[non_exhaustive]`.
  They have `pub(crate)` fields so external struct-literal
  construction was already disallowed; the marker documents that
  these structs will grow (`limits`, future config knobs) without
  another minor bump.
- `error::EncodingError`, `error::DecodingError`, `error::MuxError`
  and `types::ImageInfo` are now `#[non_exhaustive]`. Match arms must
  add `_ =>` and struct construction must go through a constructor.
  This closes the door on adding new libwebp error variants or new
  bitstream-info fields requiring another minor bump.
- `Error` gains a `LimitExceeded(LimitExceeded)` variant. Already
  `#[non_exhaustive]`, so adding the variant is a non-breaking
  additive change for `match` arms with `_ =>`.

### Fixed

- The crate now compiles under every feature combination
  (`--no-default-features` with any subset of `decode` / `encode` /
  `std` / `streaming` / `animation` / `icc`). The compat layer's
  `Encoder` / `Decoder` types are now properly gated on the matching
  feature, and `EncoderConfig`'s encoding entry points are gated on
  `encode`. CI gains a feature-combo matrix to prevent regression.
  Closes [#1].

[#1]: https://github.com/imazen/webpx/issues/1
[#4]: https://github.com/imazen/webpx/issues/4

## [0.2.0] - 2026-05-01

### Security

Soundness and bounds-checking fixes from a multi-pass audit of the FFI layer
(initial review plus parallel sweeps for integer overflow, 32-bit-only
flaws, additional `unsafe` review, and unbounded-resource exposure). Full
technical detail and reproducers will be published in a coordinated security
advisory once 0.2.0 is on crates.io. Versions 0.1.0–0.1.4 will be yanked at
that time. Users on those versions should upgrade.

### Added

- `YuvPlanes::new_checked(width, height, with_alpha) -> Option<Self>` —
  non-panicking constructor that rejects out-of-range dimensions.
- `validate_yuv_planes` (internal) — bounds-checks Y/U/V/A planes
  against stride × rows before pointer-casting into libwebp.
- Hard cap (`MAX_METADATA_CHUNK_BYTES`, 256 MiB) on ICCP/EXIF/XMP chunk
  sizes returned by `get_icc_profile` / `get_exif` / `get_xmp` to bound
  attacker-controlled allocations from the demuxer.

### Changed (breaking)

- `StreamingDecoder` is now `StreamingDecoder<'a>`. The `'a` parameter
  ties a buffer-backed decoder to the buffer's lifetime so the borrow
  checker rejects use-after-free patterns. `StreamingDecoder::new()`
  returns `StreamingDecoder<'static>`, so call sites that don't use
  `with_buffer` compile unchanged. `StreamingDecoder` no longer
  auto-implements `UnwindSafe` / `RefUnwindSafe` as a side effect.
- `AnimationDecoder::with_options` now returns `Err` for color modes
  that libwebp's animation decoder cannot satisfy
  (`ColorMode::Rgb`, `Bgr`, `Argb`). Previously the constructor
  accepted these and `WebPAnimDecoderNew` later returned NULL with
  no explanation.
- `Encoder::new_yuv` now returns an error if the supplied
  `YuvPlanesRef` has plane lengths shorter than `stride × rows`,
  if the strides are below `width`/`uv_width`, or if `u_stride !=
  v_stride` (libwebp uses a single uv_stride field).
- `AnimationDecoder::decode_all` now caps its initial `Vec::with_capacity`
  reservation at 4096 frames; declared frame counts above that are
  appended via `push()` rather than reserved up-front.

### Fixed

- Encoder zero-copy ARGB and YUV-with-alpha paths now keep their input
  buffers strictly read-only.
- `AnimationDecoder::next_frame` derives the frame buffer length from
  the configured color mode rather than a hard-coded value.
- `ImageInfo::from_webp` no longer treats `WebPBitstreamFeatures` as
  initialized when `WebPGetFeatures` reports an error.
- `decode_advanced` calls `WebPFreeDecBuffer` defensively on the error
  path and uses libwebp's reported allocation size for the output slice.
- All `width × height × bpp`, `stride × rows`, and `slice.len() × bpp`
  computations that consume user- or libwebp-supplied dimensions now
  use `saturating_mul`. Previously the unchecked products could wrap
  on 32-bit `usize` (i686 is in the CI matrix) and bypass downstream
  length guards.
- `StreamingDecoder::get_partial` and `::finish` reject negative
  strides / dimensions returned by libwebp so `as usize` cannot wrap
  into a multi-gigabyte read.
- `decode_yuv` validates `y_stride`, `uv_stride`, `height`, and
  `uv_height` are non-negative before computing plane sizes.
- `decode_into` rejects strides that would not fit in libwebp's `i32`
  stride parameter.
- `AnimationDecoder::decode_all` uses `saturating_sub` when computing
  per-frame durations from declared timestamps, preventing wrap on
  hostile non-monotonic timestamps.
- `YuvPlanes::new` validates dimensions before allocating; the
  panicking constructor now panics with a clear message rather than
  via the underlying `vec!` macro on capacity overflow.
