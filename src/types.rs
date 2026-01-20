//! Core types for image data representation.

use alloc::vec::Vec;
use rgb::alt::{BGR8, BGRA8};
use rgb::{RGB8, RGBA8};
use whereat::*;

/// Pixel format for encoding/decoding operations.
///
/// This enum describes the channel order and layout for byte-oriented APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum PixelFormat {
    /// RGBA - 4 bytes per pixel (red, green, blue, alpha)
    Rgba,
    /// BGRA - 4 bytes per pixel (blue, green, red, alpha) - Windows/GPU native
    Bgra,
    /// RGB - 3 bytes per pixel (red, green, blue)
    Rgb,
    /// BGR - 3 bytes per pixel (blue, green, red) - OpenCV native
    Bgr,
}

impl PixelFormat {
    /// Bytes per pixel for this format.
    #[must_use]
    pub const fn bytes_per_pixel(self) -> usize {
        match self {
            PixelFormat::Rgba | PixelFormat::Bgra => 4,
            PixelFormat::Rgb | PixelFormat::Bgr => 3,
        }
    }

    /// Whether this format has an alpha channel.
    #[must_use]
    pub const fn has_alpha(self) -> bool {
        matches!(self, PixelFormat::Rgba | PixelFormat::Bgra)
    }
}

/// Marker trait for pixel types that can be encoded/decoded.
///
/// This trait maps rgb crate types to their corresponding [`PixelFormat`],
/// enabling type-safe encoding and decoding operations.
///
/// # Implemented Types
///
/// - [`RGB8`] - 3-channel RGB
/// - [`RGBA8`] - 4-channel RGBA
/// - [`BGR8`] - 3-channel BGR (Windows/OpenCV)
/// - [`BGRA8`] - 4-channel BGRA (Windows/GPU native)
///
/// # Example
///
/// ```rust,no_run
/// use webpx::{Encoder, Pixel, Unstoppable};
/// use rgb::RGBA8;
///
/// let pixels: Vec<RGBA8> = vec![RGBA8::new(255, 0, 0, 255); 100 * 100];
/// let webp = Encoder::from_pixels(&pixels, 100, 100)
///     .quality(85.0)
///     .encode(Unstoppable)?;
/// # Ok::<(), webpx::At<webpx::Error>>(())
/// ```
pub trait Pixel: Copy + 'static + private::Sealed {
    /// The pixel format corresponding to this type.
    const FORMAT: PixelFormat;
}

impl Pixel for RGBA8 {
    const FORMAT: PixelFormat = PixelFormat::Rgba;
}

impl Pixel for BGRA8 {
    const FORMAT: PixelFormat = PixelFormat::Bgra;
}

impl Pixel for RGB8 {
    const FORMAT: PixelFormat = PixelFormat::Rgb;
}

impl Pixel for BGR8 {
    const FORMAT: PixelFormat = PixelFormat::Bgr;
}

mod private {
    use super::*;

    pub trait Sealed {}
    impl Sealed for RGBA8 {}
    impl Sealed for BGRA8 {}
    impl Sealed for RGB8 {}
    impl Sealed for BGR8 {}
}

/// Information about a WebP image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageInfo {
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Whether the image has an alpha channel.
    pub has_alpha: bool,
    /// Whether the image is animated.
    pub has_animation: bool,
    /// Number of frames (1 for static images).
    pub frame_count: u32,
    /// Bitstream format (lossy or lossless).
    pub format: BitstreamFormat,
}

impl ImageInfo {
    /// Get info from WebP data without decoding.
    pub fn from_webp(data: &[u8]) -> crate::Result<Self> {
        let mut width: i32 = 0;
        let mut height: i32 = 0;

        let result =
            unsafe { libwebp_sys::WebPGetInfo(data.as_ptr(), data.len(), &mut width, &mut height) };

        if result == 0 {
            return Err(at!(crate::Error::InvalidWebP));
        }

        // Get more detailed features
        let mut features = core::mem::MaybeUninit::<libwebp_sys::WebPBitstreamFeatures>::uninit();
        let status = unsafe {
            libwebp_sys::WebPGetFeatures(data.as_ptr(), data.len(), features.as_mut_ptr())
        };
        let features = unsafe { features.assume_init() };

        if status != libwebp_sys::VP8StatusCode::VP8_STATUS_OK {
            return Err(at!(crate::Error::DecodeFailed(
                crate::error::DecodingError::from(status as i32),
            )));
        }

        let format = match features.format {
            0 => BitstreamFormat::Undefined,
            1 => BitstreamFormat::Lossy,
            2 => BitstreamFormat::Lossless,
            _ => BitstreamFormat::Undefined,
        };

        Ok(ImageInfo {
            width: width as u32,
            height: height as u32,
            has_alpha: features.has_alpha != 0,
            has_animation: features.has_animation != 0,
            frame_count: if features.has_animation != 0 { 0 } else { 1 }, // Animation frame count needs demux
            format,
        })
    }
}

