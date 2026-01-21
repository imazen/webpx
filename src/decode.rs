//! WebP decoding functionality.

use crate::config::DecoderConfig;
use crate::error::{DecodingError, Error, Result};
use crate::types::{DecodePixel, ImageInfo, YuvPlanes};
use alloc::vec::Vec;
use imgref::ImgVec;
use rgb::alt::{BGR8, BGRA8};
use rgb::{RGB8, RGBA8};
use whereat::*;

/// Decode WebP data to RGBA pixels.
///
/// Returns the decoded pixels and dimensions.
///
/// # Example
///
/// ```rust,no_run
/// let webp_data: &[u8] = &[0u8; 100]; // placeholder
/// let (pixels, width, height) = webpx::decode_rgba(webp_data)?;
/// # Ok::<(), webpx::At<webpx::Error>>(())
/// ```
pub fn decode_rgba(data: &[u8]) -> Result<(Vec<u8>, u32, u32)> {
    let mut width: i32 = 0;
    let mut height: i32 = 0;

    let ptr =
        unsafe { libwebp_sys::WebPDecodeRGBA(data.as_ptr(), data.len(), &mut width, &mut height) };

    if ptr.is_null() {
        return Err(at!(Error::DecodeFailed(DecodingError::BitstreamError)));
    }

    let size = (width as usize) * (height as usize) * 4;
    let pixels = unsafe {
        let slice = core::slice::from_raw_parts(ptr, size);
        let vec = slice.to_vec();
        libwebp_sys::WebPFree(ptr as *mut _);
        vec
    };

    Ok((pixels, width as u32, height as u32))
}

/// Decode WebP data to RGB pixels (no alpha).
///
/// Returns the decoded pixels and dimensions.
pub fn decode_rgb(data: &[u8]) -> Result<(Vec<u8>, u32, u32)> {
    let mut width: i32 = 0;
    let mut height: i32 = 0;

    let ptr =
        unsafe { libwebp_sys::WebPDecodeRGB(data.as_ptr(), data.len(), &mut width, &mut height) };

    if ptr.is_null() {
        return Err(at!(Error::DecodeFailed(DecodingError::BitstreamError)));
    }

    let size = (width as usize) * (height as usize) * 3;
    let pixels = unsafe {
        let slice = core::slice::from_raw_parts(ptr, size);
        let vec = slice.to_vec();
        libwebp_sys::WebPFree(ptr as *mut _);
        vec
    };

    Ok((pixels, width as u32, height as u32))
}

/// Decode WebP data to BGRA pixels.
///
/// BGRA is the native format on Windows and some GPU APIs.
/// Returns the decoded pixels and dimensions.
pub fn decode_bgra(data: &[u8]) -> Result<(Vec<u8>, u32, u32)> {
    let mut width: i32 = 0;
    let mut height: i32 = 0;

    let ptr =
        unsafe { libwebp_sys::WebPDecodeBGRA(data.as_ptr(), data.len(), &mut width, &mut height) };

    if ptr.is_null() {
        return Err(at!(Error::DecodeFailed(DecodingError::BitstreamError)));
    }

    let size = (width as usize) * (height as usize) * 4;
    let pixels = unsafe {
        let slice = core::slice::from_raw_parts(ptr, size);
        let vec = slice.to_vec();
        libwebp_sys::WebPFree(ptr as *mut _);
        vec
    };

    Ok((pixels, width as u32, height as u32))
}

/// Decode WebP data to BGR pixels (no alpha).
///
/// BGR is common in OpenCV and some image libraries.
/// Returns the decoded pixels and dimensions.
pub fn decode_bgr(data: &[u8]) -> Result<(Vec<u8>, u32, u32)> {
    let mut width: i32 = 0;
    let mut height: i32 = 0;

    let ptr =
        unsafe { libwebp_sys::WebPDecodeBGR(data.as_ptr(), data.len(), &mut width, &mut height) };

    if ptr.is_null() {
        return Err(at!(Error::DecodeFailed(DecodingError::BitstreamError)));
    }

    let size = (width as usize) * (height as usize) * 3;
    let pixels = unsafe {
        let slice = core::slice::from_raw_parts(ptr, size);
        let vec = slice.to_vec();
        libwebp_sys::WebPFree(ptr as *mut _);
        vec
    };

    Ok((pixels, width as u32, height as u32))
}

