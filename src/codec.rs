//! `zencodec` trait implementations for `webpx`.
//!
//! Mirrors `zenwebp::zencodec`'s public types so downstream code that
//! consumes the trait surface compiles unchanged when swapping crates:
//!
//! ```rust,ignore
//! // With webpx:
//! use webpx::zencodec::WebpEncoderConfig;
//! // Or with zenwebp:
//! use zenwebp::zencodec::WebpEncoderConfig;
//!
//! use zencodec::encode::EncoderConfig;
//! let cfg = WebpEncoderConfig::lossy().with_generic_quality(85.0);
//! ```
//!
//! Single-image encode and decode are fully wired through the
//! `zencodec` trait surface. Animation and incremental streaming are
//! exposed as `zencodec` types but currently surface
//! `Error::InvalidConfig` from their constructors — the native webpx
//! API on `crate::AnimationDecoder` / `StreamingDecoder` covers those
//! paths today; the trait wrapping will land in a follow-up.

use alloc::borrow::Cow;
use alloc::format;
use alloc::sync::Arc;
use alloc::vec::Vec;

use whereat::{At, at};
use zencodec::decode::{DecodeOutput, DecodeRowSink, OutputInfo};
use zencodec::encode::EncodeOutput;
use zencodec::{
    ImageFormat, ImageInfo, ImageSequence, Metadata, ResourceLimits, UnsupportedOperation,
};
use zenpixels::{PixelBuffer, PixelDescriptor, PixelSlice};

use crate::error::Error;
use crate::{ColorMode, DecoderConfig, EncoderConfig, Limits};

// ── Encoder side ───────────────────────────────────────────────────────────

/// WebP encoder configuration implementing
/// [`zencodec::encode::EncoderConfig`].
#[derive(Clone, Debug)]
pub struct WebpEncoderConfig {
    inner: EncoderConfig,
    lossless_flag: bool,
    trait_quality: Option<f32>,
    trait_effort: Option<i32>,
    alpha_quality_override: Option<f32>,
}

impl WebpEncoderConfig {
    /// Lossy encoder config with library defaults.
    #[must_use]
    pub fn lossy() -> Self {
        Self {
            inner: EncoderConfig::new(),
            lossless_flag: false,
            trait_quality: None,
            trait_effort: None,
            alpha_quality_override: None,
        }
    }

    /// Lossless encoder config with library defaults.
    #[must_use]
    pub fn lossless() -> Self {
        Self {
            inner: EncoderConfig::new().lossless(true),
            lossless_flag: true,
            trait_quality: None,
            trait_effort: None,
            alpha_quality_override: None,
        }
    }

    /// Construct from a webpx [`crate::EncoderConfig`] directly.
    #[must_use]
    pub fn from_native(inner: EncoderConfig) -> Self {
        let lossless_flag = inner.is_lossless();
        Self {
            inner,
            lossless_flag,
            trait_quality: None,
            trait_effort: None,
            alpha_quality_override: None,
        }
    }

    /// Borrow the underlying webpx [`crate::EncoderConfig`].
    #[must_use]
    pub fn inner(&self) -> &EncoderConfig {
        &self.inner
    }

    /// Native webpx quality (0.0–100.0).
    #[must_use]
    pub fn with_quality(mut self, quality: f32) -> Self {
        self.inner = self.inner.clone().quality(quality);
        self
    }

    /// Native webpx encoding method (0–6).
    #[must_use]
    pub fn with_method(mut self, method: u8) -> Self {
        self.inner = self.inner.clone().method(method);
        self
    }

    /// Content-aware preset (lossy only).
    #[must_use]
    pub fn with_preset_value(mut self, preset: crate::Preset) -> Self {
        self.inner = self.inner.clone().preset(preset);
        self
    }

    /// Alpha plane quality (0–100). No-op when the alpha plane is absent.
    #[must_use]
    pub fn with_alpha_quality_value(mut self, quality: u8) -> Self {
        self.inner = self.inner.clone().alpha_quality(quality);
        self
    }
}

static ENCODE_DESCRIPTORS: &[PixelDescriptor] = &[
    PixelDescriptor::RGBA8_SRGB,
    PixelDescriptor::BGRA8_SRGB,
    PixelDescriptor::RGB8_SRGB,
];

