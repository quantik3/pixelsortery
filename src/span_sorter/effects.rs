use image::Rgb;
use rand::Rng;

/// "Melting": the first part of the span is stretched over the whole span.
/// `amount` = 0 changes nothing, `amount` = 1 fills the span with its first pixel.
pub fn stretch(pixels: &mut [Rgb<u8>], amount: f32) {
    let n = pixels.len();
    if n < 2 {
        return;
    }
    let keep = ((n as f32 * (1.0 - amount.clamp(0.0, 1.0))).round() as usize).clamp(1, n);
    if keep == n {
        return;
    }
    let source: Vec<Rgb<u8>> = pixels[..keep].to_vec();
    for (i, p) in pixels.iter_mut().enumerate() {
        *p = source[i * keep / n];
    }
}

/// Block glitch: the span is rotated by a random offset of up to `amount` times its length.
pub fn shift(pixels: &mut [Rgb<u8>], amount: f32) {
    let n = pixels.len();
    if n < 2 {
        return;
    }
    let max_offset = (n as f32 * amount.clamp(0.0, 1.0)).round() as usize;
    if max_offset == 0 {
        return;
    }
    let offset = rand::thread_rng().gen_range(0..=max_offset) % n;
    pixels.rotate_right(offset);
}
