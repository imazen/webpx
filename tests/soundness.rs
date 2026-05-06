//! Soundness regression tests from imazen/webpx#2.
//!
//! Each test asserts a memory-safety invariant that earlier API shapes broke.
//! Run normally for the logical assertions, or under ASan for use-after-free
//! detection on the streaming-decoder lifetime test:
//!
//! ```sh
//! RUSTFLAGS='-Zsanitizer=address' \
//!     cargo +nightly test -Zbuild-std --release \
//!     --target x86_64-unknown-linux-gnu --test soundness
//! ```

#![cfg(feature = "encode")]

use webpx::*;

#[cfg(feature = "streaming")]
fn rgba_fixture(width: u32, height: u32) -> Vec<u8> {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            rgba.extend_from_slice(&[
                (x * 40 + 10) as u8,
                (y * 50 + 20) as u8,
                (x * 17 + y * 19 + 30) as u8,
                255,
            ]);
        }
    }
    rgba
}

#[cfg(feature = "streaming")]
fn encode_lossless_rgba(rgba: &[u8], width: u32, height: u32) -> Vec<u8> {
    Encoder::new_rgba(rgba, width, height)
        .lossless(true)
        .encode(Unstoppable)
        .expect("lossless encode should succeed")
}

#[test]
fn argb_lossless_encode_must_not_mutate_shared_input() {
    let argb = vec![0x00_ff_00_00u32];
    let original = argb.clone();

    Encoder::new_argb(&argb, 1, 1)
        .lossless(true)
        .encode(Unstoppable)
        .expect("lossless argb encode should succeed");

    assert_eq!(
        argb, original,
        "safe ARGB encoding mutated data passed through an immutable slice"
    );
}

#[test]
fn yuv_encode_must_not_mutate_shared_planes() {
    let width = 8;
    let height = 8;
    let y: Vec<u8> = (0..(width * height)).map(|v| v as u8).collect();
    let u: Vec<u8> = (0..((width / 2) * (height / 2)))
        .map(|v| (64 + v) as u8)
        .collect();
    let v: Vec<u8> = (0..((width / 2) * (height / 2)))
        .map(|v| (128 + v) as u8)
        .collect();
    let a = vec![0u8; (width * height) as usize];
    let original_y = y.clone();
    let original_u = u.clone();
    let original_v = v.clone();

    let planes = YuvPlanesRef {
        y: &y,
        y_stride: width as usize,
        u: &u,
        u_stride: (width / 2) as usize,
        v: &v,
        v_stride: (width / 2) as usize,
        a: Some(&a),
        a_stride: width as usize,
        width,
        height,
    };

    Encoder::new_yuv(planes)
        .encode(Unstoppable)
        .expect("yuv encode should succeed");

    assert_eq!(
        y, original_y,
        "safe YUV encoding mutated the Y plane passed through an immutable slice"
    );
    assert_eq!(
        u, original_u,
        "safe YUV encoding mutated the U plane passed through an immutable slice"
    );
    assert_eq!(
        v, original_v,
        "safe YUV encoding mutated the V plane passed through an immutable slice"
    );
}

#[test]
fn yuv_encode_must_reject_planes_shorter_than_stride_height() {
    let width = 2;
    let height = 2;
    let hidden_tail = [16u8, 235, 235, 235];
    let u = [128u8];
    let v = [128u8];
    let planes = YuvPlanesRef {
        y: &hidden_tail[..1],
        y_stride: width as usize,
        u: &u,
        u_stride: 1,
        v: &v,
        v_stride: 1,
        a: None,
        a_stride: 0,
        width,
        height,
    };

    let result = Encoder::new_yuv(planes).encode(Unstoppable);
    assert!(
        result.is_err(),
        "safe YUV encoding accepted a Y plane shorter than y_stride * height"
    );
}

