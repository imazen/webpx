//! WebP encoding functionality.

use crate::config::{EncoderConfig, Preset};
use crate::error::{EncodingError, Error, Result};
use crate::types::YuvPlanesRef;
use alloc::vec::Vec;
use core::ptr;
use imgref::ImgRef;
use rgb::{RGB8, RGBA8};

/// Encode RGBA pixels to WebP.
///
/// # Arguments
///
/// * `data` - RGBA pixel data (4 bytes per pixel)
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
/// * `quality` - Quality factor (0.0 = smallest, 100.0 = best)
///
/// # Example
///
/// ```rust,no_run
/// let rgba: &[u8] = &[0u8; 640 * 480 * 4]; // placeholder
/// let webp = webpx::encode_rgba(rgba, 640, 480, 85.0)?;
/// # Ok::<(), webpx::Error>(())
/// ```
pub fn encode_rgba(data: &[u8], width: u32, height: u32, quality: f32) -> Result<Vec<u8>> {
    validate_dimensions(width, height)?;
    validate_buffer_size(data.len(), width, height, 4)?;

    let mut output: *mut u8 = ptr::null_mut();
    let size = unsafe {
        libwebp_sys::WebPEncodeRGBA(
            data.as_ptr(),
            width as i32,
            height as i32,
            (width * 4) as i32, // stride
            quality,
            &mut output,
        )
    };

    if size == 0 || output.is_null() {
        return Err(Error::EncodeFailed(EncodingError::OutOfMemory));
    }

    let result = unsafe {
        let slice = core::slice::from_raw_parts(output, size);
        let vec = slice.to_vec();
        libwebp_sys::WebPFree(output as *mut _);
        vec
    };

    Ok(result)
}

/// Encode RGB pixels to WebP (no alpha).
///
/// # Arguments
///
/// * `data` - RGB pixel data (3 bytes per pixel)
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
/// * `quality` - Quality factor (0.0 = smallest, 100.0 = best)
pub fn encode_rgb(data: &[u8], width: u32, height: u32, quality: f32) -> Result<Vec<u8>> {
    validate_dimensions(width, height)?;
    validate_buffer_size(data.len(), width, height, 3)?;

    let mut output: *mut u8 = ptr::null_mut();
    let size = unsafe {
        libwebp_sys::WebPEncodeRGB(
            data.as_ptr(),
            width as i32,
            height as i32,
            (width * 3) as i32, // stride
            quality,
            &mut output,
        )
    };

    if size == 0 || output.is_null() {
        return Err(Error::EncodeFailed(EncodingError::OutOfMemory));
    }

    let result = unsafe {
        let slice = core::slice::from_raw_parts(output, size);
        let vec = slice.to_vec();
        libwebp_sys::WebPFree(output as *mut _);
        vec
    };

    Ok(result)
}

/// Encode to lossless WebP.
///
/// Lossless encoding preserves all pixel values exactly.
///
/// # Arguments
///
/// * `data` - RGBA pixel data (4 bytes per pixel)
/// * `width` - Image width in pixels
/// * `height` - Image height in pixels
pub fn encode_lossless(data: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    validate_dimensions(width, height)?;
    validate_buffer_size(data.len(), width, height, 4)?;

    let mut output: *mut u8 = ptr::null_mut();
    let size = unsafe {
        libwebp_sys::WebPEncodeLosslessRGBA(
            data.as_ptr(),
            width as i32,
            height as i32,
            (width * 4) as i32,
            &mut output,
        )
    };

    if size == 0 || output.is_null() {
        return Err(Error::EncodeFailed(EncodingError::OutOfMemory));
    }

    let result = unsafe {
        let slice = core::slice::from_raw_parts(output, size);
        let vec = slice.to_vec();
        libwebp_sys::WebPFree(output as *mut _);
        vec
    };

    Ok(result)
}

