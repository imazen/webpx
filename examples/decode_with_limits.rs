//! Decode untrusted WebP input under a [`Limits`] policy.
//!
//! Run with: `cargo run --example decode_with_limits --features "decode animation icc"`
//!
//! Demonstrates the same `Limits` value carrying through the static decoder,
//! the animation decoder, and the metadata extraction path. A WebP that
//! declares an in-spec-but-huge canvas, an animation bomb (W × H × N
//! cumulative pixels), or an oversized ICCP / EXIF / XMP chunk is rejected
//! at parse time with `Error::LimitExceeded` rather than triggering an OOM.

use webpx::{
    AnimationDecoder, ColorMode, Decoder, DecoderConfig, Error, Limits, get_exif_with_limits,
    get_icc_profile_with_limits, get_xmp_with_limits,
};

/// Sensible defaults for a public-internet image-processing endpoint:
/// 64 MP per frame (≈256 MB at 4 bpp), 256 MP cumulative across animation
/// frames, 64 MB input bitstream, 1024 frames, 60 s animation, 4 MB
/// metadata chunks.
fn web_request_limits() -> Limits {
    Limits::none()
        .with_max_pixels(64 * 1024 * 1024)
        .with_max_total_pixels(256 * 1024 * 1024)
        .with_max_input_bytes(64 * 1024 * 1024)
        .with_max_frames(1024)
        .with_max_animation_ms(60_000)
        .with_max_metadata_bytes(4 * 1024 * 1024)
}

fn decode_static(webp: &[u8], limits: &Limits) -> Result<(u32, u32), webpx::At<Error>> {
    // The Decoder builder routes through the advanced API whenever
    // Limits::has_any() is true, so all configured caps are checked
    // before libwebp allocates the output buffer.
    let img = Decoder::new(webp)?
        .config(DecoderConfig::new().limits(*limits))
        .decode_rgba()?;
    Ok((img.width() as u32, img.height() as u32))
}

fn decode_animation(webp: &[u8], limits: &Limits) -> Result<usize, webpx::At<Error>> {
    let mut dec = AnimationDecoder::with_options_limits(webp, ColorMode::Rgba, true, limits)?;
    // decode_all enforces max_animation_ms against the cumulative timestamp
    // — it stops before allocating frames whose end-of-frame timestamp would
    // push past the budget.
    Ok(dec.decode_all()?.len())
}

/// `(icc_len, exif_len, xmp_len)`; `None` = chunk absent.
type MetadataLengths = (Option<usize>, Option<usize>, Option<usize>);

fn extract_metadata(webp: &[u8], limits: &Limits) -> Result<MetadataLengths, webpx::At<Error>> {
    // max_metadata_bytes layers on top of the internal 256 MiB hard cap.
    let icc = get_icc_profile_with_limits(webp, limits)?.map(|v| v.len());
    let exif = get_exif_with_limits(webp, limits)?.map(|v| v.len());
    let xmp = get_xmp_with_limits(webp, limits)?.map(|v| v.len());
    Ok((icc, exif, xmp))
}

fn main() {
    let limits = web_request_limits();

    // Synthesize a tiny in-spec image to feed the static and animation paths.
    // Real callers obviously bring their own bytes.
    let pixels = vec![0u8; 32 * 32 * 4];
    let webp = webpx::Encoder::new_rgba(&pixels, 32, 32)
        .quality(75.0)
        .encode(webpx::Unstoppable)
        .expect("encode demo image");

    match decode_static(&webp, &limits) {
        Ok((w, h)) => println!("static decode ok: {w}x{h}"),
        Err(e) => println!("static decode rejected: {e}"),
    }

    match decode_animation(&webp, &limits) {
        Ok(n) => println!("animation decode ok: {n} frames"),
        Err(e) => println!("animation decode rejected: {e}"),
    }

    match extract_metadata(&webp, &limits) {
        Ok((icc, exif, xmp)) => {
            println!("metadata: icc={icc:?} exif={exif:?} xmp={xmp:?} (None = chunk absent)",)
        }
        Err(e) => println!("metadata extraction rejected: {e}"),
    }

    // Demonstrate the rejection path: a cap below the image dimensions
    // returns `Error::LimitExceeded` rather than allocating.
    let strict = Limits::none().with_max_pixels(16);
    match decode_static(&webp, &strict) {
        Ok(_) => unreachable!("16-pixel budget should reject 32x32 image"),
        Err(e) => println!("expected rejection at strict budget: {e}"),
    }
}