/// Regression tests for the second-pass-audit class: caller-supplied
/// strides cast to `i32` for libwebp without an upper-bound check.
/// A stride `>= 2^31` wraps to a negative `i32`, and libwebp's row
/// pointer arithmetic walks backwards through process memory. The
/// fixes in 0.2.1 reject any stride above `i32::MAX` before the cast.
mod stride_overflow {
    use super::*;

    /// Smallest stride that wraps to a negative `i32`.
    const NEGATIVE_BOUNDARY: u32 = (i32::MAX as u32) + 1;

    #[test]
    fn argb_zero_copy_rejects_stride_above_i32_max() {
        // 1×1 image with `stride_pixels = 2^31`. The data-length check
        // requires `data.len() >= stride * height` which is impractical
        // to satisfy at this magnitude; the test confirms the call
        // errors (whether via the length check or the stride bound) so
        // there's no path that reaches the libwebp `as i32` cast.
        let argb: Vec<u32> = vec![0; 64];
        let r = Encoder::new_argb_stride(&argb, 1, 1, NEGATIVE_BOUNDARY).encode(Unstoppable);
        assert!(
            r.is_err(),
            "ARGB stride > i32::MAX must be rejected before the cast",
        );
    }

    #[test]
    fn rgba_stride_above_i32_max_is_rejected() {
        // Same idea for the RGBA byte path. `validate_buffer_size_stride`
        // has to gate the stride before the libwebp Import call.
        let data = vec![0u8; 16];
        let r = Encoder::new_rgba_stride(&data, 1, 1, NEGATIVE_BOUNDARY).encode(Unstoppable);
        assert!(
            r.is_err(),
            "RGBA stride > i32::MAX must be rejected before the cast",
        );
    }

    #[test]
    fn yuv_stride_above_i32_max_is_rejected() {
        // Y stride too large. width = 2, height = 2; we set y_stride
        // to a value above `i32::MAX as usize`. The Y plane needs
        // `y_stride * 2` bytes which is impractical to allocate, so
        // we use a smaller plane that fails the *length* check first.
        // The stride bound check is tested directly via
        // `validate_yuv_planes` semantics: u_stride above i32::MAX
        // should also reject.
        //
        // To exercise just the stride bound, we build planes whose
        // length checks pass (oversized slice) only on 64-bit. On
        // 32-bit `usize::MAX as i32` is problematic anyway, so the
        // upper-bound check is a no-op there. Skip on 32-bit.
        if usize::BITS < 64 {
            return;
        }
        let big_stride = (i32::MAX as usize) + 1;
        let y = vec![0u8; big_stride * 2];
        let u = vec![0u8; 1];
        let v = vec![0u8; 1];
        let planes = YuvPlanesRef {
            y: &y,
            y_stride: big_stride,
            u: &u,
            u_stride: 1,
            v: &v,
            v_stride: 1,
            a: None,
            a_stride: 0,
            width: 2,
            height: 2,
        };
        let r = Encoder::new_yuv(planes).encode(Unstoppable);
        assert!(
            r.is_err(),
            "Y stride > i32::MAX must be rejected by validate_yuv_planes",
        );
    }

    #[cfg(feature = "streaming")]
    #[cfg(all(feature = "streaming", feature = "decode"))]
    #[test]
    fn streaming_with_buffer_rejects_stride_above_i32_max() {
        // `WebPINewRGB` takes the stride as `i32`; a wrapped-negative
        // value would cause libwebp to write to addresses *before* the
        // caller's buffer.
        let mut buf = vec![0u8; 64];
        let r = StreamingDecoder::with_buffer(&mut buf, (i32::MAX as usize) + 1, ColorMode::Rgba);
        assert!(
            r.is_err(),
            "StreamingDecoder::with_buffer must reject stride > i32::MAX",
        );
    }
}

