//! Demonstrates the trivial-swap pattern between `webpx::zencodec` and
//! `zenwebp::zencodec`. The body of `main()` is identical for both
//! backends — only the `use` import below is cfg-gated.
//!
//! Build / run:
//!
//! ```sh
//! cargo run -p zencodec-swap-demo --no-default-features --features use-webpx
//! cargo run -p zencodec-swap-demo --no-default-features --features use-zenwebp
//! ```
//!
//! Both invocations produce identical-shape output through the same
//! `zencodec` trait surface.

#[cfg(feature = "use-webpx")]
use webpx::zencodec::{WebpDecoderConfig, WebpEncoderConfig};

#[cfg(feature = "use-zenwebp")]
use zenwebp::zencodec::{WebpDecoderConfig, WebpEncoderConfig};

#[cfg(not(any(feature = "use-webpx", feature = "use-zenwebp")))]
compile_error!(
    "zencodec-swap-demo: enable exactly one of the `use-webpx` or `use-zenwebp` features"
);

#[cfg(all(feature = "use-webpx", feature = "use-zenwebp"))]
compile_error!(
    "zencodec-swap-demo: enable exactly one of the `use-webpx` or `use-zenwebp` features"
);

#[cfg(any(feature = "use-webpx", feature = "use-zenwebp"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use zencodec::ImageFormat;
    use zencodec::decode::{Decode, DecodeJob, DecoderConfig};
    use zencodec::encode::{EncodeJob, Encoder, EncoderConfig};
    use zenpixels::{PixelDescriptor, PixelSlice};

    #[cfg(feature = "use-webpx")]
    let backend = "webpx (libwebp FFI)";
    #[cfg(feature = "use-zenwebp")]
    let backend = "zenwebp (pure Rust)";

    println!("backend: {backend}");
    println!(
        "encoder format: {:?}, lossy: {}, lossless: {}",
        <WebpEncoderConfig as EncoderConfig>::format(),
        <WebpEncoderConfig as EncoderConfig>::capabilities().lossy(),
        <WebpEncoderConfig as EncoderConfig>::capabilities().lossless(),
    );

    // Build a 32×32 RGBA gradient.
    let width = 32u32;
    let height = 32u32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            rgba.extend_from_slice(&[(x * 8) as u8, (y * 8) as u8, 64, 255]);
        }
    }

    // ── Encode ───────────────────────────────────────────────────────
    let cfg = WebpEncoderConfig::lossy().with_generic_quality(85.0);
    assert_eq!(cfg.generic_quality(), Some(85.0));
    let encoder = cfg.job().encoder()?;
    let pixels = PixelSlice::new(
        &rgba,
        width,
        height,
        (width * 4) as usize,
        PixelDescriptor::RGBA8_SRGB,
    )?;
    let webp = encoder.encode(pixels)?.into_vec();
    println!("encoded {} bytes of WebP", webp.len());

    // ── Probe ────────────────────────────────────────────────────────
    let dcfg = WebpDecoderConfig::new();
    let djob = dcfg.job();
    let info = djob.probe(&webp)?;
    assert_eq!(info.width, width);
    assert_eq!(info.height, height);
    assert_eq!(info.format, ImageFormat::WebP);
    println!(
        "probe: {}×{}, format={:?}",
        info.width, info.height, info.format
    );

    // ── Decode ───────────────────────────────────────────────────────
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
    assert_eq!(result.width(), width);
    assert_eq!(result.height(), height);

    println!();
    println!("Same source code produced identical-shape output via {backend}.");
    println!("Toggle `--features use-webpx` ↔ `use-zenwebp` to swap backends.");
    Ok(())
}

#[cfg(not(any(feature = "use-webpx", feature = "use-zenwebp")))]
fn main() {}