static ENCODE_CAPABILITIES: zencodec::encode::EncodeCapabilities =
    zencodec::encode::EncodeCapabilities::new()
        .with_icc(true)
        .with_exif(true)
        .with_xmp(true)
        .with_stop(true)
        .with_lossy(true)
        .with_lossless(true)
        .with_native_alpha(true)
        .with_effort_range(0, 6)
        .with_quality_range(0.0, 100.0)
        .with_enforces_max_pixels(true);

fn calibrated_webp_quality(generic_q: f32) -> f32 {
    generic_q.clamp(0.0, 100.0)
}

fn effort_to_method(effort: i32) -> u8 {
    let e = effort.clamp(0, 10) as u32;
    ((e * 6) / 10).min(6) as u8
}

impl zencodec::encode::EncoderConfig for WebpEncoderConfig {
    type Error = At<Error>;
    type Job = WebpEncodeJob;

    fn format() -> ImageFormat {
        ImageFormat::WebP
    }

    fn supported_descriptors() -> &'static [PixelDescriptor] {
        ENCODE_DESCRIPTORS
    }

    fn capabilities() -> &'static zencodec::encode::EncodeCapabilities {
        &ENCODE_CAPABILITIES
    }

    fn with_generic_quality(mut self, quality: f32) -> Self {
        let clamped = quality.clamp(0.0, 100.0);
        self.trait_quality = Some(clamped);
        self.inner = self.inner.clone().quality(calibrated_webp_quality(clamped));
        self
    }

    fn generic_quality(&self) -> Option<f32> {
        self.trait_quality
    }

    fn with_generic_effort(mut self, effort: i32) -> Self {
        let clamped = effort.clamp(0, 10);
        self.trait_effort = Some(clamped);
        self.inner = self.inner.clone().method(effort_to_method(clamped));
        self
    }

    fn generic_effort(&self) -> Option<i32> {
        self.trait_effort
    }

    fn with_lossless(mut self, lossless: bool) -> Self {
        self.lossless_flag = lossless;
        self.inner = self.inner.clone().lossless(lossless);
        self
    }

    fn is_lossless(&self) -> Option<bool> {
        Some(self.lossless_flag)
    }

    fn with_alpha_quality(mut self, quality: f32) -> Self {
        let aq = quality.clamp(0.0, 100.0);
        self.alpha_quality_override = Some(aq);
        self.inner = self.inner.clone().alpha_quality(aq as u8);
        self
    }

    fn alpha_quality(&self) -> Option<f32> {
        self.alpha_quality_override
    }

    fn job(self) -> WebpEncodeJob {
        WebpEncodeJob {
            config: self,
            stop: None,
            icc: None,
            exif: None,
            xmp: None,
            limits: ResourceLimits::none(),
        }
    }
}

/// Per-operation WebP encode job.
pub struct WebpEncodeJob {
    config: WebpEncoderConfig,
    stop: Option<zencodec::StopToken>,
    icc: Option<Arc<[u8]>>,
    exif: Option<Arc<[u8]>>,
    xmp: Option<Arc<[u8]>>,
    limits: ResourceLimits,
}

impl zencodec::encode::EncodeJob for WebpEncodeJob {
    type Error = At<Error>;
    type Enc = WebpEncoder;
    type AnimationFrameEnc = WebpAnimationFrameEncoder;

    fn with_stop(mut self, stop: zencodec::StopToken) -> Self {
        self.stop = Some(stop);
        self
    }

    fn with_metadata(mut self, meta: Metadata) -> Self {
        self.icc = meta.icc_profile;
        self.exif = meta.exif;
        self.xmp = meta.xmp;
        self
    }

    fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    fn encoder(self) -> Result<WebpEncoder, At<Error>> {
        Ok(WebpEncoder {
            config: self.config.inner.clone(),
            stop: self.stop,
            icc: self.icc,
            exif: self.exif,
            xmp: self.xmp,
            limits: Limits::from(self.limits),
        })
    }

