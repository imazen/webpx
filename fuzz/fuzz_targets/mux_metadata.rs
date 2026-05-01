//! Fuzz target: ICC / EXIF / XMP get + embed + remove.
//!
//! Hits the WebPDemuxInternal / WebPMuxCreateInternal paths by invoking
//! every public mux function. The embed paths take a separate payload from
//! the fuzzer so we exercise both small and large metadata blobs.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

const MAX_PAYLOAD: usize = 64 * 1024;

#[derive(Arbitrary, Debug)]
struct Input<'a> {
    payload: &'a [u8],
    webp: &'a [u8],
}

fuzz_target!(|input: Input<'_>| {
    // ---- Read paths: must not panic on garbage ----
    let _ = webpx::get_icc_profile(input.webp);
    let _ = webpx::get_exif(input.webp);
    let _ = webpx::get_xmp(input.webp);

    // ---- Round-trip through embed -> get -> remove ----
    if input.payload.len() > MAX_PAYLOAD {
        return;
    }

    if let Ok(with_icc) = webpx::embed_icc(input.webp, input.payload) {
        let _ = webpx::get_icc_profile(&with_icc);
        let _ = webpx::remove_icc(&with_icc);
    }
    if let Ok(with_exif) = webpx::embed_exif(input.webp, input.payload) {
        let _ = webpx::get_exif(&with_exif);
        let _ = webpx::remove_exif(&with_exif);
    }
    if let Ok(with_xmp) = webpx::embed_xmp(input.webp, input.payload) {
        let _ = webpx::get_xmp(&with_xmp);
        let _ = webpx::remove_xmp(&with_xmp);
    }

    // Removing on inputs that don't have the chunk should also be safe.
    let _ = webpx::remove_icc(input.webp);
    let _ = webpx::remove_exif(input.webp);
    let _ = webpx::remove_xmp(input.webp);
});
