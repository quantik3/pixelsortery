#![allow(unused_parens, unused)]
use eframe::egui::TextBuffer;
use image::{codecs::png::PngEncoder, GenericImageView, GrayImage, ImageResult, Rgb, RgbImage, RgbaImage};
use log::{debug, error, info, warn};
use path_creator::PathCreator;
use rayon::prelude::*;
use span_sorter::{SortingCriteria, SpanSorter};
use std::{any::Any, fmt::Debug, fs, io::{self, ErrorKind, Read, Write}, path::{Path, PathBuf}, process::{self, Command, Output, Stdio}, time::Instant};

use crate::pixel_selector::PixelSelector;

mod color_helpers;
pub mod path_creator;
pub mod pixel_selector;
pub mod span_sorter;
#[cfg(feature = "video")]
mod video;
#[cfg(feature = "video")]
pub use video::{ThreadPhone, Progress, extract_video_frame};

#[derive(Clone)]
pub struct Pixelsorter {
    pub sorter: span_sorter::SpanSorter,
    pub selector: PixelSelector,
    pub path_creator: path_creator::PathCreator,
    pub reverse: bool,
    /// Center for rays, circles and spirals, relative to the image size ((0.5, 0.5) = middle)
    pub path_center: (f32, f32),
    /// Reverse the sorting direction of every second span (herringbone pattern)
    pub alternate: bool,
    /// Which color channels are moved (the others stay where they are)
    pub channels: ChannelMode,
    /// Reduce the colors of the sorted pixels to this many levels per channel (0 or 1 = off)
    pub posterize: u8,
}

/// Which color channels of the pixels are moved by the sorting
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelMode {
    All,
    Red,
    Green,
    Blue,
}

impl ChannelMode {
    /// Index of the only channel that is written, or None for all channels
    fn single_channel(self) -> Option<usize> {
        match self {
            ChannelMode::All => None,
            ChannelMode::Red => Some(0),
            ChannelMode::Green => Some(1),
            ChannelMode::Blue => Some(2),
        }
    }
}

pub type Span = Vec<Rgb<u8>>;

impl Pixelsorter {
    // constructor
    pub fn new() -> Pixelsorter {
        Pixelsorter {
            sorter: SpanSorter::new(SortingCriteria::Brightness),
            selector: PixelSelector::Full,
            path_creator: PathCreator::AllHorizontally,
            reverse: false,
            path_center: path_creator::DEFAULT_CENTER,
            alternate: false,
            channels: ChannelMode::All,
            posterize: 0,
        }
    }
    pub fn to_long_string(&self) -> String {
        let mut s = String::new();
        s += match self.path_creator {
            PathCreator::AllHorizontally => "All Horizontally".into(),
            PathCreator::AllVertically => "All Vertically".into(),
            PathCreator::HorizontalLines => if self.reverse {"Left"} else {"Right"}.into(),
            PathCreator::VerticalLines => if self.reverse {"Up"} else {"Down"}.into(),
            PathCreator::Circles => "Circles".into(),
            PathCreator::Spiral => "Spiral".into(),
            PathCreator::SquareSpiral => "Square Spiral".into(),
            PathCreator::RectSpiral => "Rect Spiral".into(),
            PathCreator::Diagonally(a) => format!("Diagonally ({}°)", a),
            PathCreator::Hilbert => "Hilbert Curve".into(),
            p => format!("{}", p),
        }
        .as_str();
        s += "-";
        if self.reverse {
            s += "R-"
        };
        s += match self.selector {
            PixelSelector::Full => "Full".into(),
            PixelSelector::Fixed { len } => format!("Fixed length ({})", len),
            PixelSelector::Random { max } => format!("Random length ({})", max),
            PixelSelector::Edges { threshold } => format!("Edges ({})", threshold),
            PixelSelector::Threshold { min, max, criteria } => format!(
                "{}{}-{}",
                match criteria {
                    pixel_selector::PixelSelectCriteria::Hue => "Hue",
                    pixel_selector::PixelSelectCriteria::Brightness => "Brightness",
                    pixel_selector::PixelSelectCriteria::Saturation => "Saturation",
                },
                min,
                max
            ),
        }
        .as_str();
        s += "-";
        s += match self.sorter.algorithm {
            span_sorter::SortingAlgorithm::Mapsort => "Mapsort",
            span_sorter::SortingAlgorithm::Shellsort => "Shellsort",
            span_sorter::SortingAlgorithm::Glitchsort => "Glitchsort",
            span_sorter::SortingAlgorithm::DebugColor => "Debug-colors",
            span_sorter::SortingAlgorithm::Partial => "Partial",
            span_sorter::SortingAlgorithm::Stretch => "Stretch",
            span_sorter::SortingAlgorithm::Shift => "Shift",
        };
        s += "-";
        s += match self.sorter.criteria {
            SortingCriteria::Hue => "Hue",
            SortingCriteria::Brightness => "Brightness",
            SortingCriteria::Saturation => "Saturation",
            SortingCriteria::Red => "Red",
            SortingCriteria::Green => "Green",
            SortingCriteria::Blue => "Blue",
            SortingCriteria::Warmth => "Warmth",
            SortingCriteria::Lightness => "Lightness",
            SortingCriteria::Guide => "Guide",
        };

        s
    }

