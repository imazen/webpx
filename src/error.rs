//! Error types for webpx operations.
//!
//! This module uses the [`whereat`] crate for lightweight error location tracking.
//!
//! # For Callers
//!
//! `webpx` functions return `Result<T, At<Error>>`, where [`At`] wraps the error
//! with source location traces. You have several options:
//!
//! ## Option 1: Propagate traces with `.at()`
//!
//! Use [`ResultAtExt::at()`] to add your call site to the trace:
//!
//! ```rust,ignore
//! use webpx::{ResultAtExt, encode_rgba, Unstoppable};
//!
//! fn my_encode(data: &[u8]) -> webpx::Result<Vec<u8>> {
//!     encode_rgba(data, 100, 100, 85.0, Unstoppable).at()?  // Adds location
//! }
//! ```
//!
//! ## Option 2: Add your crate's info to the trace
//!
//! For traces with your crate's GitHub links, define crate info and use `at_crate!`:
//!
//! ```rust,ignore
//! whereat::define_at_crate_info!();  // In your lib.rs
//!
//! fn my_encode(data: &[u8]) -> webpx::Result<Vec<u8>> {
//!     webpx::at_crate!(webpx::encode_rgba(data, 100, 100, 85.0, Unstoppable))?
//! }
//! ```
//!
//! ## Option 3: Convert to plain errors
//!
//! Use `.into_inner()` to discard traces and get the plain [`Error`]:
//!
//! ```rust,ignore
//! let result = encode_rgba(data, 100, 100, 85.0, Unstoppable);
//! let plain_result: Result<Vec<u8>, Error> = result.map_err(|e| e.into_inner());
//! ```
//!
//! ## Option 4: Inspect errors without traces
//!
//! Use `.error()` to get a reference to the underlying error:
//!
//! ```rust,ignore
//! if let Err(at_err) = encode_rgba(data, 100, 100, 85.0, Unstoppable) {
//!     match at_err.error() {
//!         Error::InvalidInput(msg) => println!("Bad input: {}", msg),
//!         _ => println!("{:?}", at_err),  // Print with trace
//!     }
//! }
//! ```
//!
//! [`whereat`]: https://docs.rs/whereat
//! [`At`]: whereat::At
//! [`ResultAtExt::at()`]: whereat::ResultAtExt::at

use alloc::string::String;
use core::fmt;
use enough::StopReason;
use whereat::At;

/// Result type for webpx operations.
///
/// Errors include source location traces via [`At<Error>`](whereat::At).
/// See the [module documentation](self) for how to work with traced errors.
pub type Result<T> = core::result::Result<T, At<Error>>;

/// Error type for webpx operations.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Error {
    /// Invalid input parameters (dimensions, buffer size, etc.)
    InvalidInput(String),
    /// Encoding failed
    EncodeFailed(EncodingError),
    /// Decoding failed
    DecodeFailed(DecodingError),
    /// Configuration validation failed
    InvalidConfig(String),
    /// Memory allocation failed
    OutOfMemory,
    /// ICC profile operation failed
    IccError(String),
    /// Mux/demux operation failed
    MuxError(MuxError),
    /// Animation operation failed
    AnimationError(String),
    /// Streaming operation requires more data
    NeedMoreData,
    /// Invalid WebP data
    InvalidWebP,
    /// Operation was stopped via cooperative cancellation
    Stopped(StopReason),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidInput(msg) => write!(f, "invalid input: {}", msg),
            Error::EncodeFailed(e) => write!(f, "encode failed: {}", e),
            Error::DecodeFailed(e) => write!(f, "decode failed: {}", e),
            Error::InvalidConfig(msg) => write!(f, "invalid config: {}", msg),
            Error::OutOfMemory => write!(f, "out of memory"),
            Error::IccError(msg) => write!(f, "ICC error: {}", msg),
            Error::MuxError(e) => write!(f, "mux error: {}", e),
            Error::AnimationError(msg) => write!(f, "animation error: {}", msg),
            Error::NeedMoreData => write!(f, "need more data"),
            Error::InvalidWebP => write!(f, "invalid WebP data"),
            Error::Stopped(reason) => write!(f, "{}", reason),
        }
    }
}