/// Decode WebP data to typed pixels.
///
/// Returns the decoded pixels as the specified pixel type and dimensions.
/// Supports [`RGBA8`], [`RGB8`], [`BGRA8`], and [`BGR8`].
///
/// # Example
///
/// ```rust,no_run
/// use rgb::RGBA8;
///
/// let webp_data: &[u8] = &[0u8; 100]; // placeholder
/// let (pixels, width, height) = webpx::decode::<RGBA8>(webp_data)?;
/// // pixels is Vec<RGBA8>
/// # Ok::<(), webpx::At<webpx::Error>>(())
/// ```
pub fn decode<P: DecodePixel>(data: &[u8]) -> Result<(Vec<P>, u32, u32)> {
    let (ptr, width, height) = P::decode_new(data)
        .ok_or_else(|| at!(Error::DecodeFailed(DecodingError::BitstreamError)))?;

    let bpp = P::LAYOUT.bytes_per_pixel();
    let pixel_count = (width as usize) * (height as usize);
    let byte_size = pixel_count * bpp;

    let pixels = unsafe {
        // Copy from libwebp buffer to our Vec<P>
        let byte_slice = core::slice::from_raw_parts(ptr, byte_size);
        let mut vec: Vec<P> = Vec::with_capacity(pixel_count);
        core::ptr::copy_nonoverlapping(byte_slice.as_ptr(), vec.as_mut_ptr() as *mut u8, byte_size);
        vec.set_len(pixel_count);
        libwebp_sys::WebPFree(ptr as *mut _);
        vec
    };

    Ok((pixels, width as u32, height as u32))
}

/// Decode WebP data, appending typed pixels to an existing Vec.
///
/// This is useful when you want to reuse an existing buffer or
/// decode multiple images into the same Vec.
///
/// # Arguments
/// * `data` - WebP encoded data
/// * `output` - Vec to append decoded pixels to
///
/// # Returns
/// Width and height of the decoded image.
///
/// # Example
///
/// ```rust,no_run
/// use rgb::RGBA8;
///
/// let webp_data: &[u8] = &[0u8; 100]; // placeholder
/// let mut pixels: Vec<RGBA8> = Vec::new();
/// let (width, height) = webpx::decode_append::<RGBA8>(webp_data, &mut pixels)?;
/// # Ok::<(), webpx::At<webpx::Error>>(())
/// ```
pub fn decode_append<P: DecodePixel>(data: &[u8], output: &mut Vec<P>) -> Result<(u32, u32)> {
    let (ptr, width, height) = P::decode_new(data)
        .ok_or_else(|| at!(Error::DecodeFailed(DecodingError::BitstreamError)))?;

    let bpp = P::LAYOUT.bytes_per_pixel();
    let pixel_count = (width as usize) * (height as usize);
    let byte_size = pixel_count * bpp;

    unsafe {
        let byte_slice = core::slice::from_raw_parts(ptr, byte_size);
        let start = output.len();
        output.reserve(pixel_count);
        core::ptr::copy_nonoverlapping(
            byte_slice.as_ptr(),
            (output.as_mut_ptr() as *mut u8).add(start * bpp),
            byte_size,
        );
        output.set_len(start + pixel_count);
        libwebp_sys::WebPFree(ptr as *mut _);
    };

    Ok((width as u32, height as u32))
}

/// Decode WebP data to an imgref image.
///
/// Returns the decoded image as an [`ImgVec`] with the specified pixel type.
///
/// # Example
///
/// ```rust,no_run
/// use rgb::RGBA8;
///
/// let webp_data: &[u8] = &[0u8; 100]; // placeholder
/// let img: imgref::ImgVec<RGBA8> = webpx::decode_to_img(webp_data)?;
/// // Access via img.pixels(), img.width(), img.height()
/// # Ok::<(), webpx::At<webpx::Error>>(())
/// ```
pub fn decode_to_img<P: DecodePixel>(data: &[u8]) -> Result<ImgVec<P>> {
    let (pixels, width, height) = decode::<P>(data)?;
    Ok(ImgVec::new(pixels, width as usize, height as usize))
}

