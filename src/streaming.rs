//! Streaming/incremental WebP decode and encode.

use crate::error::{DecodingError, Error, Result};
#[cfg(feature = "encode")]
use crate::ffi::mem_writer::MemWriter;
#[cfg(feature = "encode")]
use crate::ffi::picture::Picture;
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
    /// Resource policy applied to `append`/`update`. Defaults to
    /// [`crate::Limits::default`]; replace via [`Self::limits`].
    limits: crate::Limits,
    /// Cumulative bytes fed in, checked against `max_input_bytes`.
    appended_bytes: u64,
    /// Set once the declared canvas dimensions have been validated.
    dims_checked: bool,
    /// Stream prefix kept until the header parses (freed afterwards).
    /// Canvas dimensions live in the first ~30 bytes for every WebP
    /// layout, so this stays tiny; 4 KiB is a defensive ceiling.
    header_stash: Vec<u8>,
    // Ties the decoder's lifetime to the caller-supplied output buffer
    // when `with_buffer` is used. For `new()` the lifetime is `'static`
    // because libwebp owns the buffer.
    _marker: PhantomData<&'a mut [u8]>,
}

// SAFETY: The WebPIDecoder is internally thread-safe for single-threaded access
#[cfg(feature = "decode")]
unsafe impl Send for StreamingDecoder<'_> {}