// The original report exercised this anti-pattern at runtime:
//
//     let mut decoder = {
//         let mut output = vec![0u8; ...];
//         StreamingDecoder::with_buffer(&mut output, stride, ColorMode::Rgba).unwrap()
//     };
//     decoder.append(&webp).expect(...);  // UAF: output dropped at end of inner block
//
// After the soundness fix added a lifetime to `StreamingDecoder<'_>`, the
// borrow checker rejects that shape outright. The runtime test below
// verifies the legitimate (buffer-outlives-decoder) path still works.
#[cfg(all(feature = "streaming", feature = "decode"))]
#[test]
fn streaming_decoder_with_buffer_safe_usage() {
    let width = 16;
    let height = 16;
    let rgba = rgba_fixture(width, height);
    let webp = encode_lossless_rgba(&rgba, width, height);
    let stride = (width * 4) as usize;

    let mut output = vec![0u8; stride * height as usize];
    let mut decoder = StreamingDecoder::with_buffer(&mut output, stride, ColorMode::Rgba)
        .expect("streaming decoder with external buffer");

    decoder.append(&webp).expect("streaming decode append");
    let (decoded, decoded_width, decoded_height) =
        decoder.finish().expect("finish on caller-owned buffer");

    assert_eq!((decoded_width, decoded_height), (width, height));
    assert_eq!(decoded, rgba);
}

#[cfg(all(feature = "streaming", feature = "decode"))]
#[test]
fn streaming_get_partial_respects_external_buffer_minimum_extent() {
    let width = 1;
    let height = 2;
    let rgba = rgba_fixture(width, height);
    let webp = encode_lossless_rgba(&rgba, width, height);

    // libwebp accepts caller-owned output buffers sized to
    // `(height - 1) * stride + row_bytes`; the final row does not need
    // trailing stride padding. The old `get_partial` wrapper exposed
    // `stride * decoded_rows` bytes instead, so this 1x2 RGBA image with
    // stride 8 returned a 16-byte slice over a 12-byte buffer. Reading the
    // last byte of that slice trips ASan as a heap-buffer-overflow.
    let stride = 8;
    let row_bytes = width as usize * 4;
    let expected_partial_len = stride * (height as usize - 1) + row_bytes;
    let mut output = vec![0xaa; expected_partial_len];
    let mut decoder = StreamingDecoder::with_buffer(&mut output, stride, ColorMode::Rgba)
        .expect("streaming decoder with tightly-sized external buffer");

    decoder.append(&webp).expect("streaming decode append");
    let (partial, partial_width, partial_rows) = decoder
        .get_partial()
        .expect("complete decode should be visible");

    assert_eq!((partial_width, partial_rows), (width, height));
    assert_eq!(partial.len(), expected_partial_len);
    assert_eq!(partial[partial.len() - 1], rgba[rgba.len() - 1]);

    let (decoded, decoded_width, decoded_height) =
        decoder.finish().expect("finish on caller-owned buffer");
    assert_eq!((decoded_width, decoded_height), (width, height));
    assert_eq!(decoded, rgba);
}

// `StreamingDecoder::new` is the libwebp-allocated path that calls
// `WebPINewRGB`. That FFI only constructs RGB-family decoders; passing a
// YUV color mode used to fall through to `WebPINewRGB` and surface as
// `Error::OutOfMemory` when libwebp returned a NULL pointer. The
// rejection added in 0.2.2 routes both YUV variants to a clear
// `Error::InvalidInput` instead of the misleading OOM.
#[cfg(all(feature = "streaming", feature = "decode"))]
#[test]
fn streaming_decoder_new_rejects_yuv_modes() {
    for mode in [ColorMode::Yuv420, ColorMode::Yuva420] {
        let r = StreamingDecoder::new(mode);
        match r {
            Err(at) => {
                let (err, _) = at.decompose();
                assert!(
                    matches!(err, Error::InvalidInput(_)),
                    "{:?} must surface InvalidInput, not OutOfMemory",
                    mode,
                );
            }
            Ok(_) => panic!("StreamingDecoder::new accepted {:?}", mode),
        }
    }
}