    pub fn to_pretty_short_string(&self) -> String {
        let mut s = String::new();
        s += match self.path_creator {
            PathCreator::AllHorizontally => "Horizontal".into(),
            PathCreator::AllVertically => "Vertical".into(),
            PathCreator::HorizontalLines => if self.reverse {"Left"} else {"Right"}.into(),
            PathCreator::VerticalLines => if self.reverse {"Up"} else {"Down"}.into(),
            PathCreator::Circles => "Circles".into(),
            PathCreator::Spiral => "Spiral".into(),
            PathCreator::SquareSpiral => "Square".into(),
            PathCreator::RectSpiral => "Rect".into(),
            PathCreator::Diagonally(a) => format!("Diag({}°)", a),
            PathCreator::Hilbert => "Hilbert".into(),
            p => format!("{}", p),
        }
        .as_str();
        if self.reverse {
            s += "{R}"
        };
        s += " | ";

        s += match self.selector {
            PixelSelector::Full => "Full".into(),
            PixelSelector::Fixed { len } => format!("Fixed ({})", len),
            PixelSelector::Random { max } => format!("Random ({})", max),
            PixelSelector::Edges { threshold } => format!("Edges ({})", threshold),
            PixelSelector::Threshold { min, max, criteria } => format!(
                "{} ({}-{})",
                match criteria {
                    pixel_selector::PixelSelectCriteria::Hue => "Hue",
                    pixel_selector::PixelSelectCriteria::Brightness => "Bright",
                    pixel_selector::PixelSelectCriteria::Saturation => "Sat",
                },
                min,
                max
            ),
        }
        .as_str();
        s += " | ";
        s += match self.sorter.algorithm {
            span_sorter::SortingAlgorithm::Mapsort => "Map",
            span_sorter::SortingAlgorithm::Shellsort => "Shell",
            span_sorter::SortingAlgorithm::Glitchsort => "Glitch",
            span_sorter::SortingAlgorithm::DebugColor => "Debug",
            span_sorter::SortingAlgorithm::Partial => "Partial",
            span_sorter::SortingAlgorithm::Stretch => "Stretch",
            span_sorter::SortingAlgorithm::Shift => "Shift",
        };
        s += "(by ";
        s += match self.sorter.criteria {
            SortingCriteria::Hue => "Hue",
            SortingCriteria::Brightness => "Bright",
            SortingCriteria::Saturation => "Sat",
            SortingCriteria::Red => "Red",
            SortingCriteria::Green => "Green",
            SortingCriteria::Blue => "Blue",
            SortingCriteria::Warmth => "Warm",
            SortingCriteria::Lightness => "Light",
            SortingCriteria::Guide => "Guide",
        };
        s += ")";

        s
    }

