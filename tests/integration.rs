//! Integration tests for webpx crate.

use webpx::*;

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
        let webp = encode_lossless(&original, width, height).expect("encode failed");

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
        let webp = encode_rgba(&original, width, height, 95.0).expect("encode failed");

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
        let webp = encode_rgb(&original, width, height, 90.0).expect("encode failed");

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
            .encode()
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

        let webp = encode_rgba(&data, width, height, 85.0).expect("encode failed");

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

        let webp = encode_rgba(&data, width, height, 85.0).expect("encode failed");

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

        let webp = encode_rgba(&data, width, height, 85.0).expect("encode failed");

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

        let webp = encode_rgba(&data, width, height, 85.0).expect("encode failed");

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

        let webp = encode_lossless(&data, 1, 1).expect("encode failed");

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

            let webp = encode_lossless(&data, width, height)
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

        let webp = encode_rgba(&data, width, height, 50.0).expect("encode failed");

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
        assert!(encode_rgba(&data, 0, 10, 85.0).is_err());
        assert!(encode_rgba(&data, 10, 0, 85.0).is_err());

        // Exceeding max dimension should fail
        assert!(encode_rgba(&data, 20000, 10, 85.0).is_err());
        assert!(encode_rgba(&data, 10, 20000, 85.0).is_err());
    }

    #[test]
    fn test_buffer_too_small() {
        let small_buffer = vec![0u8; 10];

        // Buffer too small for 100x100 RGBA
        assert!(encode_rgba(&small_buffer, 100, 100, 85.0).is_err());
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
                .encode()
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
                .encode()
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
                .encode_rgba(&data, width, height)
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
                .encode_rgba(&data, width, height)
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

            let webp = config.encode_rgba(&data, width, height).unwrap_or_else(|e| {
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

            let webp = config.encode_rgba(&data, width, height).unwrap_or_else(|e| {
                panic!("encode with partitions={} failed: {}", partitions, e)
            });

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
            .encode_rgba(&data, width, height)
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
            .encode_rgba(&data, width, height)
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
            .encode_rgba(&data, width, height)
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
            .encode_rgba(&data, width, height)
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
            let webp = config.encode_rgba(&data, width, height).unwrap_or_else(|e| {
                panic!("encode with lossless level {} failed: {}", level, e)
            });

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
            .encode_rgba(&data, width, height)
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
            .encode_rgba(&data, width, height)
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
            .encode_rgba(&data, width, height)
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

        let config = EncoderConfig::new()
            .quality(75.0)
            .pass(6)
            .segments(4);

        let webp = config
            .encode_rgba(&data, width, height)
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
            .encode_rgba(&data, width, height)
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
            .encode_rgba(&data, width, height)
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
            .encode_rgba(&data, width, height)
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
            .encode_rgba(&data, width, height)
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
            (
                Error::MuxError(MuxError::BadData),
                "mux error: bad data",
            ),
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
            (EncodingError::BitstreamOutOfMemory, "bitstream out of memory"),
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
            .encode()
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
        let webp_no_icc = encode_rgba(&data, width, height, 85.0).expect("encode failed");

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
            .encode()
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

        let webp = encode_rgba(&data, width, height, 85.0).expect("encode failed");

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

        let webp = encode_rgba(&data, width, height, 85.0).expect("encode failed");

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

        let webp = encode_lossless(&original, width, height).expect("encode failed");

        // Create streaming decoder
        let mut decoder = StreamingDecoder::new(ColorMode::Rgba).expect("decoder creation failed");

        // Feed data in chunks
        let chunk_size = webp.len() / 4;
        for chunk in webp.chunks(chunk_size) {
            match decoder.append(chunk) {
                Ok(DecodeStatus::Complete) => break,
                Ok(DecodeStatus::NeedMoreData) => continue,
                Ok(DecodeStatus::Partial(_rows)) => continue,
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

        let webp = encode_lossless(&original, width, height).expect("encode failed");

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

        encoder.add_frame(&frame1, 0).expect("add frame 1 failed");
        encoder.add_frame(&frame2, 100).expect("add frame 2 failed");
        encoder.add_frame(&frame3, 200).expect("add frame 3 failed");

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
        encoder.add_frame(&frame1, 0).expect("add frame 1 failed");
        encoder.add_frame(&frame2, 100).expect("add frame 2 failed");
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
        encoder.add_frame(&frame1, 0).expect("add frame 1 failed");
        encoder.add_frame(&frame2, 100).expect("add frame 2 failed");
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
        encoder.add_frame(&frame1, 0).expect("add frame 1 failed");
        encoder.add_frame(&frame2, 100).expect("add frame 2 failed");
        let webp = encoder.finish(200).expect("finish failed");

        let mut decoder = AnimationDecoder::new(&webp).expect("decoder creation failed");
        let frames = decoder.decode_all().expect("decode_all failed");

        // Lossless frames should match exactly
        assert_eq!(frames[0].data, frame1);
        assert_eq!(frames[1].data, frame2);
    }
}