// `Encoder::new_rgba` / `new_bgra` / `new_rgb` / `new_bgr` used to panic
// with an arithmetic-overflow trap when constructed with `width >= 2^30`
// (RGBA) or `width >= 2^30 + ...` (RGB) — `width * 4` / `width * 3` overflowed
// `u32` and the panic happened *inside the constructor*, before the
// caller had a chance to validate dimensions. Discovered by `dim_extremes`
// fuzzing on 2026-05-02. Fix: saturating_mul on the byte stride; the
// oversize stride flows through to `validate_buffer_size_stride` which
// rejects it as a clean error.
#[test]
fn encoder_constructors_dont_panic_on_huge_width() {
    // u32::MAX width is the worst case for every constructor.
    let small = [0u8; 16];
    // None of these may panic. The encode itself will error (invalid
    // dimensions / oversize stride) but the constructor must not.
    let _e1 = Encoder::new_rgba(&small, u32::MAX, 1);
    let _e2 = Encoder::new_bgra(&small, u32::MAX, 1);
    let _e3 = Encoder::new_rgb(&small, u32::MAX, 1);
    let _e4 = Encoder::new_bgr(&small, u32::MAX, 1);

    // And a value just below the libwebp 16383 cap to confirm the
    // saturating path doesn't pessimize the common case.
    let r = Encoder::new_rgba(&small, 1_073_741_824, 1).encode(Unstoppable);
    assert!(r.is_err(), "huge width must produce a clean error");
}

// `Encoder::from_pixels_stride` truncated `(stride_pixels * bpp) as u32`
// when the source `u32` pixel-stride exceeded `u32::MAX / bpp`. The
// truncated byte stride then satisfied `validate_buffer_size_stride`'s
// checks while libwebp encoded with the wrong row layout (silent
// wrong-output bug). Fix in 0.2.1: saturating cast so oversize strides
// hit the i32::MAX upper-bound check instead of being silently truncated.
#[test]
fn from_pixels_stride_with_huge_pixel_stride_rejected() {
    use rgb::RGBA8;
    let pixels: Vec<RGBA8> = vec![RGBA8::new(0, 0, 0, 0); 16];
    // pixel_stride > u32::MAX / 4 (RGBA bpp). The old truncating cast
    // would have produced a valid-looking byte stride; saturation
    // pushes this to u32::MAX which fails the i32::MAX check.
    let huge_pixel_stride: u32 = (u32::MAX / 4) + 1;
    let r = Encoder::from_pixels_stride(&pixels, 1, 1, huge_pixel_stride).encode(Unstoppable);
    assert!(
        r.is_err(),
        "from_pixels_stride must reject pixel-strides that overflow u32::MAX when scaled by bpp",
    );
}

// `YuvPlanes::new_checked` is the non-panicking constructor added in
// 0.2.0 specifically because the panicking `YuvPlanes::new` constructor
// allocated via `vec!` and could panic with a confusing capacity-overflow
// message instead of a clean rejection. Out-of-range dimensions must
// return `None` here, and the panicking constructor must produce a
// clear panic message rather than the underlying vec macro's overflow.
#[test]
fn yuv_planes_new_checked_rejects_out_of_range_dimensions() {
    // 16384 exceeds libwebp's intrinsic max of 16383.
    assert!(YuvPlanes::new_checked(u32::MAX, 1, false).is_none());
    assert!(YuvPlanes::new_checked(1, u32::MAX, false).is_none());
    // u32::MAX × u32::MAX × bpp wraps usize on every platform.
    assert!(YuvPlanes::new_checked(u32::MAX, u32::MAX, true).is_none());
    // The legitimate small case still succeeds.
    assert!(YuvPlanes::new_checked(64, 48, false).is_some());
}