/// Decode WebP data directly into a typed pixel slice.
///
/// This function decodes directly into the provided buffer, avoiding
/// allocation overhead. The buffer must be pre-allocated with sufficient space.
///
/// # Arguments
/// * `data` - WebP encoded data
/// * `output` - Pre-allocated output buffer (must be at least width * height pixels)
/// * `stride_pixels` - Row stride in pixels (must be >= width)
///
/// # Returns
/// Width and height of the decoded image.
///
/// # Example
///
/// ```rust,no_run
/// use rgb::RGBA8;
///
/// let webp_data: &[u8] = &[0u8; 100]; // placeholder
/// let info = webpx::ImageInfo::from_webp(&webp_data)?;
/// let mut buffer: Vec<RGBA8> = vec![RGBA8::default(); info.width as usize * info.height as usize];
/// let (w, h) = webpx::decode_into::<RGBA8>(&webp_data, &mut buffer, info.width)?;
/// # Ok::<(), webpx::At<webpx::Error>>(())
/// ```
pub fn decode_into<P: DecodePixel>(
    data: &[u8],
    output: &mut [P],
    stride_pixels: u32,
) -> Result<(u32, u32)> {
    let info = ImageInfo::from_webp(data)?;
    let width = info.width;
    let height = info.height;
    let bpp = P::LAYOUT.bytes_per_pixel();

    // Validate buffer
    let required_pixels = (stride_pixels as usize) * (height as usize);
    if output.len() < required_pixels {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "output buffer too small: got {} pixels, need {} (stride {} × height {})",
            output.len(),
            required_pixels,
            stride_pixels,
            height
        ))));
    }
    if stride_pixels < width {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "stride too small: got {}, minimum {}",
            stride_pixels,
            width
        ))));
    }

    let stride_bytes = (stride_pixels as usize) * bpp;
    let output_bytes = output.len() * bpp;

    // SAFETY: We've validated the buffer size and stride above
    let ok = unsafe {
        P::decode_into(
            data,
            output.as_mut_ptr() as *mut u8,
            output_bytes,
            stride_bytes as i32,
        )
    };

    if !ok {
        return Err(at!(Error::DecodeFailed(DecodingError::BitstreamError)));
    }

    Ok((width, height))
}

/// Decode WebP data directly into a pre-allocated RGBA buffer (zero-copy).
///
/// This function decodes directly into the provided buffer, avoiding
/// allocation and copy overhead.
///
/// # Arguments
/// * `data` - WebP encoded data
/// * `output` - Pre-allocated output buffer (must be at least stride * height bytes)
/// * `stride_bytes` - Row stride in bytes (must be >= width * 4)
///
/// # Returns
/// Width and height of the decoded image.
///
/// # Example
/// ```rust,no_run
/// let webp_data: &[u8] = &[]; // placeholder
/// let info = webpx::ImageInfo::from_webp(&webp_data)?;
/// let stride = info.width as usize * 4;
/// let mut buffer = vec![0u8; stride * info.height as usize];
/// let (w, h) = webpx::decode_rgba_into(&webp_data, &mut buffer, stride as u32)?;
/// # Ok::<(), webpx::At<webpx::Error>>(())
/// ```
pub fn decode_rgba_into(data: &[u8], output: &mut [u8], stride_bytes: u32) -> Result<(u32, u32)> {
    let mut width: i32 = 0;
    let mut height: i32 = 0;

    // Get dimensions first
    if unsafe { libwebp_sys::WebPGetInfo(data.as_ptr(), data.len(), &mut width, &mut height) } == 0
    {
        return Err(at!(Error::DecodeFailed(DecodingError::BitstreamError)));
    }

    // Validate buffer
    let required = (stride_bytes as usize).saturating_mul(height as usize);
    if output.len() < required {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "output buffer too small: got {}, need {} (stride {} × height {})",
            output.len(),
            required,
            stride_bytes,
            height
        ))));
    }
    if (stride_bytes as i32) < width * 4 {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "stride too small: got {}, minimum {}",
            stride_bytes,
            width * 4
        ))));
    }

    let result = unsafe {
        libwebp_sys::WebPDecodeRGBAInto(
            data.as_ptr(),
            data.len(),
            output.as_mut_ptr(),
            output.len(),
            stride_bytes as i32,
        )
    };

    if result.is_null() {
        return Err(at!(Error::DecodeFailed(DecodingError::BitstreamError)));
    }

    Ok((width as u32, height as u32))
}

