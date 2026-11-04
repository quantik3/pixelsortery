use crate::color_helpers::*;
use image::Rgb;
use rand::{
    distributions::{Distribution, Uniform},
    thread_rng,
};
use rayon::iter::{
    IndexedParallelIterator, IntoParallelIterator, IntoParallelRefMutIterator, ParallelDrainRange,
    ParallelIterator,
};
use std::{cmp::min, collections::VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelSelector {
    Full,
    Fixed {
        len: u64,
    },
    Random {
        max: u32,
    },
    Threshold {
        min: u64,
        max: u64,
        criteria: PixelSelectCriteria,
    },
    /// Spans end where the color changes a lot between two neighbouring pixels (an edge).
    /// `threshold` is the allowed difference (sum of the R, G and B differences, 0 - 765).
    Edges {
        threshold: u64,
    },
}

/// Key criteria which a (Threshold-)Selector should use as a key
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelSelectCriteria {
    Hue,
    Brightness,
    Saturation,
}

impl PixelSelector {
    /// Returns a list of pixel spans
    pub fn create_spans<'a>(
        self,
        pixels: &mut VecDeque<&'a mut Rgb<u8>>,
    ) -> Vec<Vec<&'a mut Rgb<u8>>> {
        match self {
            PixelSelector::Full => full_selector(pixels),
            PixelSelector::Fixed { len } => fixed_selector(pixels, len),
            PixelSelector::Random { max } => random_selector(pixels, max),
            PixelSelector::Threshold { min, max, criteria } => threshold_selector(pixels, criteria, min, max),
            PixelSelector::Edges { .. } => {
                // Find the spans on a copy, then split the pixels accordingly
                let copy: Vec<Rgb<u8>> = pixels.iter().map(|p| **p).collect();
                let mut spans = Vec::new();
                let mut taken = 0;
                for (start, end) in self.span_ranges(&copy) {
                    // Skip the pixels between spans (there are none for this selector)
                    pixels.drain(0..start - taken);
                    spans.push(pixels.drain(0..end - start).collect());
                    taken = end;
                }
                spans
            }
        }
    }
    /// Returns the spans of a path as index ranges `(start, end)` (end exclusive).
    ///
    /// Gives the same spans as `create_spans`, but works on a contiguous slice of pixels
    /// and doesn't need to move any pixel around.
    pub fn span_ranges(self, pixels: &[Rgb<u8>]) -> Vec<(usize, usize)> {
        let n = pixels.len();
        let mut ranges = Vec::new();
        match self {
            PixelSelector::Full => ranges.push((0, n)),
            PixelSelector::Fixed { len } => {
                // Prevent an endless loop
                if len == 0 {
                    return ranges;
                }
                let len = len as usize;
                let mut start = 0;
                while n - start >= len {
                    ranges.push((start, start + len));
                    start += len;
                }
                // The rest
                ranges.push((start, n));
            }
            PixelSelector::Random { max } => {
                // rng_range cannot be 1..1
                if max <= 1 {
                    return ranges;
                }
                let mut rng = thread_rng();
                let rng_range = Uniform::from(1..max as usize);
                let mut start = 0;
                loop {
                    let r = rng_range.sample(&mut rng);
                    if n - start < r {
                        break;
                    }
                    ranges.push((start, start + r));
                    start += r;
                }
                // The rest
                ranges.push((start, n));
            }
            PixelSelector::Threshold { min, max, criteria } => {
                // Every possible value is selected anyway: one span with everything.
                // (hue is in [0, 360], brightness and saturation in [0, 255])
                let max_possible = if criteria == PixelSelectCriteria::Hue { 360 } else { 255 };
                if min == 0 && max >= max_possible {
                    ranges.push((0, n));
                    return ranges;
                }
                let value_function = match criteria {
                    PixelSelectCriteria::Hue => get_hue,
                    PixelSelectCriteria::Brightness => get_brightness,
                    PixelSelectCriteria::Saturation => get_saturation,
                };
                let mut span_start: Option<usize> = None;
                for (i, p) in pixels.iter().enumerate() {
                    let val = value_function(p) as u64;
                    let valid = val >= min && val <= max;
                    match (valid, span_start) {
                        (true, None) => span_start = Some(i),
                        (false, Some(s)) => {
                            ranges.push((s, i));
                            span_start = None;
                        }
                        _ => {}
                    }
                }
                if let Some(s) = span_start {
                    ranges.push((s, n));
                }
            }
            PixelSelector::Edges { threshold } => {
                if n == 0 {
                    return ranges;
                }
                let mut start = 0;
                for i in 1..n {
                    if color_difference(&pixels[i - 1], &pixels[i]) > threshold {
                        ranges.push((start, i));
                        start = i;
                    }
                }
                ranges.push((start, n));
            }
        }
        ranges
    }

    pub fn info_string<'a>(self) -> String {
        match self {
            PixelSelector::Full => String::from("Selecing all pixels"),
            PixelSelector::Fixed { len } => format!("Selecting ranges of fixed length {}", len),
            PixelSelector::Random { max } => format!("Random Selector with max length {}", max),
            PixelSelector::Threshold { min, max, criteria } => format!(
                "Selecting Pixels with: [{} < {:?} < {}]",
                min, criteria, max
            ),
            PixelSelector::Edges { threshold } => format!("Spans ending at edges (color difference > {})", threshold),
        }
    }
    pub fn mask(self, pixels: &mut Vec<&mut Rgb<u8>>) -> Result<(), ()> {
        if let PixelSelector::Threshold { min, max, criteria } = self {
            let value_function = match criteria {
                PixelSelectCriteria::Hue => get_hue,
                PixelSelectCriteria::Brightness => get_brightness,
                PixelSelectCriteria::Saturation => get_saturation,
            };
            let valid = |val| (val as u64) >= min && (val as u64) <= max;
            pixels.iter_mut().for_each(|p| {
                if valid(value_function(p)) {
                    **p = Rgb { 0: [255, 255, 255] };
                } else {
                    **p = Rgb { 0: [0, 0, 0] };
                }
            });
            return Ok(());
        }
        Err(())
    }
}