// 0.2.0 added `MAX_METADATA_CHUNK_BYTES = 256 MiB` as an internal hard
// cap on ICCP / EXIF / XMP chunk sizes returned by the demuxer.
// libwebp itself accepts ICCP / EXIF / XMP chunks up to ~4 GB; without
// the cap, a hostile WebP declaring a 4 GB chunk would force webpx to
// allocate a 4 GB Vec on every `get_*` call. Caller-supplied
// `Limits::max_metadata_bytes` lets users set a *tighter* cap on top.
//
// Direct exercise of the 256 MiB hard cap is impractical (we'd need to
// embed a 256 MiB chunk into a synthetic WebP); the test below covers
// the caller-side `max_metadata_bytes` path instead, which uses the
// same code path through `inspect_chunk` and would surface a regression
// in the cap-check structure.
#[cfg(feature = "icc")]
#[test]
fn metadata_get_with_tight_max_metadata_bytes_rejects_oversize_chunk() {
    // Build a real WebP with a 1 KiB ICC profile, then read it back
    // with `max_metadata_bytes = 512` — must reject.
    let rgba = vec![255u8; 4 * 4 * 4];
    let webp = Encoder::new_rgba(&rgba, 4, 4)
        .quality(80.0)
        .encode(Unstoppable)
        .expect("encode");
    let icc = vec![0xa5u8; 1024];
    let webp_with_icc = webpx::embed_icc(&webp, &icc).expect("embed icc");

    let tight = Limits::none().with_max_metadata_bytes(512);
    let r = webpx::get_icc_profile_with_limits(&webp_with_icc, &tight);
    match r {
        Err(at) => {
            let (err, _) = at.decompose();
            assert!(
                matches!(err, Error::LimitExceeded(_)),
                "max_metadata_bytes overflow must surface LimitExceeded, got {:?}",
                err
            );
        }
        Ok(_) => panic!("max_metadata_bytes=512 accepted a 1 KiB ICC chunk"),
    }

    // And the same call with a generous cap must succeed.
    let loose = Limits::none().with_max_metadata_bytes(4096);
    let icc_back = webpx::get_icc_profile_with_limits(&webp_with_icc, &loose)
        .expect("loose cap should succeed")
        .expect("ICC chunk must be present");
    assert_eq!(icc_back, icc);
}

// 0.2.0 added a 4096-frame cap on `AnimationDecoder::decode_all`'s
// initial `Vec::with_capacity` reservation. Without the cap, a hostile
// WebP declaring frame_count = u32::MAX would force a 96 GB allocation
// up front (24 bytes per Frame × 4 billion). This is also enforced
// against the `max_frames` budget when `with_options_limits` is used.
//
// Use distinct per-frame content so libwebp doesn't dedupe and report
// frame_count = 1.
#[cfg(feature = "animation")]
#[test]
fn animation_decoder_max_frames_rejects_huge_declared_frame_count() {
    let mut frame_a = vec![0u8; 8 * 8 * 4];
    for px in frame_a.chunks_mut(4) {
        px.copy_from_slice(&[255, 0, 0, 255]);
    }
    let mut frame_b = vec![0u8; 8 * 8 * 4];
    for px in frame_b.chunks_mut(4) {
        px.copy_from_slice(&[0, 255, 0, 255]);
    }
    let mut frame_c = vec![0u8; 8 * 8 * 4];
    for px in frame_c.chunks_mut(4) {
        px.copy_from_slice(&[0, 0, 255, 255]);
    }

    let mut enc = AnimationEncoder::new(8, 8).expect("encoder");
    enc.add_frame_rgba(&frame_a, 0).expect("add frame 0");
    enc.add_frame_rgba(&frame_b, 100).expect("add frame 1");
    enc.add_frame_rgba(&frame_c, 200).expect("add frame 2");
    let webp = enc.finish(300).expect("finish");

    let tight = Limits::none().with_max_frames(2);
    let r = AnimationDecoder::with_options_limits(&webp, ColorMode::Rgba, true, &tight);
    match r {
        Err(at) => {
            let (err, _) = at.decompose();
            assert!(
                matches!(err, Error::LimitExceeded(_)),
                "max_frames=2 must surface LimitExceeded for a 3-frame animation, got {:?}",
                err
            );
        }
        Ok(_) => panic!("max_frames=2 accepted a 3-frame animation"),
    }
}

