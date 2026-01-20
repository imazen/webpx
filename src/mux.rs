//! WebP mux/demux operations for metadata (ICC, EXIF, XMP).

use crate::error::{Error, MuxError, Result};
use alloc::vec::Vec;
use core::mem::MaybeUninit;
use whereat::*;

/// Extract ICC profile from WebP data.
///
/// Returns `None` if no ICC profile is present.
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
    get_chunk(webp_data, b"ICCP")
}

/// Extract EXIF metadata from WebP data.
///
/// Returns `None` if no EXIF data is present.
pub fn get_exif(webp_data: &[u8]) -> Result<Option<Vec<u8>>> {
    get_chunk(webp_data, b"EXIF")
}

/// Extract XMP metadata from WebP data.
///
/// Returns `None` if no XMP data is present.
pub fn get_xmp(webp_data: &[u8]) -> Result<Option<Vec<u8>>> {
    get_chunk(webp_data, b"XMP ")
}

/// Helper to create a demuxer from WebP data.
unsafe fn create_demux(webp_data: &[u8]) -> *mut libwebp_sys::WebPDemuxer {
    let data = libwebp_sys::WebPData {
        bytes: webp_data.as_ptr(),
        size: webp_data.len(),
    };

    unsafe {
        libwebp_sys::WebPDemuxInternal(
            &data,
            0, // WEBP_DEMUX_ABI_VERSION check
            core::ptr::null_mut(),
            libwebp_sys::WEBP_DEMUX_ABI_VERSION as i32,
        )
    }
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

/// Get a metadata chunk from WebP data.
fn get_chunk(webp_data: &[u8], fourcc: &[u8; 4]) -> Result<Option<Vec<u8>>> {
    let demux = unsafe { create_demux(webp_data) };

    if demux.is_null() {
        return Err(at!(Error::InvalidWebP));
    }

    let mut chunk_iter = MaybeUninit::<libwebp_sys::WebPChunkIterator>::zeroed();
    let found = unsafe {
        libwebp_sys::WebPDemuxGetChunk(
            demux,
            fourcc.as_ptr() as *const i8,
            1, // First chunk
            chunk_iter.as_mut_ptr(),
        )
    };

    let result = if found != 0 {
        let chunk_iter = unsafe { chunk_iter.assume_init() };
        if !chunk_iter.chunk.bytes.is_null() && chunk_iter.chunk.size > 0 {
            let chunk_data = unsafe {
                core::slice::from_raw_parts(chunk_iter.chunk.bytes, chunk_iter.chunk.size)
            };
            let vec = Some(chunk_data.to_vec());
            unsafe {
                let mut iter = chunk_iter;
                libwebp_sys::WebPDemuxReleaseChunkIterator(&mut iter);
            }
            vec
        } else {
            None
        }
    } else {
        None
    };

    unsafe {
        libwebp_sys::WebPDemuxDelete(demux);
    }

    Ok(result)
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
            fourcc.as_ptr() as *const i8,
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
    let err = unsafe { libwebp_sys::WebPMuxDeleteChunk(mux, fourcc.as_ptr() as *const i8) };

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
