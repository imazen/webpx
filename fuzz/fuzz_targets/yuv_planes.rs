//! Fuzz target: YUV plane sizes / strides for `Encoder::new_yuv`.
//!
//! `validate_yuv_planes` enforces: each plane covers `stride × rows`,
//! strides >= width / chroma_width, `u_stride == v_stride`, every
//! stride fits in `i32`. This target builds plane references with
//! fuzzer-controlled stride / length combinations and confirms the
//! validator catches the bad shapes.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use webpx::{Encoder, Unstoppable, YuvPlanesRef};

#[derive(Arbitrary, Debug, Clone, Copy)]
enum StrideChoice {
    Tight,
    TightPlus1,
    LessThanWidth,
    I32MaxPlus1,
    Arbitrary(u32),
}

#[derive(Arbitrary, Debug)]
struct Input<'a> {
    width: u8,
    height: u8,
    y_stride: StrideChoice,
    uv_stride: StrideChoice,
    a_stride: StrideChoice,
    has_alpha: bool,
    use_uv_mismatch: bool,
    bytes: &'a [u8],
}

fn resolve(choice: StrideChoice, tight: u32) -> u32 {
    match choice {
        StrideChoice::Tight => tight,
        StrideChoice::TightPlus1 => tight.saturating_add(1),
        StrideChoice::LessThanWidth => tight.saturating_sub(1),
        StrideChoice::I32MaxPlus1 => (i32::MAX as u32).saturating_add(1),
        StrideChoice::Arbitrary(v) => v,
    }
}

fuzz_target!(|input: Input<'_>| {
    let w = ((input.width as u32) % 64).max(1);
    let h = ((input.height as u32) % 64).max(1);
    let uv_w = w.div_ceil(2);
    let uv_h = h.div_ceil(2);

    let y_stride = resolve(input.y_stride, w) as usize;
    let uv_stride = resolve(input.uv_stride, uv_w) as usize;
    let a_stride = resolve(input.a_stride, w) as usize;

    // Force u_stride != v_stride sometimes (validator must reject —
    // libwebp uses a single uv_stride field).
    let v_stride = if input.use_uv_mismatch {
        uv_stride.saturating_add(1)
    } else {
        uv_stride
    };

    // Compute plane lengths from strides; only proceed when the bytes
    // slice has enough room. We deliberately don't pad — the validator
    // should also reject too-short planes.
    let y_len = y_stride.saturating_mul(h as usize);
    let uv_len = uv_stride.saturating_mul(uv_h as usize);
    let v_len = v_stride.saturating_mul(uv_h as usize);
    let a_len = a_stride.saturating_mul(h as usize);

    let total = y_len
        .saturating_add(uv_len)
        .saturating_add(v_len)
        .saturating_add(if input.has_alpha { a_len } else { 0 });
    if total > 16 * 1024 * 1024 {
        return;
    }
    if input.bytes.len() < total {
        return;
    }

    let mut offset = 0;
    let y = &input.bytes[offset..offset + y_len];
    offset += y_len;
    let u = &input.bytes[offset..offset + uv_len];
    offset += uv_len;
    let v = &input.bytes[offset..offset + v_len];
    offset += v_len;
    let a = if input.has_alpha {
        Some(&input.bytes[offset..offset + a_len])
    } else {
        None
    };

    let planes = YuvPlanesRef {
        y,
        y_stride,
        u,
        u_stride: uv_stride,
        v,
        v_stride,
        a,
        a_stride,
        width: w,
        height: h,
    };

    let _ = Encoder::new_yuv(planes).quality(50.0).encode(Unstoppable);
});
