//! Streaming/incremental WebP decode and encode.

use crate::error::{DecodingError, Error, Result};
use crate::types::ColorMode;
use alloc::vec::Vec;
use core::marker::PhantomData;
use core::ptr;
use whereat::*;

/// Status of a streaming decode operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum DecodeStatus {
    /// Decoding completed successfully.
    Complete,
    /// More data needed to continue decoding.
    NeedMoreData,
    /// Partial data available (returns number of decoded rows).
    Partial(u32),
}

/// Streaming WebP decoder.
///
/// Allows incremental decoding as data becomes available.
///
/// # Example
///
/// ```rust,no_run
/// use webpx::{StreamingDecoder, DecodeStatus, ColorMode};
///
/// fn process_rows(_data: &[u8], _w: u32, _h: u32) {}
///
/// let data_chunks: Vec<&[u8]> = vec![];
/// let mut decoder = StreamingDecoder::new(ColorMode::Rgba)?;
///
/// // Feed data incrementally
/// for chunk in data_chunks {
///     match decoder.append(chunk)? {
///         DecodeStatus::Complete => break,
///         DecodeStatus::NeedMoreData => continue,
///         DecodeStatus::Partial(_rows) => {
///             // Can access partially decoded data
///             if let Some((data, w, h)) = decoder.get_partial() {
///                 process_rows(data, w, h);
///             }
///         }
///         _ => {} // future variants
///     }
/// }
///
/// let (pixels, width, height) = decoder.finish()?;
/// # Ok::<(), webpx::At<webpx::Error>>(())
/// ```
#[cfg(feature = "decode")]
pub struct StreamingDecoder<'a> {
    decoder: *mut libwebp_sys::WebPIDecoder,
    color_mode: ColorMode,
    width: i32,
    height: i32,
    last_y: i32,
    // Ties the decoder's lifetime to the caller-supplied output buffer
    // when `with_buffer` is used. For `new()` the lifetime is `'static`
    // because libwebp owns the buffer.
    _marker: PhantomData<&'a mut [u8]>,
}

// SAFETY: The WebPIDecoder is internally thread-safe for single-threaded access
#[cfg(feature = "decode")]
unsafe impl Send for StreamingDecoder<'_> {}

#[cfg(feature = "decode")]
impl StreamingDecoder<'static> {
    /// Create a new streaming decoder.
    ///
    /// libwebp allocates and owns the output buffer for this constructor;
    /// the returned decoder has no lifetime constraints on the caller.
    ///
    /// # Arguments
    ///
    /// * `color_mode` - Output color format (RGBA, RGB, etc.). YUV modes
    ///   ([`ColorMode::Yuv420`], [`ColorMode::Yuva420`]) are rejected with
    ///   [`Error::InvalidInput`] — `WebPINewRGB` only constructs RGB-family
    ///   decoders. Use the static [`crate::decode_yuv`] entry point for YUV
    ///   output instead.
    pub fn new(color_mode: ColorMode) -> Result<Self> {
        let csp_mode = match color_mode {
            ColorMode::Rgba => libwebp_sys::WEBP_CSP_MODE::MODE_RGBA,
            ColorMode::Bgra => libwebp_sys::WEBP_CSP_MODE::MODE_BGRA,
            ColorMode::Argb => libwebp_sys::WEBP_CSP_MODE::MODE_ARGB,
            ColorMode::Rgb => libwebp_sys::WEBP_CSP_MODE::MODE_RGB,
            ColorMode::Bgr => libwebp_sys::WEBP_CSP_MODE::MODE_BGR,
            ColorMode::Yuv420 | ColorMode::Yuva420 => {
                return Err(at!(Error::InvalidInput(
                    "StreamingDecoder does not support YUV output; use webpx::decode_yuv".into(),
                )));
            }
        };

        let decoder = unsafe {
            libwebp_sys::WebPINewRGB(
                csp_mode,
                ptr::null_mut(), // Let decoder allocate output
                0,
                0,
            )
        };

        if decoder.is_null() {
            return Err(at!(Error::OutOfMemory));
        }

        Ok(Self {
            decoder,
            color_mode,
            width: 0,
            height: 0,
            last_y: 0,
            _marker: PhantomData,
        })
    }
}

