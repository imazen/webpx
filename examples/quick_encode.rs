use std::env;
use std::fs;
use webpx::{Encoder, Unstoppable};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: quick_encode <input.png> <output.webp>");
        return;
    }
    let img = image::open(&args[1]).unwrap().to_rgba8();
    let webp = Encoder::new_rgba(img.as_raw(), img.width(), img.height())
        .quality(85.0)
        .method(4)
        .encode(Unstoppable)
        .unwrap();
    fs::write(&args[2], &webp).unwrap();
    eprintln!("Wrote {} bytes to {}", webp.len(), args[2]);
}
