//! Centralized numeric-input validation for the FFI layer.
//!
//! libwebp's API takes strides and dimensions as `i32`. Every public
//! webpx entry point that accepts a stride must reject values
//! `> i32::MAX` *before* the cast, or libwebp's row-pointer arithmetic
//! walks backwards through caller memory (this was the 0.2.1 stride
//! class). Width × height × bytes-per-pixel must not wrap on 32-bit
//! `usize`. YUV planes must cover `stride × rows`. Putting these checks
//! in one module means the audit reduces to "did the call site go
//! through a helper" rather than "is every check correctly written
//! everywhere."

use crate::error::{Error, Result};
use whereat::*;

/// Reject strides that don't fit in libwebp's `i32` parameter.
///
/// Returns the validated stride as `i32` so the caller can pass it
/// directly without a second cast.
#[inline]
pub(crate) fn stride_fits_i32(stride: usize, ctx: &'static str) -> Result<i32> {
    if stride > i32::MAX as usize {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "{}: stride {} exceeds i32::MAX (libwebp parameter is signed)",
            ctx,
            stride
        ))));
    }
    Ok(stride as i32)
}

/// Reject negative or zero dimensions cast from libwebp `i32` outputs.
///
/// libwebp can technically return negative dimensions on bitstream-level
/// errors; before any `as usize` cast on a libwebp-supplied dimension
/// goes through this so the cast cannot wrap into a multi-gigabyte
/// length.
#[allow(dead_code)] // Wired up by future migrations of decode/streaming paths.
#[inline]
pub(crate) fn webp_returned_dim_positive(dim: i32, ctx: &'static str) -> Result<u32> {
    if dim <= 0 {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "{}: libwebp returned non-positive dimension {}",
            ctx,
            dim
        ))));
    }
    Ok(dim as u32)
}

/// Saturating `width × height × bpp` for buffer sizing.
///
/// On 32-bit `usize` (i686 is in CI), `width × height × bpp` for
/// libwebp's max 16383×16383 already approaches `usize::MAX`. This
/// helper saturates so downstream comparisons (`buf.len() >= needed`)
/// fail closed rather than wrap.
#[allow(dead_code)] // Wired up by future migrations of decode paths.
#[inline]
pub(crate) fn pixel_bytes(width: u32, height: u32, bpp: usize) -> usize {
    (width as usize)
        .saturating_mul(height as usize)
        .saturating_mul(bpp)
}

/// Verify a row-major buffer covers `stride × (height - 1) + row_bytes`.
///
/// libwebp accepts caller-owned buffers sized to the minimum extent —
/// the final row does not need trailing stride padding. This helper
/// applies that rule and rejects strides smaller than `row_bytes`.
#[allow(dead_code)] // Will be wired up by the encode/streaming migrations.
pub(crate) fn buffer_for_stride(
    buf_len: usize,
    stride: usize,
    height: u32,
    row_bytes: usize,
    ctx: &'static str,
) -> Result<()> {
    if stride < row_bytes {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "{}: stride {} smaller than row_bytes {}",
            ctx,
            stride,
            row_bytes
        ))));
    }
    let last_row_offset = (height as usize).saturating_sub(1).saturating_mul(stride);
    let needed = last_row_offset.saturating_add(row_bytes);
    if buf_len < needed {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "{}: buffer too small ({} < {} for {} rows of {} bytes at stride {})",
            ctx,
            buf_len,
            needed,
            height,
            row_bytes,
            stride
        ))));
    }
    Ok(())
}
