//! Demonstration of the encoding API - "one right way" per input format.
//!
//! This example shows the recommended encoding path for each input format.
//!
//! Run with: cargo run --example pixel_api_demo --features "encode animation"
//!
//! ## Quick Reference
//!
//! | Input Format        | Recommended Method                           |
//! |---------------------|----------------------------------------------|
//! | `&[u8]` RGBA        | `Encoder::new_rgba(data, w, h).encode()`     |
//! | `&[u8]` RGB         | `Encoder::new_rgb(data, w, h).encode()`      |
//! | `&[u8]` BGRA        | `Encoder::new_bgra(data, w, h).encode()`     |
//! | `&[u8]` BGR         | `Encoder::new_bgr(data, w, h).encode()`      |
//! | `&[RGBA8]` etc      | `Encoder::from_pixels(pixels, w, h).encode()`|
//! | `ImgRef<RGBA8>` etc | `Encoder::from_img(img).encode()`            |
//! | `YuvPlanesRef`      | `Encoder::new_yuv(planes).encode()`          |
//!
//! For reusable config across multiple images, use `EncoderConfig`:
//! - `config.encode_rgba(data, w, h, stop)` for raw bytes
//! - `config.encode(pixels, w, h, stop)` for typed pixels
//! - `config.encode_img(img, stop)` for imgref