/// Decode WebP data directly into a pre-allocated BGRA buffer (zero-copy).
///
/// BGRA is the native format on Windows and some GPU APIs.
///
/// # Arguments
/// * `data` - WebP encoded data
/// * `output` - Pre-allocated output buffer (must be at least stride * height bytes)
/// * `stride_bytes` - Row stride in bytes (must be >= width * 4)
///
/// # Returns
/// Width and height of the decoded image.
pub fn decode_bgra_into(data: &[u8], output: &mut [u8], stride_bytes: u32) -> Result<(u32, u32)> {
    let mut width: i32 = 0;
    let mut height: i32 = 0;

    // Get dimensions first
    if unsafe { libwebp_sys::WebPGetInfo(data.as_ptr(), data.len(), &mut width, &mut height) } == 0
    {
        return Err(at!(Error::DecodeFailed(DecodingError::BitstreamError)));
    }

    // Validate buffer
    let required = (stride_bytes as usize).saturating_mul(height as usize);
    if output.len() < required {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "output buffer too small: got {}, need {} (stride {} × height {})",
            output.len(),
            required,
            stride_bytes,
            height
        ))));
    }
    if (stride_bytes as i32) < width * 4 {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "stride too small: got {}, minimum {}",
            stride_bytes,
            width * 4
        ))));
    }

    let result = unsafe {
        libwebp_sys::WebPDecodeBGRAInto(
            data.as_ptr(),
            data.len(),
            output.as_mut_ptr(),
            output.len(),
            stride_bytes as i32,
        )
    };

    if result.is_null() {
        return Err(at!(Error::DecodeFailed(DecodingError::BitstreamError)));
    }

    Ok((width as u32, height as u32))
}

/// Decode WebP data directly into a pre-allocated RGB buffer (zero-copy).
///
/// # Arguments
/// * `data` - WebP encoded data
/// * `output` - Pre-allocated output buffer (must be at least stride * height bytes)
/// * `stride_bytes` - Row stride in bytes (must be >= width * 3)
///
/// # Returns
/// Width and height of the decoded image.
pub fn decode_rgb_into(data: &[u8], output: &mut [u8], stride_bytes: u32) -> Result<(u32, u32)> {
    let mut width: i32 = 0;
    let mut height: i32 = 0;

    // Get dimensions first
    if unsafe { libwebp_sys::WebPGetInfo(data.as_ptr(), data.len(), &mut width, &mut height) } == 0
    {
        return Err(at!(Error::DecodeFailed(DecodingError::BitstreamError)));
    }

    // Validate buffer
    let required = (stride_bytes as usize).saturating_mul(height as usize);
    if output.len() < required {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "output buffer too small: got {}, need {} (stride {} × height {})",
            output.len(),
            required,
            stride_bytes,
            height
        ))));
    }
    if (stride_bytes as i32) < width * 3 {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "stride too small: got {}, minimum {}",
            stride_bytes,
            width * 3
        ))));
    }

    let result = unsafe {
        libwebp_sys::WebPDecodeRGBInto(
            data.as_ptr(),
            data.len(),
            output.as_mut_ptr(),
            output.len(),
            stride_bytes as i32,
        )
    };

    if result.is_null() {
        return Err(at!(Error::DecodeFailed(DecodingError::BitstreamError)));
    }

    Ok((width as u32, height as u32))
}

