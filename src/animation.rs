//! Animated WebP encoding and decoding.

use crate::config::{EncoderConfig, Preset};
use crate::error::{Error, Result};
use crate::types::{ColorMode, EncodePixel, PixelLayout};
use alloc::vec::Vec;
use core::ptr;
use whereat::*;

/// A single frame in an animation.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Frame {
    /// Frame pixel data (RGBA).
    pub data: Vec<u8>,
    /// Frame width.
    pub width: u32,
    /// Frame height.
    pub height: u32,
    /// Frame timestamp in milliseconds from animation start.
    pub timestamp_ms: i32,
    /// Frame duration in milliseconds.
    pub duration_ms: u32,
}

/// Animation metadata.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AnimationInfo {
    /// Canvas width.
    pub width: u32,
    /// Canvas height.
    pub height: u32,
    /// Number of frames.
    pub frame_count: u32,
    /// Loop count (0 = infinite).
    pub loop_count: u32,
    /// Background color (ARGB).
    pub bgcolor: u32,
}

/// Animated WebP decoder.
///
/// # Example
///
/// ```rust,no_run
/// use webpx::AnimationDecoder;
///
/// fn process_frame(_data: &[u8], _ts: i32) {}
///
/// let webp_data: &[u8] = &[0u8; 100]; // placeholder
/// let mut decoder = AnimationDecoder::new(webp_data)?;
///
/// let info = decoder.info();
/// println!("Animation: {}x{}, {} frames", info.width, info.height, info.frame_count);
///
/// while let Some(frame) = decoder.next_frame()? {
///     process_frame(&frame.data, frame.timestamp_ms);
/// }
/// # Ok::<(), webpx::At<webpx::Error>>(())
/// ```
pub struct AnimationDecoder {
    decoder: *mut libwebp_sys::WebPAnimDecoder,
    info: AnimationInfo,
    _data: Vec<u8>, // Keep data alive
}

// SAFETY: WebPAnimDecoder is thread-safe for single-threaded access
unsafe impl Send for AnimationDecoder {}

impl AnimationDecoder {
    /// Create a new animation decoder.
    pub fn new(data: &[u8]) -> Result<Self> {
        Self::with_options(data, ColorMode::Rgba, true)
    }

    /// Create a new animation decoder with options.
    ///
    /// # Arguments
    ///
    /// * `data` - WebP animation data
    /// * `color_mode` - Output color format
    /// * `use_threads` - Enable multi-threaded decoding
    pub fn with_options(data: &[u8], color_mode: ColorMode, use_threads: bool) -> Result<Self> {
        let csp_mode = match color_mode {
            ColorMode::Rgba => libwebp_sys::WEBP_CSP_MODE::MODE_RGBA,
            ColorMode::Bgra => libwebp_sys::WEBP_CSP_MODE::MODE_BGRA,
            ColorMode::Argb => libwebp_sys::WEBP_CSP_MODE::MODE_ARGB,
            ColorMode::Rgb => libwebp_sys::WEBP_CSP_MODE::MODE_RGB,
            ColorMode::Bgr => libwebp_sys::WEBP_CSP_MODE::MODE_BGR,
            _ => {
                return Err(at!(Error::InvalidInput(
                    "animation decoder only supports RGB modes".into(),
                )))
            }
        };

        // Keep a copy of the data since WebPAnimDecoder references it
        let data_copy = data.to_vec();

        let mut options = core::mem::MaybeUninit::<libwebp_sys::WebPAnimDecoderOptions>::uninit();
        let ok = unsafe { libwebp_sys::WebPAnimDecoderOptionsInit(options.as_mut_ptr()) };
        if ok == 0 {
            return Err(at!(Error::InvalidConfig(
                "failed to init decoder options".into(),
            )));
        }
        let mut options = unsafe { options.assume_init() };

        options.color_mode = csp_mode;
        options.use_threads = use_threads as i32;

        let webp_data = libwebp_sys::WebPData {
            bytes: data_copy.as_ptr(),
            size: data_copy.len(),
        };

        let decoder = unsafe { libwebp_sys::WebPAnimDecoderNew(&webp_data, &options) };

        if decoder.is_null() {
            return Err(at!(Error::InvalidWebP));
        }

        // Get animation info
        let mut anim_info = libwebp_sys::WebPAnimInfo::default();
        let ok = unsafe { libwebp_sys::WebPAnimDecoderGetInfo(decoder, &mut anim_info) };

        if ok == 0 {
            unsafe { libwebp_sys::WebPAnimDecoderDelete(decoder) };
            return Err(at!(Error::InvalidWebP));
        }

        Ok(Self {
            decoder,
            info: AnimationInfo {
                width: anim_info.canvas_width,
                height: anim_info.canvas_height,
                frame_count: anim_info.frame_count,
                loop_count: anim_info.loop_count,
                bgcolor: anim_info.bgcolor,
            },
            _data: data_copy,
        })
    }

