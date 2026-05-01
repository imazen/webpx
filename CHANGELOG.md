# Changelog

## [Unreleased]

### Fixed

- Encoder YUV path now validates each plane's slice length against
  `stride * rows` (chroma planes use `ceil(h/2)` rows) before passing
  raw pointers to libwebp, so a `YuvPlanesRef` with a too-short Y/U/V
  plane is rejected with `Error::InvalidInput` instead of letting
  libwebp read out of bounds.

## [0.2.0] - 2026-05-01

### Security

Soundness fixes from an internal audit of the FFI layer. Full technical
detail and reproducers will be published in a coordinated security
advisory once 0.2.0 is on crates.io. Versions 0.1.0–0.1.4 will be
yanked at that time. Users on those versions should upgrade.

### Changed (breaking)

- `StreamingDecoder` is now `StreamingDecoder<'a>`. The `'a` parameter
  ties a buffer-backed decoder to the buffer's lifetime so the borrow
  checker rejects use-after-free patterns. `StreamingDecoder::new()`
  returns `StreamingDecoder<'static>`, so call sites that don't use
  `with_buffer` compile unchanged. `StreamingDecoder` no longer
  auto-implements `UnwindSafe` / `RefUnwindSafe` as a side effect
  (c1adddc).
- `AnimationDecoder::with_options` now returns `Err` for color modes
  that libwebp's animation decoder cannot satisfy
  (`ColorMode::Rgb`, `Bgr`, `Argb`). Previously the constructor
  accepted these and `WebPAnimDecoderNew` later returned NULL with
  no explanation (c1adddc).

### Fixed

- Encoder: zero-copy ARGB and YUV-with-alpha paths now keep their
  input buffers strictly read-only end-to-end (c1adddc).
- `AnimationDecoder::next_frame`: frame buffer length now derives
  from the configured color mode rather than a hard-coded value
  (c1adddc).
- `ImageInfo::from_webp`: features are only treated as initialized
  after `WebPGetFeatures` reports success (c1adddc).
- `decode_advanced`: defensive `WebPFreeDecBuffer` on the error
  path; output slice length now uses libwebp's reported allocation
  size rather than a recomputed value (c1adddc).