#[cfg(feature = "decode")]
impl<'a> StreamingDecoder<'a> {
    /// Create a streaming decoder with a pre-allocated output buffer.
    ///
    /// The decoder borrows `output_buffer` for its entire lifetime — libwebp
    /// stores the raw pointer internally and writes into it on every
    /// `append` / `update` / `get_partial` / `finish` call. The lifetime
    /// parameter ties the returned decoder to the buffer so the borrow
    /// checker rejects code that drops the buffer before the decoder.
    ///
    /// # Arguments
    ///
    /// * `output_buffer` - Pre-allocated buffer for decoded pixels
    /// * `stride` - Row stride in bytes
    /// * `color_mode` - Output color format
    pub fn with_buffer(
        output_buffer: &'a mut [u8],
        stride: usize,
        color_mode: ColorMode,
    ) -> Result<Self> {
        let csp_mode = match color_mode {
            ColorMode::Rgba => libwebp_sys::WEBP_CSP_MODE::MODE_RGBA,
            ColorMode::Bgra => libwebp_sys::WEBP_CSP_MODE::MODE_BGRA,
            ColorMode::Argb => libwebp_sys::WEBP_CSP_MODE::MODE_ARGB,
            ColorMode::Rgb => libwebp_sys::WEBP_CSP_MODE::MODE_RGB,
            ColorMode::Bgr => libwebp_sys::WEBP_CSP_MODE::MODE_BGR,
            _ => {
                return Err(at!(Error::InvalidInput(
                    "YUV requires separate plane buffers".into(),
                )));
            }
        };
        // Reject stride values that would wrap to a negative i32 when
        // cast for libwebp's `output_stride` parameter. libwebp's row
        // pointer arithmetic uses the signed value, so a wrapped-negative
        // stride would write to addresses *before* `output_buffer`.
        if stride > i32::MAX as usize {
            return Err(at!(Error::InvalidInput(alloc::format!(
                "stride too large for libwebp i32 parameter: {} (max {})",
                stride,
                i32::MAX
            ))));
        }

        let decoder = unsafe {
            libwebp_sys::WebPINewRGB(
                csp_mode,
                output_buffer.as_mut_ptr(),
                output_buffer.len(),
                stride as i32,
            )
        };

        if decoder.is_null() {
            return Err(at!(Error::OutOfMemory));
        }

        Ok(Self {
            decoder,
            color_mode,
            width: 0,
            height: 0,
            last_y: 0,
            _marker: PhantomData,
        })
    }

    /// Append data to the decoder and continue decoding.
    ///
    /// Returns the decode status indicating whether more data is needed
    /// or decoding is complete.
    pub fn append(&mut self, data: &[u8]) -> Result<DecodeStatus> {
        let status = unsafe { libwebp_sys::WebPIAppend(self.decoder, data.as_ptr(), data.len()) };
        self.process_status(status)
    }

    /// Process the VP8 status code and update internal state.
    fn process_status(&mut self, status: libwebp_sys::VP8StatusCode) -> Result<DecodeStatus> {
        match status {
            libwebp_sys::VP8StatusCode::VP8_STATUS_OK => {
                // Decode complete - update dimensions
                self.update_dimensions();
                Ok(DecodeStatus::Complete)
            }
            libwebp_sys::VP8StatusCode::VP8_STATUS_SUSPENDED => {
                // In progress - update dimensions and check rows
                self.update_dimensions();

                if self.last_y > 0 {
                    Ok(DecodeStatus::Partial(self.last_y as u32))
                } else {
                    Ok(DecodeStatus::NeedMoreData)
                }
            }
            _ => Err(at!(Error::DecodeFailed(DecodingError::from(status as i32)))),
        }
    }

