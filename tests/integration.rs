//! Integration tests for webpx crate.

use webpx::*;

// Helper functions to replace removed top-level encode functions
fn encode_rgba(
    data: &[u8],
    width: u32,
    height: u32,
    quality: f32,
    stop: impl Stop,
) -> Result<Vec<u8>> {
    Encoder::new_rgba(data, width, height)
        .quality(quality)
        .encode(stop)
}

fn encode_rgb(
    data: &[u8],
    width: u32,
    height: u32,
    quality: f32,
    stop: impl Stop,
) -> Result<Vec<u8>> {
    Encoder::new_rgb(data, width, height)
        .quality(quality)
        .encode(stop)
}

fn encode_lossless(data: &[u8], width: u32, height: u32, stop: impl Stop) -> Result<Vec<u8>> {
    EncoderConfig::new()
        .lossless(true)
        .encode_rgba(data, width, height, stop)
}

fn encode_bgra(
    data: &[u8],
    width: u32,
    height: u32,
    quality: f32,
    stop: impl Stop,
) -> Result<Vec<u8>> {
    Encoder::new_bgra(data, width, height)
        .quality(quality)
        .encode(stop)
}

fn encode_bgr(
    data: &[u8],
    width: u32,
    height: u32,
    quality: f32,
    stop: impl Stop,
) -> Result<Vec<u8>> {
    Encoder::new_bgr(data, width, height)
        .quality(quality)
        .encode(stop)
}

/// Generate a solid color RGBA image.
fn generate_rgba(width: u32, height: u32, r: u8, g: u8, b: u8, a: u8) -> Vec<u8> {
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..(width * height) {
        data.push(r);
        data.push(g);
        data.push(b);
        data.push(a);
    }
    data
}

/// Generate an RGB image without alpha.
fn generate_rgb(width: u32, height: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
    let mut data = Vec::with_capacity((width * height * 3) as usize);
    for _ in 0..(width * height) {
        data.push(r);
        data.push(g);
        data.push(b);
    }
    data
}

/// Generate a gradient RGBA image.
fn generate_gradient_rgba(width: u32, height: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let r = ((x * 255) / width.max(1)) as u8;
            let g = ((y * 255) / height.max(1)) as u8;
            let b = (((x + y) * 127) / (width + height).max(1)) as u8;
            data.push(r);
            data.push(g);
            data.push(b);
            data.push(255);
        }
    }
    data
}

mod roundtrip {
    use super::*;

    #[test]
    fn test_encode_decode_rgba_lossless() {
        let width = 64;
        let height = 64;
        let original = generate_rgba(width, height, 128, 64, 192, 255);

        // Encode lossless
        let webp = encode_lossless(&original, width, height, Unstoppable).expect("encode failed");

        // Verify it's valid WebP
        let info = ImageInfo::from_webp(&webp).expect("invalid webp");
        assert_eq!(info.width, width);
        assert_eq!(info.height, height);

        // Decode
        let (decoded, dec_w, dec_h) = decode_rgba(&webp).expect("decode failed");
        assert_eq!(dec_w, width);
        assert_eq!(dec_h, height);

        // Lossless should be exact match
        assert_eq!(decoded.len(), original.len());
        assert_eq!(decoded, original, "lossless roundtrip should be exact");
    }

    #[test]
    fn test_encode_decode_rgba_lossy() {
        let width = 100;
        let height = 80;
        let original = generate_gradient_rgba(width, height);

        // Encode lossy at high quality
        let webp = encode_rgba(&original, width, height, 95.0, Unstoppable).expect("encode failed");

        // Verify dimensions
        let info = ImageInfo::from_webp(&webp).expect("invalid webp");
        assert_eq!(info.width, width);
        assert_eq!(info.height, height);

        // Decode
        let (decoded, dec_w, dec_h) = decode_rgba(&webp).expect("decode failed");
        assert_eq!(dec_w, width);
        assert_eq!(dec_h, height);
        assert_eq!(decoded.len(), original.len());

        // Lossy will have some difference, but should be close
        let mut max_diff: i32 = 0;
        for (orig, dec) in original.iter().zip(decoded.iter()) {
            let diff = (*orig as i32 - *dec as i32).abs();
            max_diff = max_diff.max(diff);
        }

        // At q=95, max difference should be small
        assert!(
            max_diff < 30,
            "max pixel difference {} too high for q=95",
            max_diff
        );
    }

    #[test]
    fn test_encode_decode_rgb() {
        let width = 50;
        let height = 50;
        let original = generate_rgb(width, height, 200, 100, 50);

        // Encode
        let webp = encode_rgb(&original, width, height, 90.0, Unstoppable).expect("encode failed");

        // Verify
        let info = ImageInfo::from_webp(&webp).expect("invalid webp");
        assert_eq!(info.width, width);
        assert_eq!(info.height, height);

        // Decode to RGB
        let (decoded, dec_w, dec_h) = decode_rgb(&webp).expect("decode failed");
        assert_eq!(dec_w, width);
        assert_eq!(dec_h, height);
        assert_eq!(decoded.len(), original.len());
    }

    #[test]
    fn test_encoder_builder() {
        let width = 32;
        let height = 32;
        let data = generate_rgba(width, height, 100, 150, 200, 255);

        let webp = Encoder::new(&data, width, height)
            .preset(Preset::Photo)
            .quality(80.0)
            .method(4)
            .encode(Unstoppable)
            .expect("encode failed");

        let info = ImageInfo::from_webp(&webp).expect("invalid webp");
        assert_eq!(info.width, width);
        assert_eq!(info.height, height);
    }

    #[test]
    fn test_decoder_builder() {
        let width = 48;
        let height = 48;
        let data = generate_rgba(width, height, 50, 100, 150, 255);

        let webp = encode_rgba(&data, width, height, 85.0, Unstoppable).expect("encode failed");

        // Decode with builder
        let decoder = Decoder::new(&webp).expect("decoder creation failed");
        let info = decoder.info();
        assert_eq!(info.width, width);
        assert_eq!(info.height, height);

        let img = decoder.decode_rgba().expect("decode failed");
        assert_eq!(img.width(), width as usize);
        assert_eq!(img.height(), height as usize);
    }

    #[test]
    fn test_decode_with_scaling() {
        let width = 100;
        let height = 100;
        let data = generate_rgba(width, height, 128, 128, 128, 255);

        let webp = encode_rgba(&data, width, height, 85.0, Unstoppable).expect("encode failed");

        // Decode at half size
        let decoder = Decoder::new(&webp).expect("decoder creation failed");
        let (decoded, dec_w, dec_h) = decoder
            .scale(50, 50)
            .decode_rgba_raw()
            .expect("decode failed");

        assert_eq!(dec_w, 50);
        assert_eq!(dec_h, 50);
        assert_eq!(decoded.len(), 50 * 50 * 4);
    }

    #[test]
    fn test_decode_with_cropping() {
        let width = 100;
        let height = 100;
        let data = generate_rgba(width, height, 128, 128, 128, 255);

        let webp = encode_rgba(&data, width, height, 85.0, Unstoppable).expect("encode failed");

        // Decode cropped region
        let decoder = Decoder::new(&webp).expect("decoder creation failed");
        let (decoded, dec_w, dec_h) = decoder
            .crop(10, 10, 50, 50)
            .decode_rgba_raw()
            .expect("decode failed");

        assert_eq!(dec_w, 50);
        assert_eq!(dec_h, 50);
        assert_eq!(decoded.len(), 50 * 50 * 4);
    }

    #[test]
    fn test_yuv_decode() {
        let width = 64;
        let height = 64;
        let data = generate_rgba(width, height, 100, 100, 100, 255);

        let webp = encode_rgba(&data, width, height, 85.0, Unstoppable).expect("encode failed");

        let yuv = decode_yuv(&webp).expect("decode failed");
        assert_eq!(yuv.width, width);
        assert_eq!(yuv.height, height);

        // YUV420 has full-res Y and half-res UV
        assert_eq!(yuv.y.len(), (yuv.y_stride * height as usize));
        let (uv_w, uv_h) = yuv.uv_dimensions();
        assert_eq!(uv_w, width.div_ceil(2));
        assert_eq!(uv_h, height.div_ceil(2));
    }
}

mod edge_cases {
    use super::*;

    #[test]
    fn test_1x1_image() {
        let data = vec![255u8, 0, 0, 255]; // Red pixel

        let webp = encode_lossless(&data, 1, 1, Unstoppable).expect("encode failed");

        let info = ImageInfo::from_webp(&webp).expect("invalid webp");
        assert_eq!(info.width, 1);
        assert_eq!(info.height, 1);

        let (decoded, w, h) = decode_rgba(&webp).expect("decode failed");
        assert_eq!(w, 1);
        assert_eq!(h, 1);
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_odd_dimensions() {
        for (width, height) in [(31, 17), (17, 31), (33, 33), (1, 100), (100, 1)] {
            let data = generate_rgba(width, height, 128, 64, 192, 255);

            let webp = encode_lossless(&data, width, height, Unstoppable)
                .unwrap_or_else(|_| panic!("encode failed for {}x{}", width, height));

            let info = ImageInfo::from_webp(&webp)
                .unwrap_or_else(|_| panic!("invalid webp for {}x{}", width, height));
            assert_eq!(info.width, width, "width mismatch for {}x{}", width, height);
            assert_eq!(
                info.height, height,
                "height mismatch for {}x{}",
                width, height
            );

            let (decoded, dec_w, dec_h) = decode_rgba(&webp)
                .unwrap_or_else(|_| panic!("decode failed for {}x{}", width, height));
            assert_eq!(
                dec_w, width,
                "decoded width mismatch for {}x{}",
                width, height
            );
            assert_eq!(
                dec_h, height,
                "decoded height mismatch for {}x{}",
                width, height
            );
            assert_eq!(
                decoded, data,
                "pixel data mismatch for {}x{}",
                width, height
            );
        }
    }

    #[test]
    fn test_max_practical_dimensions() {
        // Test a large but reasonable size (not the absolute max 16383x16383)
        let width = 1920;
        let height = 1080;
        let data = generate_rgba(width, height, 64, 128, 192, 255);

        let webp = encode_rgba(&data, width, height, 50.0, Unstoppable).expect("encode failed");

        let info = ImageInfo::from_webp(&webp).expect("invalid webp");
        assert_eq!(info.width, width);
        assert_eq!(info.height, height);

        let (_, dec_w, dec_h) = decode_rgba(&webp).expect("decode failed");
        assert_eq!(dec_w, width);
        assert_eq!(dec_h, height);
    }

    #[test]
    fn test_invalid_dimensions() {
        let data = vec![0u8; 100];

        // Zero dimensions should fail
        assert!(encode_rgba(&data, 0, 10, 85.0, Unstoppable).is_err());
        assert!(encode_rgba(&data, 10, 0, 85.0, Unstoppable).is_err());

        // Exceeding max dimension should fail
        assert!(encode_rgba(&data, 20000, 10, 85.0, Unstoppable).is_err());
        assert!(encode_rgba(&data, 10, 20000, 85.0, Unstoppable).is_err());
    }

    #[test]
    fn test_buffer_too_small() {
        let small_buffer = vec![0u8; 10];

        // Buffer too small for 100x100 RGBA
        assert!(encode_rgba(&small_buffer, 100, 100, 85.0, Unstoppable).is_err());
    }

    #[test]
    fn test_invalid_webp_data() {
        let invalid = b"not a valid webp file at all";
        assert!(ImageInfo::from_webp(invalid).is_err());
        assert!(decode_rgba(invalid).is_err());
    }
}

mod presets {
    use super::*;

    #[test]
    fn test_all_presets() {
        let width = 64;
        let height = 64;
        let data = generate_gradient_rgba(width, height);

        for preset in [
            Preset::Default,
            Preset::Picture,
            Preset::Photo,
            Preset::Drawing,
            Preset::Icon,
            Preset::Text,
        ] {
            let webp = Encoder::new(&data, width, height)
                .preset(preset)
                .quality(75.0)
                .encode(Unstoppable)
                .unwrap_or_else(|e| panic!("encode with {:?} preset failed: {}", preset, e));

            let info = ImageInfo::from_webp(&webp)
                .unwrap_or_else(|_| panic!("invalid webp for {:?}", preset));
            assert_eq!(info.width, width);
            assert_eq!(info.height, height);
        }
    }

    #[test]
    fn test_quality_values() {
        let width = 32;
        let height = 32;
        let data = generate_gradient_rgba(width, height);

        // Test various quality values
        for quality in [0.0, 25.0, 50.0, 75.0, 100.0] {
            let webp = Encoder::new(&data, width, height)
                .quality(quality)
                .encode(Unstoppable)
                .unwrap_or_else(|e| panic!("encode with q={} failed: {}", quality, e));

            let info = ImageInfo::from_webp(&webp).expect("invalid webp");
            assert_eq!(info.width, width);
            assert_eq!(info.height, height);
        }
    }
}

mod encoder_config_tests {
    use super::*;
    use webpx::{AlphaFilter, EncoderConfig, ImageHint};