impl From<StopReason> for Error {
    fn from(reason: StopReason) -> Self {
        Error::Stopped(reason)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for Error {}

/// Encoding error codes from libwebp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum EncodingError {
    /// No error
    Ok = 0,
    /// Memory allocation error
    OutOfMemory = 1,
    /// Bitstream out of memory
    BitstreamOutOfMemory = 2,
    /// NULL parameter
    NullParameter = 3,
    /// Invalid configuration
    InvalidConfiguration = 4,
    /// Bad dimension (width or height is 0 or > 16383)
    BadDimension = 5,
    /// Partition is bigger than 512k
    Partition0Overflow = 6,
    /// Partition is bigger than 16M
    PartitionOverflow = 7,
    /// Bad write callback
    BadWrite = 8,
    /// File is bigger than 4G
    FileTooBig = 9,
    /// User abort
    UserAbort = 10,
    /// Last error (unknown)
    Last = 11,
}

impl From<i32> for EncodingError {
    fn from(code: i32) -> Self {
        match code {
            0 => EncodingError::Ok,
            1 => EncodingError::OutOfMemory,
            2 => EncodingError::BitstreamOutOfMemory,
            3 => EncodingError::NullParameter,
            4 => EncodingError::InvalidConfiguration,
            5 => EncodingError::BadDimension,
            6 => EncodingError::Partition0Overflow,
            7 => EncodingError::PartitionOverflow,
            8 => EncodingError::BadWrite,
            9 => EncodingError::FileTooBig,
            10 => EncodingError::UserAbort,
            _ => EncodingError::Last,
        }
    }
}

impl fmt::Display for EncodingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            EncodingError::Ok => "ok",
            EncodingError::OutOfMemory => "out of memory",
            EncodingError::BitstreamOutOfMemory => "bitstream out of memory",
            EncodingError::NullParameter => "null parameter",
            EncodingError::InvalidConfiguration => "invalid configuration",
            EncodingError::BadDimension => "bad dimension",
            EncodingError::Partition0Overflow => "partition0 overflow",
            EncodingError::PartitionOverflow => "partition overflow",
            EncodingError::BadWrite => "bad write",
            EncodingError::FileTooBig => "file too big",
            EncodingError::UserAbort => "user abort",
            EncodingError::Last => "unknown error",
        };
        write!(f, "{}", msg)
    }
}

/// Decoding error codes from libwebp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum DecodingError {
    /// No error
    Ok = 0,
    /// Memory allocation error
    OutOfMemory = 1,
    /// Invalid parameter
    InvalidParam = 2,
    /// Bitstream error
    BitstreamError = 3,
    /// Unsupported feature
    UnsupportedFeature = 4,
    /// Suspended (need more data)
    Suspended = 5,
    /// User abort
    UserAbort = 6,
    /// Not enough data
    NotEnoughData = 7,
}

impl From<i32> for DecodingError {
    fn from(code: i32) -> Self {
        match code {
            0 => DecodingError::Ok,
            1 => DecodingError::OutOfMemory,
            2 => DecodingError::InvalidParam,
            3 => DecodingError::BitstreamError,
            4 => DecodingError::UnsupportedFeature,
            5 => DecodingError::Suspended,
            6 => DecodingError::UserAbort,
            _ => DecodingError::NotEnoughData,
        }
    }
}

impl fmt::Display for DecodingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            DecodingError::Ok => "ok",
            DecodingError::OutOfMemory => "out of memory",
            DecodingError::InvalidParam => "invalid param",
            DecodingError::BitstreamError => "bitstream error",
            DecodingError::UnsupportedFeature => "unsupported feature",
            DecodingError::Suspended => "suspended",
            DecodingError::UserAbort => "user abort",
            DecodingError::NotEnoughData => "not enough data",
        };
        write!(f, "{}", msg)
    }
}

/// Mux error codes from libwebp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum MuxError {
    /// Operation completed successfully
    Ok = 1,
    /// Object not present
    NotFound = 0,
    /// Invalid argument
    InvalidArgument = -1,
    /// Bad data
    BadData = -2,
    /// Memory error
    MemoryError = -3,
    /// Not enough data
    NotEnoughData = -4,
}

impl From<i32> for MuxError {
    fn from(code: i32) -> Self {
        match code {
            1 => MuxError::Ok,
            0 => MuxError::NotFound,
            -1 => MuxError::InvalidArgument,
            -2 => MuxError::BadData,
            -3 => MuxError::MemoryError,
            _ => MuxError::NotEnoughData,
        }
    }
}

impl fmt::Display for MuxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            MuxError::Ok => "ok",
            MuxError::NotFound => "not found",
            MuxError::InvalidArgument => "invalid argument",
            MuxError::BadData => "bad data",
            MuxError::MemoryError => "memory error",
            MuxError::NotEnoughData => "not enough data",
        };
        write!(f, "{}", msg)
    }
}