    fn animation_frame_encoder(self) -> Result<WebpAnimationFrameEncoder, At<Error>> {
        Err(at!(Error::InvalidConfig(
            "WebpAnimationFrameEncoder via the zencodec trait surface is not yet wired in webpx — \
             use the native crate::AnimationEncoder API for now"
                .into(),
        )))
    }
}

/// WebP single-image encoder.
pub struct WebpEncoder {
    config: EncoderConfig,
    stop: Option<zencodec::StopToken>,
    icc: Option<Arc<[u8]>>,
    exif: Option<Arc<[u8]>>,
    xmp: Option<Arc<[u8]>>,
    limits: Limits,
}

#[allow(clippy::type_complexity)] // Single internal helper; named struct would add boilerplate without clarity.
fn pixels_to_webp_input<'a>(
    pixels: &'a PixelSlice<'a>,
) -> Result<(Cow<'a, [u8]>, ColorMode, u32, u32, u32), At<Error>> {
    let desc = pixels.descriptor();
    let w = pixels.width();
    let h = pixels.rows();
    let stride_bytes = pixels.stride() as u32;

    if desc == PixelDescriptor::RGBA8_SRGB {
        Ok((
            Cow::Borrowed(pixels.as_strided_bytes()),
            ColorMode::Rgba,
            w,
            h,
            stride_bytes,
        ))
    } else if desc == PixelDescriptor::BGRA8_SRGB {
        Ok((
            Cow::Borrowed(pixels.as_strided_bytes()),
            ColorMode::Bgra,
            w,
            h,
            stride_bytes,
        ))
    } else if desc == PixelDescriptor::RGB8_SRGB {
        Ok((
            Cow::Borrowed(pixels.as_strided_bytes()),
            ColorMode::Rgb,
            w,
            h,
            stride_bytes,
        ))
    } else {
        Err(at!(Error::InvalidInput(format!(
            "unsupported pixel format for WebP encode: {:?}",
            desc
        ))))
    }
}

fn apply_metadata(
    webp: Vec<u8>,
    icc: Option<&[u8]>,
    exif: Option<&[u8]>,
    xmp: Option<&[u8]>,
) -> Result<Vec<u8>, At<Error>> {
    #[cfg(feature = "icc")]
    {
        let mut webp = webp;
        if let Some(icc) = icc {
            webp = crate::embed_icc(&webp, icc)?;
        }
        if let Some(exif) = exif {
            webp = crate::embed_exif(&webp, exif)?;
        }
        if let Some(xmp) = xmp {
            webp = crate::embed_xmp(&webp, xmp)?;
        }
        Ok(webp)
    }
    #[cfg(not(feature = "icc"))]
    {
        if icc.is_some() || exif.is_some() || xmp.is_some() {
            return Err(at!(Error::InvalidConfig(
                "webpx built without `icc` feature; metadata cannot be embedded".into(),
            )));
        }
        Ok(webp)
    }
}

/// Bridges `zencodec::StopToken` into `enough::Stop`.
struct StopBridge<'a>(Option<&'a zencodec::StopToken>);

impl enough::Stop for StopBridge<'_> {
    fn check(&self) -> Result<(), enough::StopReason> {
        match self.0 {
            Some(t) => t.check(),
            None => Ok(()),
        }
    }
    fn should_stop(&self) -> bool {
        self.0.map(|t| t.should_stop()).unwrap_or(false)
    }
}

impl zencodec::encode::Encoder for WebpEncoder {
    type Error = At<Error>;

    fn reject(op: UnsupportedOperation) -> At<Error> {
        at!(Error::InvalidConfig(format!(
            "operation not supported by webpx WebP encoder: {op:?}"
        )))
    }

