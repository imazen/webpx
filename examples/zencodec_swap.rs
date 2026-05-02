//! Demonstrates the trivial-swap pattern between `webpx` and `zenwebp`.
//!
//! Both crates expose a `zencodec` module with the same wrapper-type
//! names (`WebpEncoderConfig`, `WebpDecoderConfig`, `WebpEncoder`,
//! `WebpDecoder`, …) implementing the same `zencodec` traits. The only
//! line that changes when swapping crates is the import path.
//!
//! Run with:
//!
//! ```sh
//! cargo run --features zencodec --example zencodec_swap
//! ```

#[cfg(feature = "zencodec")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ── The swap line ────────────────────────────────────────────────
    //
    // To switch backends, change ONLY this `use` statement. Every
    // other line below is identical between the two crates.
    use webpx::zencodec::{WebpDecoderConfig, WebpEncoderConfig};
    // use zenwebp::zencodec::{WebpDecoderConfig, WebpEncoderConfig};

    // Trait imports come from the shared `zencodec` crate — same
    // import either way.
    use zencodec::ImageFormat;
    use zencodec::decode::{Decode, DecodeJob, DecoderConfig};
    use zencodec::encode::{EncodeJob, Encoder, EncoderConfig};
    use zenpixels::{PixelDescriptor, PixelSlice};

    // Build a small RGBA test image (16×16 gradient).
    let width = 16u32;
    let height = 16u32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            rgba.extend_from_slice(&[(x * 16) as u8, (y * 16) as u8, 128, 255]);
        }
    }

    // ── Encode through the zencodec trait surface ────────────────────
    let cfg = WebpEncoderConfig::lossy().with_generic_quality(85.0);
    println!(
        "encoder format: {:?}, generic_quality: {:?}, capabilities.lossy: {}",
        <WebpEncoderConfig as EncoderConfig>::format(),
        cfg.generic_quality(),
        <WebpEncoderConfig as EncoderConfig>::capabilities().lossy(),
    );

    let job = cfg.job();
    let encoder = job.encoder()?;
    let pixels = PixelSlice::new(
        &rgba,
        width,
        height,
        (width * 4) as usize,
        PixelDescriptor::RGBA8_SRGB,
    )?;
    let webp = encoder.encode(pixels)?.into_vec();
    println!("encoded {} bytes of WebP", webp.len());
    assert_eq!(
        <WebpEncoderConfig as EncoderConfig>::format(),
        ImageFormat::WebP
    );

    // ── Decode through the zencodec trait surface ────────────────────
    let dcfg = WebpDecoderConfig::new();
    let djob = dcfg.job();

    let info = djob.probe(&webp)?;
    println!(
        "probe: {}×{}, format={:?}",
        info.width, info.height, info.format,
    );

    let decoder = djob.decoder(
        std::borrow::Cow::Borrowed(&webp),
        &[PixelDescriptor::RGBA8_SRGB],
    )?;
    let result = decoder.decode()?;
    println!(
        "decoded: {}×{}, has_alpha={}",
        result.width(),
        result.height(),
        result.has_alpha(),
    );

    println!("\nThis program runs identically against zenwebp — change");
    println!("only the `use` line at the top of `main()`.");
    Ok(())
}

#[cfg(not(feature = "zencodec"))]
fn main() {
    eprintln!("Build with `--features zencodec` to run this example.");
}
