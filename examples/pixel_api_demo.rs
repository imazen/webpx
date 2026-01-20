//! Demonstration of the typed pixel API with imgref, rgb crate, and raw bytes.
//!
//! This example shows zero-copy encoding paths for different pixel sources.
//!
//! Run with: cargo run --example pixel_api_demo --features "encode animation"

use rgb::alt::BGRA8;
use rgb::{RGB8, RGBA8};
use webpx::{AnimationEncoder, Encoder, EncoderConfig, Unstoppable};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // =========================================================================
    // STILL FRAMES
    // =========================================================================

    // -------------------------------------------------------------------------
    // 1. From Vec<RGBA8> (rgb crate) - zero copy
    // -------------------------------------------------------------------------
    let pixels: Vec<RGBA8> = (0..100 * 100)
        .map(|i| {
            let x = (i % 100) as u8;
            let y = (i / 100) as u8;
            RGBA8::new(x, y, 128, 255)
        })
        .collect();

    // Option A: Using Encoder builder (recommended for complex settings)
    let webp = Encoder::from_pixels(&pixels, 100, 100)
        .quality(85.0)
        .encode(Unstoppable)?;
    println!("Encoder::from_pixels: {} bytes", webp.len());

    // Option B: Using EncoderConfig (reusable config)
    let config = EncoderConfig::new().quality(85.0);
    let webp = config.encode(&pixels, 100, 100, Unstoppable)?;
    println!("EncoderConfig::encode: {} bytes", webp.len());

    // -------------------------------------------------------------------------
    // 2. From imgref::ImgVec<RGBA8> - zero copy with stride handling
    // -------------------------------------------------------------------------
    // imgref is commonly used for image processing with non-contiguous buffers
    let img = imgref::ImgVec::new(pixels.clone(), 100, 100);

    // Direct from ImgRef - handles stride automatically
    let webp = Encoder::from_rgba(img.as_ref())
        .quality(85.0)
        .encode(Unstoppable)?;
    println!("Encoder::from_rgba(ImgRef): {} bytes", webp.len());

    // -------------------------------------------------------------------------
    // 3. From imgref with padding (stride > width) - zero copy
    // -------------------------------------------------------------------------
    // Simulate a buffer with 128-pixel stride but only 100 pixels of data per row
    let padded_buf: Vec<RGBA8> = vec![RGBA8::new(0, 0, 0, 255); 128 * 100];
    let img_with_stride = imgref::Img::new_stride(padded_buf.as_slice(), 100, 100, 128);

    let webp = Encoder::from_rgba(img_with_stride)
        .quality(85.0)
        .encode(Unstoppable)?;
    println!("Encoder::from_rgba (with stride): {} bytes", webp.len());

    // -------------------------------------------------------------------------
    // 4. From Vec<RGB8> (no alpha) - zero copy
    // -------------------------------------------------------------------------
    let rgb_pixels: Vec<RGB8> = (0..100 * 100)
        .map(|i| RGB8::new((i % 256) as u8, ((i / 100) % 256) as u8, 128))
        .collect();

    let webp = Encoder::from_pixels(&rgb_pixels, 100, 100)
        .quality(90.0)
        .encode(Unstoppable)?;
    println!("RGB8 pixels: {} bytes", webp.len());

    // -------------------------------------------------------------------------
    // 5. From Vec<BGRA8> (Windows/GPU native) - zero copy
    // -------------------------------------------------------------------------
    let bgra_pixels: Vec<BGRA8> = (0..100 * 100)
        .map(|_| BGRA8 { b: 255, g: 128, r: 64, a: 255 })
        .collect();

    let webp = Encoder::from_pixels(&bgra_pixels, 100, 100)
        .quality(85.0)
        .encode(Unstoppable)?;
    println!("BGRA8 pixels: {} bytes", webp.len());

    // Also works with imgref
    let bgra_img = imgref::ImgVec::new(bgra_pixels.clone(), 100, 100);
    let webp = Encoder::from_bgra(bgra_img.as_ref())
        .quality(85.0)
        .encode(Unstoppable)?;
    println!("BGRA8 via imgref: {} bytes", webp.len());

    // -------------------------------------------------------------------------
    // 6. From raw &[u8] bytes - when you have bytes from a file decoder
    // -------------------------------------------------------------------------
    let raw_rgba: Vec<u8> = vec![0u8; 100 * 100 * 4];

    // Option A: Encoder builder with explicit format
    let webp = Encoder::new_rgba(&raw_rgba, 100, 100)
        .quality(85.0)
        .encode(Unstoppable)?;
    println!("Raw bytes via Encoder::new_rgba: {} bytes", webp.len());

    // Option B: EncoderConfig with explicit format
    let webp = config.encode_rgba(&raw_rgba, 100, 100, Unstoppable)?;
    println!("Raw bytes via config.encode_rgba: {} bytes", webp.len());

    // Option C: Top-level function for quick encoding
    let webp = webpx::encode_rgba(&raw_rgba, 100, 100, 85.0, Unstoppable)?;
    println!("Raw bytes via encode_rgba: {} bytes", webp.len());

    // =========================================================================
    // ANIMATION
    // =========================================================================

    // -------------------------------------------------------------------------
    // 7. Animation with Vec<RGBA8> frames - zero copy
    // -------------------------------------------------------------------------
    let frame1: Vec<RGBA8> = vec![RGBA8::new(255, 0, 0, 255); 64 * 64];
    let frame2: Vec<RGBA8> = vec![RGBA8::new(0, 255, 0, 255); 64 * 64];
    let frame3: Vec<RGBA8> = vec![RGBA8::new(0, 0, 255, 255); 64 * 64];

    let mut encoder = AnimationEncoder::new(64, 64)?;
    encoder.set_quality(80.0);
    encoder.add_frame(&frame1, 0)?; // Uses typed pixel API
    encoder.add_frame(&frame2, 100)?;
    encoder.add_frame(&frame3, 200)?;
    let webp = encoder.finish(300)?;
    println!("Animation (typed): {} bytes, 3 frames", webp.len());

    // -------------------------------------------------------------------------
    // 8. Animation with BGRA frames - zero copy
    // -------------------------------------------------------------------------
    let bgra_frame1: Vec<BGRA8> = vec![BGRA8 { b: 255, g: 0, r: 0, a: 255 }; 64 * 64];
    let bgra_frame2: Vec<BGRA8> = vec![BGRA8 { b: 0, g: 255, r: 0, a: 255 }; 64 * 64];

    let mut encoder = AnimationEncoder::new(64, 64)?;
    encoder.add_frame(&bgra_frame1, 0)?; // BGRA8 also works
    encoder.add_frame(&bgra_frame2, 100)?;
    let webp = encoder.finish(200)?;
    println!("Animation (BGRA8): {} bytes", webp.len());

    // -------------------------------------------------------------------------
    // 9. Animation with raw bytes - when frames come as &[u8]
    // -------------------------------------------------------------------------
    let raw_frame1: Vec<u8> = vec![255u8; 64 * 64 * 4];
    let raw_frame2: Vec<u8> = vec![128u8; 64 * 64 * 4];

    let mut encoder = AnimationEncoder::new(64, 64)?;
    encoder.add_frame_rgba(&raw_frame1, 0)?; // Explicit format
    encoder.add_frame_rgba(&raw_frame2, 100)?;
    let webp = encoder.finish(200)?;
    println!("Animation (raw bytes): {} bytes", webp.len());

    // -------------------------------------------------------------------------
    // 10. Mixed formats in animation - each frame can be different format
    // -------------------------------------------------------------------------
    let rgba_frame: Vec<RGBA8> = vec![RGBA8::new(255, 0, 0, 255); 64 * 64];
    let bgra_frame: Vec<BGRA8> = vec![BGRA8 { b: 0, g: 255, r: 0, a: 255 }; 64 * 64];
    let rgb_frame: Vec<RGB8> = vec![RGB8::new(0, 0, 255); 64 * 64];
    let raw_frame: Vec<u8> = vec![128u8; 64 * 64 * 4];

    let mut encoder = AnimationEncoder::new(64, 64)?;
    encoder.add_frame(&rgba_frame, 0)?; // RGBA8
    encoder.add_frame(&bgra_frame, 100)?; // BGRA8
    encoder.add_frame(&rgb_frame, 200)?; // RGB8 (no alpha)
    encoder.add_frame_rgba(&raw_frame, 300)?; // raw bytes
    let webp = encoder.finish(400)?;
    println!("Animation (mixed formats): {} bytes, 4 frames", webp.len());

    // =========================================================================
    // CONVERTING FROM OTHER SOURCES
    // =========================================================================

    // -------------------------------------------------------------------------
    // 11. From image crate (hypothetical - shows the pattern)
    // -------------------------------------------------------------------------
    // If you have an image::RgbaImage, you can convert:
    //
    // ```rust
    // use image::RgbaImage;
    // let img: RgbaImage = load_image();
    //
    // // image crate stores as contiguous Vec<u8>, so use raw bytes API:
    // let webp = Encoder::new_rgba(img.as_raw(), img.width(), img.height())
    //     .quality(85.0)
    //     .encode(Unstoppable)?;
    //
    // // Or convert to Vec<RGBA8> if you want type safety:
    // let pixels: Vec<RGBA8> = img.pixels()
    //     .map(|p| RGBA8::new(p[0], p[1], p[2], p[3]))
    //     .collect();
    // let webp = Encoder::from_pixels(&pixels, img.width(), img.height())...
    // ```

    // -------------------------------------------------------------------------
    // 12. From a GPU texture (hypothetical - shows BGRA path)
    // -------------------------------------------------------------------------
    // Many GPU APIs (DirectX, Metal, Vulkan) use BGRA format.
    // If you have a mapped texture buffer:
    //
    // ```rust
    // let texture_data: &[u8] = mapped_buffer.as_slice();
    // let webp = Encoder::new_bgra(texture_data, width, height)
    //     .quality(85.0)
    //     .encode(Unstoppable)?;
    //
    // // Or if you have it as BGRA8 pixels:
    // let pixels: &[BGRA8] = bytemuck::cast_slice(texture_data);
    // let webp = Encoder::from_pixels(pixels, width, height)...
    // ```

    println!("\nAll examples completed successfully!");

    Ok(())
}
