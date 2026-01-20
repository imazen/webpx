//! Compatibility shims for migration from other WebP crates.
//!
//! These modules provide API-compatible wrappers to ease migration from:
//! - [`webp`](webp) - the `webp` crate (0.3.x)
//! - [`webp_animation`](webp_animation) - the `webp-animation` crate (0.9.x)
//!
//! # Migration Example
//!
//! ```rust,ignore
//! // Before (webp crate)
//! use webp::{Encoder, Decoder};
//!
//! // After (webpx compat)
//! use webpx::compat::webp::{Encoder, Decoder};
//! ```

pub mod webp;
pub mod webp_animation;
