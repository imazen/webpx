//! Fuzz target: Decoder builder with crop and scale.
//!
//! Exercises the advanced libwebp decoder path (cropping + rescaling) which
//! has historically been a source of integer-overflow and OOB bugs (CVE-2023-4863
//! lived in the rescaler / Huffman code on this path).

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

const MAX_PIXELS: u64 = 16 * 1024 * 1024;
const MAX_SCALE: u32 = 8192;

#[derive(Arbitrary, Debug)]
struct Input<'a> {
    use_crop: bool,
    crop_left: u16,
    crop_top: u16,
    crop_w: u16,
    crop_h: u16,
    use_scale: bool,
    scale_w: u16,
    scale_h: u16,
    data: &'a [u8],
}

fuzz_target!(|input: Input<'_>| {
    let info = match webpx::ImageInfo::from_webp(input.data) {
        Ok(i) => i,
        Err(_) => return,
    };
    if (info.width as u64) * (info.height as u64) > MAX_PIXELS {
        return;
    }

    // Cap scale dimensions so we don't try to allocate a 64k×64k output buffer
    // every iteration.
    let scale_w = (input.scale_w as u32).min(MAX_SCALE);
    let scale_h = (input.scale_h as u32).min(MAX_SCALE);
    if input.use_scale && (scale_w as u64) * (scale_h as u64) > MAX_PIXELS {
        return;
    }

    macro_rules! run {
        ($call:ident) => {
            if let Ok(mut dec) = webpx::Decoder::new(input.data) {
                if input.use_crop {
                    dec = dec.crop(
                        input.crop_left as u32,
                        input.crop_top as u32,
                        input.crop_w as u32,
                        input.crop_h as u32,
                    );
                }
                if input.use_scale {
                    dec = dec.scale(scale_w, scale_h);
                }
                let _ = dec.$call();
            }
        };
    }

    run!(decode_rgba);
    run!(decode_rgb);
    run!(decode_bgra_raw);
    run!(decode_bgr_raw);
});