use rgb::alt::BGRA8;
use rgb::{RGB8, RGBA8};
use webpx::{AnimationEncoder, Encoder, EncoderConfig, Unstoppable};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== STILL IMAGE ENCODING ===\n");

    // =========================================================================
    // RAW BYTES - use explicit format methods
    // =========================================================================

    println!("--- Raw &[u8] bytes ---");

    // RGBA bytes
    let rgba_bytes: Vec<u8> = [255, 0, 0, 255].repeat(100 * 100);
    let webp = Encoder::new_rgba(&rgba_bytes, 100, 100)
        .quality(85.0)
        .encode(Unstoppable)?;
    println!("  Encoder::new_rgba: {} bytes", webp.len());

    // RGB bytes (no alpha)
    let rgb_bytes: Vec<u8> = [255, 0, 0].repeat(100 * 100);
    let webp = Encoder::new_rgb(&rgb_bytes, 100, 100)
        .quality(85.0)
        .encode(Unstoppable)?;
    println!("  Encoder::new_rgb: {} bytes", webp.len());

    // BGRA bytes (Windows/GPU native)
    let bgra_bytes: Vec<u8> = [0, 0, 255, 255].repeat(100 * 100); // Blue in BGRA
    let webp = Encoder::new_bgra(&bgra_bytes, 100, 100)
        .quality(85.0)
        .encode(Unstoppable)?;
    println!("  Encoder::new_bgra: {} bytes", webp.len());

    // BGR bytes (OpenCV)
    let bgr_bytes: Vec<u8> = [0, 0, 255].repeat(100 * 100);
    let webp = Encoder::new_bgr(&bgr_bytes, 100, 100)
        .quality(85.0)
        .encode(Unstoppable)?;
    println!("  Encoder::new_bgr: {} bytes", webp.len());

    // =========================================================================
    // TYPED PIXELS - format inferred from type
    // =========================================================================

    println!("\n--- Typed &[P] pixels ---");

    // Vec<RGBA8>
    let rgba_pixels: Vec<RGBA8> = vec![RGBA8::new(255, 0, 0, 255); 100 * 100];
    let webp = Encoder::from_pixels(&rgba_pixels, 100, 100)
        .quality(85.0)
        .encode(Unstoppable)?;
    println!("  Encoder::from_pixels (RGBA8): {} bytes", webp.len());

    // Vec<RGB8>
    let rgb_pixels: Vec<RGB8> = vec![RGB8::new(0, 255, 0); 100 * 100];
    let webp = Encoder::from_pixels(&rgb_pixels, 100, 100)
        .quality(85.0)
        .encode(Unstoppable)?;
    println!("  Encoder::from_pixels (RGB8): {} bytes", webp.len());

    // Vec<BGRA8>
    let bgra_pixels: Vec<BGRA8> = vec![BGRA8 { b: 0, g: 0, r: 255, a: 255 }; 100 * 100];
    let webp = Encoder::from_pixels(&bgra_pixels, 100, 100)
        .quality(85.0)
        .encode(Unstoppable)?;
    println!("  Encoder::from_pixels (BGRA8): {} bytes", webp.len());

    // =========================================================================
    // IMGREF - format inferred from type, stride handled automatically
    // =========================================================================

    println!("\n--- imgref::ImgRef<P> ---");

    // Contiguous imgref
    let pixels: Vec<RGBA8> = vec![RGBA8::new(0, 0, 255, 255); 100 * 100];
    let img = imgref::Img::new(pixels.as_slice(), 100, 100);
    let webp = Encoder::from_img(img)
        .quality(85.0)
        .encode(Unstoppable)?;
    println!("  Encoder::from_img (contiguous): {} bytes", webp.len());

    // imgref with stride (e.g., cropped region)
    let padded: Vec<RGBA8> = vec![RGBA8::new(128, 128, 128, 255); 128 * 100];
    let img_with_stride = imgref::Img::new_stride(padded.as_slice(), 100, 100, 128);
    let webp = Encoder::from_img(img_with_stride)
        .quality(85.0)
        .encode(Unstoppable)?;
    println!("  Encoder::from_img (with stride): {} bytes", webp.len());

    // =========================================================================
    // REUSABLE CONFIG - for encoding multiple images
    // =========================================================================

    println!("\n--- EncoderConfig (reusable) ---");

    let config = EncoderConfig::new().quality(90.0).method(6);

    // With raw bytes
    let webp1 = config.encode_rgba(&rgba_bytes, 100, 100, Unstoppable)?;
    println!("  config.encode_rgba: {} bytes", webp1.len());

    // With typed pixels
    let webp2 = config.encode(&rgba_pixels, 100, 100, Unstoppable)?;
    println!("  config.encode (typed): {} bytes", webp2.len());

    // With imgref
    let img = imgref::Img::new(rgba_pixels.as_slice(), 100, 100);
    let webp3 = config.encode_img(img, Unstoppable)?;
    println!("  config.encode_img: {} bytes", webp3.len());

    // =========================================================================
    // ANIMATION
    // =========================================================================

    println!("\n=== ANIMATION ENCODING ===\n");

    // Typed pixels - format inferred
    println!("--- AnimationEncoder with typed pixels ---");
    let frame1: Vec<RGBA8> = vec![RGBA8::new(255, 0, 0, 255); 64 * 64];
    let frame2: Vec<RGBA8> = vec![RGBA8::new(0, 255, 0, 255); 64 * 64];
    let frame3: Vec<RGBA8> = vec![RGBA8::new(0, 0, 255, 255); 64 * 64];

    let mut encoder = AnimationEncoder::new(64, 64)?;
    encoder.set_quality(80.0);
    encoder.add_frame(&frame1, 0)?;
    encoder.add_frame(&frame2, 100)?;
    encoder.add_frame(&frame3, 200)?;
    let webp = encoder.finish(300)?;
    println!("  add_frame (RGBA8): {} bytes, 3 frames", webp.len());

    // Raw bytes - explicit format
    println!("\n--- AnimationEncoder with raw bytes ---");
    let raw1: Vec<u8> = vec![255; 64 * 64 * 4];
    let raw2: Vec<u8> = vec![128; 64 * 64 * 4];

    let mut encoder = AnimationEncoder::new(64, 64)?;
    encoder.add_frame_rgba(&raw1, 0)?;
    encoder.add_frame_rgba(&raw2, 100)?;
    let webp = encoder.finish(200)?;
    println!("  add_frame_rgba: {} bytes, 2 frames", webp.len());

    println!("\n=== SUMMARY ===");
    println!("
The 'one right way' for each input format:

  &[u8] with known format  →  Encoder::new_rgba/rgb/bgra/bgr()
  &[P] typed pixels        →  Encoder::from_pixels()
  ImgRef<P>                →  Encoder::from_img()
  YuvPlanesRef             →  Encoder::new_yuv()

For reusable config:

  &[u8]       →  config.encode_rgba/rgb/bgra/bgr()
  &[P]        →  config.encode()
  ImgRef<P>   →  config.encode_img()

For animation:

  &[P]        →  encoder.add_frame()
  &[u8]       →  encoder.add_frame_rgba/rgb/bgra/bgr()
");

    Ok(())
}