    /// Get animation information.
    pub fn info(&self) -> &AnimationInfo {
        &self.info
    }

    /// Check if there are more frames to decode.
    pub fn has_more_frames(&self) -> bool {
        unsafe { libwebp_sys::WebPAnimDecoderHasMoreFrames(self.decoder) != 0 }
    }

    /// Decode the next frame.
    ///
    /// Returns `None` when all frames have been decoded.
    pub fn next_frame(&mut self) -> Result<Option<Frame>> {
        if !self.has_more_frames() {
            return Ok(None);
        }

        let mut buf: *mut u8 = ptr::null_mut();
        let mut timestamp: i32 = 0;

        let ok =
            unsafe { libwebp_sys::WebPAnimDecoderGetNext(self.decoder, &mut buf, &mut timestamp) };

        if ok == 0 {
            return Err(at!(Error::DecodeFailed(
                crate::error::DecodingError::BitstreamError,
            )));
        }

        // Copy the frame data (buffer is owned by decoder)
        let size = (self.info.width as usize) * (self.info.height as usize) * 4;
        let data = unsafe { core::slice::from_raw_parts(buf, size).to_vec() };

        // Calculate duration (difference from previous frame)
        // For first frame, we'll set duration later from next frame timestamp
        let duration_ms = 0; // Will be calculated by caller if needed

        Ok(Some(Frame {
            data,
            width: self.info.width,
            height: self.info.height,
            timestamp_ms: timestamp,
            duration_ms,
        }))
    }

    /// Reset the decoder to the first frame.
    pub fn reset(&mut self) {
        unsafe {
            libwebp_sys::WebPAnimDecoderReset(self.decoder);
        }
    }

    /// Decode all frames into a vector.
    pub fn decode_all(&mut self) -> Result<Vec<Frame>> {
        self.reset();

        let mut frames = Vec::with_capacity(self.info.frame_count as usize);
        let mut prev_timestamp = 0i32;

        while let Some(mut frame) = self.next_frame()? {
            // Calculate duration from timestamp difference
            frame.duration_ms = (frame.timestamp_ms - prev_timestamp).max(0) as u32;
            prev_timestamp = frame.timestamp_ms;
            frames.push(frame);
        }

        // Set the last frame's duration (assume same as previous or default)
        let len = frames.len();
        if len > 0 {
            let prev_duration = if len > 1 {
                frames[len - 2].duration_ms
            } else {
                100 // Default 100ms for single frame
            };
            frames[len - 1].duration_ms = prev_duration;
        }

        Ok(frames)
    }
}

impl Drop for AnimationDecoder {
    fn drop(&mut self) {
        if !self.decoder.is_null() {
            unsafe {
                libwebp_sys::WebPAnimDecoderDelete(self.decoder);
            }
        }
    }
}

