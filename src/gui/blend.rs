//! Sorting a layer and blending the result over the image below

use image::{GrayImage, RgbImage};
use pixelsortery::Pixelsorter;
use rayon::prelude::*;

/// How a sorted layer is combined with the image below it
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BlendMode {
    Normal,
    Lighten,
    Darken,
    Multiply,
    Screen,
    Overlay,
    Difference,
}

impl BlendMode {
    pub(crate) const ALL: [BlendMode; 7] = [
        BlendMode::Normal,
        BlendMode::Lighten,
        BlendMode::Darken,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Difference,
    ];

    /// Blends one channel: `below` is the image below, `top` the sorted layer (both 0 - 1)
    fn apply(self, below: f32, top: f32) -> f32 {
        match self {
            BlendMode::Normal => top,
            BlendMode::Lighten => below.max(top),
            BlendMode::Darken => below.min(top),
            BlendMode::Multiply => below * top,
            BlendMode::Screen => 1.0 - (1.0 - below) * (1.0 - top),
            BlendMode::Overlay => {
                if below < 0.5 {
                    2.0 * below * top
                } else {
                    1.0 - 2.0 * (1.0 - below) * (1.0 - top)
                }
            }
            BlendMode::Difference => (below - top).abs(),
        }
    }
}

/// Sorts `input` and blends the result over it.
///
/// The mask decides where the layer is applied: white = fully, gray = partly (soft edges),
/// black = not at all. Every pixel with a mask value above 0 gets sorted.
/// `opacity` (0 - 1) fades the whole layer.
pub(crate) fn render_layer(
    ps: &Pixelsorter,
    blend: BlendMode,
    opacity: f32,
    input: &RgbImage,
    mask: Option<&GrayImage>,
    guide: Option<&GrayImage>,
) -> RgbImage {
    let mask = mask.filter(|m| m.dimensions() == input.dimensions());
    let soft = mask.map(|m| m.as_raw().iter().any(|&v| v != 0 && v != 255)).unwrap_or(false);
    // The sorting itself only knows "sort" (>= 128) or "don't sort", so faint pixels are made white for it
    let sort_mask = if soft {
        mask.map(|m| {
            let mut binary = m.clone();
            let raw: &mut [u8] = &mut binary;
            raw.iter_mut().for_each(|v| *v = if *v > 0 { 255 } else { 0 });
            binary
        })
    } else {
        None
    };

    let mut sorted = input.clone();
    ps.sort_with_mask_and_guide(&mut sorted, sort_mask.as_ref().or(mask), guide);

    let opacity = opacity.clamp(0.0, 1.0);
    if blend == BlendMode::Normal && opacity >= 1.0 && !soft {
        // Nothing to blend: outside of a hard mask the pixels are unchanged anyway
        return sorted;
    }

    let mask_raw = mask.map(|m| m.as_raw().as_slice());
    let below = input.as_raw().as_slice();
    let out: &mut [u8] = &mut sorted;
    out.par_chunks_mut(3).enumerate().for_each(|(i, px)| {
        let alpha = opacity * mask_raw.map(|m| m[i] as f32 / 255.0).unwrap_or(1.0);
        let below_px = &below[3 * i..3 * i + 3];
        if alpha <= 0.0 {
            px.copy_from_slice(below_px);
            return;
        }
        for c in 0..3 {
            let b = below_px[c] as f32 / 255.0;
            let t = px[c] as f32 / 255.0;
            let blended = blend.apply(b, t);
            px[c] = ((b + (blended - b) * alpha) * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    });
    sorted
}