    fn encode(self, pixels: PixelSlice<'_>) -> Result<EncodeOutput, At<Error>> {
        let (buf, mode, w, h, stride_bytes) = pixels_to_webp_input(&pixels)?;
        self.limits
            .check_dimensions(w, h)
            .map_err(|e| at!(Error::LimitExceeded(e)))?;

        let stop = StopBridge(self.stop.as_ref());
        let webp = match mode {
            ColorMode::Rgba => crate::Encoder::new_rgba_stride(&buf, w, h, stride_bytes)
                .config(self.config.clone())
                .encode(&stop)?,
            ColorMode::Bgra => crate::Encoder::new_bgra_stride(&buf, w, h, stride_bytes)
                .config(self.config.clone())
                .encode(&stop)?,
            ColorMode::Rgb => crate::Encoder::new_rgb_stride(&buf, w, h, stride_bytes)
                .config(self.config.clone())
                .encode(&stop)?,
            _ => unreachable!(),
        };

        let webp = apply_metadata(
            webp,
            self.icc.as_deref(),
            self.exif.as_deref(),
            self.xmp.as_deref(),
        )?;

        self.limits
            .check_output_size(webp.len() as u64)
            .map_err(|e| at!(Error::LimitExceeded(e)))?;

        Ok(EncodeOutput::new(webp, ImageFormat::WebP))
    }
}

/// Animation encoder. Currently surfaces an
/// `Error::InvalidConfig` from constructor paths — see module docs.
pub struct WebpAnimationFrameEncoder {
    _marker: core::marker::PhantomData<()>,
}

impl zencodec::encode::AnimationFrameEncoder for WebpAnimationFrameEncoder {
    type Error = At<Error>;

    fn reject(_op: UnsupportedOperation) -> At<Error> {
        at!(Error::InvalidConfig(
            "WebpAnimationFrameEncoder not yet wired through zencodec; use crate::AnimationEncoder"
                .into(),
        ))
    }

    fn push_frame(
        &mut self,
        _pixels: PixelSlice<'_>,
        _duration_ms: u32,
        _stop: Option<&dyn enough::Stop>,
    ) -> Result<(), At<Error>> {
        Err(Self::reject(UnsupportedOperation::AnimationEncode))
    }

    fn finish(self, _stop: Option<&dyn enough::Stop>) -> Result<EncodeOutput, At<Error>> {
        Err(Self::reject(UnsupportedOperation::AnimationEncode))
    }
}

// ── Decoder side ───────────────────────────────────────────────────────────

/// WebP decoder configuration.
#[derive(Clone, Debug, Default)]
pub struct WebpDecoderConfig {
    inner: DecoderConfig,
}

impl WebpDecoderConfig {
    /// Decoder config with library defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: DecoderConfig::new(),
        }
    }

    /// Construct from a webpx [`crate::DecoderConfig`] directly.
    #[must_use]
    pub fn from_native(inner: DecoderConfig) -> Self {
        Self { inner }
    }
}

static DECODE_FORMATS: &[ImageFormat] = &[ImageFormat::WebP];

static DECODE_DESCRIPTORS: &[PixelDescriptor] = &[
    PixelDescriptor::RGBA8_SRGB,
    PixelDescriptor::BGRA8_SRGB,
    PixelDescriptor::RGB8_SRGB,
];

static DECODE_CAPABILITIES: zencodec::decode::DecodeCapabilities =
    zencodec::decode::DecodeCapabilities::new()
        .with_icc(true)
        .with_exif(true)
        .with_xmp(true)
        .with_stop(true)
        .with_enforces_max_pixels(true);

impl zencodec::decode::DecoderConfig for WebpDecoderConfig {
    type Error = At<Error>;
    type Job<'a> = WebpDecodeJob;

    fn formats() -> &'static [ImageFormat] {
        DECODE_FORMATS
    }

    fn supported_descriptors() -> &'static [PixelDescriptor] {
        DECODE_DESCRIPTORS
    }

    fn capabilities() -> &'static zencodec::decode::DecodeCapabilities {
        &DECODE_CAPABILITIES
    }

    fn job<'a>(self) -> Self::Job<'a> {
        WebpDecodeJob {
            config: self,
            stop: None,
            limits: ResourceLimits::none(),
        }
    }
}

/// Per-operation WebP decode job.
pub struct WebpDecodeJob {
    config: WebpDecoderConfig,
    stop: Option<zencodec::StopToken>,
    limits: ResourceLimits,
}

fn build_zencodec_image_info(info: crate::ImageInfo) -> ImageInfo {
    let mut out = ImageInfo::new(info.width, info.height, ImageFormat::WebP);
    if info.has_alpha {
        out = out.with_alpha(true);
    }
    if info.has_animation {
        out = out.with_sequence(ImageSequence::Animation {
            frame_count: Some(info.frame_count),
            loop_count: None,
            random_access: false,
        });
    }
    out
}