    /// Update cached dimensions from the decoder.
    fn update_dimensions(&mut self) {
        let mut last_y = 0i32;
        let mut width = 0i32;
        let mut height = 0i32;

        unsafe {
            libwebp_sys::WebPIDecGetRGB(
                self.decoder,
                &mut last_y,
                &mut width,
                &mut height,
                ptr::null_mut(),
            );
        }

        self.width = width;
        self.height = height;
        self.last_y = last_y;
    }

    /// Update decoder with complete data (alternative to append for non-streaming).
    ///
    /// Unlike `append`, this expects the data to be the complete input or
    /// a complete prefix of it (not just a new chunk).
    pub fn update(&mut self, data: &[u8]) -> Result<DecodeStatus> {
        let status = unsafe { libwebp_sys::WebPIUpdate(self.decoder, data.as_ptr(), data.len()) };
        self.process_status(status)
    }

    /// Get the current image dimensions (available after some data is decoded).
    pub fn dimensions(&self) -> Option<(u32, u32)> {
        if self.width > 0 && self.height > 0 {
            Some((self.width as u32, self.height as u32))
        } else {
            None
        }
    }

    /// Get the number of decoded rows so far.
    pub fn decoded_rows(&self) -> u32 {
        self.last_y.max(0) as u32
    }

    /// Get partial decoded data (rows decoded so far).
    ///
    /// Returns a slice to the internally allocated buffer.
    /// Only valid while the decoder is alive.
    pub fn get_partial(&self) -> Option<(&[u8], u32, u32)> {
        if self.last_y <= 0 || self.width <= 0 {
            return None;
        }

        let mut last_y = 0i32;
        let mut width = 0i32;
        let mut height = 0i32;
        let mut stride = 0i32;

        let ptr = unsafe {
            libwebp_sys::WebPIDecGetRGB(
                self.decoder,
                &mut last_y,
                &mut width,
                &mut height,
                &mut stride,
            )
        };

        // Reject negative dims/stride so `as usize` cannot wrap into a huge
        // value that bypasses libwebp's allocation bounds.
        if ptr.is_null() || last_y <= 0 || stride <= 0 || width <= 0 {
            return None;
        }

        let bpp = self.color_mode.bytes_per_pixel().unwrap_or(4);
        let row_bytes = (width as usize).checked_mul(bpp)?;
        let stride = stride as usize;
        if stride < row_bytes {
            return None;
        }
        let decoded_rows = last_y as usize;
        let size = decoded_rows
            .checked_sub(1)?
            .checked_mul(stride)?
            .checked_add(row_bytes)?;
        if size > isize::MAX as usize {
            return None;
        }

        let data = unsafe { core::slice::from_raw_parts(ptr, size) };

        Some((data, width as u32, last_y as u32))
    }

    /// Finish decoding and return the complete image.
    ///
    /// Returns an error if decoding is not complete.
    pub fn finish(self) -> Result<(Vec<u8>, u32, u32)> {
        let mut last_y = 0i32;
        let mut width = 0i32;
        let mut height = 0i32;
        let mut stride = 0i32;

        let ptr = unsafe {
            libwebp_sys::WebPIDecGetRGB(
                self.decoder,
                &mut last_y,
                &mut width,
                &mut height,
                &mut stride,
            )
        };

        if ptr.is_null() || last_y < height || stride <= 0 || width <= 0 || height <= 0 {
            return Err(at!(Error::NeedMoreData));
        }

        let bpp = self.color_mode.bytes_per_pixel().unwrap_or(4);

        // Copy to contiguous buffer (stride may differ from width * bpp).
        // saturating_mul guards 32-bit usize against unexpectedly large
        // libwebp-returned strides.
        let total = (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(bpp);
        let mut result = Vec::with_capacity(total);

        let row_bytes = (width as usize).saturating_mul(bpp);
        for y in 0..height {
            let row_start = (y as usize).saturating_mul(stride as usize);
            // ptr.add requires the offset to fit in isize; the guard here
            // matches what libwebp's allocation guarantees for its own
            // returned (stride, height) pair.
            if row_start > isize::MAX as usize {
                return Err(at!(Error::DecodeFailed(DecodingError::BitstreamError)));
            }
            let row_data = unsafe { core::slice::from_raw_parts(ptr.add(row_start), row_bytes) };
            result.extend_from_slice(row_data);
        }

        Ok((result, width as u32, height as u32))
    }
}

#[cfg(feature = "decode")]
impl Drop for StreamingDecoder<'_> {
    fn drop(&mut self) {
        if !self.decoder.is_null() {
            unsafe {
                libwebp_sys::WebPIDelete(self.decoder);
            }
        }
    }
}