    pub fn to_compact_string(&self) -> String {
        let mut s = String::new();
        s += match self.path_creator {
            PathCreator::AllHorizontally => "hor".into(),
            PathCreator::AllVertically => "vert".into(),
            PathCreator::HorizontalLines => "lr".into(),
            PathCreator::VerticalLines => "ud".into(),
            PathCreator::Circles => "circ".into(),
            PathCreator::Spiral => "sprl".into(),
            PathCreator::SquareSpiral => "spSq".into(),
            PathCreator::RectSpiral => "spRe".into(),
            PathCreator::Diagonally(a) => format!("diag{}", a),
            PathCreator::Hilbert => "hilb".into(),
            p => format!("{}", p).to_lowercase(),
        }
        .as_str();
        s += "-";
        if self.reverse {
            s += "R-"
        };
        s += match self.selector {
            PixelSelector::Full => "full".into(),
            PixelSelector::Fixed { len } => format!("fixed{}", len),
            PixelSelector::Random { max } => format!("rand{}", max),
            PixelSelector::Edges { threshold } => format!("edges{}", threshold),
            PixelSelector::Threshold { min, max, criteria } => format!(
                "{}{}-{}",
                match criteria {
                    pixel_selector::PixelSelectCriteria::Hue => "hue",
                    pixel_selector::PixelSelectCriteria::Brightness => "bright",
                    pixel_selector::PixelSelectCriteria::Saturation => "sat",
                },
                min,
                max
            ),
        }
        .as_str();
        s += "-";
        s += match self.sorter.algorithm {
            span_sorter::SortingAlgorithm::Mapsort => "map",
            span_sorter::SortingAlgorithm::Shellsort => "shell",
            span_sorter::SortingAlgorithm::Glitchsort => "gl",
            span_sorter::SortingAlgorithm::DebugColor => "debug",
            span_sorter::SortingAlgorithm::Partial => "partial",
            span_sorter::SortingAlgorithm::Stretch => "stretch",
            span_sorter::SortingAlgorithm::Shift => "shift",
        };
        s += "-";
        s += match self.sorter.criteria {
            SortingCriteria::Hue => "hue",
            SortingCriteria::Brightness => "bright",
            SortingCriteria::Saturation => "sat",
            SortingCriteria::Red => "red",
            SortingCriteria::Green => "green",
            SortingCriteria::Blue => "blue",
            SortingCriteria::Warmth => "warm",
            SortingCriteria::Lightness => "light",
            SortingCriteria::Guide => "guide",
        };

        s
    }

    // sorting without creating spans
    pub fn sort_all_pixels(&self, img: &mut RgbImage) {
        let mut pixels: Vec<Rgb<u8>> = img.pixels().copied().collect();
        self.sorter.sort(&mut pixels);
        for (dst, src) in img.pixels_mut().zip(pixels) {
            *dst = src;
        }
    }

    /// Sort a given image in place
    pub fn sort(&self, img: &mut RgbImage) {
        self.sort_with_mask(img, None);
    }

    /// Sort a given image in place, but only the pixels where the mask is white (>= 128).
    /// A mask with a different size than the image is ignored.
    pub fn sort_with_mask(&self, img: &mut RgbImage, mask: Option<&GrayImage>) {
        self.sort_with_mask_and_guide(img, mask, None);
    }

    /// Like `sort_with_mask`, with an optional guide image: when sorting by the criteria `Guide`,
    /// the pixels are sorted by the brightness of the guide image at their position.
    /// Mask and guide are ignored if their size differs from the image.
    pub fn sort_with_mask_and_guide(&self, img: &mut RgbImage, mask: Option<&GrayImage>, guide: Option<&GrayImage>) {
        self.sort_with_extras(img, mask, guide, None);
    }