/// Decode WebP data directly into a pre-allocated BGR buffer (zero-copy).
///
/// BGR is common in OpenCV and some image libraries.
///
/// # Arguments
/// * `data` - WebP encoded data
/// * `output` - Pre-allocated output buffer (must be at least stride * height bytes)
/// * `stride_bytes` - Row stride in bytes (must be >= width * 3)
///
/// # Returns
/// Width and height of the decoded image.
pub fn decode_bgr_into(data: &[u8], output: &mut [u8], stride_bytes: u32) -> Result<(u32, u32)> {
    let mut width: i32 = 0;
    let mut height: i32 = 0;

    // Get dimensions first
    if unsafe { libwebp_sys::WebPGetInfo(data.as_ptr(), data.len(), &mut width, &mut height) } == 0
    {
        return Err(at!(Error::DecodeFailed(DecodingError::BitstreamError)));
    }

    // Validate buffer
    let required = (stride_bytes as usize).saturating_mul(height as usize);
    if output.len() < required {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "output buffer too small: got {}, need {} (stride {} × height {})",
            output.len(),
            required,
            stride_bytes,
            height
        ))));
    }
    if (stride_bytes as i32) < width * 3 {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "stride too small: got {}, minimum {}",
            stride_bytes,
            width * 3
        ))));
    }

    let result = unsafe {
        libwebp_sys::WebPDecodeBGRInto(
            data.as_ptr(),
            data.len(),
            output.as_mut_ptr(),
            output.len(),
            stride_bytes as i32,
        )
    };

    if result.is_null() {
        return Err(at!(Error::DecodeFailed(DecodingError::BitstreamError)));
    }

    Ok((width as u32, height as u32))
}

/// Decode WebP data to YUV planes.
///
/// Returns YUV420 planar data.
pub fn decode_yuv(data: &[u8]) -> Result<YuvPlanes> {
    let mut width: i32 = 0;
    let mut height: i32 = 0;
    let mut u_ptr: *mut u8 = core::ptr::null_mut();
    let mut v_ptr: *mut u8 = core::ptr::null_mut();
    let mut y_stride: i32 = 0;
    let mut uv_stride: i32 = 0;

    let y_ptr = unsafe {
        libwebp_sys::WebPDecodeYUV(
            data.as_ptr(),
            data.len(),
            &mut width,
            &mut height,
            &mut u_ptr,
            &mut v_ptr,
            &mut y_stride,
            &mut uv_stride,
        )
    };

    if y_ptr.is_null() {
        return Err(at!(Error::DecodeFailed(DecodingError::BitstreamError)));
    }

    let _uv_width = (width + 1) / 2;
    let uv_height = (height + 1) / 2;

    let y_size = (y_stride as usize) * (height as usize);
    let uv_size = (uv_stride as usize) * (uv_height as usize);

    let (y, u, v) = unsafe {
        let y = core::slice::from_raw_parts(y_ptr, y_size).to_vec();
        let u = core::slice::from_raw_parts(u_ptr, uv_size).to_vec();
        let v = core::slice::from_raw_parts(v_ptr, uv_size).to_vec();
        libwebp_sys::WebPFree(y_ptr as *mut _);
        // u and v are part of the same allocation as y, don't free separately
        (y, u, v)
    };

    Ok(YuvPlanes {
        y,
        y_stride: y_stride as usize,
        u,
        u_stride: uv_stride as usize,
        v,
        v_stride: uv_stride as usize,
        a: None,
        a_stride: 0,
        width: width as u32,
        height: height as u32,
    })
}

/// WebP decoder with advanced options.
///
/// # Example
///
/// ```rust,no_run
/// use webpx::Decoder;
///
/// let webp_data: &[u8] = &[0u8; 100]; // placeholder
/// let decoder = Decoder::new(webp_data)?;
/// let info = decoder.info();
/// println!("Image: {}x{}, alpha: {}", info.width, info.height, info.has_alpha);
///
/// let img: imgref::ImgVec<rgb::RGBA8> = decoder.decode_rgba()?;
/// # Ok::<(), webpx::At<webpx::Error>>(())
/// ```
pub struct Decoder<'a> {
    data: &'a [u8],
    info: ImageInfo,
    config: DecoderConfig,
}

impl<'a> Decoder<'a> {
    /// Create a new decoder for the given WebP data.
    pub fn new(data: &'a [u8]) -> Result<Self> {
        let info = ImageInfo::from_webp(data)?;
        Ok(Self {
            data,
            info,
            config: DecoderConfig::default(),
        })
    }