/// Streaming WebP encoder.
///
/// Note: libwebp doesn't have a true streaming encoder API like the decoder.
/// This provides a callback-based interface for output streaming.
///
/// # Example
///
/// ```rust,no_run
/// use webpx::StreamingEncoder;
///
/// let rgba_data = vec![0u8; 640 * 480 * 4];
/// let mut output = Vec::new();
///
/// let mut encoder = StreamingEncoder::new(640, 480)?;
/// encoder.set_quality(85.0);
///
/// // Encode with callback for output chunks
/// encoder.encode_rgba_with_callback(&rgba_data, |chunk| {
///     // Write chunk to file/network
///     output.extend_from_slice(chunk);
///     Ok(())
/// })?;
/// # Ok::<(), webpx::At<webpx::Error>>(())
/// ```
#[cfg(feature = "encode")]
pub struct StreamingEncoder {
    width: u32,
    height: u32,
    config: crate::config::EncoderConfig,
}

#[cfg(feature = "encode")]
impl StreamingEncoder {
    /// Create a new streaming encoder.
    pub fn new(width: u32, height: u32) -> Result<Self> {
        if width == 0 || height == 0 || width > 16383 || height > 16383 {
            return Err(at!(Error::InvalidInput("invalid dimensions".into())));
        }

        Ok(Self {
            width,
            height,
            config: crate::config::EncoderConfig::default(),
        })
    }

    /// Set encoding quality (0.0 = smallest, 100.0 = best).
    pub fn set_quality(&mut self, quality: f32) {
        self.config.quality = quality;
    }

    /// Set content-aware preset.
    pub fn set_preset(&mut self, preset: crate::config::Preset) {
        self.config.preset = preset;
    }

    /// Enable lossless compression.
    pub fn set_lossless(&mut self, lossless: bool) {
        self.config.lossless = lossless;
    }

    /// Encode RGBA data with a callback for output chunks.
    ///
    /// The callback is called with encoded data chunks as they're produced.
    pub fn encode_rgba_with_callback<F>(&self, data: &[u8], mut callback: F) -> Result<()>
    where
        F: FnMut(&[u8]) -> Result<()>,
    {
        let expected = (self.width as usize)
            .saturating_mul(self.height as usize)
            .saturating_mul(4);
        if data.len() < expected {
            return Err(at!(Error::InvalidInput("buffer too small".into())));
        }

        let webp_config = self.config.to_libwebp()?;

        let mut picture = libwebp_sys::WebPPicture::new()
            .map_err(|_| at!(Error::InvalidConfig("failed to init picture".into())))?;

        picture.width = self.width as i32;
        picture.height = self.height as i32;
        picture.use_argb = 1;

        let import_ok = unsafe {
            libwebp_sys::WebPPictureImportRGBA(&mut picture, data.as_ptr(), (self.width * 4) as i32)
        };

        if import_ok == 0 {
            unsafe { libwebp_sys::WebPPictureFree(&mut picture) };
            return Err(at!(Error::OutOfMemory));
        }

        // Use a custom writer that calls our callback
        struct CallbackContext<'a, F: FnMut(&[u8]) -> Result<()>> {
            callback: &'a mut F,
            error: Option<whereat::At<Error>>,
        }

