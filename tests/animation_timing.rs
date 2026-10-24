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

/// Frame-by-frame decoding reports the same durations as `decode_all`, and
/// `reset` starts the timeline over.
#[test]
fn next_frame_durations_match_demux() {
    let durations = [40, 0, 130, 250];
    let data = mux_animation(8, 8, &durations, 0, 0xffff_ffff);
    let mut dec = AnimationDecoder::new(&data).unwrap();
    for pass in 0..2 {
        let mut got = Vec::new();
        while let Some(frame) = dec.next_frame().unwrap() {
            got.push(frame.duration_ms);
        }
        assert_eq!(got, durations, "pass {pass}");
        dec.reset();
    }
    // A partial pass followed by reset must not carry the timeline over.
    dec.next_frame().unwrap();
    dec.reset();
    assert_eq!(dec.next_frame().unwrap().unwrap().duration_ms, 40);
}

/// `max_animation_ms` (default 5 min) applies to frame-by-frame decoding, not
/// only to `decode_all`.
#[test]
fn next_frame_enforces_max_animation_ms() {
    let data = mux_animation(8, 8, &[200_000, 200_000], 0, 0xffff_ffff);
    let mut dec = AnimationDecoder::new(&data).unwrap();
    assert_eq!(dec.next_frame().unwrap().unwrap().timestamp_ms, 200_000);
    let err = dec
        .next_frame()
        .expect_err("400 s exceeds the 300 s default cap");
    assert!(
        matches!(err.error(), webpx::Error::LimitExceeded(_)),
        "{err:?}"
    );
}

/// `AnimationInfo` reports the ANIM chunk's loop count and background color
/// verbatim. The chunk stores the color as bytes [B, G, R, A]; libwebp reads
/// it little-endian, so `bgcolor` is 0xAARRGGBB.
#[test]
fn anim_chunk_fields_round_trip() {
    for (loop_count, bgcolor) in [(0u16, 0xffff_ffffu32), (1, 0x8040_2010), (65535, 0)] {
        let data = mux_animation(8, 8, &[10, 20], loop_count, bgcolor);
        let info = AnimationDecoder::new(&data).unwrap().info().clone();
        assert_eq!(info.loop_count, u32::from(loop_count));
        assert_eq!(info.bgcolor, bgcolor);
        assert_eq!(info.frame_count, 2);
    }
    // Byte order: [B, G, R, A] = [0x10, 0x20, 0x40, 0x80] is ARGB 0x80402010.
    let data = mux_animation(8, 8, &[10, 20], 0, 0x8040_2010);
    let anim = data.windows(4).position(|w| w == b"ANIM").unwrap();
    assert_eq!(&data[anim + 8..anim + 12], &[0x10, 0x20, 0x40, 0x80]);
}

/// Embedding ICC/EXIF/XMP re-muxes the file; frame durations, loop count and
/// background color must survive, and the getters return the bytes verbatim.
#[cfg(feature = "icc")]
#[test]
fn metadata_embedding_preserves_animation() {
    let durations = [40, 0, 130];
    let mut data = mux_animation(8, 8, &durations, 7, 0x8040_2010);
    let icc = b"fake icc profile, odd length".to_vec();
    let exif = b"Exif\0\0MM\0*".to_vec();
    let xmp = b"<x:xmpmeta/>".to_vec();
    data = webpx::embed_icc(&data, &icc).unwrap();
    data = webpx::embed_exif(&data, &exif).unwrap();
    data = webpx::embed_xmp(&data, &xmp).unwrap();
    assert_eq!(demux_durations(&data), durations);
    let mut dec = AnimationDecoder::new(&data).unwrap();
    assert_eq!(dec.info().loop_count, 7);
    assert_eq!(dec.info().bgcolor, 0x8040_2010);
    let got: Vec<u32> = dec
        .decode_all()
        .unwrap()
        .iter()
        .map(|f| f.duration_ms)
        .collect();
    assert_eq!(got, durations);
    assert_eq!(webpx::get_icc_profile(&data).unwrap(), Some(icc));
    assert_eq!(webpx::get_exif(&data).unwrap(), Some(exif));
    assert_eq!(webpx::get_xmp(&data).unwrap(), Some(xmp));
}

/// `AnimationEncoder` stores the per-frame durations its timestamps imply,
/// including a 0 ms frame, as libwebp's demuxer reads them.
#[test]
fn animation_encoder_durations_reach_demux() {
    let durations = [40u32, 0, 130];
    let mut enc = webpx::AnimationEncoder::with_options(8, 8, false, 2).unwrap();
    enc.set_lossless(true);
    let mut t = 0i32;
    for (i, &d) in durations.iter().enumerate() {
        enc.add_frame_rgba(&frame_pixels(8, 8, i as u8 * 40 + 1), t)
            .unwrap();
        t += d as i32;
    }
    let data = enc.finish(t).unwrap();
    assert_eq!(demux_durations(&data), durations);
    let dec = AnimationDecoder::new(&data).unwrap();
    assert_eq!(dec.info().loop_count, 2);
}

/// libwebp's `WebPAnimEncoderAssemble` writes a still image when one frame
/// remains (a single frame, or identical frames merged); the duration and loop
/// count are not stored. Pinned so a libwebp change shows up here.
#[test]
fn single_frame_animation_encodes_as_still() {
    let px = frame_pixels(8, 8, 1);
    for frames in [1, 2] {
        let mut enc = webpx::AnimationEncoder::with_options(8, 8, false, 3).unwrap();
        enc.set_lossless(true);
        for i in 0..frames {
            enc.add_frame_rgba(&px, i * 100).unwrap();
        }
        let data = enc.finish(250).unwrap();
        assert_eq!(&data[12..16], b"VP8L", "{frames} frame(s)");
        assert!(!webpx::ImageInfo::from_webp(&data).unwrap().has_animation);
        let mut dec = AnimationDecoder::new(&data).unwrap();
        assert_eq!(dec.info().loop_count, 1);
        let got: Vec<u32> = dec
            .decode_all()
            .unwrap()
            .iter()
            .map(|f| f.duration_ms)
            .collect();
        assert_eq!(got, [0]);
    }
}