/// Animated WebP encoder.
///
/// # Example
///
/// ```rust,no_run
/// use webpx::AnimationEncoder;
/// use rgb::RGBA8;
///
/// // Create frames with typed pixels (preferred)
/// let frame1: Vec<RGBA8> = vec![RGBA8::new(255, 0, 0, 255); 640 * 480];
/// let frame2: Vec<RGBA8> = vec![RGBA8::new(0, 255, 0, 255); 640 * 480];
/// let frame3: Vec<RGBA8> = vec![RGBA8::new(0, 0, 255, 255); 640 * 480];
///
/// let mut encoder = AnimationEncoder::with_options(640, 480, false, 0)?;
/// encoder.set_quality(85.0);
///
/// encoder.add_frame(&frame1, 0)?;      // First frame at t=0
/// encoder.add_frame(&frame2, 100)?;    // Second frame at t=100ms
/// encoder.add_frame(&frame3, 200)?;    // Third frame at t=200ms
///
/// let webp_data = encoder.finish(300)?;     // Total duration 300ms
/// # Ok::<(), webpx::At<webpx::Error>>(())
/// ```
pub struct AnimationEncoder {
    encoder: *mut libwebp_sys::WebPAnimEncoder,
    width: u32,
    height: u32,
    config: EncoderConfig,
    #[cfg(feature = "icc")]
    icc_profile: Option<Vec<u8>>,
}

// SAFETY: WebPAnimEncoder is thread-safe for single-threaded access
unsafe impl Send for AnimationEncoder {}

impl AnimationEncoder {
    /// Create a new animation encoder.
    pub fn new(width: u32, height: u32) -> Result<Self> {
        Self::with_options(width, height, true, 0)
    }

    /// Create a new animation encoder with options.
    ///
    /// # Arguments
    ///
    /// * `width` - Canvas width
    /// * `height` - Canvas height
    /// * `allow_mixed` - Allow mixing lossy and lossless frames
    /// * `loop_count` - Animation loop count (0 = infinite)
    pub fn with_options(
        width: u32,
        height: u32,
        allow_mixed: bool,
        loop_count: u32,
    ) -> Result<Self> {
        if width == 0 || height == 0 || width > 16383 || height > 16383 {
            return Err(at!(Error::InvalidInput("invalid dimensions".into())));
        }

        let mut options = core::mem::MaybeUninit::<libwebp_sys::WebPAnimEncoderOptions>::uninit();
        let ok = unsafe {
            libwebp_sys::WebPAnimEncoderOptionsInitInternal(
                options.as_mut_ptr(),
                libwebp_sys::WEBP_MUX_ABI_VERSION as i32,
            )
        };
        if ok == 0 {
            return Err(at!(Error::InvalidConfig(
                "failed to init encoder options".into(),
            )));
        }
        let mut options = unsafe { options.assume_init() };

        options.allow_mixed = allow_mixed as i32;
        options.anim_params.loop_count = loop_count as i32;

        let encoder = unsafe {
            libwebp_sys::WebPAnimEncoderNewInternal(
                width as i32,
                height as i32,
                &options,
                libwebp_sys::WEBP_MUX_ABI_VERSION as i32,
            )
        };

        if encoder.is_null() {
            return Err(at!(Error::OutOfMemory));
        }

        Ok(Self {
            encoder,
            width,
            height,
            config: EncoderConfig::default(),
            #[cfg(feature = "icc")]
            icc_profile: None,
        })
    }

    /// Set encoding quality.
    pub fn set_quality(&mut self, quality: f32) {
        self.config.quality = quality;
    }

    /// Set content-aware preset.
    pub fn set_preset(&mut self, preset: Preset) {
        self.config.preset = preset;
    }

    /// Enable lossless compression for all frames.
    pub fn set_lossless(&mut self, lossless: bool) {
        self.config.lossless = lossless;
    }

