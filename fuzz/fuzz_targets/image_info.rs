//! Fuzz target: header probe.
//!
//! `ImageInfo::from_webp` is the cheapest path through libwebp's parser —
//! useful as a pure header-fuzz target separate from the heavyweight decode
//! targets so the fuzzer can iterate fast on container/RIFF parsing.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = webpx::ImageInfo::from_webp(data);
});
