//! EXIF orientation reporting on the native API.

#![cfg(all(feature = "encode", feature = "decode", feature = "icc"))]

use webpx::{ImageInfo, Orientation, Unstoppable};

/// Raw little-endian TIFF with a single IFD0 Orientation (SHORT) entry.
fn orientation_tiff(value: u16) -> Vec<u8> {
    let mut t = b"II".to_vec();
    t.extend_from_slice(&42u16.to_le_bytes());
    t.extend_from_slice(&8u32.to_le_bytes());
    t.extend_from_slice(&1u16.to_le_bytes());
    t.extend_from_slice(&0x0112u16.to_le_bytes());
    t.extend_from_slice(&3u16.to_le_bytes());
    t.extend_from_slice(&1u32.to_le_bytes());
    t.extend_from_slice(&value.to_le_bytes());
    t.extend_from_slice(&[0, 0]);
    t.extend_from_slice(&0u32.to_le_bytes());
    t
}

fn still() -> Vec<u8> {
    let px: Vec<u8> = (0..6 * 4 * 4).map(|i| (i * 7) as u8).collect();
    webpx::Encoder::new_rgba(&px, 6, 4)
        .lossless(true)
        .encode(Unstoppable)
        .unwrap()
}

#[test]
fn image_info_reports_exif_orientation() {
    let plain = still();
    assert_eq!(ImageInfo::from_webp(&plain).unwrap().orientation, None);
    for v in 1..=8u16 {
        let webp = webpx::embed_exif(&plain, &orientation_tiff(v)).unwrap();
        let info = ImageInfo::from_webp(&webp).unwrap();
        assert_eq!(info.orientation, Orientation::from_exif(v as u8), "tag {v}");
        // Stored dimensions are unchanged; the tag says how to display them.
        assert_eq!((info.width, info.height), (6, 4));
    }
    let bad = webpx::embed_exif(&plain, &orientation_tiff(9)).unwrap();
    assert_eq!(ImageInfo::from_webp(&bad).unwrap().orientation, None);
}

#[cfg(feature = "animation")]
#[test]
fn animation_info_reports_exif_orientation() {
    let mut enc = webpx::AnimationEncoder::new(6, 4).unwrap();
    enc.set_lossless(true);
    let a: Vec<u8> = vec![200; 6 * 4 * 4];
    let b: Vec<u8> = vec![20; 6 * 4 * 4];
    enc.add_frame_rgba(&a, 0).unwrap();
    enc.add_frame_rgba(&b, 100).unwrap();
    let anim = enc.finish(200).unwrap();
    let anim = webpx::embed_exif(&anim, &orientation_tiff(6)).unwrap();
    let dec = webpx::AnimationDecoder::new(&anim).unwrap();
    assert_eq!(dec.info().orientation, Some(Orientation::Rotate90));
    assert_eq!(
        ImageInfo::from_webp(&anim).unwrap().orientation,
        Some(Orientation::Rotate90)
    );
}