    /// Set ICC profile to embed.
    #[cfg(feature = "icc")]
    pub fn set_icc_profile(&mut self, profile: Vec<u8>) {
        self.icc_profile = Some(profile);
    }

    /// Add a frame with typed pixel data.
    ///
    /// This is the preferred method for type-safe frame addition with rgb crate types.
    ///
    /// # Supported Types
    /// - [`rgb::RGBA8`] - 4-channel RGBA
    /// - [`rgb::RGB8`] - 3-channel RGB
    /// - [`rgb::alt::BGRA8`] - 4-channel BGRA (Windows/GPU native)
    /// - [`rgb::alt::BGR8`] - 3-channel BGR (OpenCV)
    ///
    /// # Arguments
    ///
    /// * `pixels` - Frame pixel data
    /// * `timestamp_ms` - Frame timestamp in milliseconds from animation start
    pub fn add_frame<P: EncodePixel>(&mut self, pixels: &[P], timestamp_ms: i32) -> Result<()> {
        let bpp = P::LAYOUT.bytes_per_pixel();
        let data = unsafe {
            core::slice::from_raw_parts(pixels.as_ptr() as *const u8, pixels.len() * bpp)
        };
        self.add_frame_internal(data, timestamp_ms, P::LAYOUT)
    }

    /// Add a frame with RGBA byte data.
    ///
    /// # Arguments
    ///
    /// * `data` - Frame pixel data (RGBA, 4 bytes per pixel)
    /// * `timestamp_ms` - Frame timestamp in milliseconds from animation start
    pub fn add_frame_rgba(&mut self, data: &[u8], timestamp_ms: i32) -> Result<()> {
        self.add_frame_internal(data, timestamp_ms, PixelLayout::Rgba)
    }

    /// Add a frame with RGB byte data (no alpha).
    ///
    /// # Arguments
    ///
    /// * `data` - Frame pixel data (RGB, 3 bytes per pixel)
    /// * `timestamp_ms` - Frame timestamp in milliseconds from animation start
    pub fn add_frame_rgb(&mut self, data: &[u8], timestamp_ms: i32) -> Result<()> {
        self.add_frame_internal(data, timestamp_ms, PixelLayout::Rgb)
    }

    /// Add a frame with BGRA byte data.
    ///
    /// BGRA is the native format on Windows and some GPU APIs.
    ///
    /// # Arguments
    ///
    /// * `data` - Frame pixel data (BGRA, 4 bytes per pixel)
    /// * `timestamp_ms` - Frame timestamp in milliseconds from animation start
    pub fn add_frame_bgra(&mut self, data: &[u8], timestamp_ms: i32) -> Result<()> {
        self.add_frame_internal(data, timestamp_ms, PixelLayout::Bgra)
    }

    /// Add a frame with BGR byte data (no alpha).
    ///
    /// BGR is common in OpenCV and some image libraries.
    ///
    /// # Arguments
    ///
    /// * `data` - Frame pixel data (BGR, 3 bytes per pixel)
    /// * `timestamp_ms` - Frame timestamp in milliseconds from animation start
    pub fn add_frame_bgr(&mut self, data: &[u8], timestamp_ms: i32) -> Result<()> {
        self.add_frame_internal(data, timestamp_ms, PixelLayout::Bgr)
    }