/// Internal: Encode with full config (called by EncoderConfig).
pub(crate) fn encode_with_config(
    data: &[u8],
    width: u32,
    height: u32,
    bpp: u8,
    config: &EncoderConfig,
) -> Result<Vec<u8>> {
    validate_dimensions(width, height)?;
    validate_buffer_size(data.len(), width, height, bpp as u32)?;

    let webp_config = config.to_libwebp()?;

    // Initialize picture
    let mut picture = libwebp_sys::WebPPicture::new()
        .map_err(|_| Error::InvalidConfig("failed to init picture".into()))?;

    picture.width = width as i32;
    picture.height = height as i32;
    picture.use_argb = 1;

    // Import pixel data
    let import_ok = if bpp == 4 {
        unsafe {
            libwebp_sys::WebPPictureImportRGBA(&mut picture, data.as_ptr(), (width * 4) as i32)
        }
    } else {
        unsafe {
            libwebp_sys::WebPPictureImportRGB(&mut picture, data.as_ptr(), (width * 3) as i32)
        }
    };

    if import_ok == 0 {
        unsafe { libwebp_sys::WebPPictureFree(&mut picture) };
        return Err(Error::EncodeFailed(EncodingError::OutOfMemory));
    }

    // Setup memory writer
    let mut writer = core::mem::MaybeUninit::<libwebp_sys::WebPMemoryWriter>::uninit();
    unsafe { libwebp_sys::WebPMemoryWriterInit(writer.as_mut_ptr()) };
    let mut writer = unsafe { writer.assume_init() };

    picture.writer = Some(libwebp_sys::WebPMemoryWrite);
    picture.custom_ptr = &mut writer as *mut _ as *mut _;

    // Encode
    let ok = unsafe { libwebp_sys::WebPEncode(&webp_config, &mut picture) };

    let result = if ok == 0 {
        let error = EncodingError::from(picture.error_code as i32);
        unsafe {
            libwebp_sys::WebPPictureFree(&mut picture);
            libwebp_sys::WebPMemoryWriterClear(&mut writer);
        }
        Err(Error::EncodeFailed(error))
    } else {
        let webp_data = unsafe {
            let slice = core::slice::from_raw_parts(writer.mem, writer.size);
            slice.to_vec()
        };
        unsafe {
            libwebp_sys::WebPPictureFree(&mut picture);
            libwebp_sys::WebPMemoryWriterClear(&mut writer);
        }
        Ok(webp_data)
    };

    // Embed metadata if present
    #[cfg(feature = "icc")]
    if let Ok(mut webp_data) = result {
        if let Some(ref icc) = config.icc_profile {
            webp_data = crate::mux::embed_icc(&webp_data, icc)?;
        }
        if let Some(ref exif) = config.exif_data {
            webp_data = crate::mux::embed_exif(&webp_data, exif)?;
        }
        if let Some(ref xmp) = config.xmp_data {
            webp_data = crate::mux::embed_xmp(&webp_data, xmp)?;
        }
        return Ok(webp_data);
    }

    result
}

/// WebP encoder with full configuration options.
///
/// This is a convenience wrapper around [`EncoderConfig`]. For new code,
/// prefer using `EncoderConfig` directly for its cleaner API.
///
/// # Example
///
/// ```rust,no_run
/// use webpx::{Encoder, Preset};
///
/// let rgba: &[u8] = &[0u8; 640 * 480 * 4]; // placeholder
/// let webp = Encoder::new(rgba, 640, 480)
///     .preset(Preset::Photo)
///     .quality(85.0)
///     .encode()?;
/// # Ok::<(), webpx::Error>(())
/// ```
pub struct Encoder<'a> {
    data: EncoderInput<'a>,
    width: u32,
    height: u32,
    config: EncoderConfig,
    #[cfg(feature = "icc")]
    icc_profile: Option<&'a [u8]>,
}