    /// Get image information.
    pub fn info(&self) -> &ImageInfo {
        &self.info
    }

    /// Set decoder configuration.
    pub fn config(mut self, config: DecoderConfig) -> Self {
        self.config = config;
        self
    }

    /// Enable cropping.
    pub fn crop(mut self, left: u32, top: u32, width: u32, height: u32) -> Self {
        self.config.use_cropping = true;
        self.config.crop_left = left;
        self.config.crop_top = top;
        self.config.crop_width = width;
        self.config.crop_height = height;
        self
    }

    /// Enable scaling.
    pub fn scale(mut self, width: u32, height: u32) -> Self {
        self.config.use_scaling = true;
        self.config.scaled_width = width;
        self.config.scaled_height = height;
        self
    }

    /// Decode to RGBA ImgVec.
    pub fn decode_rgba(self) -> Result<ImgVec<RGBA8>> {
        let (pixels, width, height) = self.decode_rgba_raw()?;

        // Convert &[u8] to Vec<RGBA8>
        let rgba_pixels: Vec<RGBA8> = pixels
            .chunks_exact(4)
            .map(|c| RGBA8::new(c[0], c[1], c[2], c[3]))
            .collect();

        Ok(ImgVec::new(rgba_pixels, width as usize, height as usize))
    }

    /// Decode to RGB ImgVec (no alpha).
    pub fn decode_rgb(self) -> Result<ImgVec<RGB8>> {
        let (pixels, width, height) = self.decode_rgb_raw()?;

        // Convert &[u8] to Vec<RGB8>
        let rgb_pixels: Vec<RGB8> = pixels
            .chunks_exact(3)
            .map(|c| RGB8::new(c[0], c[1], c[2]))
            .collect();

        Ok(ImgVec::new(rgb_pixels, width as usize, height as usize))
    }

    /// Decode to raw RGBA bytes.
    pub fn decode_rgba_raw(self) -> Result<(Vec<u8>, u32, u32)> {
        if self.config.use_cropping || self.config.use_scaling {
            self.decode_advanced(libwebp_sys::WEBP_CSP_MODE::MODE_RGBA)
        } else {
            decode_rgba(self.data)
        }
    }

    /// Decode to raw RGB bytes.
    pub fn decode_rgb_raw(self) -> Result<(Vec<u8>, u32, u32)> {
        if self.config.use_cropping || self.config.use_scaling {
            self.decode_advanced(libwebp_sys::WEBP_CSP_MODE::MODE_RGB)
        } else {
            decode_rgb(self.data)
        }
    }

    /// Decode to BGRA ImgVec.
    ///
    /// BGRA is the native format on Windows and some GPU APIs.
    pub fn decode_bgra(self) -> Result<ImgVec<BGRA8>> {
        let (pixels, width, height) = self.decode_bgra_raw()?;

        // Convert &[u8] to Vec<BGRA8>
        let bgra_pixels: Vec<BGRA8> = pixels
            .chunks_exact(4)
            .map(|c| BGRA8 {
                b: c[0],
                g: c[1],
                r: c[2],
                a: c[3],
            })
            .collect();

        Ok(ImgVec::new(bgra_pixels, width as usize, height as usize))
    }

    /// Decode to BGR ImgVec (no alpha).
    ///
    /// BGR is common in OpenCV and some image libraries.
    pub fn decode_bgr(self) -> Result<ImgVec<BGR8>> {
        let (pixels, width, height) = self.decode_bgr_raw()?;

        // Convert &[u8] to Vec<BGR8>
        let bgr_pixels: Vec<BGR8> = pixels
            .chunks_exact(3)
            .map(|c| BGR8 {
                b: c[0],
                g: c[1],
                r: c[2],
            })
            .collect();

        Ok(ImgVec::new(bgr_pixels, width as usize, height as usize))
    }

    /// Decode to raw BGRA bytes.
    pub fn decode_bgra_raw(self) -> Result<(Vec<u8>, u32, u32)> {
        if self.config.use_cropping || self.config.use_scaling {
            self.decode_advanced(libwebp_sys::WEBP_CSP_MODE::MODE_BGRA)
        } else {
            decode_bgra(self.data)
        }
    }