fn full_selector<'a>(pixels: &mut VecDeque<&'a mut Rgb<u8>>) -> Vec<Vec<&'a mut Rgb<u8>>> {
    let mut spans: Vec<Vec<&'a mut Rgb<u8>>> = Vec::new();

    let mut span: Vec<&mut Rgb<u8>> = Vec::new();
    while !pixels.is_empty() {
        span.push(pixels.pop_front().unwrap());
    }
    spans.push(span);
    spans
}

fn fixed_selector<'a>(
    pixels: &mut VecDeque<&'a mut Rgb<u8>>,
    len: u64,
) -> Vec<Vec<&'a mut Rgb<u8>>> {
    let mut spans: Vec<Vec<&'a mut Rgb<u8>>> = Vec::new();

    // Prevent an endless loop
    if len == 0 {
        return spans;
    }

    while pixels.len() >= len as usize {
        // Take len pixels and put into new span
        spans.push(pixels.drain(0..len as usize).collect());
    }
    // Push the rest
    spans.push(pixels.drain(..).collect());

    spans
}

fn random_selector<'a>(
    pixels: &mut VecDeque<&'a mut Rgb<u8>>,
    max: u32,
) -> Vec<Vec<&'a mut Rgb<u8>>> {
    let mut spans: Vec<Vec<&'a mut Rgb<u8>>> = Vec::new();
    // rng_range cannot be 1..1
    if max <= 1 {
        return spans;
    }
    let mut rng = thread_rng();
    let rng_range = Uniform::from(1..max as usize);

    loop {
        let mut r = rng_range.sample(&mut rng);
        if pixels.len() < r {
            break;
        }
        // Take r pixels and put into new span
        spans.push(pixels.drain(0..r).collect());
    }
    // Push the rest
    spans.push(pixels.drain(..).collect());

    spans
}

fn threshold_selector<'a>(
    pixels: &mut VecDeque<&'a mut Rgb<u8>>,
    criteria: PixelSelectCriteria,
    min: u64,
    max: u64,
) -> Vec<Vec<&'a mut Rgb<u8>>> {
    let mut spans: Vec<Vec<&'a mut Rgb<u8>>> = Vec::new();

    let value_function = match criteria {
        PixelSelectCriteria::Hue => get_hue,
        PixelSelectCriteria::Brightness => get_brightness,
        PixelSelectCriteria::Saturation => get_saturation,
    };

    // Function that checks if a value is valid
    let valid = |val| (val as u64) >= min && (val as u64) <= max;

    let mut span: Vec<&mut Rgb<u8>> = Vec::new();
    for _ in 0..pixels.len() {
        let value = value_function(pixels.get(0).unwrap());
        let px = pixels.pop_front().unwrap();

        if valid(value) {
            // A valid pixel. Add to span
            span.push(px);
        } else {
            // A invalid pixel, close the span and create a new one
            // Only do that when the current span isn't empty anyway
            if span.len() > 0 {
                spans.push(span);
                span = Vec::new();
            }
        }
    }
    spans.push(span);
    spans
}


/// Sum of the absolute differences of the R, G and B channels (0 - 765)
fn color_difference(a: &Rgb<u8>, b: &Rgb<u8>) -> u64 {
    a.0.iter().zip(b.0.iter()).map(|(x, y)| (*x as i64 - *y as i64).unsigned_abs()).sum()
}