    /// Internal: Add a frame with a specific pixel layout.
    fn add_frame_internal(
        &mut self,
        data: &[u8],
        timestamp_ms: i32,
        layout: PixelLayout,
    ) -> Result<()> {
        let bpp = layout.bytes_per_pixel();
        let expected = (self.width as usize) * (self.height as usize) * bpp;
        if data.len() < expected {
            return Err(at!(Error::InvalidInput("buffer too small".into())));
        }

        let webp_config = self.config.to_libwebp()?;

        let mut picture = libwebp_sys::WebPPicture::new()
            .map_err(|_| at!(Error::InvalidConfig("failed to init picture".into())))?;

        picture.width = self.width as i32;
        picture.height = self.height as i32;
        picture.use_argb = 1;

        let stride = (self.width as usize * bpp) as i32;
        let import_ok = unsafe {
            match layout {
                PixelLayout::Rgba => {
                    libwebp_sys::WebPPictureImportRGBA(&mut picture, data.as_ptr(), stride)
                }
                PixelLayout::Rgb => {
                    libwebp_sys::WebPPictureImportRGB(&mut picture, data.as_ptr(), stride)
                }
                PixelLayout::Bgra => {
                    libwebp_sys::WebPPictureImportBGRA(&mut picture, data.as_ptr(), stride)
                }
                PixelLayout::Bgr => {
                    libwebp_sys::WebPPictureImportBGR(&mut picture, data.as_ptr(), stride)
                }
            }
        };

        if import_ok == 0 {
            unsafe { libwebp_sys::WebPPictureFree(&mut picture) };
            return Err(at!(Error::OutOfMemory));
        }

        let ok = unsafe {
            libwebp_sys::WebPAnimEncoderAdd(self.encoder, &mut picture, timestamp_ms, &webp_config)
        };

        unsafe { libwebp_sys::WebPPictureFree(&mut picture) };

        if ok == 0 {
            let error_msg = unsafe {
                let ptr = libwebp_sys::WebPAnimEncoderGetError(self.encoder);
                if ptr.is_null() {
                    "unknown error"
                } else {
                    core::ffi::CStr::from_ptr(ptr)
                        .to_str()
                        .unwrap_or("unknown error")
                }
            };
            return Err(at!(Error::AnimationError(error_msg.into())));
        }

        Ok(())
    }

    /// Finish encoding and return the WebP data.
    ///
    /// # Arguments
    ///
    /// * `end_timestamp_ms` - End timestamp (determines duration of last frame)
    pub fn finish(self, end_timestamp_ms: i32) -> Result<Vec<u8>> {
        // Add NULL frame to signal end
        let ok = unsafe {
            libwebp_sys::WebPAnimEncoderAdd(
                self.encoder,
                ptr::null_mut(),
                end_timestamp_ms,
                ptr::null(),
            )
        };

        if ok == 0 {
            return Err(at!(Error::AnimationError(
                "failed to finalize animation".into()
            )));
        }

        // Assemble the animation
        let mut webp_data = libwebp_sys::WebPData::default();
        let ok = unsafe { libwebp_sys::WebPAnimEncoderAssemble(self.encoder, &mut webp_data) };

        if ok == 0 {
            return Err(at!(Error::AnimationError(
                "failed to assemble animation".into()
            )));
        }

        let result = unsafe {
            if webp_data.bytes.is_null() || webp_data.size == 0 {
                return Err(at!(Error::AnimationError("empty output".into())));
            }
            let slice = core::slice::from_raw_parts(webp_data.bytes, webp_data.size);
            let vec = slice.to_vec();
            libwebp_sys::WebPDataClear(&mut webp_data);
            vec
        };

        // Embed ICC profile if set
        #[cfg(feature = "icc")]
        if let Some(ref icc) = self.icc_profile {
            return crate::mux::embed_icc(&result, icc);
        }

        Ok(result)
    }
}

impl Drop for AnimationEncoder {
    fn drop(&mut self) {
        if !self.encoder.is_null() {
            unsafe {
                libwebp_sys::WebPAnimEncoderDelete(self.encoder);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_animation_encoder_creation() {
        let encoder = AnimationEncoder::new(100, 100);
        assert!(encoder.is_ok());
    }

    #[test]
    fn test_animation_encoder_invalid_dimensions() {
        assert!(AnimationEncoder::new(0, 100).is_err());
        assert!(AnimationEncoder::new(100, 0).is_err());
        assert!(AnimationEncoder::new(20000, 100).is_err());
    }
}