    /// Like `sort_with_mask_and_guide`, with painted directions for the path `PathCreator::Drawn`
    /// (one angle in radians per pixel, NaN = not painted).
    pub fn sort_with_extras(
        &self,
        img: &mut RgbImage,
        mask: Option<&GrayImage>,
        guide: Option<&GrayImage>,
        directions: Option<&[f32]>,
    ) {
        let (w, h) = (img.width(), img.height());
        let fits = |m: &&GrayImage| m.width() == w && m.height() == h;
        let mask = mask.filter(fits).map(|m| m.as_raw().as_slice());
        let guide = guide.filter(fits).map(|m| m.as_raw().as_slice());
        let directions = directions.filter(|d| d.len() == (w as usize) * (h as usize));
        let raw: &mut [u8] = img;
        self.sort_raw_full(raw, 3, w as u64, h as u64, mask, guide, directions);
    }

    /// Sort raw pixel data in place.
    ///
    /// `channels` is the number of bytes per pixel (3 for RGB, 4 for RGBA). Only the first three
    /// channels are used and moved, other channels (alpha) stay where they are.
    /// `mask` has one byte per pixel; only pixels with a value >= 128 are sorted.
    pub fn sort_raw(&self, raw: &mut [u8], channels: usize, w: u64, h: u64, mask: Option<&[u8]>) {
        self.sort_raw_with_guide(raw, channels, w, h, mask, None);
    }

    /// Like `sort_raw`, with an optional guide (one brightness byte per pixel), see `sort_with_mask_and_guide`.
    pub fn sort_raw_with_guide(&self, raw: &mut [u8], channels: usize, w: u64, h: u64, mask: Option<&[u8]>, guide: Option<&[u8]>) {
        self.sort_raw_full(raw, channels, w, h, mask, guide, None);
    }

    /// Sort raw pixel data with all optional inputs (see `sort_raw` and `sort_with_extras`)
    pub fn sort_raw_full(
        &self,
        raw: &mut [u8],
        channels: usize,
        w: u64,
        h: u64,
        mask: Option<&[u8]>,
        guide: Option<&[u8]>,
        directions: Option<&[f32]>,
    ) {
        let pixelcount = (w * h) as usize;
        let guide = guide.filter(|g| g.len() >= pixelcount && self.sorter.criteria == SortingCriteria::Guide);
        let single_channel = self.channels.single_channel();
        let posterize_levels = self.posterize;
        assert!(channels >= 3, "At least 3 channels per pixel are needed");
        assert!(raw.len() >= pixelcount * channels, "Pixel buffer is too small for the image size");

        info!("Image information: {} x {} ({} pixels)", w, h, pixelcount);
        info!(
            "Sorting with:\n   | {}{}\n   | {}\n   | {}",
            self.path_creator.info_string(),
            if self.reverse { " [Reversed]" } else { "" },
            self.selector.info_string(),
            self.sorter.info_string(),
        );

        // CUT IMAGE INTO PATHS (lists of pixel indices that never overlap)
        let mut timestart = Instant::now();
        let paths = self.path_creator.create_index_paths(w, h, self.reverse, mask, self.path_center, directions);
        info!("TIME [Creating Paths]:\t{:?}", timestart.elapsed());
        timestart = Instant::now();

        // For every path: copy its pixels into a contiguous buffer, select spans,
        // sort the spans and write the pixels back. This is much more cache friendly
        // than sorting through pointers that are scattered all over the image.
        let buffer = SharedPixelBuffer { ptr: raw.as_mut_ptr(), len: raw.len(), channels };
        let buffer = &buffer;
        paths.par_iter().for_each(|path| {
            // SAFETY: the paths are disjoint (every index occurs only once in all paths),
            // so every pixel is only read and written by the one thread working on its path.
            let mut pixels: Vec<Rgb<u8>> = path.iter().map(|&i| unsafe { buffer.get(i as usize) }).collect();
            // The spans are selected on the original colors
            let ranges = self.selector.span_ranges(&pixels);
            if posterize_levels >= 2 {
                pixels.iter_mut().for_each(|p| *p = posterize(*p, posterize_levels));
            }
            // Values from the guide image at the original position of every pixel
            let guide_values: Option<Vec<u16>> = guide.map(|g| path.iter().map(|&i| g[i as usize] as u16).collect());
            for (n, (start, end)) in ranges.into_iter().enumerate() {
                let span = &mut pixels[start..end];
                match &guide_values {
                    Some(values) => self.sorter.sort_by_values(span, &values[start..end]),
                    None => self.sorter.sort(span),
                }
                // Herringbone: every second span is sorted the other way round
                if self.alternate && n % 2 == 1 {
                    span.reverse();
                }
            }
            for (&i, px) in path.iter().zip(pixels.iter()) {
                match single_channel {
                    None => unsafe { buffer.set(i as usize, *px) },
                    // Only one channel moves, the other two stay in place
                    Some(c) => unsafe { buffer.set_channel(i as usize, c, px.0[c]) },
                }
            }
        });

        info!("TIME [Selecting + Sorting]:\t{:?}", timestart.elapsed());
    }