    /// Decode to raw BGR bytes.
    pub fn decode_bgr_raw(self) -> Result<(Vec<u8>, u32, u32)> {
        if self.config.use_cropping || self.config.use_scaling {
            self.decode_advanced(libwebp_sys::WEBP_CSP_MODE::MODE_BGR)
        } else {
            decode_bgr(self.data)
        }
    }

    /// Decode to YUV planes.
    pub fn decode_yuv(self) -> Result<YuvPlanes> {
        // For YUV, we use the simple API since advanced YUV decoding
        // requires more complex buffer management
        decode_yuv(self.data)
    }

    /// Advanced decode with cropping/scaling support.
    fn decode_advanced(self, mode: libwebp_sys::WEBP_CSP_MODE) -> Result<(Vec<u8>, u32, u32)> {
        let mut dec_config = libwebp_sys::WebPDecoderConfig::new()
            .map_err(|_| at!(Error::InvalidConfig("failed to init decoder config".into())))?;

        // Get features
        let status = unsafe {
            libwebp_sys::WebPGetFeatures(self.data.as_ptr(), self.data.len(), &mut dec_config.input)
        };
        if status != libwebp_sys::VP8StatusCode::VP8_STATUS_OK {
            return Err(at!(Error::DecodeFailed(DecodingError::from(status as i32))));
        }

        // Configure output
        dec_config.output.colorspace = mode;

        // Configure options
        if self.config.use_cropping {
            dec_config.options.use_cropping = 1;
            dec_config.options.crop_left = self.config.crop_left as i32;
            dec_config.options.crop_top = self.config.crop_top as i32;
            dec_config.options.crop_width = self.config.crop_width as i32;
            dec_config.options.crop_height = self.config.crop_height as i32;
        }

        if self.config.use_scaling {
            dec_config.options.use_scaling = 1;
            dec_config.options.scaled_width = self.config.scaled_width as i32;
            dec_config.options.scaled_height = self.config.scaled_height as i32;
        }

        dec_config.options.bypass_filtering = self.config.bypass_filtering as i32;
        dec_config.options.no_fancy_upsampling = self.config.no_fancy_upsampling as i32;
        dec_config.options.use_threads = self.config.use_threads as i32;
        dec_config.options.flip = self.config.flip as i32;
        dec_config.options.alpha_dithering_strength = self.config.alpha_dithering as i32;

        // Decode
        let status = unsafe {
            libwebp_sys::WebPDecode(self.data.as_ptr(), self.data.len(), &mut dec_config)
        };

        if status != libwebp_sys::VP8StatusCode::VP8_STATUS_OK {
            return Err(at!(Error::DecodeFailed(DecodingError::from(status as i32))));
        }

        // Get output dimensions
        let width = if self.config.use_scaling {
            self.config.scaled_width
        } else if self.config.use_cropping {
            self.config.crop_width
        } else {
            dec_config.input.width as u32
        };

        let height = if self.config.use_scaling {
            self.config.scaled_height
        } else if self.config.use_cropping {
            self.config.crop_height
        } else {
            dec_config.input.height as u32
        };

        let bpp = match mode {
            libwebp_sys::WEBP_CSP_MODE::MODE_RGB | libwebp_sys::WEBP_CSP_MODE::MODE_BGR => 3,
            _ => 4,
        };

        let size = (width as usize) * (height as usize) * bpp;
        let pixels = unsafe {
            if dec_config.output.u.RGBA.rgba.is_null() {
                return Err(at!(Error::DecodeFailed(DecodingError::OutOfMemory)));
            }
            let slice = core::slice::from_raw_parts(dec_config.output.u.RGBA.rgba, size);
            let vec = slice.to_vec();
            libwebp_sys::WebPFreeDecBuffer(&mut dec_config.output);
            vec
        };

        Ok((pixels, width, height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test with a minimal valid WebP (would need actual test data)
    #[test]
    fn test_image_info_invalid() {
        let invalid_data = b"not a webp";
        assert!(ImageInfo::from_webp(invalid_data).is_err());
    }
}