    #[test]
    fn test_image_hints() {
        let width = 32;
        let height = 32;
        let data = generate_gradient_rgba(width, height);

        for hint in [
            ImageHint::Default,
            ImageHint::Picture,
            ImageHint::Photo,
            ImageHint::Graph,
        ] {
            let config = EncoderConfig::new().quality(75.0).hint(hint);

            let webp = config
                .encode_rgba(&data, width, height, Unstoppable)
                .unwrap_or_else(|e| panic!("encode with {:?} hint failed: {}", hint, e));

            let info = ImageInfo::from_webp(&webp).expect("invalid webp");
            assert_eq!(info.width, width);
            assert_eq!(info.height, height);
        }
    }

    #[test]
    fn test_alpha_filter_modes() {
        let width = 32;
        let height = 32;
        // Create image with variable alpha
        let mut data = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                data.push(128); // R
                data.push(128); // G
                data.push(128); // B
                data.push(((x + y) * 4) as u8); // Variable alpha
            }
        }

        for filter in [AlphaFilter::None, AlphaFilter::Fast, AlphaFilter::Best] {
            let config = EncoderConfig::new()
                .quality(75.0)
                .alpha_filter(filter)
                .alpha_quality(90);

            let webp = config
                .encode_rgba(&data, width, height, Unstoppable)
                .unwrap_or_else(|e| panic!("encode with {:?} alpha filter failed: {}", filter, e));

            let info = ImageInfo::from_webp(&webp).expect("invalid webp");
            assert_eq!(info.width, width);
            assert_eq!(info.height, height);
            assert!(info.has_alpha, "should have alpha");
        }
    }

    #[test]
    fn test_preprocessing_levels() {
        let width = 64;
        let height = 64;
        let data = generate_gradient_rgba(width, height);

        for preprocessing in [0, 1, 2, 4, 7] {
            let config = EncoderConfig::new()
                .quality(75.0)
                .preprocessing(preprocessing);

            let webp = config
                .encode_rgba(&data, width, height, Unstoppable)
                .unwrap_or_else(|e| {
                    panic!("encode with preprocessing={} failed: {}", preprocessing, e)
                });

            let info = ImageInfo::from_webp(&webp).expect("invalid webp");
            assert_eq!(info.width, width);
        }
    }

    #[test]
    fn test_partitions() {
        let width = 64;
        let height = 64;
        let data = generate_gradient_rgba(width, height);

        // Test partition values 0-3 (1, 2, 4, 8 partitions)
        for partitions in 0..=3 {
            let config = EncoderConfig::new()
                .quality(75.0)
                .partitions(partitions)
                .partition_limit(50);

            let webp = config
                .encode_rgba(&data, width, height, Unstoppable)
                .unwrap_or_else(|e| panic!("encode with partitions={} failed: {}", partitions, e));

            let info = ImageInfo::from_webp(&webp).expect("invalid webp");
            assert_eq!(info.width, width);
        }
    }

    #[test]
    fn test_delta_palette_lossless() {
        let width = 32;
        let height = 32;
        let data = generate_gradient_rgba(width, height);

        let config = EncoderConfig::new_lossless().delta_palette(true);

        let webp = config
            .encode_rgba(&data, width, height, Unstoppable)
            .expect("encode with delta_palette failed");

        let info = ImageInfo::from_webp(&webp).expect("invalid webp");
        assert_eq!(info.width, width);
        assert_eq!(info.height, height);

        // Verify lossless roundtrip
        let (decoded, _, _) = decode_rgba(&webp).expect("decode failed");
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_quality_range() {
        let width = 32;
        let height = 32;
        let data = generate_gradient_rgba(width, height);

        let config = EncoderConfig::new().quality(75.0).quality_range(30, 80);

        let webp = config
            .encode_rgba(&data, width, height, Unstoppable)
            .expect("encode with quality_range failed");

        let info = ImageInfo::from_webp(&webp).expect("invalid webp");
        assert_eq!(info.width, width);
        assert_eq!(info.height, height);
    }

    #[test]
    fn test_max_compression_config() {
        let width = 32;
        let height = 32;
        let data = generate_gradient_rgba(width, height);

        let config = EncoderConfig::max_compression();
        let webp = config
            .encode_rgba(&data, width, height, Unstoppable)
            .expect("max_compression encode failed");

        let info = ImageInfo::from_webp(&webp).expect("invalid webp");
        assert_eq!(info.width, width);
    }

    #[test]
    fn test_max_compression_lossless_config() {
        let width = 32;
        let height = 32;
        let data = generate_gradient_rgba(width, height);

        let config = EncoderConfig::max_compression_lossless();
        let webp = config
            .encode_rgba(&data, width, height, Unstoppable)
            .expect("max_compression_lossless encode failed");

        let info = ImageInfo::from_webp(&webp).expect("invalid webp");
        assert_eq!(info.width, width);

        // Verify lossless
        let (decoded, _, _) = decode_rgba(&webp).expect("decode failed");
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_lossless_levels() {
        let width = 32;
        let height = 32;
        let data = generate_gradient_rgba(width, height);

        // Test all lossless compression levels 0-9
        for level in 0..=9 {
            let config = EncoderConfig::new_lossless_level(level);
            let webp = config
                .encode_rgba(&data, width, height, Unstoppable)
                .unwrap_or_else(|e| panic!("encode with lossless level {} failed: {}", level, e));

            let info = ImageInfo::from_webp(&webp).expect("invalid webp");
            assert_eq!(info.width, width);

            // All lossless levels should produce exact roundtrip
            let (decoded, _, _) = decode_rgba(&webp).expect("decode failed");
            assert_eq!(decoded, data, "lossless level {} should be exact", level);
        }
    }

    #[test]
    fn test_encoder_config_accessors() {
        let config = EncoderConfig::new()
            .quality(85.0)
            .preset(Preset::Photo)
            .lossless(true)
            .method(5);

        assert_eq!(config.get_quality(), 85.0);
        assert_eq!(config.get_preset(), Preset::Photo);
        assert!(config.is_lossless());
        assert_eq!(config.get_method(), 5);
    }

    #[test]
    fn test_sharp_yuv() {
        let width = 32;
        let height = 32;
        let data = generate_gradient_rgba(width, height);

        let config = EncoderConfig::new().quality(75.0).sharp_yuv(true);

        let webp = config
            .encode_rgba(&data, width, height, Unstoppable)
            .expect("encode with sharp_yuv failed");

        let info = ImageInfo::from_webp(&webp).expect("invalid webp");
        assert_eq!(info.width, width);
    }

    #[test]
    fn test_filter_settings() {
        let width = 32;
        let height = 32;
        let data = generate_gradient_rgba(width, height);

        let config = EncoderConfig::new()
            .quality(75.0)
            .filter_strength(80)
            .filter_sharpness(3)
            .filter_type(1)
            .autofilter(true)
            .sns_strength(80);

        let webp = config
            .encode_rgba(&data, width, height, Unstoppable)
            .expect("encode with filter settings failed");

        let info = ImageInfo::from_webp(&webp).expect("invalid webp");
        assert_eq!(info.width, width);
    }

    #[test]
    fn test_target_size() {
        let width = 64;
        let height = 64;
        let data = generate_gradient_rgba(width, height);

        // Request a small target size
        let config = EncoderConfig::new().target_size(500);

        let webp = config
            .encode_rgba(&data, width, height, Unstoppable)
            .expect("encode with target_size failed");

        // File should be close to target (with some tolerance)
        assert!(
            webp.len() < 2000,
            "file size {} should be near target",
            webp.len()
        );
    }

    #[test]
    fn test_pass_and_segments() {
        let width = 32;
        let height = 32;
        let data = generate_gradient_rgba(width, height);

        let config = EncoderConfig::new().quality(75.0).pass(6).segments(4);

        let webp = config
            .encode_rgba(&data, width, height, Unstoppable)
            .expect("encode with pass/segments failed");

        let info = ImageInfo::from_webp(&webp).expect("invalid webp");
        assert_eq!(info.width, width);
    }

    #[test]
    fn test_low_memory_mode() {
        let width = 32;
        let height = 32;
        let data = generate_gradient_rgba(width, height);

        let config = EncoderConfig::new().quality(75.0).low_memory(true);

        let webp = config
            .encode_rgba(&data, width, height, Unstoppable)
            .expect("encode with low_memory failed");

        let info = ImageInfo::from_webp(&webp).expect("invalid webp");
        assert_eq!(info.width, width);
    }

    #[test]
    fn test_exact_mode() {
        let width = 32;
        let height = 32;
        // Create image with transparent areas
        let mut data = vec![0u8; (width * height * 4) as usize];
        for i in 0..(width * height) as usize {
            data[i * 4] = 100; // R
            data[i * 4 + 1] = 150; // G
            data[i * 4 + 2] = 200; // B
            data[i * 4 + 3] = if i % 2 == 0 { 0 } else { 255 }; // Alternating alpha
        }

        // With exact mode, RGB values under transparent pixels are preserved
        let config = EncoderConfig::new_lossless().exact(true);

        let webp = config
            .encode_rgba(&data, width, height, Unstoppable)
            .expect("encode with exact failed");

        let (decoded, _, _) = decode_rgba(&webp).expect("decode failed");
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_config_validate() {
        let config = EncoderConfig::new().quality(75.0);
        assert!(config.validate().is_ok());

        let config_lossless = EncoderConfig::new_lossless();
        assert!(config_lossless.validate().is_ok());
    }

    #[test]
    fn test_near_lossless() {
        let width = 32;
        let height = 32;
        let data = generate_gradient_rgba(width, height);

        // Near-lossless with some preprocessing
        let config = EncoderConfig::new_lossless().near_lossless(60);

        let webp = config
            .encode_rgba(&data, width, height, Unstoppable)
            .expect("encode with near_lossless failed");

        let info = ImageInfo::from_webp(&webp).expect("invalid webp");
        assert_eq!(info.width, width);
    }

    #[test]
    fn test_with_preset_constructor() {
        let width = 32;
        let height = 32;
        let data = generate_gradient_rgba(width, height);

        let config = EncoderConfig::with_preset(Preset::Photo, 85.0);

        let webp = config
            .encode_rgba(&data, width, height, Unstoppable)
            .expect("encode with_preset failed");

        let info = ImageInfo::from_webp(&webp).expect("invalid webp");
        assert_eq!(info.width, width);
    }
}

mod encode_stats_tests {
    use super::*;
    use webpx::EncoderConfig;

    #[test]
    fn test_encode_rgba_with_stats() {
        let width = 64;
        let height = 64;
        let data = generate_gradient_rgba(width, height);

        let config = EncoderConfig::new().quality(75.0);

        let (webp, stats) = config
            .encode_rgba_with_stats(&data, width, height)
            .expect("encode_with_stats failed");

        // Verify webp is valid
        let info = ImageInfo::from_webp(&webp).expect("invalid webp");
        assert_eq!(info.width, width);
        assert_eq!(info.height, height);

        // Verify stats contain reasonable values
        assert!(stats.coded_size > 0, "coded_size should be > 0");
        assert_eq!(stats.coded_size as usize, webp.len());

        // PSNR values should be positive for lossy encoding
        assert!(stats.psnr[4] > 0.0, "overall PSNR should be > 0");
    }

    #[test]
    fn test_encode_rgb_with_stats() {
        let width = 64;
        let height = 64;
        let data = generate_rgb(width, height, 100, 150, 200);

        let config = EncoderConfig::new().quality(75.0);

        let (webp, stats) = config
            .encode_rgb_with_stats(&data, width, height)
            .expect("encode_rgb_with_stats failed");

        // Verify webp is valid
        let info = ImageInfo::from_webp(&webp).expect("invalid webp");
        assert_eq!(info.width, width);
        assert_eq!(info.height, height);

        // Verify stats
        assert!(stats.coded_size > 0);
    }

    #[test]
    fn test_encode_lossless_with_stats() {
        let width = 32;
        let height = 32;
        let data = generate_gradient_rgba(width, height);

        let config = EncoderConfig::new_lossless();

        let (webp, stats) = config
            .encode_rgba_with_stats(&data, width, height)
            .expect("encode_lossless_with_stats failed");

        // Verify webp is valid
        let info = ImageInfo::from_webp(&webp).expect("invalid webp");
        assert_eq!(info.width, width);

        // For lossless, check lossless-specific stats
        assert!(stats.lossless_size > 0, "lossless_size should be > 0");
        assert!(stats.coded_size > 0);
    }

    #[test]
    fn test_stats_segment_info() {
        let width = 64;
        let height = 64;
        let data = generate_gradient_rgba(width, height);

        let config = EncoderConfig::new().quality(50.0).segments(4);

        let (_, stats) = config
            .encode_rgba_with_stats(&data, width, height)
            .expect("encode_with_stats failed");

        // At least some segments should have data
        let total_blocks: u32 = stats.block_count.iter().sum();
        assert!(total_blocks > 0, "should have some blocks");
    }
}

mod error_tests {
    use webpx::{DecodingError, EncodingError, Error, MuxError};

    #[test]
    fn test_error_display() {
        let errors = [
            (Error::InvalidInput("test".into()), "invalid input: test"),
            (
                Error::EncodeFailed(EncodingError::OutOfMemory),
                "encode failed: out of memory",
            ),
            (
                Error::DecodeFailed(DecodingError::BitstreamError),
                "decode failed: bitstream error",
            ),
            (Error::InvalidConfig("bad".into()), "invalid config: bad"),
            (Error::OutOfMemory, "out of memory"),
            (Error::IccError("icc fail".into()), "ICC error: icc fail"),
            (Error::MuxError(MuxError::BadData), "mux error: bad data"),
            (
                Error::AnimationError("anim fail".into()),
                "animation error: anim fail",
            ),
            (Error::NeedMoreData, "need more data"),
            (Error::InvalidWebP, "invalid WebP data"),
        ];

        for (error, expected) in errors {
            assert_eq!(format!("{}", error), expected);
        }
    }

    #[test]
    fn test_encoding_error_from_i32() {
        assert_eq!(EncodingError::from(0), EncodingError::Ok);
        assert_eq!(EncodingError::from(1), EncodingError::OutOfMemory);
        assert_eq!(EncodingError::from(2), EncodingError::BitstreamOutOfMemory);
        assert_eq!(EncodingError::from(3), EncodingError::NullParameter);
        assert_eq!(EncodingError::from(4), EncodingError::InvalidConfiguration);
        assert_eq!(EncodingError::from(5), EncodingError::BadDimension);
        assert_eq!(EncodingError::from(6), EncodingError::Partition0Overflow);
        assert_eq!(EncodingError::from(7), EncodingError::PartitionOverflow);
        assert_eq!(EncodingError::from(8), EncodingError::BadWrite);
        assert_eq!(EncodingError::from(9), EncodingError::FileTooBig);
        assert_eq!(EncodingError::from(10), EncodingError::UserAbort);
        assert_eq!(EncodingError::from(999), EncodingError::Last);
    }

    #[test]
    fn test_encoding_error_display() {
        let errors = [
            (EncodingError::Ok, "ok"),
            (EncodingError::OutOfMemory, "out of memory"),
            (
                EncodingError::BitstreamOutOfMemory,
                "bitstream out of memory",
            ),
            (EncodingError::NullParameter, "null parameter"),
            (EncodingError::InvalidConfiguration, "invalid configuration"),
            (EncodingError::BadDimension, "bad dimension"),
            (EncodingError::Partition0Overflow, "partition0 overflow"),
            (EncodingError::PartitionOverflow, "partition overflow"),
            (EncodingError::BadWrite, "bad write"),
            (EncodingError::FileTooBig, "file too big"),
            (EncodingError::UserAbort, "user abort"),
            (EncodingError::Last, "unknown error"),
        ];

        for (error, expected) in errors {
            assert_eq!(format!("{}", error), expected);
        }
    }

    #[test]
    fn test_decoding_error_from_i32() {
        assert_eq!(DecodingError::from(0), DecodingError::Ok);
        assert_eq!(DecodingError::from(1), DecodingError::OutOfMemory);
        assert_eq!(DecodingError::from(2), DecodingError::InvalidParam);
        assert_eq!(DecodingError::from(3), DecodingError::BitstreamError);
        assert_eq!(DecodingError::from(4), DecodingError::UnsupportedFeature);
        assert_eq!(DecodingError::from(5), DecodingError::Suspended);
        assert_eq!(DecodingError::from(6), DecodingError::UserAbort);
        assert_eq!(DecodingError::from(999), DecodingError::NotEnoughData);
    }

    #[test]
    fn test_decoding_error_display() {
        let errors = [
            (DecodingError::Ok, "ok"),
            (DecodingError::OutOfMemory, "out of memory"),
            (DecodingError::InvalidParam, "invalid param"),
            (DecodingError::BitstreamError, "bitstream error"),
            (DecodingError::UnsupportedFeature, "unsupported feature"),
            (DecodingError::Suspended, "suspended"),
            (DecodingError::UserAbort, "user abort"),
            (DecodingError::NotEnoughData, "not enough data"),
        ];

        for (error, expected) in errors {
            assert_eq!(format!("{}", error), expected);
        }
    }

    #[test]
    fn test_mux_error_from_i32() {
        assert_eq!(MuxError::from(1), MuxError::Ok);
        assert_eq!(MuxError::from(0), MuxError::NotFound);
        assert_eq!(MuxError::from(-1), MuxError::InvalidArgument);
        assert_eq!(MuxError::from(-2), MuxError::BadData);
        assert_eq!(MuxError::from(-3), MuxError::MemoryError);
        assert_eq!(MuxError::from(-999), MuxError::NotEnoughData);
    }

    #[test]
    fn test_mux_error_display() {
        let errors = [
            (MuxError::Ok, "ok"),
            (MuxError::NotFound, "not found"),
            (MuxError::InvalidArgument, "invalid argument"),
            (MuxError::BadData, "bad data"),
            (MuxError::MemoryError, "memory error"),
            (MuxError::NotEnoughData, "not enough data"),
        ];

        for (error, expected) in errors {
            assert_eq!(format!("{}", error), expected);
        }
    }

    #[test]
    fn test_error_clone_and_eq() {
        let e1 = Error::InvalidInput("test".into());
        let e2 = e1.clone();
        assert_eq!(e1, e2);

        let e3 = Error::OutOfMemory;
        let e4 = Error::OutOfMemory;
        assert_eq!(e3, e4);
    }

    #[test]
    fn test_stopped_error() {
        use webpx::StopReason;

        let stopped = Error::Stopped(StopReason::Cancelled);
        assert_eq!(format!("{}", stopped), "operation cancelled");

        let timed_out = Error::Stopped(StopReason::TimedOut);
        assert_eq!(format!("{}", timed_out), "operation timed out");
    }
}

mod stop_tests {
    use super::generate_gradient_rgba;
    use core::sync::atomic::{AtomicBool, Ordering};
    use webpx::{Encoder, Error, Stop, StopReason};

    /// A Stop implementation that cancels immediately.
    struct ImmediateCanceller;

    impl Stop for ImmediateCanceller {
        fn check(&self) -> Result<(), StopReason> {
            Err(StopReason::Cancelled)
        }
    }

    /// A Stop implementation that cancels after N checks.
    struct DelayedCanceller {
        cancel_after: usize,
        counter: AtomicBool,
        checks: std::sync::atomic::AtomicUsize,
    }

    impl DelayedCanceller {
        fn new(cancel_after: usize) -> Self {
            Self {
                cancel_after,
                counter: AtomicBool::new(false),
                checks: std::sync::atomic::AtomicUsize::new(0),
            }
        }
    }

    impl Stop for DelayedCanceller {
        fn check(&self) -> Result<(), StopReason> {
            let count = self.checks.fetch_add(1, Ordering::SeqCst);
            if count >= self.cancel_after {
                self.counter.store(true, Ordering::SeqCst);
                Err(StopReason::Cancelled)
            } else {
                Ok(())
            }
        }
    }

    #[test]
    fn test_encode_cancelled_immediately() {
        let data = generate_gradient_rgba(32, 32);
        let result = Encoder::new_rgba(&data, 32, 32)
            .quality(85.0)
            .encode(ImmediateCanceller);

        match result {
            Err(ref e) if matches!(e.error(), Error::Stopped(StopReason::Cancelled)) => {} // expected
            other => panic!("expected Stopped(Cancelled), got {:?}", other),
        }
    }

    #[test]
    fn test_encode_cancelled_during_progress() {
        // Use a larger image so progress callbacks are called
        let data = generate_gradient_rgba(256, 256);
        let stopper = DelayedCanceller::new(1); // Cancel after first progress callback
        let result = Encoder::new_rgba(&data, 256, 256)
            .quality(85.0)
            .encode(&stopper);

        match result {
            Err(ref e) if matches!(e.error(), Error::Stopped(StopReason::Cancelled)) => {} // expected
            other => panic!("expected Stopped(Cancelled), got {:?}", other),
        }
    }
}

#[cfg(feature = "icc")]
mod icc_tests {
    use super::*;

    /// A minimal but valid ICC profile header (sRGB).
    /// This is a simplified profile for testing purposes.
    fn create_minimal_icc_profile() -> Vec<u8> {
        // This is a minimal valid ICC profile structure
        // Real ICC profiles are more complex, but this tests the embedding mechanism
        let mut profile = vec![0u8; 128];

        // Profile size (big endian)
        let size: u32 = 128;
        profile[0..4].copy_from_slice(&size.to_be_bytes());

        // CMM type
        profile[4..8].copy_from_slice(b"    ");

        // Profile version (4.3)
        profile[8] = 4;
        profile[9] = 0x30;

        // Profile/Device class: 'mntr' (monitor)
        profile[12..16].copy_from_slice(b"mntr");

        // Color space: 'RGB '
        profile[16..20].copy_from_slice(b"RGB ");

        // Profile connection space: 'XYZ '
        profile[20..24].copy_from_slice(b"XYZ ");

        // Creation date/time
        profile[24..36].fill(0);

        // Signature: 'acsp'
        profile[36..40].copy_from_slice(b"acsp");

        // Primary platform: 'APPL'
        profile[40..44].copy_from_slice(b"APPL");

        profile
    }

    #[test]
    fn test_icc_embed_extract() {
        let width = 32;
        let height = 32;
        let data = generate_rgba(width, height, 100, 150, 200, 255);

        // Create test ICC profile
        let icc_original = create_minimal_icc_profile();

        // Encode with ICC
        let webp = Encoder::new(&data, width, height)
            .quality(85.0)
            .icc_profile(&icc_original)
            .encode(Unstoppable)
            .expect("encode with ICC failed");

        // Extract ICC
        let icc_extracted = get_icc_profile(&webp)
            .expect("get ICC failed")
            .expect("no ICC profile found");

        assert_eq!(icc_extracted, icc_original, "ICC profile should match");
    }

    #[test]
    fn test_icc_embed_into_existing() {
        let width = 32;
        let height = 32;
        let data = generate_rgba(width, height, 100, 150, 200, 255);

        // Encode without ICC
        let webp_no_icc =
            encode_rgba(&data, width, height, 85.0, Unstoppable).expect("encode failed");

        // Verify no ICC
        let existing = get_icc_profile(&webp_no_icc).expect("get ICC failed");
        assert!(existing.is_none(), "should have no ICC profile");

        // Embed ICC
        let icc = create_minimal_icc_profile();
        let webp_with_icc = embed_icc(&webp_no_icc, &icc).expect("embed ICC failed");

        // Verify ICC present
        let extracted = get_icc_profile(&webp_with_icc)
            .expect("get ICC failed")
            .expect("should have ICC profile");
        assert_eq!(extracted, icc);
    }

    #[test]
    fn test_icc_remove() {
        let width = 32;
        let height = 32;
        let data = generate_rgba(width, height, 100, 150, 200, 255);

        // Create with ICC
        let icc = create_minimal_icc_profile();
        let webp_with_icc = Encoder::new(&data, width, height)
            .quality(85.0)
            .icc_profile(&icc)
            .encode(Unstoppable)
            .expect("encode with ICC failed");

        // Verify ICC present
        assert!(get_icc_profile(&webp_with_icc)
            .expect("get ICC failed")
            .is_some());

        // Remove ICC
        let webp_no_icc = remove_icc(&webp_with_icc).expect("remove ICC failed");

        // Verify ICC removed
        assert!(get_icc_profile(&webp_no_icc)
            .expect("get ICC failed")
            .is_none());

        // Image should still be decodable
        let (_, w, h) = decode_rgba(&webp_no_icc).expect("decode failed");
        assert_eq!(w, width);
        assert_eq!(h, height);
    }
}

#[cfg(feature = "icc")]
mod metadata_tests {
    use super::*;

    #[test]
    fn test_exif_embed_extract() {
        let width = 32;
        let height = 32;
        let data = generate_rgba(width, height, 100, 150, 200, 255);

        let webp = encode_rgba(&data, width, height, 85.0, Unstoppable).expect("encode failed");

        // Verify no EXIF initially
        assert!(get_exif(&webp).expect("get EXIF failed").is_none());

        // Embed EXIF (just some test bytes)
        let exif_data = b"Exif\0\0MM\0*test exif data".to_vec();
        let webp_with_exif = embed_exif(&webp, &exif_data).expect("embed EXIF failed");

        // Extract EXIF
        let extracted = get_exif(&webp_with_exif)
            .expect("get EXIF failed")
            .expect("should have EXIF");
        assert_eq!(extracted, exif_data);
    }

    #[test]
    fn test_xmp_embed_extract() {
        let width = 32;
        let height = 32;
        let data = generate_rgba(width, height, 100, 150, 200, 255);

        let webp = encode_rgba(&data, width, height, 85.0, Unstoppable).expect("encode failed");

        // Verify no XMP initially
        assert!(get_xmp(&webp).expect("get XMP failed").is_none());

        // Embed XMP
        let xmp_data = b"<?xpacket begin=''?><x:xmpmeta>test</x:xmpmeta>".to_vec();
        let webp_with_xmp = embed_xmp(&webp, &xmp_data).expect("embed XMP failed");

        // Extract XMP
        let extracted = get_xmp(&webp_with_xmp)
            .expect("get XMP failed")
            .expect("should have XMP");
        assert_eq!(extracted, xmp_data);
    }
}

#[cfg(feature = "streaming")]
mod streaming_tests {
    use super::*;

    #[test]
    fn test_streaming_decode() {
        let width = 64;
        let height = 64;
        let original = generate_rgba(width, height, 128, 64, 192, 255);

        let webp = encode_lossless(&original, width, height, Unstoppable).expect("encode failed");

        // Create streaming decoder
        let mut decoder = StreamingDecoder::new(ColorMode::Rgba).expect("decoder creation failed");

        // Feed data in chunks
        let chunk_size = webp.len() / 4;
        for chunk in webp.chunks(chunk_size) {
            match decoder.append(chunk) {
                Ok(DecodeStatus::Complete) => break,
                Ok(DecodeStatus::NeedMoreData) => continue,
                Ok(DecodeStatus::Partial(_rows)) => continue,
                Ok(_) => continue, // future variants
                Err(e) => panic!("decode error: {}", e),
            }
        }

        // Get final result
        let (decoded, dec_w, dec_h) = decoder.finish().expect("finish failed");
        assert_eq!(dec_w, width);
        assert_eq!(dec_h, height);
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_streaming_decode_single_chunk() {
        let width = 32;
        let height = 32;
        let original = generate_rgba(width, height, 100, 200, 50, 255);

        let webp = encode_lossless(&original, width, height, Unstoppable).expect("encode failed");

        let mut decoder = StreamingDecoder::new(ColorMode::Rgba).expect("decoder creation failed");

        // Feed all data at once
        let status = decoder.append(&webp).expect("append failed");
        assert_eq!(status, DecodeStatus::Complete);

        let (decoded, dec_w, dec_h) = decoder.finish().expect("finish failed");
        assert_eq!(dec_w, width);
        assert_eq!(dec_h, height);
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_streaming_encoder() {
        let width = 64;
        let height = 64;
        let data = generate_rgba(width, height, 150, 100, 50, 255);

        let mut encoder = StreamingEncoder::new(width, height).expect("encoder creation failed");
        encoder.set_quality(85.0);

        let mut output = Vec::new();
        encoder
            .encode_rgba_with_callback(&data, |chunk| {
                output.extend_from_slice(chunk);
                Ok(())
            })
            .expect("encode failed");

        assert!(!output.is_empty());

        // Verify output is valid WebP
        let info = ImageInfo::from_webp(&output).expect("invalid webp");
        assert_eq!(info.width, width);
        assert_eq!(info.height, height);

        // Decode and verify
        let (_, dec_w, dec_h) = decode_rgba(&output).expect("decode failed");
        assert_eq!(dec_w, width);
        assert_eq!(dec_h, height);
    }
}

mod types_tests {
    use webpx::{
        BitstreamFormat, ColorMode, Encoder, EncoderConfig, ImageInfo, Unstoppable, YuvPlanes,
        YuvPlanesRef,
    };

    #[test]
    fn test_color_mode_bytes_per_pixel() {
        assert_eq!(ColorMode::Rgba.bytes_per_pixel(), Some(4));
        assert_eq!(ColorMode::Bgra.bytes_per_pixel(), Some(4));
        assert_eq!(ColorMode::Argb.bytes_per_pixel(), Some(4));
        assert_eq!(ColorMode::Rgb.bytes_per_pixel(), Some(3));
        assert_eq!(ColorMode::Bgr.bytes_per_pixel(), Some(3));
        assert_eq!(ColorMode::Yuv420.bytes_per_pixel(), None);
        assert_eq!(ColorMode::Yuva420.bytes_per_pixel(), None);
    }

    #[test]
    fn test_color_mode_has_alpha() {
        assert!(ColorMode::Rgba.has_alpha());
        assert!(ColorMode::Bgra.has_alpha());
        assert!(ColorMode::Argb.has_alpha());
        assert!(ColorMode::Yuva420.has_alpha());
        assert!(!ColorMode::Rgb.has_alpha());
        assert!(!ColorMode::Bgr.has_alpha());
        assert!(!ColorMode::Yuv420.has_alpha());
    }

    #[test]
    fn test_color_mode_is_yuv() {
        assert!(ColorMode::Yuv420.is_yuv());
        assert!(ColorMode::Yuva420.is_yuv());
        assert!(!ColorMode::Rgba.is_yuv());
        assert!(!ColorMode::Rgb.is_yuv());
        assert!(!ColorMode::Bgra.is_yuv());
    }

    #[test]
    fn test_color_mode_default() {
        assert_eq!(ColorMode::default(), ColorMode::Rgba);
    }

    #[test]
    fn test_yuv_planes_new() {
        let planes = YuvPlanes::new(64, 48, false);
        assert_eq!(planes.width, 64);
        assert_eq!(planes.height, 48);
        assert_eq!(planes.y_stride, 64);
        assert_eq!(planes.y.len(), 64 * 48);
        assert_eq!(planes.u_stride, 32);
        assert_eq!(planes.u.len(), 32 * 24);
        assert_eq!(planes.v.len(), 32 * 24);
        assert!(planes.a.is_none());
    }

    #[test]
    fn test_yuv_planes_new_with_alpha() {
        let planes = YuvPlanes::new(64, 48, true);
        assert!(planes.a.is_some());
        assert_eq!(planes.a.as_ref().unwrap().len(), 64 * 48);
        assert_eq!(planes.a_stride, 64);
    }

    #[test]
    fn test_yuv_planes_uv_dimensions() {
        let planes = YuvPlanes::new(100, 100, false);
        let (uv_w, uv_h) = planes.uv_dimensions();
        assert_eq!(uv_w, 50);
        assert_eq!(uv_h, 50);

        // Odd dimensions
        let planes_odd = YuvPlanes::new(101, 101, false);
        let (uv_w, uv_h) = planes_odd.uv_dimensions();
        assert_eq!(uv_w, 51);
        assert_eq!(uv_h, 51);
    }

    #[test]
    fn test_yuv_planes_ref_from() {
        let planes = YuvPlanes::new(32, 32, true);
        let planes_ref: YuvPlanesRef = (&planes).into();

        assert_eq!(planes_ref.width, 32);
        assert_eq!(planes_ref.height, 32);
        assert_eq!(planes_ref.y.len(), planes.y.len());
        assert!(planes_ref.a.is_some());
    }

    #[test]
    fn test_bitstream_format_default() {
        assert_eq!(BitstreamFormat::default(), BitstreamFormat::Undefined);
    }

    #[test]
    fn test_image_info_lossy_format() {
        use super::generate_gradient_rgba;
        let data = generate_gradient_rgba(32, 32);
        let webp = Encoder::new_rgba(&data, 32, 32)
            .quality(85.0)
            .encode(Unstoppable)
            .expect("encode");
        let info = ImageInfo::from_webp(&webp).expect("info");
        assert_eq!(info.format, BitstreamFormat::Lossy);
        assert!(!info.has_animation);
        assert_eq!(info.frame_count, 1);
    }

    #[test]
    fn test_image_info_lossless_format() {
        use super::generate_rgba;
        let data = generate_rgba(32, 32, 100, 150, 200, 255);
        let webp = EncoderConfig::new()
            .lossless(true)
            .encode_rgba(&data, 32, 32, Unstoppable)
            .expect("encode");
        let info = ImageInfo::from_webp(&webp).expect("info");
        assert_eq!(info.format, BitstreamFormat::Lossless);
    }

    #[test]
    fn test_image_info_clone_eq() {
        use super::generate_rgba;
        let data = generate_rgba(32, 32, 100, 150, 200, 255);
        let webp = Encoder::new_rgba(&data, 32, 32)
            .quality(85.0)
            .encode(Unstoppable)
            .expect("encode");
        let info1 = ImageInfo::from_webp(&webp).expect("info");
        let info2 = info1.clone();
        assert_eq!(info1, info2);
    }
}

mod decoder_tests {
    use super::*;
    use webpx::{Decoder, DecoderConfig};

    #[test]
    fn test_decoder_decode_rgb() {
        let width = 32;
        let height = 32;
        let data = generate_rgb(width, height, 100, 150, 200);
        let webp = encode_rgb(&data, width, height, 85.0, Unstoppable).expect("encode");

        let decoder = Decoder::new(&webp).expect("decoder");
        let img = decoder.decode_rgb().expect("decode_rgb");
        assert_eq!(img.width(), width as usize);
        assert_eq!(img.height(), height as usize);
    }

    #[test]
    fn test_decoder_decode_yuv() {
        let width = 32;
        let height = 32;
        let data = generate_rgba(width, height, 100, 150, 200, 255);
        let webp = encode_rgba(&data, width, height, 85.0, Unstoppable).expect("encode");

        let decoder = Decoder::new(&webp).expect("decoder");
        let yuv = decoder.decode_yuv().expect("decode_yuv");
        assert_eq!(yuv.width, width);
        assert_eq!(yuv.height, height);
    }

    #[test]
    fn test_decoder_config() {
        let width = 64;
        let height = 64;
        let data = generate_rgba(width, height, 100, 150, 200, 255);
        let webp = encode_rgba(&data, width, height, 85.0, Unstoppable).expect("encode");

        let config = DecoderConfig::new()
            .bypass_filtering(true)
            .no_fancy_upsampling(true)
            .use_threads(true)
            .flip(false)
            .alpha_dithering(50);

        let decoder = Decoder::new(&webp).expect("decoder");
        let (decoded, w, h) = decoder.config(config).decode_rgba_raw().expect("decode");
        assert_eq!(w, width);
        assert_eq!(h, height);
        assert_eq!(decoded.len(), (width * height * 4) as usize);
    }

    #[test]
    fn test_decoder_crop_and_scale() {
        let width = 100;
        let height = 100;
        let data = generate_rgba(width, height, 100, 150, 200, 255);
        let webp = encode_rgba(&data, width, height, 85.0, Unstoppable).expect("encode");

        // Crop then scale
        let decoder = Decoder::new(&webp).expect("decoder");
        let (decoded, w, h) = decoder
            .crop(10, 10, 80, 80)
            .scale(40, 40)
            .decode_rgba_raw()
            .expect("decode");

        assert_eq!(w, 40);
        assert_eq!(h, 40);
        assert_eq!(decoded.len(), 40 * 40 * 4);
    }

    #[test]
    fn test_decoder_rgb_with_scaling() {
        let width = 100;
        let height = 100;
        let data = generate_rgb(width, height, 100, 150, 200);
        let webp = encode_rgb(&data, width, height, 85.0, Unstoppable).expect("encode");

        let decoder = Decoder::new(&webp).expect("decoder");
        let (decoded, w, h) = decoder.scale(50, 50).decode_rgb_raw().expect("decode");

        assert_eq!(w, 50);
        assert_eq!(h, 50);
        assert_eq!(decoded.len(), 50 * 50 * 3);
    }
}

#[cfg(feature = "streaming")]
mod streaming_advanced_tests {
    use super::*;
    use webpx::{ColorMode, DecodeStatus, StreamingDecoder, StreamingEncoder};

    #[test]
    fn test_streaming_decoder_with_buffer() {
        let width = 64;
        let height = 64;
        let original = generate_rgba(width, height, 100, 150, 200, 255);
        let webp = encode_lossless(&original, width, height, Unstoppable).expect("encode");

        // Pre-allocate buffer
        let mut buffer = vec![0u8; (width * height * 4) as usize];
        let stride = (width * 4) as usize;

        let mut decoder =
            StreamingDecoder::with_buffer(&mut buffer, stride, ColorMode::Rgba).expect("decoder");

        let status = decoder.append(&webp).expect("append");
        assert_eq!(status, DecodeStatus::Complete);
    }

    #[test]
    fn test_streaming_decoder_update() {
        let width = 32;
        let height = 32;
        let original = generate_rgba(width, height, 100, 150, 200, 255);
        let webp = encode_lossless(&original, width, height, Unstoppable).expect("encode");

        let mut decoder = StreamingDecoder::new(ColorMode::Rgba).expect("decoder");

        // Use update instead of append (expects complete data)
        let status = decoder.update(&webp).expect("update");
        assert_eq!(status, DecodeStatus::Complete);

        let (decoded, w, h) = decoder.finish().expect("finish");
        assert_eq!(w, width);
        assert_eq!(h, height);
        assert_eq!(decoded, original);
    }

    #[test]
    fn test_streaming_decoder_dimensions_and_rows() {
        let width = 64;
        let height = 64;
        let original = generate_rgba(width, height, 100, 150, 200, 255);
        let webp = encode_lossless(&original, width, height, Unstoppable).expect("encode");

        let mut decoder = StreamingDecoder::new(ColorMode::Rgba).expect("decoder");

        // Initially no dimensions
        assert!(decoder.dimensions().is_none());
        assert_eq!(decoder.decoded_rows(), 0);

        // Feed data in chunks
        let chunk_size = 100;
        for chunk in webp.chunks(chunk_size) {
            let status = decoder.append(chunk);
            if matches!(status, Ok(DecodeStatus::Complete)) {
                break;
            }
        }

        // After decode completes, dimensions MUST be available
        let (w, h) = decoder
            .dimensions()
            .expect("dimensions should be available after decode");
        assert_eq!(w, width);
        assert_eq!(h, height);
        assert_eq!(decoder.decoded_rows(), height);
    }

    #[test]
    fn test_streaming_decoder_get_partial() {
        let width = 64;
        let height = 64;
        let original = generate_rgba(width, height, 100, 150, 200, 255);
        let webp = encode_lossless(&original, width, height, Unstoppable).expect("encode");

        let mut decoder = StreamingDecoder::new(ColorMode::Rgba).expect("decoder");

        // Feed data in small chunks to get partial decoding
        let chunk_size = 50;
        for chunk in webp.chunks(chunk_size) {
            match decoder.append(chunk) {
                Ok(DecodeStatus::Complete) => break,
                Ok(DecodeStatus::Partial(_rows)) => {
                    // Try to get partial data
                    if let Some((data, w, h)) = decoder.get_partial() {
                        assert!(w > 0);
                        assert!(h > 0);
                        assert!(!data.is_empty());
                    }
                }
                Ok(DecodeStatus::NeedMoreData) => continue,
                Ok(_) => continue, // future variants
                Err(e) => panic!("decode error: {}", e),
            }
        }
    }

    #[test]
    fn test_streaming_decoder_color_modes() {
        let width = 32;
        let height = 32;
        let original = generate_rgba(width, height, 100, 150, 200, 255);
        let webp = encode_lossless(&original, width, height, Unstoppable).expect("encode");

        for mode in [
            ColorMode::Rgba,
            ColorMode::Bgra,
            ColorMode::Argb,
            ColorMode::Rgb,
            ColorMode::Bgr,
        ] {
            let mut decoder = StreamingDecoder::new(mode).expect("decoder");
            let status = decoder.append(&webp).expect("append");
            assert_eq!(status, DecodeStatus::Complete);

            let (decoded, w, h) = decoder.finish().expect("finish");
            assert_eq!(w, width);
            assert_eq!(h, height);
            let bpp = mode.bytes_per_pixel().unwrap();
            assert_eq!(decoded.len(), (width * height) as usize * bpp);
        }
    }

    #[test]
    fn test_streaming_decoder_yuv_buffer_error() {
        let mut buffer = vec![0u8; 1000];
        // YUV modes require separate plane buffers
        let result = StreamingDecoder::with_buffer(&mut buffer, 100, ColorMode::Yuv420);
        assert!(result.is_err());
    }

    #[test]
    fn test_streaming_encoder_rgb() {
        let width = 32;
        let height = 32;
        let data = generate_rgb(width, height, 100, 150, 200);

        let mut encoder = StreamingEncoder::new(width, height).expect("encoder");
        encoder.set_quality(75.0);
        encoder.set_preset(webpx::Preset::Photo);
        encoder.set_lossless(false);

        let mut output = Vec::new();
        encoder
            .encode_rgb_with_callback(&data, |chunk| {
                output.extend_from_slice(chunk);
                Ok(())
            })
            .expect("encode");

        assert!(!output.is_empty());

        let info = ImageInfo::from_webp(&output).expect("info");
        assert_eq!(info.width, width);
        assert_eq!(info.height, height);
    }

    #[test]
    fn test_streaming_encoder_buffer_too_small() {
        let encoder = StreamingEncoder::new(100, 100).expect("encoder");
        let small_buffer = vec![0u8; 10];

        let result = encoder.encode_rgba_with_callback(&small_buffer, |_| Ok(()));
        assert!(result.is_err());

        let encoder2 = StreamingEncoder::new(100, 100).expect("encoder");
        let result2 = encoder2.encode_rgb_with_callback(&small_buffer, |_| Ok(()));
        assert!(result2.is_err());
    }

    #[test]
    fn test_streaming_encoder_callback_error() {
        let width = 32;
        let height = 32;
        let data = generate_rgba(width, height, 100, 150, 200, 255);

        let encoder = StreamingEncoder::new(width, height).expect("encoder");

        // Callback returns error
        let result = encoder.encode_rgba_with_callback(&data, |_| {
            Err(webpx::at(webpx::Error::InvalidInput("test error".into())))
        });

        assert!(result.is_err());
    }

    #[test]
    fn test_streaming_decoder_finish_incomplete() {
        let decoder = StreamingDecoder::new(ColorMode::Rgba).expect("decoder");

        // Try to finish without any data - should error
        let result = decoder.finish();
        assert!(result.is_err());
    }
}

mod encoder_advanced_tests {
    use super::*;
    use webpx::{Encoder, Preset};

    #[test]
    fn test_encoder_new_rgb() {
        let width = 32;
        let height = 32;
        let data = generate_rgb(width, height, 100, 150, 200);

        let webp = Encoder::new_rgb(&data, width, height)
            .quality(85.0)
            .encode(Unstoppable)
            .expect("encode");

        let info = ImageInfo::from_webp(&webp).expect("info");
        assert_eq!(info.width, width);
        assert_eq!(info.height, height);
    }

    #[test]
    fn test_encoder_new_yuv() {
        use webpx::YuvPlanes;

        let width = 32u32;
        let height = 32u32;

        // Create YUV planes with valid data
        let mut planes = YuvPlanes::new(width, height, false);
        // Fill with some data (gray)
        planes.y.fill(128);
        planes.u.fill(128);
        planes.v.fill(128);

        let planes_ref = (&planes).into();
        let webp = Encoder::new_yuv(planes_ref)
            .quality(85.0)
            .encode(Unstoppable)
            .expect("encode");

        let info = ImageInfo::from_webp(&webp).expect("info");
        assert_eq!(info.width, width);
        assert_eq!(info.height, height);
    }

    #[test]
    fn test_encoder_new_argb_zero_copy() {
        // Test the zero-copy ARGB fast path
        let width = 32u32;
        let height = 32u32;

        // Create ARGB data as u32 values: 0xAARRGGBB
        // Red pixels: alpha=255, red=255, green=0, blue=0
        let red_pixel: u32 = 0xFF_FF_00_00;
        let argb_data: Vec<u32> = vec![red_pixel; (width * height) as usize];

        let webp = Encoder::new_argb(&argb_data, width, height)
            .quality(85.0)
            .encode(Unstoppable)
            .expect("encode");

        let info = ImageInfo::from_webp(&webp).expect("info");
        assert_eq!(info.width, width);
        assert_eq!(info.height, height);

        // Decode and verify the pixels are red
        let (decoded, dec_w, dec_h) = decode_rgba(&webp).expect("decode");
        assert_eq!(dec_w, width);
        assert_eq!(dec_h, height);

        // Check first pixel is roughly red (allowing for lossy compression)
        let r = decoded[0];
        let g = decoded[1];
        let b = decoded[2];
        assert!(r > 200, "expected red > 200, got {}", r);
        assert!(g < 50, "expected green < 50, got {}", g);
        assert!(b < 50, "expected blue < 50, got {}", b);
    }

    #[test]
    fn test_encoder_new_argb_with_stride() {
        // Test ARGB with non-contiguous stride
        let width = 16u32;
        let height = 16u32;
        let stride = 32u32; // Larger than width

        // Create ARGB data with stride padding
        // Green pixels: alpha=255, red=0, green=255, blue=0
        let green_pixel: u32 = 0xFF_00_FF_00;
        let argb_data: Vec<u32> = vec![green_pixel; (stride * height) as usize];

        let webp = Encoder::new_argb_stride(&argb_data, width, height, stride)
            .quality(85.0)
            .encode(Unstoppable)
            .expect("encode");

        let info = ImageInfo::from_webp(&webp).expect("info");
        assert_eq!(info.width, width);
        assert_eq!(info.height, height);
    }

    #[test]
    fn test_encoder_new_argb_lossless() {
        // Test lossless encoding with ARGB zero-copy path
        let width = 8u32;
        let height = 8u32;

        // Blue pixel: alpha=255, red=0, green=0, blue=255
        let blue_pixel: u32 = 0xFF_00_00_FF;
        let argb_data: Vec<u32> = vec![blue_pixel; (width * height) as usize];

        let webp = Encoder::new_argb(&argb_data, width, height)
            .lossless(true)
            .encode(Unstoppable)
            .expect("encode");

        // Decode and verify exact pixel values (lossless)
        let (decoded, _, _) = decode_rgba(&webp).expect("decode");
        // ARGB 0xFF_00_00_FF -> RGBA should be R=0, G=0, B=255, A=255
        assert_eq!(decoded[0], 0, "red should be 0");
        assert_eq!(decoded[1], 0, "green should be 0");
        assert_eq!(decoded[2], 255, "blue should be 255");
        assert_eq!(decoded[3], 255, "alpha should be 255");
    }

    #[test]
    fn test_encoder_from_rgba() {
        use imgref::ImgVec;
        use rgb::RGBA8;

        let width = 16usize;
        let height = 16usize;
        let pixels: Vec<RGBA8> = (0..width * height)
            .map(|i| RGBA8::new((i % 256) as u8, 100, 150, 255))
            .collect();

        let img = ImgVec::new(pixels, width, height);
        let webp = Encoder::from_rgba(img.as_ref())
            .quality(85.0)
            .encode(Unstoppable)
            .expect("encode");

        let info = ImageInfo::from_webp(&webp).expect("info");
        assert_eq!(info.width, width as u32);
        assert_eq!(info.height, height as u32);
    }

    #[test]
    fn test_encoder_from_rgb() {
        use imgref::ImgVec;
        use rgb::RGB8;

        let width = 16usize;
        let height = 16usize;
        let pixels: Vec<RGB8> = (0..width * height)
            .map(|i| RGB8::new((i % 256) as u8, 100, 150))
            .collect();

        let img = ImgVec::new(pixels, width, height);
        let webp = Encoder::from_rgb(img.as_ref())
            .quality(85.0)
            .encode(Unstoppable)
            .expect("encode");

        let info = ImageInfo::from_webp(&webp).expect("info");
        assert_eq!(info.width, width as u32);
        assert_eq!(info.height, height as u32);
    }

    #[test]
    fn test_encoder_all_options() {
        let width = 32;
        let height = 32;
        let data = generate_rgba(width, height, 100, 150, 200, 255);

        let webp = Encoder::new(&data, width, height)
            .preset(Preset::Photo)
            .quality(85.0)
            .method(4)
            .lossless(false)
            .near_lossless(100)
            .alpha_quality(90)
            .exact(false)
            .target_size(0)
            .sharp_yuv(true)
            .encode(Unstoppable)
            .expect("encode");

        let info = ImageInfo::from_webp(&webp).expect("info");
        assert_eq!(info.width, width);
    }

    #[test]
    fn test_encoder_with_config() {
        use webpx::EncoderConfig;

        let width = 32;
        let height = 32;
        let data = generate_rgba(width, height, 100, 150, 200, 255);

        let config = EncoderConfig::new().quality(90.0).method(5);

        let webp = Encoder::new(&data, width, height)
            .config(config)
            .encode(Unstoppable)
            .expect("encode");

        let info = ImageInfo::from_webp(&webp).expect("info");
        assert_eq!(info.width, width);
    }
}

#[cfg(feature = "animation")]
mod animation_tests {
    use super::*;

    #[test]
    fn test_animation_encode_decode() {
        let width = 32;
        let height = 32;

        // Create 3 frames with different colors
        let frame1 = generate_rgba(width, height, 255, 0, 0, 255); // Red
        let frame2 = generate_rgba(width, height, 0, 255, 0, 255); // Green
        let frame3 = generate_rgba(width, height, 0, 0, 255, 255); // Blue

        // Encode animation
        let mut encoder = AnimationEncoder::new(width, height).expect("encoder creation failed");
        encoder.set_quality(85.0);

        encoder
            .add_frame_rgba(&frame1, 0)
            .expect("add frame 1 failed");
        encoder
            .add_frame_rgba(&frame2, 100)
            .expect("add frame 2 failed");
        encoder
            .add_frame_rgba(&frame3, 200)
            .expect("add frame 3 failed");

        let webp = encoder.finish(300).expect("finish failed");

        // Verify it's a valid animated WebP
        let info = ImageInfo::from_webp(&webp).expect("invalid webp");
        assert_eq!(info.width, width);
        assert_eq!(info.height, height);
        assert!(info.has_animation, "should be animated");

        // Decode animation
        let mut decoder = AnimationDecoder::new(&webp).expect("decoder creation failed");
        let anim_info = decoder.info();
        assert_eq!(anim_info.width, width);
        assert_eq!(anim_info.height, height);
        assert_eq!(anim_info.frame_count, 3);

        // Decode all frames
        let frames = decoder.decode_all().expect("decode_all failed");
        assert_eq!(frames.len(), 3);

        // WebP timestamps represent frame END times, not START times
        // So frames added at 0, 100, 200 with finish(300) become 100, 200, 300
        assert_eq!(frames[0].timestamp_ms, 100);
        assert_eq!(frames[1].timestamp_ms, 200);
        assert_eq!(frames[2].timestamp_ms, 300);
    }

    #[test]
    fn test_animation_decode_frame_by_frame() {
        let width = 32;
        let height = 32;

        let frame1 = generate_rgba(width, height, 255, 0, 0, 255);
        let frame2 = generate_rgba(width, height, 0, 255, 0, 255);

        let mut encoder = AnimationEncoder::new(width, height).expect("encoder creation failed");
        encoder
            .add_frame_rgba(&frame1, 0)
            .expect("add frame 1 failed");
        encoder
            .add_frame_rgba(&frame2, 100)
            .expect("add frame 2 failed");
        let webp = encoder.finish(200).expect("finish failed");

        let mut decoder = AnimationDecoder::new(&webp).expect("decoder creation failed");

        // Decode frame by frame
        let mut count = 0;
        while let Some(frame) = decoder.next_frame().expect("next_frame failed") {
            assert_eq!(frame.width, width);
            assert_eq!(frame.height, height);
            assert_eq!(frame.data.len(), (width * height * 4) as usize);
            count += 1;
        }

        assert_eq!(count, 2);
    }

    #[test]
    fn test_animation_reset() {
        let width = 16;
        let height = 16;

        let frame1 = generate_rgba(width, height, 255, 0, 0, 255);
        let frame2 = generate_rgba(width, height, 0, 255, 0, 255);

        let mut encoder = AnimationEncoder::new(width, height).expect("encoder creation failed");
        encoder
            .add_frame_rgba(&frame1, 0)
            .expect("add frame 1 failed");
        encoder
            .add_frame_rgba(&frame2, 100)
            .expect("add frame 2 failed");
        let webp = encoder.finish(200).expect("finish failed");

        let mut decoder = AnimationDecoder::new(&webp).expect("decoder creation failed");

        // Decode all frames
        let frames1 = decoder.decode_all().expect("decode_all failed");
        assert_eq!(frames1.len(), 2);

        // Reset and decode again
        decoder.reset();
        let frames2 = decoder.decode_all().expect("decode_all after reset failed");
        assert_eq!(frames2.len(), 2);

        // Frames should be identical
        assert_eq!(frames1[0].timestamp_ms, frames2[0].timestamp_ms);
        assert_eq!(frames1[1].timestamp_ms, frames2[1].timestamp_ms);
    }

    #[test]
    fn test_animation_lossless() {
        let width = 16;
        let height = 16;

        let frame1 = generate_rgba(width, height, 100, 150, 200, 255);
        let frame2 = generate_rgba(width, height, 200, 150, 100, 255);

        let mut encoder = AnimationEncoder::new(width, height).expect("encoder creation failed");
        encoder.set_lossless(true);
        encoder
            .add_frame_rgba(&frame1, 0)
            .expect("add frame 1 failed");
        encoder
            .add_frame_rgba(&frame2, 100)
            .expect("add frame 2 failed");
        let webp = encoder.finish(200).expect("finish failed");

        let mut decoder = AnimationDecoder::new(&webp).expect("decoder creation failed");
        let frames = decoder.decode_all().expect("decode_all failed");

        // Lossless frames should match exactly
        assert_eq!(frames[0].data, frame1);
        assert_eq!(frames[1].data, frame2);
    }

    #[test]
    fn test_animation_with_options() {
        use webpx::{AnimationDecoder, AnimationEncoder, ColorMode};

        let width = 16;
        let height = 16;
        // Use different frames to avoid libwebp frame deduplication
        let frame1 = generate_rgba(width, height, 100, 150, 200, 255);
        let frame2 = generate_rgba(width, height, 200, 100, 50, 255);

        // Create with options: allow mixed, loop 3 times
        let mut encoder = AnimationEncoder::with_options(width, height, true, 3).expect("encoder");
        encoder.set_quality(80.0);
        encoder.set_preset(webpx::Preset::Picture);
        encoder.set_lossless(true);

        encoder.add_frame_rgba(&frame1, 0).expect("add frame 1");
        encoder.add_frame_rgba(&frame2, 100).expect("add frame 2");
        let webp = encoder.finish(200).expect("finish");

        // Decode with options
        let decoder =
            AnimationDecoder::with_options(&webp, ColorMode::Bgra, false).expect("decoder");
        let info = decoder.info();
        assert_eq!(info.width, width);
        assert_eq!(info.height, height);
        assert_eq!(info.frame_count, 2);
        assert_eq!(info.loop_count, 3);
    }

    #[test]
    fn test_animation_add_frame_rgb() {
        use webpx::AnimationEncoder;

        let width = 16;
        let height = 16;
        let frame_rgb = generate_rgb(width, height, 100, 150, 200);

        let mut encoder = AnimationEncoder::new(width, height).expect("encoder");
        encoder.add_frame_rgb(&frame_rgb, 0).expect("add frame rgb");
        encoder
            .add_frame_rgb(&frame_rgb, 100)
            .expect("add frame rgb 2");

        let webp = encoder.finish(200).expect("finish");
        let info = ImageInfo::from_webp(&webp).expect("info");
        // Animation with 2+ frames should be marked animated
        // (single frame might not be)
        assert_eq!(info.width, width);
        assert_eq!(info.height, height);
    }

    #[test]
    fn test_animation_decode_all_with_durations() {
        use webpx::{AnimationDecoder, AnimationEncoder};

        let width = 8;
        let height = 8;
        let frame1 = generate_rgba(width, height, 255, 0, 0, 255);
        let frame2 = generate_rgba(width, height, 0, 255, 0, 255);
        let frame3 = generate_rgba(width, height, 0, 0, 255, 255);

        let mut encoder = AnimationEncoder::new(width, height).expect("encoder");
        encoder.add_frame_rgba(&frame1, 0).expect("add 1");
        encoder.add_frame_rgba(&frame2, 100).expect("add 2");
        encoder.add_frame_rgba(&frame3, 200).expect("add 3");
        let webp = encoder.finish(300).expect("finish");

        let mut decoder = AnimationDecoder::new(&webp).expect("decoder");
        let frames = decoder.decode_all().expect("decode_all");

        assert_eq!(frames.len(), 3);
        // Timestamps are reported as END times by libwebp
        // Frame 1 ends at 0+duration, etc.
        assert!(frames[0].timestamp_ms >= 0);
        assert!(frames[1].timestamp_ms > frames[0].timestamp_ms);
        assert!(frames[2].timestamp_ms > frames[1].timestamp_ms);
    }

    #[test]
    fn test_animation_has_more_frames() {
        use webpx::{AnimationDecoder, AnimationEncoder};

        let width = 8;
        let height = 8;
        let frame = generate_rgba(width, height, 100, 100, 100, 255);

        let mut encoder = AnimationEncoder::new(width, height).expect("encoder");
        encoder.add_frame_rgba(&frame, 0).expect("add");
        let webp = encoder.finish(100).expect("finish");

        let mut decoder = AnimationDecoder::new(&webp).expect("decoder");
        assert!(decoder.has_more_frames());

        decoder.next_frame().expect("next").expect("frame");
        assert!(!decoder.has_more_frames());
    }

    #[test]
    fn test_animation_color_mode_rgba() {
        use webpx::{AnimationDecoder, AnimationEncoder, ColorMode};

        let width = 8;
        let height = 8;
        let frame1 = generate_rgba(width, height, 100, 150, 200, 255);
        let frame2 = generate_rgba(width, height, 200, 150, 100, 255);

        let mut encoder = AnimationEncoder::new(width, height).expect("encoder");
        encoder.add_frame_rgba(&frame1, 0).expect("add");
        encoder.add_frame_rgba(&frame2, 100).expect("add");
        let webp = encoder.finish(200).expect("finish");

        // Test RGBA mode
        let mut decoder =
            AnimationDecoder::with_options(&webp, ColorMode::Rgba, true).expect("decoder");
        let decoded = decoder.next_frame().expect("next").expect("frame");
        assert_eq!(decoded.width, width);
        assert_eq!(decoded.height, height);
    }

    #[test]
    fn test_animation_color_mode_bgra() {
        use webpx::{AnimationDecoder, AnimationEncoder, ColorMode};

        let width = 8;
        let height = 8;
        let frame1 = generate_rgba(width, height, 100, 150, 200, 255);
        let frame2 = generate_rgba(width, height, 200, 150, 100, 255);

        let mut encoder = AnimationEncoder::new(width, height).expect("encoder");
        encoder.add_frame_rgba(&frame1, 0).expect("add");
        encoder.add_frame_rgba(&frame2, 100).expect("add");
        let webp = encoder.finish(200).expect("finish");

        // Test BGRA mode
        let mut decoder =
            AnimationDecoder::with_options(&webp, ColorMode::Bgra, false).expect("decoder");
        let decoded = decoder.next_frame().expect("next").expect("frame");
        assert_eq!(decoded.width, width);
        assert_eq!(decoded.height, height);
    }

    #[test]
    fn test_animation_decoder_yuv_error() {
        use webpx::{AnimationDecoder, AnimationEncoder, ColorMode};

        let width = 8;
        let height = 8;
        let frame = generate_rgba(width, height, 100, 150, 200, 255);

        let mut encoder = AnimationEncoder::new(width, height).expect("encoder");
        encoder.add_frame_rgba(&frame, 0).expect("add");
        let webp = encoder.finish(100).expect("finish");

        // YUV modes not supported for animation decoder
        let result = AnimationDecoder::with_options(&webp, ColorMode::Yuv420, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_animation_invalid_frame_size() {
        use webpx::AnimationEncoder;

        let mut encoder = AnimationEncoder::new(100, 100).expect("encoder");
        let small_frame = vec![0u8; 10];
        let result = encoder.add_frame_rgba(&small_frame, 0);
        assert!(result.is_err());

        let result_rgb = encoder.add_frame_rgb(&small_frame, 0);
        assert!(result_rgb.is_err());
    }

    #[cfg(feature = "icc")]
    #[test]
    fn test_animation_with_icc_profile() {
        use webpx::AnimationEncoder;

        let width = 8;
        let height = 8;
        let frame = generate_rgba(width, height, 100, 150, 200, 255);

        // Minimal ICC profile header (zeros are not a valid ICC but libwebp accepts them)
        let fake_icc = vec![0u8; 128];

        let mut encoder = AnimationEncoder::new(width, height).expect("encoder");
        encoder.set_icc_profile(fake_icc.clone());
        encoder.add_frame_rgba(&frame, 0).expect("add");

        // libwebp mux accepts arbitrary byte sequences as ICC data
        let webp = encoder
            .finish(100)
            .expect("finish should succeed even with invalid ICC");

        // Verify the ICC data was embedded
        let extracted = webpx::get_icc_profile(&webp).expect("should extract ICC");
        assert_eq!(extracted, Some(fake_icc), "ICC profile should round-trip");
    }
}

mod compat_webp_tests {
    use super::{generate_rgb, generate_rgba};
    use webpx::compat::webp::{BitstreamFeatures, Decoder, Encoder, PixelLayout};

    #[test]
    fn test_pixel_layout_bytes_per_pixel() {
        assert_eq!(PixelLayout::Rgb.bytes_per_pixel(), 3);
        assert_eq!(PixelLayout::Rgba.bytes_per_pixel(), 4);
    }

    #[test]
    fn test_encoder_from_rgb() {
        let rgb = generate_rgb(8, 8, 100, 150, 200);
        let encoder = Encoder::from_rgb(&rgb, 8, 8);
        let webp = encoder.encode(85.0);
        assert!(!webp.is_empty());
    }

    #[test]
    fn test_encoder_from_rgba() {
        let rgba = generate_rgba(8, 8, 100, 150, 200, 255);
        let encoder = Encoder::from_rgba(&rgba, 8, 8);
        let webp = encoder.encode_lossless();
        assert!(!webp.is_empty());
    }

    #[test]
    fn test_encoder_encode_simple() {
        let rgba = generate_rgba(8, 8, 100, 150, 200, 255);
        let encoder = Encoder::new(&rgba, PixelLayout::Rgba, 8, 8);

        // Lossy
        let webp_lossy = encoder.encode_simple(false, 85.0).expect("encode lossy");
        assert!(!webp_lossy.is_empty());

        // Lossless
        let webp_lossless = encoder.encode_simple(true, 75.0).expect("encode lossless");
        assert!(!webp_lossless.is_empty());
    }

    #[test]
    fn test_webp_memory_deref() {
        let rgba = generate_rgba(4, 4, 255, 0, 0, 255);
        let encoder = Encoder::from_rgba(&rgba, 4, 4);
        let webp = encoder.encode(85.0);

        // Test Deref
        let slice: &[u8] = &webp;
        assert_eq!(slice.len(), webp.len());

        // Test AsRef
        let as_ref: &[u8] = webp.as_ref();
        assert_eq!(as_ref.len(), webp.len());
    }

    #[test]
    fn test_decoder_decode_without_alpha() {
        let rgb = generate_rgb(8, 8, 100, 150, 200);
        let encoder = Encoder::from_rgb(&rgb, 8, 8);
        let webp = encoder.encode(85.0);

        // Verify the encoded WebP doesn't have alpha
        let features = BitstreamFeatures::new(&webp).expect("features");
        assert!(!features.has_alpha(), "RGB encode should not have alpha");

        let decoder = Decoder::new(&webp);
        let image = decoder.decode().expect("decode");

        assert_eq!(image.width(), 8);
        assert_eq!(image.height(), 8);
        assert_eq!(image.layout(), PixelLayout::Rgb);
        assert_eq!(image.data().len(), 8 * 8 * 3); // RGB = 3 bytes per pixel
    }

    #[test]
    fn test_webp_image_accessors() {
        // Use semi-transparent alpha to ensure alpha channel is preserved
        let rgba = generate_rgba(4, 4, 100, 150, 200, 128);
        let encoder = Encoder::from_rgba(&rgba, 4, 4);
        let webp = encoder.encode_lossless();

        let features = BitstreamFeatures::new(&webp).expect("features");
        assert!(
            features.has_alpha(),
            "semi-transparent RGBA should have alpha"
        );

        let decoder = Decoder::new(&webp);
        let image = decoder.decode().expect("decode");

        assert_eq!(image.width(), 4);
        assert_eq!(image.height(), 4);
        assert_eq!(image.layout(), PixelLayout::Rgba);
        assert_eq!(image.data().len(), 4 * 4 * 4); // RGBA = 4 bytes per pixel
    }

    #[test]
    fn test_bitstream_features_accessors() {
        let rgba = generate_rgba(16, 16, 100, 150, 200, 128);
        let encoder = Encoder::from_rgba(&rgba, 16, 16);
        let webp = encoder.encode(85.0);

        let features = BitstreamFeatures::new(&webp).expect("features");
        assert_eq!(features.width(), 16);
        assert_eq!(features.height(), 16);
        assert!(features.has_alpha(), "RGBA encode should have alpha");
        assert!(!features.has_animation());
    }

    #[test]
    fn test_bitstream_features_invalid() {
        let invalid = b"not webp";
        let features = BitstreamFeatures::new(invalid);
        assert!(features.is_none());
    }

    #[test]
    fn test_decoder_decode_animated() {
        // Create an animated webp with multiple distinct frames
        let frame1 = generate_rgba(8, 8, 100, 150, 200, 255);
        let frame2 = generate_rgba(8, 8, 200, 150, 100, 255);
        let mut encoder = webpx::AnimationEncoder::new(8, 8).expect("encoder");
        encoder.add_frame_rgba(&frame1, 0).expect("add");
        encoder.add_frame_rgba(&frame2, 100).expect("add");
        let webp = encoder.finish(200).expect("finish");

        // Verify it's detected as animated
        let features = BitstreamFeatures::new(&webp).expect("features");
        assert!(
            features.has_animation(),
            "multi-frame WebP should be flagged as animated"
        );

        // compat decoder should return None for animated images
        // (mimics original webp crate behavior)
        let decoder = Decoder::new(&webp);
        let result = decoder.decode();
        assert!(
            result.is_none(),
            "compat decoder should return None for animated WebP"
        );
    }
}

#[cfg(feature = "animation")]
mod compat_webp_animation_tests {
    use super::generate_rgba;
    use webpx::compat::webp_animation::{
        ColorMode, Decoder, DecoderOptions, Encoder, EncoderOptions, EncodingConfig, EncodingType,
        Error, Frame, LossyEncodingConfig,
    };

    #[test]
    fn test_color_mode_size() {
        assert_eq!(ColorMode::Rgb.size(), 3);
        assert_eq!(ColorMode::Rgba.size(), 4);
        assert_eq!(ColorMode::Bgra.size(), 4);
        assert_eq!(ColorMode::Bgr.size(), 3);
    }

    #[test]
    fn test_color_mode_default() {
        assert_eq!(ColorMode::default(), ColorMode::Rgba);
    }

    #[test]
    fn test_webp_data_accessors() {
        let frame = generate_rgba(4, 4, 100, 150, 200, 255);
        let mut encoder = Encoder::new((4, 4)).expect("encoder");
        encoder.add_frame(&frame, 0).expect("add");
        let webp = encoder.finalize(100).expect("finalize");

        assert!(!webp.is_empty());

        // Test Deref and AsRef
        let slice: &[u8] = &webp;
        assert!(!slice.is_empty());
        let as_ref: &[u8] = webp.as_ref();
        assert!(!as_ref.is_empty());
    }

    #[test]
    fn test_encoder_with_options() {
        let frame = generate_rgba(4, 4, 100, 150, 200, 255);

        let options = EncoderOptions {
            kmin: 0,
            kmax: 0,
            encoding_config: Some(EncodingConfig {
                quality: 90.0,
                encoding_type: EncodingType::Lossy(LossyEncodingConfig {
                    segments: 4,
                    alpha_compression: true,
                }),
            }),
        };

        let mut encoder = Encoder::new_with_options((4, 4), options).expect("encoder");
        encoder.add_frame(&frame, 0).expect("add");
        let webp = encoder.finalize(100).expect("finalize");
        assert!(!webp.is_empty());
    }

    #[test]
    fn test_encoder_lossless_config() {
        let frame = generate_rgba(4, 4, 100, 150, 200, 255);

        let options = EncoderOptions {
            kmin: 0,
            kmax: 0,
            encoding_config: Some(EncodingConfig {
                quality: 75.0,
                encoding_type: EncodingType::Lossless,
            }),
        };

        let mut encoder = Encoder::new_with_options((4, 4), options).expect("encoder");
        encoder.add_frame(&frame, 0).expect("add");
        let webp = encoder.finalize(100).expect("finalize");
        assert!(!webp.is_empty());
    }

    #[test]
    fn test_encoder_zero_dimensions_error() {
        let result = Encoder::new((0, 0));
        assert!(matches!(result, Err(Error::DimensionsMustbePositive)));
    }

    #[test]
    fn test_encoder_no_frames_error() {
        let encoder = Encoder::new((4, 4)).expect("encoder");
        let result = encoder.finalize(100);
        assert!(matches!(result, Err(Error::NoFramesAdded)));
    }

    #[test]
    fn test_decoder_with_options() {
        let frame = generate_rgba(4, 4, 100, 150, 200, 255);
        let mut encoder = Encoder::new((4, 4)).expect("encoder");
        encoder.add_frame(&frame, 0).expect("add");
        let webp = encoder.finalize(100).expect("finalize");

        let options = DecoderOptions {
            use_threads: true,
            color_mode: ColorMode::Bgra,
        };

        let decoder = Decoder::new_with_options(&webp, options).expect("decoder");
        let frames = decoder.decode().expect("decode");
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].color_mode(), ColorMode::Bgra);
    }

    #[test]
    fn test_decoder_empty_error() {
        let result = Decoder::new(&[]);
        assert!(matches!(result, Err(Error::DecodeFailed)));
    }

    #[test]
    fn test_frame_accessors() {
        let frame_data1 = generate_rgba(8, 8, 100, 150, 200, 255);
        let frame_data2 = generate_rgba(8, 8, 200, 150, 100, 255);
        let mut encoder = Encoder::new((8, 8)).expect("encoder");
        encoder.add_frame(&frame_data1, 0).expect("add");
        encoder.add_frame(&frame_data2, 100).expect("add");
        let webp = encoder.finalize(200).expect("finalize");

        let decoder = Decoder::new(&webp).expect("decoder");
        let frames = decoder.decode().expect("decode");

        assert_eq!(frames.len(), 2);
        let frame = &frames[0];
        assert_eq!(frame.dimensions(), (8, 8));
        // Timestamp is end time, not start time
        assert!(frame.timestamp() >= 0);
        assert_eq!(frame.color_mode(), ColorMode::Rgba);
        assert!(!frame.data().is_empty());
    }

    #[test]
    fn test_decoder_iterator() {
        let frame1 = generate_rgba(4, 4, 255, 0, 0, 255);
        let frame2 = generate_rgba(4, 4, 0, 255, 0, 255);

        let mut encoder = Encoder::new((4, 4)).expect("encoder");
        encoder.add_frame(&frame1, 0).expect("add 1");
        encoder.add_frame(&frame2, 100).expect("add 2");
        let webp = encoder.finalize(200).expect("finalize");

        let decoder = Decoder::new(&webp).expect("decoder");
        let frames: Vec<Frame> = decoder.into_iter().collect();

        assert_eq!(frames.len(), 2);
        // Timestamps are end times per libwebp API
        assert!(frames[0].timestamp() >= 0);
        assert!(frames[1].timestamp() > frames[0].timestamp());
    }

    #[test]
    fn test_compat_error_display() {
        // Test the compat Error type display
        let errors: Vec<(Error, &str)> = vec![
            (Error::EncoderCreateFailed, "Encoder creation failed"),
            (Error::EncoderAddFailed, "Frame add failed"),
            (Error::EncoderAssmebleFailed, "Encoder assembly failed"),
            (Error::DecodeFailed, "Decode failed"),
            (
                Error::BufferSizeFailed(10, 100),
                "Buffer size mismatch: got 10, expected 100",
            ),
            (
                Error::TimestampMustBeHigherThanPrevious(50, 100),
                "Timestamp 50 must be higher than previous 100",
            ),
            (Error::NoFramesAdded, "No frames added"),
            (
                Error::DimensionsMustbePositive,
                "Dimensions must be positive",
            ),
        ];

        for (error, expected) in errors {
            assert_eq!(format!("{}", error), expected);
        }
    }

    #[test]
    fn test_encoding_config_default() {
        let config = EncodingConfig::default();
        assert_eq!(config.quality, 75.0);
        assert!(matches!(config.encoding_type, EncodingType::Lossy(_)));
    }
}

mod bgra_tests {
    use super::*;

    /// Generate BGRA data from RGBA by swapping R and B channels.
    fn generate_bgra(width: u32, height: u32, b: u8, g: u8, r: u8, a: u8) -> Vec<u8> {
        let mut data = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..(width * height) {
            data.push(b);
            data.push(g);
            data.push(r);
            data.push(a);
        }
        data
    }

    /// Generate BGR data.
    fn generate_bgr(width: u32, height: u32, b: u8, g: u8, r: u8) -> Vec<u8> {
        let mut data = Vec::with_capacity((width * height * 3) as usize);
        for _ in 0..(width * height) {
            data.push(b);
            data.push(g);
            data.push(r);
        }
        data
    }

    #[test]
    fn test_encode_bgra() {
        let width = 32;
        let height = 32;
        let bgra = generate_bgra(width, height, 255, 128, 64, 255);

        let webp = encode_bgra(&bgra, width, height, 85.0, Unstoppable).unwrap();
        assert!(!webp.is_empty());

        // Decode and verify it's valid
        let (decoded, w, h) = decode_rgba(&webp).unwrap();
        assert_eq!((w, h), (width, height));
        assert_eq!(decoded.len(), (width * height * 4) as usize);
    }

    #[test]
    fn test_encode_bgr() {
        let width = 32;
        let height = 32;
        let bgr = generate_bgr(width, height, 255, 128, 64);

        let webp = encode_bgr(&bgr, width, height, 85.0, Unstoppable).unwrap();
        assert!(!webp.is_empty());

        // Decode and verify it's valid
        let (decoded, w, h) = decode_rgb(&webp).unwrap();
        assert_eq!((w, h), (width, height));
        assert_eq!(decoded.len(), (width * height * 3) as usize);
    }

    #[test]
    fn test_decode_bgra() {
        let width = 32;
        let height = 32;
        let rgba = generate_rgba(width, height, 64, 128, 255, 200);

        let webp = encode_rgba(&rgba, width, height, 100.0, Unstoppable).unwrap();
        let (bgra, w, h) = decode_bgra(&webp).unwrap();

        assert_eq!((w, h), (width, height));
        assert_eq!(bgra.len(), (width * height * 4) as usize);
    }

    #[test]
    fn test_decode_bgr() {
        let width = 32;
        let height = 32;
        let rgb = generate_rgb(width, height, 64, 128, 255);

        let webp = encode_rgb(&rgb, width, height, 100.0, Unstoppable).unwrap();
        let (bgr, w, h) = decode_bgr(&webp).unwrap();

        assert_eq!((w, h), (width, height));
        assert_eq!(bgr.len(), (width * height * 3) as usize);
    }

    #[test]
    fn test_encoder_new_bgra() {
        let width = 16;
        let height = 16;
        let bgra = generate_bgra(width, height, 100, 150, 200, 255);

        let webp = Encoder::new_bgra(&bgra, width, height)
            .quality(80.0)
            .encode(Unstoppable)
            .unwrap();

        assert!(!webp.is_empty());
    }

    #[test]
    fn test_encoder_new_bgr() {
        let width = 16;
        let height = 16;
        let bgr = generate_bgr(width, height, 100, 150, 200);

        let webp = Encoder::new_bgr(&bgr, width, height)
            .quality(80.0)
            .encode(Unstoppable)
            .unwrap();

        assert!(!webp.is_empty());
    }

    #[test]
    fn test_decoder_decode_bgra() {
        let width = 16;
        let height = 16;
        let rgba = generate_rgba(width, height, 200, 150, 100, 255);

        let webp = encode_rgba(&rgba, width, height, 100.0, Unstoppable).unwrap();
        let decoder = Decoder::new(&webp).unwrap();
        let img = decoder.decode_bgra().unwrap();

        assert_eq!(img.width(), width as usize);
        assert_eq!(img.height(), height as usize);
    }

    #[test]
    fn test_decoder_decode_bgr() {
        let width = 16;
        let height = 16;
        let rgb = generate_rgb(width, height, 200, 150, 100);

        let webp = encode_rgb(&rgb, width, height, 100.0, Unstoppable).unwrap();
        let decoder = Decoder::new(&webp).unwrap();
        let img = decoder.decode_bgr().unwrap();

        assert_eq!(img.width(), width as usize);
        assert_eq!(img.height(), height as usize);
    }
}

mod zero_copy_tests {
    use super::*;

    #[test]
    fn test_decode_rgba_into() {
        let width = 32;
        let height = 32;
        let rgba = generate_rgba(width, height, 100, 150, 200, 255);

        let webp = encode_rgba(&rgba, width, height, 100.0, Unstoppable).unwrap();

        // Allocate buffer
        let stride = (width * 4) as usize;
        let mut buffer = vec![0u8; stride * height as usize];

        let (w, h) = decode_rgba_into(&webp, &mut buffer, stride as u32).unwrap();

        assert_eq!((w, h), (width, height));
        // Buffer should be filled with non-zero data
        assert!(buffer.iter().any(|&x| x != 0));
    }

    #[test]
    fn test_decode_bgra_into() {
        let width = 32;
        let height = 32;
        let rgba = generate_rgba(width, height, 100, 150, 200, 255);

        let webp = encode_rgba(&rgba, width, height, 100.0, Unstoppable).unwrap();

        // Allocate buffer
        let stride = (width * 4) as usize;
        let mut buffer = vec![0u8; stride * height as usize];

        let (w, h) = decode_bgra_into(&webp, &mut buffer, stride as u32).unwrap();

        assert_eq!((w, h), (width, height));
        assert!(buffer.iter().any(|&x| x != 0));
    }

    #[test]
    fn test_decode_rgb_into() {
        let width = 32;
        let height = 32;
        let rgb = generate_rgb(width, height, 100, 150, 200);

        let webp = encode_rgb(&rgb, width, height, 100.0, Unstoppable).unwrap();

        // Allocate buffer
        let stride = (width * 3) as usize;
        let mut buffer = vec![0u8; stride * height as usize];

        let (w, h) = decode_rgb_into(&webp, &mut buffer, stride as u32).unwrap();

        assert_eq!((w, h), (width, height));
        assert!(buffer.iter().any(|&x| x != 0));
    }

    #[test]
    fn test_decode_bgr_into() {
        let width = 32;
        let height = 32;
        let rgb = generate_rgb(width, height, 100, 150, 200);

        let webp = encode_rgb(&rgb, width, height, 100.0, Unstoppable).unwrap();

        // Allocate buffer
        let stride = (width * 3) as usize;
        let mut buffer = vec![0u8; stride * height as usize];

        let (w, h) = decode_bgr_into(&webp, &mut buffer, stride as u32).unwrap();

        assert_eq!((w, h), (width, height));
        assert!(buffer.iter().any(|&x| x != 0));
    }

    #[test]
    fn test_decode_into_with_stride() {
        let width = 16;
        let height = 16;
        let rgba = generate_rgba(width, height, 128, 64, 192, 255);

        let webp = encode_rgba(&rgba, width, height, 100.0, Unstoppable).unwrap();

        // Use a larger stride (e.g., aligned to 64 bytes)
        let stride = ((width * 4).div_ceil(64) * 64) as usize;
        let mut buffer = vec![0u8; stride * height as usize];

        let (w, h) = decode_rgba_into(&webp, &mut buffer, stride as u32).unwrap();

        assert_eq!((w, h), (width, height));

        // Check that data is in the right place
        for y in 0..height as usize {
            let row_start = y * stride;
            let row_data = &buffer[row_start..row_start + (width * 4) as usize];
            assert!(
                row_data.iter().any(|&x| x != 0),
                "Row {} should have data",
                y
            );
        }
    }

    #[test]
    fn test_decode_into_buffer_too_small() {
        let width = 32;
        let height = 32;
        let rgba = generate_rgba(width, height, 100, 150, 200, 255);

        let webp = encode_rgba(&rgba, width, height, 100.0, Unstoppable).unwrap();

        // Buffer is too small
        let mut buffer = vec![0u8; 100];

        let result = decode_rgba_into(&webp, &mut buffer, width * 4);
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_into_stride_too_small() {
        let width = 32;
        let height = 32;
        let rgba = generate_rgba(width, height, 100, 150, 200, 255);

        let webp = encode_rgba(&rgba, width, height, 100.0, Unstoppable).unwrap();

        // Buffer is big enough but stride is too small
        let mut buffer = vec![0u8; (width * height * 4) as usize];

        let result = decode_rgba_into(&webp, &mut buffer, 10); // stride too small
        assert!(result.is_err());
    }
}

mod stride_tests {
    use super::*;

    #[test]
    fn test_encoder_with_stride() {
        let width = 16u32;
        let height = 16u32;
        // Create buffer with padding (stride > width * bpp)
        let stride = (width * 4 + 16) as usize; // 16 bytes padding per row
        let mut data = vec![0u8; stride * height as usize];

        // Fill only the image portion
        for y in 0..height as usize {
            for x in 0..width as usize {
                let offset = y * stride + x * 4;
                data[offset] = 100; // R
                data[offset + 1] = 150; // G
                data[offset + 2] = 200; // B
                data[offset + 3] = 255; // A
            }
        }

        let webp = Encoder::new_rgba_stride(&data, width, height, stride as u32)
            .quality(85.0)
            .encode(Unstoppable)
            .unwrap();

        assert!(!webp.is_empty());

        // Decode and verify dimensions
        let (_, w, h) = decode_rgba(&webp).unwrap();
        assert_eq!((w, h), (width, height));
    }

    #[test]
    fn test_encoder_bgra_with_stride() {
        let width = 16u32;
        let height = 16u32;
        let stride = (width * 4 + 32) as usize;
        let mut data = vec![0u8; stride * height as usize];

        for y in 0..height as usize {
            for x in 0..width as usize {
                let offset = y * stride + x * 4;
                data[offset] = 200; // B
                data[offset + 1] = 150; // G
                data[offset + 2] = 100; // R
                data[offset + 3] = 255; // A
            }
        }

        let webp = Encoder::new_bgra_stride(&data, width, height, stride as u32)
            .quality(85.0)
            .encode(Unstoppable)
            .unwrap();

        assert!(!webp.is_empty());
    }

    #[test]
    fn test_encoder_rgb_with_stride() {
        let width = 16u32;
        let height = 16u32;
        let stride = (width * 3 + 8) as usize;
        let mut data = vec![0u8; stride * height as usize];

        for y in 0..height as usize {
            for x in 0..width as usize {
                let offset = y * stride + x * 3;
                data[offset] = 100; // R
                data[offset + 1] = 150; // G
                data[offset + 2] = 200; // B
            }
        }

        let webp = Encoder::new_rgb_stride(&data, width, height, stride as u32)
            .quality(85.0)
            .encode(Unstoppable)
            .unwrap();

        assert!(!webp.is_empty());
    }

    #[test]
    fn test_stride_validation_too_small() {
        let width = 32u32;
        let height = 32u32;
        let data = vec![0u8; (width * height * 4) as usize];

        // Stride is smaller than width * 4
        let result = Encoder::new_rgba_stride(&data, width, height, 10).encode(Unstoppable);

        assert!(result.is_err());
    }
}

mod typed_pixel_tests {
    use super::*;
    use rgb::alt::{BGR8, BGRA8};
    use rgb::{RGB8, RGBA8};

    #[test]
    fn test_from_pixels_rgba() {
        let width = 32u32;
        let height = 32u32;
        let pixels: Vec<RGBA8> = (0..(width * height))
            .map(|i| {
                RGBA8::new(
                    (i % 256) as u8,
                    ((i * 2) % 256) as u8,
                    ((i * 3) % 256) as u8,
                    255,
                )
            })
            .collect();

        let webp = Encoder::from_pixels(&pixels, width, height)
            .quality(85.0)
            .encode(Unstoppable)
            .unwrap();

        assert!(!webp.is_empty());
        assert!(webp.starts_with(b"RIFF"));

        // Decode and verify dimensions
        let (decoded, w, h) = decode_rgba(&webp).unwrap();
        assert_eq!(w, width);
        assert_eq!(h, height);
        assert_eq!(decoded.len(), (width * height * 4) as usize);
    }

    #[test]
    fn test_from_pixels_rgb() {
        let width = 32u32;
        let height = 32u32;
        let pixels: Vec<RGB8> = (0..(width * height))
            .map(|i| {
                RGB8::new(
                    (i % 256) as u8,
                    ((i * 2) % 256) as u8,
                    ((i * 3) % 256) as u8,
                )
            })
            .collect();

        let webp = Encoder::from_pixels(&pixels, width, height)
            .quality(85.0)
            .encode(Unstoppable)
            .unwrap();

        assert!(!webp.is_empty());
        assert!(webp.starts_with(b"RIFF"));

        // Decode and verify dimensions
        let (decoded, w, h) = decode_rgb(&webp).unwrap();
        assert_eq!(w, width);
        assert_eq!(h, height);
        assert_eq!(decoded.len(), (width * height * 3) as usize);
    }

    #[test]
    fn test_from_pixels_bgra() {
        let width = 32u32;
        let height = 32u32;
        let pixels: Vec<BGRA8> = (0..(width * height))
            .map(|i| BGRA8 {
                b: (i % 256) as u8,
                g: ((i * 2) % 256) as u8,
                r: ((i * 3) % 256) as u8,
                a: 255,
            })
            .collect();

        let webp = Encoder::from_pixels(&pixels, width, height)
            .quality(85.0)
            .encode(Unstoppable)
            .unwrap();

        assert!(!webp.is_empty());
        assert!(webp.starts_with(b"RIFF"));

        // Decode and verify dimensions
        let (decoded, w, h) = decode_bgra(&webp).unwrap();
        assert_eq!(w, width);
        assert_eq!(h, height);
        assert_eq!(decoded.len(), (width * height * 4) as usize);
    }

    #[test]
    fn test_from_pixels_bgr() {
        let width = 32u32;
        let height = 32u32;
        let pixels: Vec<BGR8> = (0..(width * height))
            .map(|i| BGR8 {
                b: (i % 256) as u8,
                g: ((i * 2) % 256) as u8,
                r: ((i * 3) % 256) as u8,
            })
            .collect();

        let webp = Encoder::from_pixels(&pixels, width, height)
            .quality(85.0)
            .encode(Unstoppable)
            .unwrap();

        assert!(!webp.is_empty());
        assert!(webp.starts_with(b"RIFF"));

        // Decode and verify dimensions
        let (decoded, w, h) = decode_bgr(&webp).unwrap();
        assert_eq!(w, width);
        assert_eq!(h, height);
        assert_eq!(decoded.len(), (width * height * 3) as usize);
    }

    #[test]
    fn test_from_pixels_stride() {
        let width = 30u32;
        let height = 20u32;
        let stride = 32u32; // 32 pixels per row (2 pixels padding)

        // Allocate buffer with stride
        let pixels: Vec<RGBA8> = vec![RGBA8::new(128, 64, 32, 255); (stride * height) as usize];

        let webp = Encoder::from_pixels_stride(&pixels, width, height, stride)
            .quality(85.0)
            .encode(Unstoppable)
            .unwrap();

        assert!(!webp.is_empty());

        // Decode and verify dimensions
        let (decoded, w, h) = decode_rgba(&webp).unwrap();
        assert_eq!(w, width);
        assert_eq!(h, height);
        assert_eq!(decoded.len(), (width * height * 4) as usize);
    }

    #[test]
    fn test_from_pixels_lossless() {
        let width = 16u32;
        let height = 16u32;
        let pixels: Vec<RGBA8> = (0..(width * height))
            .map(|i| {
                RGBA8::new(
                    (i % 256) as u8,
                    ((i * 2) % 256) as u8,
                    ((i * 3) % 256) as u8,
                    255,
                )
            })
            .collect();

        let webp = Encoder::from_pixels(&pixels, width, height)
            .lossless(true)
            .encode(Unstoppable)
            .unwrap();

        assert!(!webp.is_empty());

        // Decode and verify exact roundtrip for lossless
        let (decoded, w, h) = decode_rgba(&webp).unwrap();
        assert_eq!(w, width);
        assert_eq!(h, height);

        // For lossless, pixel values should be exact
        for (i, pixel) in pixels.iter().enumerate() {
            let idx = i * 4;
            assert_eq!(decoded[idx], pixel.r, "red mismatch at pixel {}", i);
            assert_eq!(decoded[idx + 1], pixel.g, "green mismatch at pixel {}", i);
            assert_eq!(decoded[idx + 2], pixel.b, "blue mismatch at pixel {}", i);
            assert_eq!(decoded[idx + 3], pixel.a, "alpha mismatch at pixel {}", i);
        }
    }
}