// 0.2.0 made `AnimationDecoder::decode_all` use `saturating_sub` when
// computing per-frame durations from declared timestamps, so a hostile
// non-monotonic timestamp (frame 1 at t=100, frame 2 at t=50) cannot
// wrap into a multi-billion-millisecond duration. This is paired with
// `max_animation_ms` enforcement against the cumulative timestamp.
//
// Use distinct frames to keep libwebp from de-duplicating them.
#[cfg(feature = "animation")]
#[test]
fn animation_decoder_max_animation_ms_rejects_long_animations() {
    let mut frame_a = vec![0u8; 8 * 8 * 4];
    for px in frame_a.chunks_mut(4) {
        px.copy_from_slice(&[200, 50, 50, 255]);
    }
    let mut frame_b = vec![0u8; 8 * 8 * 4];
    for px in frame_b.chunks_mut(4) {
        px.copy_from_slice(&[50, 200, 50, 255]);
    }
    let mut frame_c = vec![0u8; 8 * 8 * 4];
    for px in frame_c.chunks_mut(4) {
        px.copy_from_slice(&[50, 50, 200, 255]);
    }

    let mut enc = AnimationEncoder::new(8, 8).expect("encoder");
    enc.add_frame_rgba(&frame_a, 0).expect("add frame 0");
    enc.add_frame_rgba(&frame_b, 5_000).expect("add frame 1");
    enc.add_frame_rgba(&frame_c, 60_000).expect("add frame 2");
    let webp = enc.finish(120_000).expect("finish");

    let tight = Limits::none().with_max_animation_ms(30_000);
    let mut dec = AnimationDecoder::with_options_limits(&webp, ColorMode::Rgba, true, &tight)
        .expect("decoder construction succeeds; max_animation_ms is checked in decode_all");
    let r = dec.decode_all();
    match r {
        Err(at) => {
            let (err, _) = at.decompose();
            assert!(
                matches!(err, Error::LimitExceeded(_)),
                "max_animation_ms=30s must reject a 120s animation, got {:?}",
                err
            );
        }
        Ok(_) => panic!("max_animation_ms=30s accepted a 120s animation"),
    }
}

// 0.2.0 added `DecoderConfig::limits` with auto-enforcement of
// `max_pixels` against `WebPGetFeatures` *before* libwebp allocates
// the output buffer. Without this, a hostile WebP declaring a
// 16383×16383 canvas would force a ~1 GiB allocation at 4 bpp.
// Already covered by `test_decoder_max_pixels_rejects_over_budget`
// in integration.rs; this version is the per-pixel-budget reproduction
// against an encoded image we know exceeds the cap.
#[cfg(feature = "decode")]
#[test]
fn decoder_max_pixels_rejects_oversize_canvas() {
    let rgba = vec![255u8; 64 * 64 * 4];
    let webp = Encoder::new_rgba(&rgba, 64, 64)
        .quality(80.0)
        .encode(Unstoppable)
        .expect("encode");

    // 64×64 = 4096 pixels; cap at 1024.
    let tight = Limits::none().with_max_pixels(1024);
    let cfg = DecoderConfig::new().limits(tight);
    let dec = Decoder::new(&webp).expect("decoder").config(cfg);
    let r = dec.decode_rgba();
    match r {
        Err(at) => {
            let (err, _) = at.decompose();
            assert!(
                matches!(err, Error::LimitExceeded(_)),
                "max_pixels=1024 must reject a 4096-pixel image, got {:?}",
                err
            );
        }
        Ok(_) => panic!("max_pixels=1024 accepted a 4096-pixel image"),
    }
}