    /// Sort pixels given as a list of references (in row order)
    pub fn sort_pixels(&self, mut all_pixels: Vec<&mut Rgb<u8>>, w: u64, h: u64) {
        let mut raw: Vec<u8> = Vec::with_capacity(all_pixels.len() * 3);
        for p in all_pixels.iter() {
            raw.extend_from_slice(&p.0);
        }
        self.sort_raw(&mut raw, 3, w, h, None);
        for (p, c) in all_pixels.iter_mut().zip(raw.chunks_exact(3)) {
            p.0.copy_from_slice(c);
        }
    }

    pub fn mask(&self, img: &mut RgbImage) -> bool {
        let mut all_pixels: Vec<&mut Rgb<u8>> = img.pixels_mut().collect();
        if let PixelSelector::Threshold { min, max, criteria } = self.selector {
            self.selector.mask(&mut all_pixels);
            return true;
        }
        return false;
    }
}

/// A raw pointer to pixel data that may be shared between threads.
/// Only used with disjoint pixel indices per thread (see `Pixelsorter::sort_raw`).
struct SharedPixelBuffer {
    ptr: *mut u8,
    len: usize,
    channels: usize,
}

unsafe impl Sync for SharedPixelBuffer {}
unsafe impl Send for SharedPixelBuffer {}

impl SharedPixelBuffer {
    /// Reads the RGB values of pixel `i`.
    /// Safety: no other thread may write pixel `i` at the same time.
    #[inline]
    unsafe fn get(&self, i: usize) -> Rgb<u8> {
        let offset = i * self.channels;
        assert!(offset + 3 <= self.len);
        let p = self.ptr.add(offset);
        Rgb([*p, *p.add(1), *p.add(2)])
    }

    /// Writes one channel of pixel `i`.
    /// Safety: no other thread may read or write pixel `i` at the same time.
    #[inline]
    unsafe fn set_channel(&self, i: usize, channel: usize, value: u8) {
        let offset = i * self.channels + channel;
        assert!(channel < 3 && offset < self.len);
        *self.ptr.add(offset) = value;
    }

    /// Writes the RGB values of pixel `i` (other channels stay untouched).
    /// Safety: no other thread may read or write pixel `i` at the same time.
    #[inline]
    unsafe fn set(&self, i: usize, px: Rgb<u8>) {
        let offset = i * self.channels;
        assert!(offset + 3 <= self.len);
        let p = self.ptr.add(offset);
        *p = px.0[0];
        *p.add(1) = px.0[1];
        *p.add(2) = px.0[2];
    }
}

/// Reduces every channel to `levels` evenly spaced values
fn posterize(p: Rgb<u8>, levels: u8) -> Rgb<u8> {
    let steps = (levels.max(2) - 1) as f32;
    Rgb(p.0.map(|c| (((c as f32 / 255.0 * steps).round() / steps) * 255.0).round() as u8))
}