        extern "C" fn write_callback<F: FnMut(&[u8]) -> Result<()>>(
            data: *const u8,
            data_size: usize,
            picture: *const libwebp_sys::WebPPicture,
        ) -> i32 {
            let ctx = unsafe { &mut *((*picture).custom_ptr as *mut CallbackContext<F>) };

            let slice = unsafe { core::slice::from_raw_parts(data, data_size) };

            match (ctx.callback)(slice) {
                Ok(()) => 1,
                Err(e) => {
                    ctx.error = Some(e);
                    0
                }
            }
        }

        let mut ctx = CallbackContext {
            callback: &mut callback,
            error: None,
        };

        picture.writer = Some(write_callback::<F>);
        picture.custom_ptr = &mut ctx as *mut _ as *mut _;

        let ok = unsafe { libwebp_sys::WebPEncode(&webp_config, &mut picture) };

        unsafe { libwebp_sys::WebPPictureFree(&mut picture) };

        if let Some(e) = ctx.error {
            return Err(e);
        }

        if ok == 0 {
            return Err(at!(Error::EncodeFailed(crate::error::EncodingError::from(
                picture.error_code as i32,
            ))));
        }

        Ok(())
    }

    /// Encode RGB data (no alpha) with a callback for output chunks.
    pub fn encode_rgb_with_callback<F>(&self, data: &[u8], mut callback: F) -> Result<()>
    where
        F: FnMut(&[u8]) -> Result<()>,
    {
        let expected = (self.width as usize)
            .saturating_mul(self.height as usize)
            .saturating_mul(3);
        if data.len() < expected {
            return Err(at!(Error::InvalidInput("buffer too small".into())));
        }

        let webp_config = self.config.to_libwebp()?;

        let mut picture = libwebp_sys::WebPPicture::new()
            .map_err(|_| at!(Error::InvalidConfig("failed to init picture".into())))?;

        picture.width = self.width as i32;
        picture.height = self.height as i32;
        picture.use_argb = 1;

        let import_ok = unsafe {
            libwebp_sys::WebPPictureImportRGB(&mut picture, data.as_ptr(), (self.width * 3) as i32)
        };

        if import_ok == 0 {
            unsafe { libwebp_sys::WebPPictureFree(&mut picture) };
            return Err(at!(Error::OutOfMemory));
        }

        // Use memory writer and send all at once for simplicity
        // (libwebp doesn't truly stream the output)
        let mut writer = core::mem::MaybeUninit::<libwebp_sys::WebPMemoryWriter>::zeroed();
        unsafe { libwebp_sys::WebPMemoryWriterInit(writer.as_mut_ptr()) };
        let mut writer = unsafe { writer.assume_init() };

        picture.writer = Some(libwebp_sys::WebPMemoryWrite);
        picture.custom_ptr = &mut writer as *mut _ as *mut _;

        let ok = unsafe { libwebp_sys::WebPEncode(&webp_config, &mut picture) };

        if ok == 0 {
            let error = crate::error::EncodingError::from(picture.error_code as i32);
            unsafe {
                libwebp_sys::WebPPictureFree(&mut picture);
                libwebp_sys::WebPMemoryWriterClear(&mut writer);
            }
            return Err(at!(Error::EncodeFailed(error)));
        }

        let result = unsafe {
            let slice = core::slice::from_raw_parts(writer.mem, writer.size);
            callback(slice)
        };

        unsafe {
            libwebp_sys::WebPPictureFree(&mut picture);
            libwebp_sys::WebPMemoryWriterClear(&mut writer);
        }

        result
    }
}

#[cfg(all(test, feature = "decode", feature = "encode"))]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_decoder_creation() {
        let decoder = StreamingDecoder::new(ColorMode::Rgba);
        assert!(decoder.is_ok());
    }

    #[test]
    fn test_streaming_encoder_creation() {
        let encoder = StreamingEncoder::new(640, 480);
        assert!(encoder.is_ok());

        // Invalid dimensions
        assert!(StreamingEncoder::new(0, 480).is_err());
        assert!(StreamingEncoder::new(640, 0).is_err());
        assert!(StreamingEncoder::new(20000, 480).is_err());
    }
}