/// Extract the declared canvas dimensions from a stream prefix, if the
/// header has arrived.
///
/// VP8X is handled with a direct fixed-offset parse: the spec requires
/// VP8X to be the first chunk after the RIFF header, and the canvas size
/// sits in its last six bytes — so 30 bytes of prefix always suffice,
/// even when the actual image chunk is megabytes downstream behind
/// metadata chunks (`WebPGetFeatures` reports "not enough data" for such
/// prefixes). Everything else (VP8-first, VP8L-first, headerless raw
/// streams) resolves via [`crate::ImageInfo::from_webp`] within the
/// first ~30 bytes as well.
#[cfg(feature = "decode")]
fn probe_canvas_dims(prefix: &[u8]) -> Option<(u32, u32)> {
    if prefix.len() >= 30 && &prefix[0..4] == b"RIFF" && &prefix[8..16] == b"WEBPVP8X" {
        let w = 1 + u32::from_le_bytes([prefix[24], prefix[25], prefix[26], 0]);
        let h = 1 + u32::from_le_bytes([prefix[27], prefix[28], prefix[29], 0]);
        return Some((w, h));
    }
    crate::ImageInfo::from_webp(prefix)
        .ok()
        .map(|info| (info.width, info.height))
}

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
            limits: crate::Limits::default(),
            appended_bytes: 0,
            dims_checked: false,
            header_stash: Vec::new(),
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
        let stride_i32 =
            crate::ffi::validate::stride_fits_i32(stride, "StreamingDecoder::with_buffer")?;

        let decoder = unsafe {
            libwebp_sys::WebPINewRGB(
                csp_mode,
                output_buffer.as_mut_ptr(),
                output_buffer.len(),
                stride_i32,
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
            limits: crate::Limits::default(),
            appended_bytes: 0,
            dims_checked: false,
            header_stash: Vec::new(),
            _marker: PhantomData,
        })
    }

    /// Replace the resource-limit policy.
    ///
    /// Defaults to [`crate::Limits::default`] — production caps suited
    /// to untrusted input. Pass [`crate::Limits::none`] for trusted
    /// streams. Applies to subsequent [`Self::append`] / [`Self::update`]
    /// calls: `max_input_bytes` is checked against the cumulative bytes
    /// fed in, and the dimension/pixel caps are checked once, when the
    /// bitstream header first parses (before libwebp allocates the
    /// canvas). Frame-count and animation-duration budgets do not apply
    /// here — the incremental decoder handles still images only; use
    /// [`crate::AnimationDecoder`] for animations.
    #[must_use]
    pub fn limits(mut self, limits: crate::Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Append data to the decoder and continue decoding.
    ///
    /// Returns the decode status indicating whether more data is needed
    /// or decoding is complete.
    ///
    /// Enforces the decoder's [`crate::Limits`] (default:
    /// [`crate::Limits::default`]; see [`Self::limits`]).
    pub fn append(&mut self, data: &[u8]) -> Result<DecodeStatus> {
        self.check_append_limits(data)?;
        let status = unsafe { libwebp_sys::WebPIAppend(self.decoder, data.as_ptr(), data.len()) };
        self.process_status(status)
    }

    /// Limits gate run before every chunk is handed to libwebp.
    ///
    /// Dimension caps must fire *before* `WebPIAppend` completes the
    /// header parse, because that is the moment libwebp allocates the
    /// whole-canvas output buffer (for the `new()` constructor). The
    /// stash mirrors the stream prefix so the declared dimensions can be
    /// validated first: for VP8X files the canvas size sits at fixed
    /// offsets in the first 30 bytes (the image chunk itself may be
    /// megabytes away, behind ICCP/EXIF chunks, so `WebPGetFeatures`
    /// alone would report "not enough data" right up until the same call
    /// that allocates); for VP8/VP8L-first layouts `ImageInfo::from_webp`
    /// resolves within the first ~30 bytes too.
    fn check_append_limits(&mut self, data: &[u8]) -> Result<()> {
        self.appended_bytes = self.appended_bytes.saturating_add(data.len() as u64);
        if let Err(e) = self.limits.check_input_size(self.appended_bytes) {
            return Err(at!(Error::LimitExceeded(e)));
        }
        if self.dims_checked || !self.limits.has_any() {
            return Ok(());
        }
        const STASH_CAP: usize = 4096;
        if self.header_stash.len() < STASH_CAP {
            let take = data.len().min(STASH_CAP - self.header_stash.len());
            self.header_stash.extend_from_slice(&data[..take]);
        }
        if let Some((width, height)) = probe_canvas_dims(&self.header_stash) {
            if let Err(e) = self.limits.check_still_image(width, height) {
                return Err(at!(Error::LimitExceeded(e)));
            }
            self.dims_checked = true;
            self.header_stash = Vec::new();
        }
        Ok(())
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
    ///
    /// # Implementation note
    ///
    /// Earlier webpx versions (≤ 0.3.3) called libwebp's `WebPIUpdate`
    /// here. That function keeps a raw pointer to the input buffer and
    /// re-reads it on subsequent calls — but our `&[u8]` parameter
    /// doesn't outlive the call, so a follow-up `update` / `finish` /
    /// `get_partial` would read freed memory (use-after-free).
    ///
    /// The borrow checker did not catch this because the input
    /// lifetime is not tied to the decoder. Reachable from safe Rust
    /// without an `unsafe` block. Routed to [`Self::append`] (which
    /// makes libwebp copy the data) to close the UAF.
    ///
    /// Note that `Update` is documented by libwebp as "data buffer is
    /// not copied to the internal memory" — webpx never re-exposes
    /// that contract through a sound lifetime, so functional behavior
    /// when called with the complete input in a single call is
    /// unchanged.
    pub fn update(&mut self, data: &[u8]) -> Result<DecodeStatus> {
        // Append (which copies) instead of Update (which retains the
        // raw pointer). The semantic difference is a memcpy — for
        // single-call uses (the only sound use of `update`) the cost
        // is irrelevant.
        self.append(data)
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

        // Use webpx's zeroed RAII wrapper, not
        // `libwebp_sys::WebPPicture::new()`: the generated helper starts from
        // uninitialized memory, but bindgen exposes libwebp's reserved fields
        // as normal Rust fields. If libwebp leaves any of those fields
        // untouched, `assume_init` would construct an invalid Rust value.
        let mut picture = Picture::new()?;
        picture.inner_mut().width = self.width as i32;
        picture.inner_mut().height = self.height as i32;
        picture.inner_mut().use_argb = 1;

        let import_ok = unsafe {
            libwebp_sys::WebPPictureImportRGBA(
                picture.as_mut_ptr(),
                data.as_ptr(),
                (self.width * 4) as i32,
            )
        };

        if import_ok == 0 {
            return Err(at!(Error::OutOfMemory));
        }

        // Use a custom writer that calls our callback. Captures any
        // panic from the user-supplied callback so the panic doesn't
        // unwind through libwebp's C frames (UB) — re-raised after
        // `WebPEncode` returns.
        struct CallbackContext<'a, F: FnMut(&[u8]) -> Result<()>> {
            callback: &'a mut F,
            error: Option<whereat::At<Error>>,
            #[cfg(feature = "std")]
            panic: core::cell::Cell<Option<alloc::boxed::Box<dyn core::any::Any + Send + 'static>>>,
        }

        extern "C" fn write_callback<F: FnMut(&[u8]) -> Result<()>>(
            data: *const u8,
            data_size: usize,
            picture: *const libwebp_sys::WebPPicture,
        ) -> i32 {
            let ctx = unsafe { &mut *((*picture).custom_ptr as *mut CallbackContext<F>) };

            // libwebp normally provides a non-null pointer for non-empty
            // chunks. Guard the empty/null case anyway so we never build a
            // Rust slice from a null raw pointer.
            let slice: &[u8] = if data.is_null() || data_size == 0 {
                &[]
            } else {
                unsafe { core::slice::from_raw_parts(data, data_size) }
            };

            #[cfg(feature = "std")]
            {
                // SAFETY: `&mut F` here is type-erased into a raw pointer
                // through `picture.custom_ptr`. We re-acquire it as
                // `&mut F` above; re-borrowing inside the closure is
                // sound because we don't reuse `ctx.callback` outside
                // the closure body.
                let cb = &mut ctx.callback;
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (cb)(slice))) {
                    Ok(Ok(())) => 1,
                    Ok(Err(e)) => {
                        ctx.error = Some(e);
                        0
                    }
                    Err(payload) => {
                        ctx.panic.set(Some(payload));
                        0
                    }
                }
            }
            #[cfg(not(feature = "std"))]
            {
                match (ctx.callback)(slice) {
                    Ok(()) => 1,
                    Err(e) => {
                        ctx.error = Some(e);
                        0
                    }
                }
            }
        }

        let mut ctx = CallbackContext {
            callback: &mut callback,
            error: None,
            #[cfg(feature = "std")]
            panic: core::cell::Cell::new(None),
        };

        picture.inner_mut().writer = Some(write_callback::<F>);
        picture.inner_mut().custom_ptr = &mut ctx as *mut _ as *mut _;

        let ok = unsafe { libwebp_sys::WebPEncode(&webp_config, picture.as_mut_ptr()) };

        // Re-raise any panic captured from the user's callback before
        // we dispatch on the encode result. The catch_unwind guard
        // inside `write_callback` ensures the panic doesn't cross the
        // libwebp C frame; we propagate it here from a Rust frame.
        #[cfg(feature = "std")]
        if let Some(payload) = ctx.panic.take() {
            std::panic::resume_unwind(payload);
        }

        if let Some(e) = ctx.error {
            return Err(e);
        }

        if ok == 0 {
            let error = crate::error::EncodingError::from(picture.inner_mut().error_code as i32);
            return Err(at!(Error::EncodeFailed(error)));
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

        // Same initializedness invariant as the RGBA callback path above:
        // avoid the generated `WebPPicture::new()` helper and keep all
        // libwebp-reserved fields zeroed before C initialization.
        let mut picture = Picture::new()?;
        picture.inner_mut().width = self.width as i32;
        picture.inner_mut().height = self.height as i32;
        picture.inner_mut().use_argb = 1;

        let import_ok = unsafe {
            libwebp_sys::WebPPictureImportRGB(
                picture.as_mut_ptr(),
                data.as_ptr(),
                (self.width * 3) as i32,
            )
        };

        if import_ok == 0 {
            return Err(at!(Error::OutOfMemory));
        }

        // Use the zeroed RAII writer wrapper for the same reason as
        // `Picture`: the generated C initializer may leave reserved fields
        // alone, and Drop must always clear libwebp's allocation on errors.
        // We send the encoded bytes all at once because libwebp doesn't truly
        // stream encoder output.
        let mut writer = MemWriter::new();

        picture.inner_mut().writer = Some(libwebp_sys::WebPMemoryWrite);
        picture.inner_mut().custom_ptr = writer.as_mut_ptr() as *mut _;

        let ok = unsafe { libwebp_sys::WebPEncode(&webp_config, picture.as_mut_ptr()) };

        if ok == 0 {
            let error = crate::error::EncodingError::from(picture.inner_mut().error_code as i32);
            return Err(at!(Error::EncodeFailed(error)));
        }

        let encoded = writer.to_vec();
        callback(&encoded)
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
