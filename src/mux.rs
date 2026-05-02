//! WebP mux/demux operations for metadata (ICC, EXIF, XMP).

use crate::error::{Error, MuxError, Result};
use crate::ffi::demux::Demux;
use alloc::vec::Vec;
use whereat::*;

/// Extract ICC profile from WebP data.
///
/// Returns `None` if no ICC profile is present. Subject to the internal
/// 256 MiB hard cap; for a tighter cap, use [`get_icc_profile_with_limits`].
///
/// # Example
///
/// ```rust,no_run
/// let webp_data: &[u8] = &[0u8; 100]; // placeholder
/// if let Some(icc) = webpx::get_icc_profile(webp_data)? {
///     println!("Found ICC profile: {} bytes", icc.len());
/// }
/// # Ok::<(), webpx::At<webpx::Error>>(())
/// ```
pub fn get_icc_profile(webp_data: &[u8]) -> Result<Option<Vec<u8>>> {
    get_chunk(webp_data, b"ICCP", &crate::Limits::none())
}

/// Extract EXIF metadata from WebP data.
///
/// Returns `None` if no EXIF data is present. Subject to the internal
/// 256 MiB hard cap; for a tighter cap, use [`get_exif_with_limits`].
pub fn get_exif(webp_data: &[u8]) -> Result<Option<Vec<u8>>> {
    get_chunk(webp_data, b"EXIF", &crate::Limits::none())
}

/// Extract XMP metadata from WebP data.
///
/// Returns `None` if no XMP data is present. Subject to the internal
/// 256 MiB hard cap; for a tighter cap, use [`get_xmp_with_limits`].
pub fn get_xmp(webp_data: &[u8]) -> Result<Option<Vec<u8>>> {
    get_chunk(webp_data, b"XMP ", &crate::Limits::none())
}

/// Extract ICC profile, rejecting chunks larger than `limits.max_metadata_bytes`.
pub fn get_icc_profile_with_limits(
    webp_data: &[u8],
    limits: &crate::Limits,
) -> Result<Option<Vec<u8>>> {
    get_chunk(webp_data, b"ICCP", limits)
}

/// Extract EXIF metadata, rejecting chunks larger than `limits.max_metadata_bytes`.
pub fn get_exif_with_limits(webp_data: &[u8], limits: &crate::Limits) -> Result<Option<Vec<u8>>> {
    get_chunk(webp_data, b"EXIF", limits)
}

/// Extract XMP metadata, rejecting chunks larger than `limits.max_metadata_bytes`.
pub fn get_xmp_with_limits(webp_data: &[u8], limits: &crate::Limits) -> Result<Option<Vec<u8>>> {
    get_chunk(webp_data, b"XMP ", limits)
}

/// Helper to create a mux from WebP data.
unsafe fn create_mux_from_data(webp_data: &[u8], copy_data: bool) -> *mut libwebp_sys::WebPMux {
    let data = libwebp_sys::WebPData {
        bytes: webp_data.as_ptr(),
        size: webp_data.len(),
    };

    unsafe {
        libwebp_sys::WebPMuxCreateInternal(
            &data,
            copy_data as i32,
            libwebp_sys::WEBP_MUX_ABI_VERSION as i32,
        )
    }
}

/// Hard cap on metadata chunk size copied out of libwebp memory.
///
/// libwebp itself accepts ICCP/EXIF/XMP chunks up to ~4 GB. Without a cap
/// here, a hostile WebP declaring a 4 GB ICCP chunk would force webpx to
/// allocate a 4 GB Vec on every `get_*` call. 256 MiB is generous for any
/// real-world ICC profile / EXIF / XMP block while bounding the worst case.
pub(crate) const MAX_METADATA_CHUNK_BYTES: usize = 256 * 1024 * 1024;

/// Get a metadata chunk from WebP data, applying both the internal 256 MiB
/// hard cap and any caller-supplied [`crate::Limits::max_metadata_bytes`].
///
/// All libwebp resource cleanup (demuxer + chunk iterator) runs
/// unconditionally via the RAII handles from [`crate::ffi::demux`].
fn get_chunk(
    webp_data: &[u8],
    fourcc: &[u8; 4],
    limits: &crate::Limits,
) -> Result<Option<Vec<u8>>> {
    // Input-size cap fires before we hand the data to libwebp's demuxer.
    limits
        .check_input_size(webp_data.len() as u64)
        .map_err(|e| at!(Error::LimitExceeded(e)))?;

    let demux = Demux::new(webp_data)?;
    let Some(chunk) = demux.get_chunk(fourcc) else {
        return Ok(None);
    };
    let bytes = chunk.bytes();
    if bytes.is_empty() {
        return Ok(None);
    }
    // Internal hard cap: protects against `Limits::default()` callers
    // who didn't set max_metadata_bytes themselves.
    if bytes.len() > MAX_METADATA_CHUNK_BYTES {
        return Err(at!(Error::InvalidInput(alloc::format!(
            "metadata chunk exceeds internal hard cap: {} bytes (max {})",
            bytes.len(),
            MAX_METADATA_CHUNK_BYTES
        ))));
    }
    // Caller-side `Limits::max_metadata_bytes`. Saturate the cast
    // because slice len is usize but max_metadata_bytes is u32; on
    // 64-bit a > 4 GiB chunk would already be caught by the hard
    // cap above, so this saturation never widens the threshold.
    limits
        .check_metadata_bytes(u32::try_from(bytes.len()).unwrap_or(u32::MAX))
        .map_err(|e| at!(Error::LimitExceeded(e)))?;
    Ok(Some(bytes.to_vec()))
}