fn pick_decode_descriptor(preferred: &[PixelDescriptor]) -> PixelDescriptor {
    for &p in preferred {
        if p == PixelDescriptor::RGBA8_SRGB
            || p == PixelDescriptor::BGRA8_SRGB
            || p == PixelDescriptor::RGB8_SRGB
        {
            return p;
        }
    }
    PixelDescriptor::RGBA8_SRGB
}

impl<'a> zencodec::decode::DecodeJob<'a> for WebpDecodeJob {
    type Error = At<Error>;
    type Dec = WebpDecoder<'a>;
    type StreamDec = WebpStreamingDecoder;
    type AnimationFrameDec = WebpAnimationFrameDecoder;

    fn with_stop(mut self, stop: zencodec::StopToken) -> Self {
        self.stop = Some(stop);
        self
    }

    fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    fn probe(&self, data: &[u8]) -> Result<ImageInfo, At<Error>> {
        let info = crate::ImageInfo::from_webp(data)?;
        Ok(build_zencodec_image_info(info))
    }

    fn output_info(&self, data: &[u8]) -> Result<OutputInfo, At<Error>> {
        let info = crate::ImageInfo::from_webp(data)?;
        let descriptor = if info.has_alpha {
            PixelDescriptor::RGBA8_SRGB
        } else {
            PixelDescriptor::RGB8_SRGB
        };
        Ok(OutputInfo::full_decode(info.width, info.height, descriptor))
    }

    fn decoder(
        self,
        data: Cow<'a, [u8]>,
        preferred: &[PixelDescriptor],
    ) -> Result<WebpDecoder<'a>, At<Error>> {
        let chosen = pick_decode_descriptor(preferred);
        Ok(WebpDecoder {
            data,
            config: self.config.inner,
            stop: self.stop,
            chosen,
            limits: Limits::from(self.limits),
        })
    }

    fn push_decoder(
        self,
        data: Cow<'a, [u8]>,
        sink: &mut dyn DecodeRowSink,
        preferred: &[PixelDescriptor],
    ) -> Result<OutputInfo, At<Error>> {
        // Decode normally, then hand the bytes to the sink as a single
        // strip. A future iteration could push the libwebp decoder
        // through scanlines, but row-level WebP decoding requires the
        // incremental API plus more bookkeeping than we want here.
        let dec_job = self;
        let info_z = dec_job.output_info(&data)?;
        let chosen = pick_decode_descriptor(preferred);
        let dec = WebpDecoder {
            data,
            config: dec_job.config.inner,
            stop: dec_job.stop,
            chosen,
            limits: Limits::from(dec_job.limits),
        };
        let out = <WebpDecoder<'_> as zencodec::decode::Decode>::decode(dec)?;
        let buf = out.into_buffer();
        let src = buf.as_slice();
        let w = src.width();
        let h = src.rows();
        sink.begin(w, h, chosen)
            .map_err(|e| at!(Error::InvalidInput(format!("sink begin: {e}"))))?;
        let mut dst = sink
            .provide_next_buffer(0, h, w, chosen)
            .map_err(|e| at!(Error::InvalidInput(format!("sink provide: {e}"))))?;
        let src_bytes = src.as_strided_bytes();
        let bpp = chosen.bytes_per_pixel();
        let row_bytes = w as usize * bpp;
        let src_stride = src.stride();
        for y in 0..h as usize {
            let src_row = &src_bytes[y * src_stride..y * src_stride + row_bytes];
            let dst_row = dst.row_mut(y as u32);
            dst_row[..row_bytes].copy_from_slice(src_row);
        }
        sink.finish()
            .map_err(|e| at!(Error::InvalidInput(format!("sink finish: {e}"))))?;
        Ok(info_z)
    }

    fn streaming_decoder(
        self,
        _data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> Result<WebpStreamingDecoder, At<Error>> {
        Err(at!(Error::InvalidConfig(
            "WebpStreamingDecoder via the zencodec trait surface is not yet wired in webpx".into(),
        )))
    }

    fn animation_frame_decoder(
        self,
        _data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> Result<WebpAnimationFrameDecoder, At<Error>> {
        Err(at!(Error::InvalidConfig(
            "WebpAnimationFrameDecoder via the zencodec trait surface is not yet wired in webpx"
                .into(),
        )))
    }
}

/// WebP single-image decoder.
pub struct WebpDecoder<'a> {
    data: Cow<'a, [u8]>,
    config: DecoderConfig,
    stop: Option<zencodec::StopToken>,
    chosen: PixelDescriptor,
    limits: Limits,
}