enum EncoderInput<'a> {
    Rgba(&'a [u8]),
    Rgb(&'a [u8]),
    Yuv(YuvPlanesRef<'a>),
}

impl<'a> Encoder<'a> {
    /// Create a new encoder for RGBA data.
    #[must_use]
    pub fn new(data: &'a [u8], width: u32, height: u32) -> Self {
        Self {
            data: EncoderInput::Rgba(data),
            width,
            height,
            config: EncoderConfig::default(),
            #[cfg(feature = "icc")]
            icc_profile: None,
        }
    }

    /// Create a new encoder for RGB data (no alpha).
    #[must_use]
    pub fn new_rgb(data: &'a [u8], width: u32, height: u32) -> Self {
        Self {
            data: EncoderInput::Rgb(data),
            width,
            height,
            config: EncoderConfig::default(),
            #[cfg(feature = "icc")]
            icc_profile: None,
        }
    }

    /// Create a new encoder for YUV planar data.
    #[must_use]
    pub fn new_yuv(planes: YuvPlanesRef<'a>) -> Self {
        let width = planes.width;
        let height = planes.height;
        Self {
            data: EncoderInput::Yuv(planes),
            width,
            height,
            config: EncoderConfig::default(),
            #[cfg(feature = "icc")]
            icc_profile: None,
        }
    }

    /// Create encoder from an imgref ImgRef<RGBA8>.
    #[must_use]
    pub fn from_rgba(img: ImgRef<'a, RGBA8>) -> Self {
        // SAFETY: RGBA8 is repr(C) and has the same layout as [u8; 4]
        let data = unsafe {
            core::slice::from_raw_parts(img.buf().as_ptr() as *const u8, img.buf().len() * 4)
        };
        Self::new(data, img.width() as u32, img.height() as u32)
    }

    /// Create encoder from an imgref ImgRef<RGB8>.
    #[must_use]
    pub fn from_rgb(img: ImgRef<'a, RGB8>) -> Self {
        // SAFETY: RGB8 is repr(C) and has the same layout as [u8; 3]
        let data = unsafe {
            core::slice::from_raw_parts(img.buf().as_ptr() as *const u8, img.buf().len() * 3)
        };
        Self::new_rgb(data, img.width() as u32, img.height() as u32)
    }

    /// Set encoding quality (0.0 = smallest, 100.0 = best).
    #[must_use]
    pub fn quality(mut self, quality: f32) -> Self {
        self.config = self.config.quality(quality);
        self
    }

    /// Set content-aware preset.
    #[must_use]
    pub fn preset(mut self, preset: Preset) -> Self {
        self.config = self.config.preset(preset);
        self
    }

    /// Enable lossless compression.
    #[must_use]
    pub fn lossless(mut self, lossless: bool) -> Self {
        self.config = self.config.lossless(lossless);
        self
    }

    /// Set quality/speed tradeoff (0 = fast, 6 = slower but better).
    #[must_use]
    pub fn method(mut self, method: u8) -> Self {
        self.config = self.config.method(method);
        self
    }

    /// Set near-lossless preprocessing (0 = max, 100 = off).
    #[must_use]
    pub fn near_lossless(mut self, value: u8) -> Self {
        self.config = self.config.near_lossless(value);
        self
    }

    /// Set alpha quality (0-100).
    #[must_use]
    pub fn alpha_quality(mut self, quality: u8) -> Self {
        self.config = self.config.alpha_quality(quality);
        self
    }

    /// Preserve exact RGB values under transparent areas.
    #[must_use]
    pub fn exact(mut self, exact: bool) -> Self {
        self.config = self.config.exact(exact);
        self
    }

    /// Set target file size in bytes (0 = disabled).
    #[must_use]
    pub fn target_size(mut self, size: u32) -> Self {
        self.config = self.config.target_size(size);
        self
    }

    /// Use sharp YUV conversion (slower but better).
    #[must_use]
    pub fn sharp_yuv(mut self, enable: bool) -> Self {
        self.config = self.config.sharp_yuv(enable);
        self
    }

    /// Set full encoder configuration.
    #[must_use]
    pub fn config(mut self, config: EncoderConfig) -> Self {
        self.config = config;
        self
    }

    /// Set ICC profile to embed.
    #[cfg(feature = "icc")]
    #[must_use]
    pub fn icc_profile(mut self, profile: &'a [u8]) -> Self {
        self.icc_profile = Some(profile);
        self
    }

    /// Encode to WebP bytes.
    pub fn encode(self) -> Result<Vec<u8>> {
        validate_dimensions(self.width, self.height)?;

        let webp_config = self.config.to_libwebp()?;

        // Initialize picture
        let mut picture = libwebp_sys::WebPPicture::new()
            .map_err(|_| Error::InvalidConfig("failed to init picture".into()))?;

        picture.width = self.width as i32;
        picture.height = self.height as i32;

        // Import pixel data
        let import_ok = match &self.data {
            EncoderInput::Rgba(data) => {
                validate_buffer_size(data.len(), self.width, self.height, 4)?;
                picture.use_argb = 1;
                unsafe {
                    libwebp_sys::WebPPictureImportRGBA(
                        &mut picture,
                        data.as_ptr(),
                        (self.width * 4) as i32,
                    )
                }
            }
            EncoderInput::Rgb(data) => {
                validate_buffer_size(data.len(), self.width, self.height, 3)?;
                picture.use_argb = 1;
                unsafe {
                    libwebp_sys::WebPPictureImportRGB(
                        &mut picture,
                        data.as_ptr(),
                        (self.width * 3) as i32,
                    )
                }
            }
            EncoderInput::Yuv(planes) => {
                picture.use_argb = 0;
                picture.colorspace = if planes.a.is_some() {
                    libwebp_sys::WebPEncCSP::WEBP_YUV420A
                } else {
                    libwebp_sys::WebPEncCSP::WEBP_YUV420
                };
                picture.y = planes.y.as_ptr() as *mut _;
                picture.u = planes.u.as_ptr() as *mut _;
                picture.v = planes.v.as_ptr() as *mut _;
                picture.y_stride = planes.y_stride as i32;
                picture.uv_stride = planes.u_stride as i32;
                if let Some(a) = &planes.a {
                    picture.a = a.as_ptr() as *mut _;
                    picture.a_stride = planes.a_stride as i32;
                }
                1 // YUV doesn't use import functions
            }
        };

        if import_ok == 0 {
            unsafe { libwebp_sys::WebPPictureFree(&mut picture) };
            return Err(Error::EncodeFailed(EncodingError::OutOfMemory));
        }

        // Setup memory writer
        let mut writer = core::mem::MaybeUninit::<libwebp_sys::WebPMemoryWriter>::uninit();
        unsafe { libwebp_sys::WebPMemoryWriterInit(writer.as_mut_ptr()) };
        let mut writer = unsafe { writer.assume_init() };

        picture.writer = Some(libwebp_sys::WebPMemoryWrite);
        picture.custom_ptr = &mut writer as *mut _ as *mut _;

        // Encode
        let ok = unsafe { libwebp_sys::WebPEncode(&webp_config, &mut picture) };

        let result = if ok == 0 {
            let error = EncodingError::from(picture.error_code as i32);
            unsafe {
                libwebp_sys::WebPPictureFree(&mut picture);
                libwebp_sys::WebPMemoryWriterClear(&mut writer);
            }
            Err(Error::EncodeFailed(error))
        } else {
            let webp_data = unsafe {
                let slice = core::slice::from_raw_parts(writer.mem, writer.size);
                slice.to_vec()
            };
            unsafe {
                libwebp_sys::WebPPictureFree(&mut picture);
                libwebp_sys::WebPMemoryWriterClear(&mut writer);
            }

            #[cfg(feature = "icc")]
            if let Some(icc) = self.icc_profile {
                return crate::mux::embed_icc(&webp_data, icc);
            }

            Ok(webp_data)
        };

        result
    }
}

pub(crate) fn validate_dimensions(width: u32, height: u32) -> Result<()> {
    const MAX_DIMENSION: u32 = 16383;

    if width == 0 || height == 0 {
        return Err(Error::InvalidInput(
            "width and height must be non-zero".into(),
        ));
    }
    if width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(Error::InvalidInput(alloc::format!(
            "dimensions exceed maximum ({} x {})",
            MAX_DIMENSION,
            MAX_DIMENSION
        )));
    }
    Ok(())
}

pub(crate) fn validate_buffer_size(size: usize, width: u32, height: u32, bpp: u32) -> Result<()> {
    let expected = (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(bpp as usize);

    if size < expected {
        return Err(Error::InvalidInput(alloc::format!(
            "buffer too small: got {}, expected {}",
            size,
            expected
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_dimensions() {
        assert!(validate_dimensions(0, 100).is_err());
        assert!(validate_dimensions(100, 0).is_err());
        assert!(validate_dimensions(20000, 100).is_err());
        assert!(validate_dimensions(100, 100).is_ok());
    }

    #[test]
    fn test_validate_buffer_size() {
        assert!(validate_buffer_size(100, 10, 10, 4).is_err());
        assert!(validate_buffer_size(400, 10, 10, 4).is_ok());
        assert!(validate_buffer_size(500, 10, 10, 4).is_ok());
    }
}