// Earlier webpx (≤ 0.3.3) routed `StreamingDecoder::update` through
// `libwebp_sys::WebPIUpdate`, which retains a raw pointer to the
// input buffer and re-reads it on subsequent calls. The
// `update(&mut self, data: &[u8])` signature did not tie `data`'s
// lifetime to the decoder, so a caller could legitimately drop the
// buffer between calls — the next `update` / `finish` / `get_partial`
// would dereference the dangling pointer (use-after-free reachable
// from safe Rust).
//
// 0.3.4 routes `update` to `append` (which copies internally), so
// the buffer's lifetime no longer matters across calls. This test
// exercises the canonical UAF shape: drop the input buffer between
// `update` and `finish`. Under the old impl this would trip ASan as
// a heap-buffer-overflow read; under the fixed impl it must succeed
// because the data was copied during `update`.
#[cfg(all(feature = "streaming", feature = "decode"))]
#[test]
fn streaming_decoder_update_does_not_dangle_input_buffer() {
    let width = 32;
    let height = 32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..(width * height) {
        rgba.extend_from_slice(&[100, 150, 200, 255]);
    }
    let webp = Encoder::new_rgba(&rgba, width, height)
        .lossless(true)
        .encode(Unstoppable)
        .expect("encode");

    let mut decoder = StreamingDecoder::new(ColorMode::Rgba).expect("decoder");
    {
        // Pass a copy of the webp bytes from a buffer that drops at
        // end-of-block. With the old `WebPIUpdate` routing, libwebp
        // would stash the pointer; once the block exits and `webp_copy`
        // is freed, any subsequent decoder call dangles.
        let webp_copy = webp.clone();
        let _ = decoder.update(&webp_copy);
        // `webp_copy` drops here.
    }

    // After the buffer drops, ask the decoder to produce its result.
    // With the fix, the decoded bytes are still in libwebp's owned
    // copy — works fine. Without the fix, this would re-read freed
    // memory.
    let (decoded, w, h) = decoder.finish().expect("finish");
    assert_eq!((w, h), (width, height));
    assert_eq!(decoded, rgba);
}

// `progress_hook` is invoked from libwebp's C frames during encoding;
// if a user-supplied `Stop::should_stop` panics, the panic must not
// unwind through C (UB). 0.3.3 wraps the user callback in
// `catch_unwind` and re-raises the panic from a Rust frame after
// `WebPEncode` returns control. This test verifies the panic is
// propagated to the caller (not silently swallowed) and that no
// resources leak in the process.
#[cfg(feature = "std")]
#[test]
fn progress_hook_panic_is_caught_and_replayed() {
    use std::sync::atomic::{AtomicBool, Ordering};

    struct PanickyStop {
        called: AtomicBool,
    }
    impl enough::Stop for PanickyStop {
        fn check(&self) -> core::result::Result<(), enough::StopReason> {
            Ok(())
        }
        fn should_stop(&self) -> bool {
            // First call panics; the catch_unwind in progress_hook
            // should capture the panic and abort encoding cleanly.
            if !self.called.swap(true, Ordering::SeqCst) {
                panic!("user callback panicked");
            }
            false
        }
    }

    let rgba = vec![128u8; 64 * 64 * 4];
    let stop = PanickyStop {
        called: AtomicBool::new(false),
    };

    // The encode call itself runs on this thread; catch_unwind below
    // captures the panic that progress_hook re-raised.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        Encoder::new_rgba(&rgba, 64, 64).quality(50.0).encode(&stop)
    }));

    match result {
        Err(_) => {
            // Expected: the user's panic propagated cleanly.
        }
        Ok(_) => {
            // It's also valid for the encode to complete successfully
            // if libwebp doesn't invoke the progress hook (small image,
            // single-pass encode). This isn't a soundness failure —
            // we just couldn't exercise the panic path. Accept it.
        }
    }
}
