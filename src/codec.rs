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
//! All four executor traits — `Encoder`, `Decode`,
//! `AnimationFrameEncoder`, `AnimationFrameDecoder`, `StreamingDecode`
//! — are wired through `zencodec`. Animation and streaming require the
//! matching webpx Cargo features (`animation`, `streaming`); when
//! disabled, the corresponding wrapper types stub out and surface
//! `Error::InvalidConfig` from their constructors.

use alloc::borrow::Cow;
use alloc::format;
use alloc::sync::Arc;
use alloc::vec::Vec;

use whereat::{At, ResultAtExt, at};
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

/// Piecewise-linear lookup with clamping at the table bounds; `table` is sorted
/// ascending by metric. Shared by the Fidelity metric-target arms.
fn interp_quality(table: &[(f32, f32)], x: f32) -> f32 {
    if x <= table[0].0 {
        return table[0].1;
    }
    let last = table[table.len() - 1];
    if x >= last.0 {
        return last.1;
    }
    for w in table.windows(2) {
        if x <= w[1].0 {
            let t = (x - w[0].0) / (w[1].0 - w[0].0);
            return w[0].1 + t * (w[1].1 - w[0].1);
        }
    }
    last.1
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

    /// Honor a [`Fidelity`](zencodec::encode::Fidelity) target as natively as
    /// WebP allows.
    ///
    /// - `Lossless` → VP8L lossless.
    /// - `Lossy(CodecSpecificQuality(q))` → the VP8 quality dial.
    /// - `Lossy(ApproxSsim2(s))` → the WebP quality that *measured* SSIM2 `s`
    ///   (no native SSIM2 loop; reported as `codec_quality`).
    /// - `Lossy(ApproxButteraugli(d))` → the WebP quality that measured
    ///   butteraugli max-norm `d`.
    ///
    /// The two metric arms interpolate a measured inverse table (median over
    /// 600 images × 3 knobs × 15 q from the cvvdp-v15rc / multi-codec omni
    /// fleet sweep, 2026-06-23) instead of the old linear `100 − 12·d` guess.
    /// The sweep drove the **native** VP8 quality dial, and `webpx`'s
    /// `calibrated_webp_quality` is the identity, so `with_generic_quality`
    /// feeds the table value to libwebp's `quality` un-remapped — the same
    /// operating point the sweep measured (libwebp tracks the zenwebp encoder
    /// the table was swept on to ~0.3 % bytes at matched quality). See
    /// `zenwebp/benchmarks/codec_metric_to_q_2026-06-23.md`.
    ///
    /// The lossy arms switch to VP8 first (`with_lossless(false)`) so the
    /// request is honored and `resolved_target_fidelity` reports it correctly
    /// even from a lossless-configured start.
    fn with_fidelity(self, fidelity: zencodec::encode::Fidelity) -> Self {
        use zencodec::encode::{Fidelity, LossyTarget};

        // Measured metric → VP8-quality inverse tables, sorted ascending by the
        // metric (provenance above). `BUTTER_MAX_TO_Q`: butteraugli max-norm
        // (lower better, quality descends); `SSIM2_TO_Q`: SSIM2 (higher better,
        // quality ascends).
        const BUTTER_MAX_TO_Q: &[(f32, f32)] = &[
            (2.41, 95.0),
            (2.57, 90.0),
            (2.86, 85.0),
            (2.93, 80.0),
            (3.39, 75.0),
            (3.51, 65.0),
            (3.72, 60.0),
            (3.80, 55.0),
            (4.00, 45.0),
            (4.90, 35.0),
            (5.57, 25.0),
            (6.55, 15.0),
            (7.47, 10.0),
            (9.07, 5.0),
        ];
        const SSIM2_TO_Q: &[(f32, f32)] = &[
            (53.1, 5.0),
            (58.3, 10.0),
            (62.6, 15.0),
            (69.0, 25.0),
            (71.1, 30.0),
            (73.2, 35.0),
            (76.6, 45.0),
            (78.8, 55.0),
            (79.7, 60.0),
            (80.5, 65.0),
            (82.1, 75.0),
            (84.0, 80.0),
            (85.3, 85.0),
            (86.8, 90.0),
            (87.8, 95.0),
        ];

        match fidelity {
            Fidelity::Lossless => self.with_lossless(true),
            Fidelity::Lossy(LossyTarget::CodecSpecificQuality(q)) => {
                self.with_lossless(false).with_generic_quality(q)
            }
            Fidelity::Lossy(LossyTarget::ApproxSsim2(s)) => {
                let q = interp_quality(SSIM2_TO_Q, s).clamp(0.0, 100.0);
                self.with_lossless(false).with_generic_quality(q)
            }
            Fidelity::Lossy(LossyTarget::ApproxButteraugli(d)) => {
                let q = interp_quality(BUTTER_MAX_TO_Q, d).clamp(0.0, 100.0);
                self.with_lossless(false).with_generic_quality(q)
            }
            // `Fidelity` / `LossyTarget` are `#[non_exhaustive]`.
            _ => self.with_lossless(false),
        }
    }

    fn resolved_target_fidelity(&self) -> Option<zencodec::encode::Fidelity> {
        use zencodec::encode::Fidelity;
        if self.is_lossless() == Some(true) {
            Some(Fidelity::Lossless)
        } else {
            self.generic_quality().map(Fidelity::codec_quality)
        }
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
        #[cfg(not(feature = "animation"))]
        {
            Err(at!(Error::InvalidConfig(
                "webpx built without `animation` feature".into(),
            )))
        }
        #[cfg(feature = "animation")]
        {
            // The trait contract gives us pixel data on push_frame, so
            // we can't construct the underlying encoder until the first
            // frame arrives — that's the only point we know the
            // canvas dimensions. Defer the AnimationEncoder::new call.
            Ok(WebpAnimationFrameEncoder {
                inner: None,
                config: self.config.inner.clone(),
                stop: self.stop,
                icc: self.icc,
                exif: self.exif,
                xmp: self.xmp,
                limits: Limits::from(self.limits),
                last_timestamp_ms: 0,
                canvas_dims: None,
            })
        }
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

/// WebP animation-frame encoder implementing
/// [`zencodec::encode::AnimationFrameEncoder`].
///
/// The underlying [`crate::AnimationEncoder`] requires canvas
/// dimensions at construction, but `zencodec`'s trait gives us pixels
/// only at `push_frame` time. We defer the underlying encoder's
/// construction until the first frame arrives so we can read its
/// dimensions; subsequent frames must match.
#[cfg(feature = "animation")]
pub struct WebpAnimationFrameEncoder {
    inner: Option<crate::AnimationEncoder>,
    config: EncoderConfig,
    /// Stored at construction time but not currently propagated to
    /// libwebp's animation encoder (which doesn't take a stop token).
    /// Keep the field so `with_stop` from the EncodeJob still flows
    /// through and a future libwebp/webpx version can wire it in.
    #[allow(dead_code)]
    stop: Option<zencodec::StopToken>,
    icc: Option<Arc<[u8]>>,
    exif: Option<Arc<[u8]>>,
    xmp: Option<Arc<[u8]>>,
    limits: Limits,
    /// Cumulative timestamp of frames added so far. webpx's animation
    /// API takes per-frame timestamps; we track them ourselves so the
    /// trait's `duration_ms` parameter maps cleanly.
    last_timestamp_ms: u32,
    /// Canvas dimensions captured from the first frame (lazy init).
    /// Subsequent frames must match these — libwebp's animation
    /// encoder is fixed-canvas, so a mismatch silently corrupts output
    /// or panics inside libwebp. Reject up front instead.
    canvas_dims: Option<(u32, u32)>,
}

// `inner: Option<crate::AnimationEncoder>` carries `crate::AnimationEncoder`
// which has its own `Drop` impl that calls `WebPAnimEncoderDelete`. So
// when `WebpAnimationFrameEncoder` is dropped without `finish()` being
// called (panic, premature drop, error before finish), the inner
// encoder's Drop fires automatically and frees libwebp's resources.
// No explicit `impl Drop` for `WebpAnimationFrameEncoder` is required —
// the field-level Drop already does the right thing.

#[cfg(feature = "animation")]
impl zencodec::encode::AnimationFrameEncoder for WebpAnimationFrameEncoder {
    type Error = At<Error>;

    fn reject(op: UnsupportedOperation) -> At<Error> {
        at!(Error::InvalidConfig(format!(
            "operation not supported by webpx animation encoder: {op:?}"
        )))
    }

    fn push_frame(
        &mut self,
        pixels: PixelSlice<'_>,
        duration_ms: u32,
        _stop: Option<&dyn enough::Stop>,
    ) -> Result<(), At<Error>> {
        let (buf, mode, w, h, _stride_bytes) = pixels_to_webp_input(&pixels)?;

        self.limits
            .check_dimensions(w, h)
            .map_err(|e| at!(Error::LimitExceeded(e)))?;

        // Construct the underlying AnimationEncoder lazily on the
        // first frame so we know the canvas size. Subsequent frames
        // must match — libwebp's animation encoder is fixed-canvas.
        match self.canvas_dims {
            None => {
                let mut enc = crate::AnimationEncoder::new(w, h)?;
                enc.set_quality(self.config.get_quality());
                if self.config.is_lossless() {
                    enc.set_lossless(true);
                }
                self.inner = Some(enc);
                self.canvas_dims = Some((w, h));
            }
            Some((cw, ch)) if cw != w || ch != h => {
                return Err(at!(Error::InvalidInput(format!(
                    "animation frame dimensions {}x{} do not match canvas {}x{} \
                     (set on first frame)",
                    w, h, cw, ch
                ))));
            }
            Some(_) => {
                // Dimensions match; nothing to do.
            }
        }

        let enc = self.inner.as_mut().expect("just initialized");
        let timestamp_ms = self.last_timestamp_ms as i32;

        // The native `add_frame_*` family takes tightly-packed pixel
        // slices (stride = width * bpp). When the caller passes a
        // strided slice, copy into a contiguous buffer first. The
        // common case (stride == width * bpp) is zero-copy via
        // `Cow::Borrowed` from `pixels_to_webp_input`.
        let bpp = match mode {
            ColorMode::Rgba | ColorMode::Bgra => 4,
            ColorMode::Rgb | ColorMode::Bgr => 3,
            _ => unreachable!(),
        };
        let row_bytes = (w as usize).saturating_mul(bpp);
        let stride_bytes_usize = pixels.stride();
        let contiguous: Cow<'_, [u8]> = if stride_bytes_usize == row_bytes {
            buf
        } else {
            // Strided source: copy into a tightly-packed buffer.
            let mut packed = alloc::vec![0u8; row_bytes * h as usize];
            for y in 0..h as usize {
                let src = &buf[y * stride_bytes_usize..y * stride_bytes_usize + row_bytes];
                packed[y * row_bytes..(y + 1) * row_bytes].copy_from_slice(src);
            }
            Cow::Owned(packed)
        };

        match mode {
            ColorMode::Rgba => enc.add_frame_rgba(&contiguous, timestamp_ms)?,
            ColorMode::Bgra => enc.add_frame_bgra(&contiguous, timestamp_ms)?,
            ColorMode::Rgb => enc.add_frame_rgb(&contiguous, timestamp_ms)?,
            ColorMode::Bgr => enc.add_frame_bgr(&contiguous, timestamp_ms)?,
            _ => unreachable!(),
        }

        self.last_timestamp_ms = self.last_timestamp_ms.saturating_add(duration_ms);
        Ok(())
    }

    fn finish(mut self, _stop: Option<&dyn enough::Stop>) -> Result<EncodeOutput, At<Error>> {
        let enc = self.inner.take().ok_or_else(|| {
            at!(Error::InvalidConfig(
                "AnimationFrameEncoder::finish called before any push_frame".into(),
            ))
        })?;
        let webp = enc.finish(self.last_timestamp_ms as i32)?;
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

/// Stub when the `animation` feature is off.
#[cfg(not(feature = "animation"))]
pub struct WebpAnimationFrameEncoder {
    _marker: core::marker::PhantomData<()>,
}

#[cfg(not(feature = "animation"))]
impl zencodec::encode::AnimationFrameEncoder for WebpAnimationFrameEncoder {
    type Error = At<Error>;

    fn reject(_op: UnsupportedOperation) -> At<Error> {
        at!(Error::InvalidConfig(
            "webpx built without `animation` feature".into(),
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
        let row_bytes = (w as usize).saturating_mul(bpp);
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
        #[cfg(not(feature = "streaming"))]
        {
            Err(at!(Error::InvalidConfig(
                "webpx built without `streaming` feature".into(),
            )))
        }
        #[cfg(feature = "streaming")]
        {
            // Probe the bitstream up-front for the ImageInfo we need
            // to expose via the trait, plus to surface limit
            // violations before any allocation.
            let probe = crate::ImageInfo::from_webp(&_data)?;
            let limits = Limits::from(self.limits);
            limits
                .check_dimensions(probe.width, probe.height)
                .map_err(|e| at!(Error::LimitExceeded(e)))?;

            let descriptor = pick_decode_descriptor(_preferred);
            let zen_info = build_zencodec_image_info(probe);
            // Buffered approach: stash the input, expose the whole
            // image as a single batch via the native one-shot
            // decoder. A row-by-row implementation against
            // crate::StreamingDecoder is possible but that API takes
            // mutable input chunks and we already own the data here;
            // the buffered shape is simpler and matches zenwebp's.
            Ok(WebpStreamingDecoder {
                data: _data.into_owned(),
                descriptor,
                info: zen_info,
                config: self.config.inner,
                limits,
                emitted: false,
                decoded: None,
            })
        }
    }

    fn animation_frame_decoder(
        self,
        _data: Cow<'a, [u8]>,
        _preferred: &[PixelDescriptor],
    ) -> Result<WebpAnimationFrameDecoder, At<Error>> {
        #[cfg(not(feature = "animation"))]
        {
            Err(at!(Error::InvalidConfig(
                "webpx built without `animation` feature".into(),
            )))
        }
        #[cfg(feature = "animation")]
        {
            // libwebp's animation decoder only produces RGBA / BGRA;
            // map preferred descriptor to one of those.
            let mode = match pick_decode_descriptor(_preferred) {
                PixelDescriptor::BGRA8_SRGB => ColorMode::Bgra,
                _ => ColorMode::Rgba,
            };
            let descriptor = match mode {
                ColorMode::Bgra => PixelDescriptor::BGRA8_SRGB,
                _ => PixelDescriptor::RGBA8_SRGB,
            };
            let owned = _data.into_owned();
            let limits = Limits::from(self.limits);
            let inner = crate::AnimationDecoder::with_options_limits(&owned, mode, true, &limits)?;
            let n_info = inner.info();
            let zen_info = ImageInfo::new(n_info.width, n_info.height, ImageFormat::WebP)
                .with_alpha(true)
                .with_sequence(ImageSequence::Animation {
                    frame_count: Some(n_info.frame_count),
                    loop_count: Some(n_info.loop_count),
                    random_access: false,
                });
            Ok(WebpAnimationFrameDecoder {
                inner,
                _data: owned,
                info: zen_info,
                descriptor,
                last_pixels: Vec::new(),
                last_width: 0,
                last_height: 0,
            })
        }
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
            .map_err_at(|inner| Error::InvalidInput(format!("{inner:?}")))?;
        let info = ImageInfo::new(w, h, ImageFormat::WebP).with_alpha(descriptor.has_alpha());
        Ok(DecodeOutput::new(buf, info))
    }
}

/// WebP streaming decoder implementing
/// [`zencodec::decode::StreamingDecode`].
///
/// The current shape buffers the full input, then hands the entire
/// decoded image back as a single batch on the first `next_batch`
/// call. libwebp's true incremental decoder ([`crate::StreamingDecoder`])
/// can yield rows as they're decoded; wiring that in row-batch form
/// is left for a future iteration. The trait contract permits this
/// shape (a single `(0, full_image)` batch followed by `Ok(None)`).
#[cfg(feature = "streaming")]
pub struct WebpStreamingDecoder {
    data: Vec<u8>,
    descriptor: PixelDescriptor,
    info: ImageInfo,
    config: DecoderConfig,
    limits: Limits,
    emitted: bool,
    decoded: Option<(Vec<u8>, u32, u32)>,
}

#[cfg(feature = "streaming")]
impl zencodec::decode::StreamingDecode for WebpStreamingDecoder {
    type Error = At<Error>;

    fn next_batch(&mut self) -> Result<Option<(u32, PixelSlice<'_>)>, At<Error>> {
        if self.emitted {
            return Ok(None);
        }
        if self.decoded.is_none() {
            let cfg = self.config.clone().limits(self.limits);
            let dec = crate::Decoder::new(&self.data)?.config(cfg);
            let (bytes, w, h) = match self.descriptor {
                PixelDescriptor::RGBA8_SRGB => dec.decode_rgba_raw()?,
                PixelDescriptor::BGRA8_SRGB => dec.decode_bgra_raw()?,
                PixelDescriptor::RGB8_SRGB => dec.decode_rgb_raw()?,
                _ => unreachable!("pick_decode_descriptor only returns supported variants"),
            };
            self.decoded = Some((bytes, w, h));
        }
        self.emitted = true;
        let (bytes, w, h) = self.decoded.as_ref().unwrap();
        let stride_bytes = (*w as usize).saturating_mul(self.descriptor.bytes_per_pixel());
        let slice = PixelSlice::new(bytes, *w, *h, stride_bytes, self.descriptor)
            .map_err_at(|inner| Error::InvalidInput(format!("{inner:?}")))?;
        Ok(Some((0, slice)))
    }

    fn info(&self) -> &ImageInfo {
        &self.info
    }
}

/// Stub when the `streaming` feature is off.
#[cfg(not(feature = "streaming"))]
pub struct WebpStreamingDecoder {
    _marker: core::marker::PhantomData<()>,
}

#[cfg(not(feature = "streaming"))]
impl zencodec::decode::StreamingDecode for WebpStreamingDecoder {
    type Error = At<Error>;

    fn next_batch(&mut self) -> Result<Option<(u32, PixelSlice<'_>)>, At<Error>> {
        Err(at!(Error::InvalidConfig(
            "webpx built without `streaming` feature".into(),
        )))
    }

    fn info(&self) -> &ImageInfo {
        unreachable!("streaming feature not enabled");
    }
}

/// WebP animation-frame decoder implementing
/// [`zencodec::decode::AnimationFrameDecoder`].
#[cfg(feature = "animation")]
pub struct WebpAnimationFrameDecoder {
    inner: crate::AnimationDecoder,
    /// Holds the input bytes alive for the duration of the decoder.
    /// libwebp's demuxer keeps a raw pointer into this slice.
    _data: Vec<u8>,
    info: ImageInfo,
    descriptor: PixelDescriptor,
    /// Most recently produced frame's pixels. Owned so the
    /// `AnimationFrame<'_>` we return can borrow from it; cleared on
    /// each new `render_next_frame` call.
    last_pixels: Vec<u8>,
    last_width: u32,
    last_height: u32,
}

#[cfg(feature = "animation")]
impl zencodec::decode::AnimationFrameDecoder for WebpAnimationFrameDecoder {
    type Error = At<Error>;

    fn wrap_sink_error(e: zencodec::decode::SinkError) -> At<Error> {
        at!(Error::InvalidInput(format!("sink error: {e}")))
    }

    fn info(&self) -> &ImageInfo {
        &self.info
    }

    fn render_next_frame(
        &mut self,
        _stop: Option<&dyn enough::Stop>,
    ) -> Result<Option<zencodec::decode::AnimationFrame<'_>>, At<Error>> {
        let frame = match self.inner.next_frame()? {
            Some(f) => f,
            None => return Ok(None),
        };
        // Cache pixels so the returned PixelSlice can borrow safely.
        self.last_pixels = frame.data;
        self.last_width = frame.width;
        self.last_height = frame.height;
        let stride_bytes =
            (self.last_width as usize).saturating_mul(self.descriptor.bytes_per_pixel());
        let slice = PixelSlice::new(
            &self.last_pixels,
            self.last_width,
            self.last_height,
            stride_bytes,
            self.descriptor,
        )
        .map_err_at(|inner| Error::InvalidInput(format!("{inner:?}")))?;
        Ok(Some(zencodec::decode::AnimationFrame::new(
            slice,
            frame.duration_ms,
            // frame_index isn't tracked here; pass 0 as a placeholder.
            // libwebp's animation decoder doesn't expose a per-frame
            // index distinct from iteration order.
            0,
        )))
    }

    fn render_next_frame_to_sink(
        &mut self,
        stop: Option<&dyn enough::Stop>,
        sink: &mut dyn DecodeRowSink,
    ) -> Result<Option<OutputInfo>, At<Error>> {
        let descriptor = self.descriptor;
        let frame = match self.render_next_frame(stop)? {
            Some(f) => f,
            None => return Ok(None),
        };
        let pixels = frame.pixels();
        let w = pixels.width();
        let h = pixels.rows();
        sink.begin(w, h, descriptor)
            .map_err(|e| at!(Error::InvalidInput(format!("sink begin: {e}"))))?;
        let mut dst = sink
            .provide_next_buffer(0, h, w, descriptor)
            .map_err(|e| at!(Error::InvalidInput(format!("sink provide: {e}"))))?;
        let bpp = descriptor.bytes_per_pixel();
        let row_bytes = (w as usize).saturating_mul(bpp);
        let src_bytes = pixels.as_strided_bytes();
        let src_stride = pixels.stride();
        for y in 0..h as usize {
            let src_row = &src_bytes[y * src_stride..y * src_stride + row_bytes];
            let dst_row = dst.row_mut(y as u32);
            dst_row[..row_bytes].copy_from_slice(src_row);
        }
        sink.finish()
            .map_err(|e| at!(Error::InvalidInput(format!("sink finish: {e}"))))?;
        Ok(Some(OutputInfo::full_decode(w, h, descriptor)))
    }

    fn frame_count(&self) -> Option<u32> {
        Some(self.inner.info().frame_count)
    }

    fn loop_count(&self) -> Option<u32> {
        Some(self.inner.info().loop_count)
    }
}

/// Stub when the `animation` feature is off.
#[cfg(not(feature = "animation"))]
pub struct WebpAnimationFrameDecoder {
    _marker: core::marker::PhantomData<()>,
}

#[cfg(not(feature = "animation"))]
impl zencodec::decode::AnimationFrameDecoder for WebpAnimationFrameDecoder {
    type Error = At<Error>;

    fn wrap_sink_error(_e: zencodec::decode::SinkError) -> At<Error> {
        at!(Error::InvalidConfig(
            "webpx built without `animation` feature".into(),
        ))
    }

    fn info(&self) -> &ImageInfo {
        unreachable!("animation feature not enabled");
    }

    fn render_next_frame(
        &mut self,
        _stop: Option<&dyn enough::Stop>,
    ) -> Result<Option<zencodec::decode::AnimationFrame<'_>>, At<Error>> {
        Err(at!(Error::InvalidConfig(
            "webpx built without `animation` feature".into(),
        )))
    }

    fn render_next_frame_to_sink(
        &mut self,
        _stop: Option<&dyn enough::Stop>,
        _sink: &mut dyn zencodec::decode::DecodeRowSink,
    ) -> Result<Option<OutputInfo>, At<Error>> {
        Err(at!(Error::InvalidConfig(
            "webpx built without `animation` feature".into(),
        )))
    }
}
