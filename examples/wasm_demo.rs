//! WASM demo for webpx.
//!
//! Build with:
//! ```sh
//! source ~/emsdk/emsdk_env.sh
//! cargo build --example wasm_demo --target wasm32-unknown-emscripten --release
//! ```

use webpx::{decode_rgba, encode_rgba, ImageInfo, Unstoppable};

/// Encode RGBA data to WebP and return the size.
#[no_mangle]
pub extern "C" fn webp_encode_test() -> i32 {
    // Create a small test image (4x4 red square)
    let width = 4u32;
    let height = 4u32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..(width * height) {
        rgba.extend_from_slice(&[255, 0, 0, 255]); // Red
    }

    match encode_rgba(&rgba, width, height, 85.0, Unstoppable) {
        Ok(webp) => webp.len() as i32,
        Err(_) => -1,
    }
}

/// Decode WebP data and return dimensions.
///
/// # Safety
/// - `data` must point to valid memory of at least `len` bytes
/// - The memory must remain valid for the duration of this call
#[no_mangle]
pub unsafe extern "C" fn webp_decode_test(data: *const u8, len: usize) -> i32 {
    if data.is_null() || len == 0 {
        return -1;
    }

    let slice = unsafe { std::slice::from_raw_parts(data, len) };

    match ImageInfo::from_webp(slice) {
        Ok(info) => (info.width * info.height) as i32,
        Err(_) => -1,
    }
}

/// Simple roundtrip test.
#[no_mangle]
pub extern "C" fn webp_roundtrip_test() -> i32 {
    let width = 8u32;
    let height = 8u32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            rgba.push((x * 32) as u8);
            rgba.push((y * 32) as u8);
            rgba.push(128);
            rgba.push(255);
        }
    }

    // Encode
    let webp = match encode_rgba(&rgba, width, height, 95.0, Unstoppable) {
        Ok(w) => w,
        Err(_) => return -1,
    };

    // Decode
    let (decoded, dec_w, dec_h) = match decode_rgba(&webp) {
        Ok(d) => d,
        Err(_) => return -2,
    };

    if dec_w != width || dec_h != height {
        return -3;
    }

    // Check roughly similar (lossy encoding)
    let mut max_diff = 0i32;
    for (a, b) in rgba.iter().zip(decoded.iter()) {
        let diff = (*a as i32 - *b as i32).abs();
        max_diff = max_diff.max(diff);
    }

    if max_diff > 30 {
        return -4; // Too much difference
    }

    1 // Success
}

fn main() {
    println!("WebP WASM demo");
    println!("Encode test: {} bytes", webp_encode_test());
    println!("Roundtrip test: {}", webp_roundtrip_test());
}
