//! Painting into a mask with a round brush

use image::GrayImage;

/// Paints a line of round brush stamps from `from` to `to` (image coordinates) into the mask.
///
/// `hardness` (0 - 1): 1 = hard edge, smaller values fade out towards the edge of the brush.
/// Painting only makes the mask brighter, erasing only darker, so overlapping stamps don't add up.
/// Returns the changed region `[x0, y0, x1, y1]` (end exclusive), or None if nothing inside the image was touched.
pub(crate) fn paint_mask_stroke(
    mask: &mut GrayImage,
    from: (f32, f32),
    to: (f32, f32),
    radius: f32,
    erase: bool,
    hardness: f32,
) -> Option<[u32; 4]> {
    let (w, h) = (mask.width() as i64, mask.height() as i64);
    let r = radius.max(0.5);
    let hardness = hardness.clamp(0.0, 1.0);
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let distance = (dx * dx + dy * dy).sqrt();
    // Place the stamps close enough to get a smooth line
    let steps = ((distance / (r * 0.3).max(0.5)).ceil() as usize).max(1);
    let raw: &mut [u8] = mask;
    let mut region: Option<[i64; 4]> = None;

    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let (cx, cy) = (from.0 + dx * t, from.1 + dy * t);
        let x0 = ((cx - r).floor() as i64).max(0);
        let y0 = ((cy - r).floor() as i64).max(0);
        let x1 = ((cx + r).ceil() as i64).min(w - 1);
        let y1 = ((cy + r).ceil() as i64).min(h - 1);
        if x0 > x1 || y0 > y1 {
            continue;
        }
        for y in y0..=y1 {
            let py = y as f32 + 0.5 - cy;
            for x in x0..=x1 {
                let px = x as f32 + 0.5 - cx;
                let d = (px * px + py * py).sqrt() / r;
                if d > 1.0 {
                    continue;
                }
                let strength = brush_strength(d, hardness);
                let value = (strength * 255.0).round() as u8;
                let m = &mut raw[(y * w + x) as usize];
                if erase {
                    *m = (*m).min(255 - value);
                } else {
                    *m = (*m).max(value);
                }
            }
        }
        region = Some(match region {
            None => [x0, y0, x1 + 1, y1 + 1],
            Some([a, b, c, d]) => [a.min(x0), b.min(y0), c.max(x1 + 1), d.max(y1 + 1)],
        });
    }
    region.map(|[a, b, c, d]| [a as u32, b as u32, c as u32, d as u32])
}

/// Strength of the brush at the relative distance `d` (0 = center, 1 = edge)
fn brush_strength(d: f32, hardness: f32) -> f32 {
    if d <= hardness || hardness >= 1.0 {
        return 1.0;
    }
    // Smooth fade from the hard core to the edge
    let t = ((1.0 - d) / (1.0 - hardness)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Paints the direction of the stroke (from `from` to `to`) into the direction map with a round, hard brush.
/// `directions` has one angle per pixel (NaN = not painted); erasing sets NaN.
/// Returns the changed region `[x0, y0, x1, y1]` (end exclusive), or None if nothing was painted
/// (the mouse didn't move yet, so there is no direction, or the stroke is outside the image).
pub(crate) fn paint_direction_stroke(
    directions: &mut [f32],
    w: u32,
    h: u32,
    from: (f32, f32),
    to: (f32, f32),
    radius: f32,
    erase: bool,
) -> Option<[u32; 4]> {
    let (w, h) = (w as i64, h as i64);
    if directions.len() < (w * h) as usize {
        return None;
    }
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let distance = (dx * dx + dy * dy).sqrt();
    if distance < 0.5 && !erase {
        return None;
    }
    let value = if erase { f32::NAN } else { dy.atan2(dx) };
    let r = radius.max(0.5);
    let steps = ((distance / (r * 0.3).max(0.5)).ceil() as usize).max(1);
    let mut region: Option<[i64; 4]> = None;
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let (cx, cy) = (from.0 + dx * t, from.1 + dy * t);
        let x0 = ((cx - r).floor() as i64).max(0);
        let y0 = ((cy - r).floor() as i64).max(0);
        let x1 = ((cx + r).ceil() as i64).min(w - 1);
        let y1 = ((cy + r).ceil() as i64).min(h - 1);
        if x0 > x1 || y0 > y1 {
            continue;
        }
        for y in y0..=y1 {
            let py = y as f32 + 0.5 - cy;
            for x in x0..=x1 {
                let px = x as f32 + 0.5 - cx;
                if px * px + py * py <= r * r {
                    directions[(y * w + x) as usize] = value;
                }
            }
        }
        region = Some(match region {
            None => [x0, y0, x1 + 1, y1 + 1],
            Some([a, b, c, d]) => [a.min(x0), b.min(y0), c.max(x1 + 1), d.max(y1 + 1)],
        });
    }
    region.map(|[a, b, c, d]| [a as u32, b as u32, c as u32, d as u32])
}
