//! Fuzz target: dimension parameters at libwebp's 16383 limit and beyond.
//!
//! libwebp caps width and height at 16383. Encoder entry points that
//! accept dimensions must reject values past the cap (and zero) before
//! `width × height × bpp` overflows on 32-bit `usize`. This target
//! exercises every encoder constructor with a dimension drawn from
//! libwebp's intrinsic limits.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use webpx::{Encoder, Unstoppable};

#[derive(Arbitrary, Debug, Clone, Copy)]
enum DimChoice {
    Zero,
    One,
    Small(u8),
    Sixteen383, // libwebp's intrinsic cap
    Sixteen384, // one past cap
    U16Max,
    U32Max,
    Arbitrary(u32),
}

#[derive(Arbitrary, Debug)]
struct Input<'a> {
    width: DimChoice,
    height: DimChoice,
    pixels: &'a [u8],
}

fn resolve(choice: DimChoice) -> u32 {
    match choice {
        DimChoice::Zero => 0,
        DimChoice::One => 1,
        DimChoice::Small(v) => v as u32,
        DimChoice::Sixteen383 => 16383,
        DimChoice::Sixteen384 => 16384,
        DimChoice::U16Max => u16::MAX as u32,
        DimChoice::U32Max => u32::MAX,
        DimChoice::Arbitrary(v) => v,
    }
}

fuzz_target!(|input: Input<'_>| {
    let w = resolve(input.width);
    let h = resolve(input.height);

    // Cap actually-used product; oversized inputs are expected to error
    // out at the dimension validator before any allocation.
    let pixels_clamped = (w as u64).saturating_mul(h as u64);
    if pixels_clamped > 4 * 1024 * 1024 {
        // Still poke the constructors to verify they reject these
        // dimensions, but use a small data slice so we never allocate
        // the full buffer.
        let small = [0u8; 32];
        let _ = Encoder::new_rgba(&small, w, h)
            .quality(50.0)
            .encode(Unstoppable);
        let _ = Encoder::new_rgb(&small, w, h)
            .quality(50.0)
            .encode(Unstoppable);
        return;
    }

    // Tractable case: provide a real buffer.
    let needed_rgba = (w as usize).saturating_mul(h as usize).saturating_mul(4);
    let needed_rgb = (w as usize).saturating_mul(h as usize).saturating_mul(3);

    if input.pixels.len() >= needed_rgba {
        let _ = Encoder::new_rgba(&input.pixels[..needed_rgba], w, h)
            .quality(50.0)
            .encode(Unstoppable);
        let _ = Encoder::new_bgra(&input.pixels[..needed_rgba], w, h)
            .quality(50.0)
            .encode(Unstoppable);
    }
    if input.pixels.len() >= needed_rgb {
        let _ = Encoder::new_rgb(&input.pixels[..needed_rgb], w, h)
            .quality(50.0)
            .encode(Unstoppable);
        let _ = Encoder::new_bgr(&input.pixels[..needed_rgb], w, h)
            .quality(50.0)
            .encode(Unstoppable);
    }
});
