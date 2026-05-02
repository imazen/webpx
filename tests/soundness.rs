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
