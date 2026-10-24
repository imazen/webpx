//! Animation timing and container fields, checked against libwebp's own
//! demuxer.
//!
//! Fixtures are written chunk by chunk so each ANMF chunk carries exactly the
//! duration the test chose; libwebp's demuxer then reads them back as the
//! oracle.
#![cfg(all(feature = "animation", feature = "decode", feature = "encode"))]

use webpx::{AnimationDecoder, ColorMode, Limits, Unstoppable};

/// A distinct, opaque `w × h` RGBA frame per `seed`.
fn frame_pixels(w: u32, h: u32, seed: u8) -> Vec<u8> {
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for i in 0..w * h {
        px.extend_from_slice(&[seed, (i as u8).wrapping_mul(7), seed ^ 0x5a, 255]);
    }
    px
}

/// Append a RIFF chunk (`fourcc`, little-endian size, payload, pad byte).
fn put_chunk(out: &mut Vec<u8>, fourcc: &[u8; 4], payload: &[u8]) {
    out.extend_from_slice(fourcc);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    if payload.len() % 2 == 1 {
        out.push(0);
    }
}

fn put_u24(out: &mut Vec<u8>, v: u32) {
    assert!(v < 1 << 24);
    out.extend_from_slice(&v.to_le_bytes()[..3]);
}

/// Write an animated WebP container by hand (VP8X + ANIM + one ANMF per
/// duration, each wrapping a distinct lossless frame).
///
/// libwebp's mux would rewrite a single full-canvas frame as a still image
/// (`MuxCleanup`), and `WebPAnimEncoder` merges identical frames, so neither
/// can produce every fixture these tests need.
fn mux_animation(w: u32, h: u32, durations: &[u32], loop_count: u16, bgcolor: u32) -> Vec<u8> {
    let mut body = Vec::new();
    let mut vp8x = vec![0x02, 0, 0, 0]; // animation flag
    put_u24(&mut vp8x, w - 1);
    put_u24(&mut vp8x, h - 1);
    put_chunk(&mut body, b"VP8X", &vp8x);
    let mut anim = bgcolor.to_le_bytes().to_vec();
    anim.extend_from_slice(&loop_count.to_le_bytes());
    put_chunk(&mut body, b"ANIM", &anim);
    for (i, &duration) in durations.iter().enumerate() {
        let still = webpx::Encoder::new_rgba(&frame_pixels(w, h, i as u8 * 40 + 1), w, h)
            .lossless(true)
            .encode(Unstoppable)
            .expect("encode still frame");
        // A simple lossless file is RIFF header + one VP8L chunk.
        assert_eq!(&still[12..16], b"VP8L");
        let mut anmf = Vec::new();
        put_u24(&mut anmf, 0); // x offset / 2
        put_u24(&mut anmf, 0); // y offset / 2
        put_u24(&mut anmf, w - 1);
        put_u24(&mut anmf, h - 1);
        put_u24(&mut anmf, duration);
        anmf.push(0x02); // do not blend, do not dispose
        anmf.extend_from_slice(&still[12..]);
        put_chunk(&mut body, b"ANMF", &anmf);
    }
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32 + 4).to_le_bytes());
    out.extend_from_slice(b"WEBP");
    out.extend_from_slice(&body);
    out
}

/// Per-frame durations as libwebp's demuxer reads them from the ANMF chunks.
fn demux_durations(data: &[u8]) -> Vec<u32> {
    unsafe {
        let webp_data = libwebp_sys::WebPData {
            bytes: data.as_ptr(),
            size: data.len(),
        };
        let demux = libwebp_sys::WebPDemuxInternal(
            &webp_data,
            0,
            core::ptr::null_mut(),
            libwebp_sys::WEBP_DEMUX_ABI_VERSION as i32,
        );
        assert!(!demux.is_null());
        let mut durations = Vec::new();
        let mut iter = core::mem::MaybeUninit::<libwebp_sys::WebPIterator>::zeroed();
        if libwebp_sys::WebPDemuxGetFrame(demux, 1, iter.as_mut_ptr()) != 0 {
            loop {
                durations.push((*iter.as_ptr()).duration as u32);
                if libwebp_sys::WebPDemuxNextFrame(iter.as_mut_ptr()) == 0 {
                    break;
                }
            }
            libwebp_sys::WebPDemuxReleaseIterator(iter.as_mut_ptr());
        }
        libwebp_sys::WebPDemuxDelete(demux);
        durations
    }
}

/// Every frame's `duration_ms` from `decode_all` equals the duration stored in
/// its ANMF chunk, including the last frame and a 0 ms frame (issue #18).
#[test]
fn decode_all_durations_match_demux() {
    let cases: &[&[u32]] = &[&[40, 0, 130], &[250], &[40, 60], &[0], &[16_777_215, 1]];
    let mut wrong = Vec::new();
    for &durations in cases {
        let data = mux_animation(8, 8, durations, 0, 0xffff_ffff);
        assert_eq!(demux_durations(&data), durations, "fixture");
        // `Limits::none()`: the 2^24 - 1 ms case exceeds the default 5 min cap.
        let frames =
            AnimationDecoder::with_options_limits(&data, ColorMode::Rgba, false, &Limits::none())
                .unwrap()
                .decode_all()
                .unwrap();
        let got: Vec<u32> = frames.iter().map(|f| f.duration_ms).collect();
        if got != durations {
            wrong.push((durations, got));
        }
    }
    assert!(
        wrong.is_empty(),
        "decode_all durations (want, got): {wrong:?}"
    );
}