impl zencodec::decode::Decode for WebpDecoder<'_> {
    type Error = At<Error>;

    fn decode(self) -> Result<DecodeOutput, At<Error>> {
        let _ = &self.stop;
        let cfg = self.config.limits(self.limits);
        let dec = crate::Decoder::new(&self.data)?.config(cfg);
        let (bytes, w, h, descriptor) = match self.chosen {
            PixelDescriptor::RGBA8_SRGB => {
                let (b, w, h) = dec.decode_rgba_raw()?;
                (b, w, h, PixelDescriptor::RGBA8_SRGB)
            }
            PixelDescriptor::BGRA8_SRGB => {
                let (b, w, h) = dec.decode_bgra_raw()?;
                (b, w, h, PixelDescriptor::BGRA8_SRGB)
            }
            PixelDescriptor::RGB8_SRGB => {
                let (b, w, h) = dec.decode_rgb_raw()?;
                (b, w, h, PixelDescriptor::RGB8_SRGB)
            }
            _ => unreachable!(),
        };

        let buf = PixelBuffer::from_vec(bytes, w, h, descriptor)
            .map_err(|e| at!(Error::InvalidInput(format!("{e:?}"))))?;
        let info = ImageInfo::new(w, h, ImageFormat::WebP).with_alpha(descriptor.has_alpha());
        Ok(DecodeOutput::new(buf, info))
    }
}

/// Streaming decoder. Constructor surfaces an
/// `Error::InvalidConfig` for now — see module docs.
pub struct WebpStreamingDecoder {
    _marker: core::marker::PhantomData<()>,
}

impl zencodec::decode::StreamingDecode for WebpStreamingDecoder {
    type Error = At<Error>;

    fn next_batch(&mut self) -> Result<Option<(u32, PixelSlice<'_>)>, At<Error>> {
        Err(at!(Error::InvalidConfig(
            "WebpStreamingDecoder not wired through zencodec; use crate::StreamingDecoder".into(),
        )))
    }

    fn info(&self) -> &ImageInfo {
        // This panic is unreachable in normal use because `streaming_decoder`
        // never returns Ok(_) — callers can never get a `WebpStreamingDecoder`
        // instance through the trait surface.
        panic!("WebpStreamingDecoder::info called on stub instance")
    }
}

/// Animation frame decoder. Constructor surfaces an
/// `Error::InvalidConfig` for now — see module docs.
pub struct WebpAnimationFrameDecoder {
    _marker: core::marker::PhantomData<()>,
}

impl zencodec::decode::AnimationFrameDecoder for WebpAnimationFrameDecoder {
    type Error = At<Error>;

    fn wrap_sink_error(e: zencodec::decode::SinkError) -> At<Error> {
        at!(Error::InvalidInput(format!("sink error: {e}")))
    }

    fn info(&self) -> &ImageInfo {
        panic!("WebpAnimationFrameDecoder::info called on stub instance")
    }

    fn render_next_frame(
        &mut self,
        _stop: Option<&dyn enough::Stop>,
    ) -> Result<Option<zencodec::decode::AnimationFrame<'_>>, At<Error>> {
        Err(at!(Error::InvalidConfig(
            "WebpAnimationFrameDecoder not wired through zencodec; use crate::AnimationDecoder"
                .into(),
        )))
    }

    fn render_next_frame_to_sink(
        &mut self,
        _stop: Option<&dyn enough::Stop>,
        _sink: &mut dyn zencodec::decode::DecodeRowSink,
    ) -> Result<Option<OutputInfo>, At<Error>> {
        Err(at!(Error::InvalidConfig(
            "WebpAnimationFrameDecoder not wired through zencodec; use crate::AnimationDecoder"
                .into(),
        )))
    }
}