/// Bitstream format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum BitstreamFormat {
    /// Format not determined.
    #[default]
    Undefined,
    /// Lossy compression (VP8).
    Lossy,
    /// Lossless compression (VP8L).
    Lossless,
}

/// Color mode for output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum ColorMode {
    /// RGBA (8 bits per channel, 32 bits per pixel).
    #[default]
    Rgba,
    /// BGRA (8 bits per channel, 32 bits per pixel).
    Bgra,
    /// ARGB (8 bits per channel, 32 bits per pixel).
    Argb,
    /// RGB (8 bits per channel, 24 bits per pixel).
    Rgb,
    /// BGR (8 bits per channel, 24 bits per pixel).
    Bgr,
    /// YUV420 (separate Y, U, V planes).
    Yuv420,
    /// YUVA420 (YUV420 with alpha plane).
    Yuva420,
}

impl ColorMode {
    /// Bytes per pixel for packed formats.
    pub fn bytes_per_pixel(self) -> Option<usize> {
        match self {
            ColorMode::Rgba | ColorMode::Bgra | ColorMode::Argb => Some(4),
            ColorMode::Rgb | ColorMode::Bgr => Some(3),
            ColorMode::Yuv420 | ColorMode::Yuva420 => None, // Planar
        }
    }

    /// Whether this mode has an alpha channel.
    pub fn has_alpha(self) -> bool {
        matches!(
            self,
            ColorMode::Rgba | ColorMode::Bgra | ColorMode::Argb | ColorMode::Yuva420
        )
    }

    /// Whether this is a planar YUV format.
    pub fn is_yuv(self) -> bool {
        matches!(self, ColorMode::Yuv420 | ColorMode::Yuva420)
    }
}

/// YUV plane data for planar formats.
#[derive(Debug, Clone)]
pub struct YuvPlanes {
    /// Y (luma) plane data.
    pub y: Vec<u8>,
    /// Y plane stride in bytes.
    pub y_stride: usize,
    /// U (chroma blue) plane data.
    pub u: Vec<u8>,
    /// U plane stride in bytes.
    pub u_stride: usize,
    /// V (chroma red) plane data.
    pub v: Vec<u8>,
    /// V plane stride in bytes.
    pub v_stride: usize,
    /// Alpha plane data (optional).
    pub a: Option<Vec<u8>>,
    /// Alpha plane stride in bytes.
    pub a_stride: usize,
    /// Image width.
    pub width: u32,
    /// Image height.
    pub height: u32,
}

impl YuvPlanes {
    /// Create new YUV planes with the given dimensions.
    ///
    /// Allocates planes for YUV420 format where U and V are half the resolution.
    pub fn new(width: u32, height: u32, with_alpha: bool) -> Self {
        let y_stride = width as usize;
        let uv_stride = (width as usize).div_ceil(2);
        let uv_height = (height as usize).div_ceil(2);

        Self {
            y: alloc::vec![0u8; y_stride * height as usize],
            y_stride,
            u: alloc::vec![0u8; uv_stride * uv_height],
            u_stride: uv_stride,
            v: alloc::vec![0u8; uv_stride * uv_height],
            v_stride: uv_stride,
            a: if with_alpha {
                Some(alloc::vec![0u8; y_stride * height as usize])
            } else {
                None
            },
            a_stride: y_stride,
            width,
            height,
        }
    }

    /// Get the U plane dimensions (half width/height for YUV420).
    pub fn uv_dimensions(&self) -> (u32, u32) {
        (self.width.div_ceil(2), self.height.div_ceil(2))
    }
}

/// Reference to YUV planes (borrowed version).
#[derive(Debug, Clone, Copy)]
pub struct YuvPlanesRef<'a> {
    /// Y (luma) plane data.
    pub y: &'a [u8],
    /// Y plane stride in bytes.
    pub y_stride: usize,
    /// U (chroma blue) plane data.
    pub u: &'a [u8],
    /// U plane stride in bytes.
    pub u_stride: usize,
    /// V (chroma red) plane data.
    pub v: &'a [u8],
    /// V plane stride in bytes.
    pub v_stride: usize,
    /// Alpha plane data (optional).
    pub a: Option<&'a [u8]>,
    /// Alpha plane stride in bytes.
    pub a_stride: usize,
    /// Image width.
    pub width: u32,
    /// Image height.
    pub height: u32,
}

impl<'a> From<&'a YuvPlanes> for YuvPlanesRef<'a> {
    fn from(planes: &'a YuvPlanes) -> Self {
        Self {
            y: &planes.y,
            y_stride: planes.y_stride,
            u: &planes.u,
            u_stride: planes.u_stride,
            v: &planes.v,
            v_stride: planes.v_stride,
            a: planes.a.as_deref(),
            a_stride: planes.a_stride,
            width: planes.width,
            height: planes.height,
        }
    }
}
