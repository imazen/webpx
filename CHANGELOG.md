# Changelog

## [Unreleased]

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