/// Embed ICC profile into WebP data.
///
/// Takes existing WebP data and adds or replaces the ICC profile.
pub fn embed_icc(webp_data: &[u8], icc_profile: &[u8]) -> Result<Vec<u8>> {
    embed_chunk(webp_data, b"ICCP", icc_profile)
}

/// Embed EXIF metadata into WebP data.
pub fn embed_exif(webp_data: &[u8], exif_data: &[u8]) -> Result<Vec<u8>> {
    embed_chunk(webp_data, b"EXIF", exif_data)
}

/// Embed XMP metadata into WebP data.
pub fn embed_xmp(webp_data: &[u8], xmp_data: &[u8]) -> Result<Vec<u8>> {
    embed_chunk(webp_data, b"XMP ", xmp_data)
}

/// Embed a metadata chunk into WebP data.
fn embed_chunk(webp_data: &[u8], fourcc: &[u8; 4], chunk_data: &[u8]) -> Result<Vec<u8>> {
    // Create mux from existing WebP data
    let mux = unsafe { create_mux_from_data(webp_data, true) };

    if mux.is_null() {
        return Err(at!(Error::MuxError(MuxError::BadData)));
    }

    // Set the chunk
    let chunk = libwebp_sys::WebPData {
        bytes: chunk_data.as_ptr(),
        size: chunk_data.len(),
    };

    let err = unsafe {
        libwebp_sys::WebPMuxSetChunk(
            mux,
            fourcc.as_ptr() as *const core::ffi::c_char,
            &chunk,
            1, // copy_data = true
        )
    };

    if err != libwebp_sys::WebPMuxError::WEBP_MUX_OK {
        unsafe { libwebp_sys::WebPMuxDelete(mux) };
        return Err(at!(Error::MuxError(MuxError::from(err as i32))));
    }

    // Assemble the output
    let mut output_data = libwebp_sys::WebPData::default();
    let err = unsafe { libwebp_sys::WebPMuxAssemble(mux, &mut output_data) };

    if err != libwebp_sys::WebPMuxError::WEBP_MUX_OK {
        unsafe { libwebp_sys::WebPMuxDelete(mux) };
        return Err(at!(Error::MuxError(MuxError::from(err as i32))));
    }

    let result = unsafe {
        if output_data.bytes.is_null() || output_data.size == 0 {
            libwebp_sys::WebPMuxDelete(mux);
            return Err(at!(Error::MuxError(MuxError::MemoryError)));
        }
        let slice = core::slice::from_raw_parts(output_data.bytes, output_data.size);
        let vec = slice.to_vec();
        libwebp_sys::WebPDataClear(&mut output_data);
        libwebp_sys::WebPMuxDelete(mux);
        vec
    };

    Ok(result)
}

/// Remove ICC profile from WebP data.
pub fn remove_icc(webp_data: &[u8]) -> Result<Vec<u8>> {
    remove_chunk(webp_data, b"ICCP")
}

/// Remove EXIF metadata from WebP data.
pub fn remove_exif(webp_data: &[u8]) -> Result<Vec<u8>> {
    remove_chunk(webp_data, b"EXIF")
}

/// Remove XMP metadata from WebP data.
pub fn remove_xmp(webp_data: &[u8]) -> Result<Vec<u8>> {
    remove_chunk(webp_data, b"XMP ")
}

/// Remove a metadata chunk from WebP data.
fn remove_chunk(webp_data: &[u8], fourcc: &[u8; 4]) -> Result<Vec<u8>> {
    let mux = unsafe { create_mux_from_data(webp_data, true) };

    if mux.is_null() {
        return Err(at!(Error::MuxError(MuxError::BadData)));
    }

    // Delete the chunk (ignore NotFound error)
    let err = unsafe {
        libwebp_sys::WebPMuxDeleteChunk(mux, fourcc.as_ptr() as *const core::ffi::c_char)
    };

    if err != libwebp_sys::WebPMuxError::WEBP_MUX_OK
        && err != libwebp_sys::WebPMuxError::WEBP_MUX_NOT_FOUND
    {
        unsafe { libwebp_sys::WebPMuxDelete(mux) };
        return Err(at!(Error::MuxError(MuxError::from(err as i32))));
    }

    // Assemble output
    let mut output_data = libwebp_sys::WebPData::default();
    let err = unsafe { libwebp_sys::WebPMuxAssemble(mux, &mut output_data) };

    if err != libwebp_sys::WebPMuxError::WEBP_MUX_OK {
        unsafe { libwebp_sys::WebPMuxDelete(mux) };
        return Err(at!(Error::MuxError(MuxError::from(err as i32))));
    }

    let result = unsafe {
        if output_data.bytes.is_null() || output_data.size == 0 {
            libwebp_sys::WebPMuxDelete(mux);
            return Err(at!(Error::MuxError(MuxError::MemoryError)));
        }
        let slice = core::slice::from_raw_parts(output_data.bytes, output_data.size);
        let vec = slice.to_vec();
        libwebp_sys::WebPDataClear(&mut output_data);
        libwebp_sys::WebPMuxDelete(mux);
        vec
    };

    Ok(result)
}

#[cfg(test)]
mod tests {
    // Tests would require actual WebP test data
}
