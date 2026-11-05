use egui::TextBuffer;
use image::{Rgb, RgbImage};
use log::{error, info, warn};
use rayon::prelude::*;
use std::{f64::consts::PI, fmt::Display, time::Instant};

mod gilbert;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathCreator {
    AllHorizontally,
    AllVertically,
    HorizontalLines,
    VerticalLines,
    Rays,
    Circles,
    Spiral,
    SquareSpiral,
    RectSpiral,
    Diagonally(f32),
    Hilbert,
    X3(i32, i64, f64),
    /// Wavy lines: (amplitude in pixels, wavelength in pixels, horizontal lines?)
    Waves(f32, f32, bool),
    /// Lines that follow a swirling noise field: (size of the swirls in pixels, random seed)
    FlowField(f32, u32),
    /// Rings shaped like a star around the center: (number of points, inner radius 0-1).
    /// With an inner radius of cos(180°/points) the star becomes a regular polygon.
    Star(u32, f32),
}

/// The default center for rays, circles and spirals (relative to the image size)
pub const DEFAULT_CENTER: (f32, f32) = (0.5, 0.5);

impl std::fmt::Display for PathCreator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                PathCreator::AllHorizontally => "All Horizontally".into(),
                PathCreator::AllVertically => "All Vertically".into(),
                PathCreator::HorizontalLines => "Left/Right".into(),
                PathCreator::VerticalLines => "Up/Down".into(),
                PathCreator::Rays => "Rays".into(),
                PathCreator::Circles => "Circles".into(),
                PathCreator::Spiral => "Spiral".into(),
                PathCreator::SquareSpiral => "Square Spiral".into(),
                PathCreator::RectSpiral => "Rectangular Spiral".into(),
                PathCreator::Diagonally(a) => format!("Diagonally ({}°)", a),
                PathCreator::Hilbert => "Hilbert Curve".into(),
                PathCreator::X3(_, _, _) => "x³".into(),
                PathCreator::FlowField(scale, seed) => format!("Flow Field ({}/{})", scale, seed),
                PathCreator::Star(points, inner) => format!("Star ({}/{:.2})", points, inner),
                PathCreator::Waves(a, l, horizontal) => format!(
                    "Waves {} ({}/{})",
                    if *horizontal { "horizontal" } else { "vertical" },
                    a,
                    l
                ),
            }
        )
    }
}

impl PathCreator {
    pub fn info_string(self) -> String {
        format!("Direction/Order: [{:?}]", self)
    }

    /// Does this path have a center that can be moved?
    pub fn uses_center(self) -> bool {
        matches!(
            self,
            PathCreator::Rays
                | PathCreator::Circles
                | PathCreator::Spiral
                | PathCreator::SquareSpiral
                | PathCreator::RectSpiral
                | PathCreator::Star(..)
        )
    }

    /// Creates the raw index paths (may contain invalid or duplicate indices).
    /// `center` is relative to the image size ((0.5, 0.5) is the middle).
    fn index_paths(self, w: u64, h: u64, center: (f32, f32)) -> Vec<Vec<u64>> {
        // New paths that always use the center
        if let PathCreator::Star(points, inner) = self {
            let cx = center.0.clamp(0.0, 1.0) as f64 * w as f64;
            let cy = center.1.clamp(0.0, 1.0) as f64 * h as f64;
            return path_star(w, h, cx, cy, points, inner);
        }
        // The original functions are used for the middle, so old results look exactly the same
        let custom_center = (center != DEFAULT_CENTER && self.uses_center()).then(|| {
            (
                center.0.clamp(0.0, 1.0) as f64 * w as f64,
                center.1.clamp(0.0, 1.0) as f64 * h as f64,
            )
        });
        if let Some((cx, cy)) = custom_center {
            return match self {
                PathCreator::Rays => path_rays_at(w, h, cx as f32, cy as f32),
                PathCreator::Circles => path_circles_at(w, h, cx, cy),
                PathCreator::Spiral => path_round_spiral_at(w, h, cx, cy),
                PathCreator::SquareSpiral => path_rect_spiral_at(w, h, cx, cy, true),
                _ => path_rect_spiral_at(w, h, cx, cy, false),
            };
        }
        match self {
            PathCreator::AllHorizontally => path_all_horizontally(w, h),
            PathCreator::AllVertically => path_all_vertically(w, h),
            PathCreator::HorizontalLines => path_horizontal_lines(w, h),
            PathCreator::VerticalLines => path_vertical_lines(w, h),
            PathCreator::Rays => path_rays(w, h),
            PathCreator::SquareSpiral => path_rect_spiral(w, h, true),
            PathCreator::RectSpiral => path_rect_spiral(w, h, false),
            PathCreator::Diagonally(angle) => path_diagonal_lines(w, h, angle),
            PathCreator::Circles => path_circles(w, h),
            PathCreator::Spiral => path_round_spiral(w, h),
            PathCreator::Hilbert => gilbert::path_hilbert(w, h),
            PathCreator::X3(offset_y, scalar_w, scalar_l) => path_x3(w, h, offset_y, scalar_w as f64, scalar_l as f64),
            PathCreator::Waves(amplitude, wavelength, horizontal) => path_waves(w, h, amplitude, wavelength, horizontal),
            PathCreator::FlowField(scale, seed) => path_flow_field(w, h, scale, seed),
            // Handled above
            PathCreator::Star(..) => Vec::new(),
        }
    }
    pub fn create_paths(self, all_pixels: Vec<&mut Rgb<u8>>, w: u64, h: u64, reverse: bool) -> Vec<Vec<&mut Rgb<u8>>> {

        let mut total_timestart = Instant::now();
        let mut timestart = Instant::now();

        // Actual path algorithms
        // Ideas/missing:
        // In waves
        // In star shape
        let mut all_paths_indices = self.index_paths(w, h, DEFAULT_CENTER);
        let timeend_pathing = timestart.elapsed();
        timestart = Instant::now();

        // Reverse spans if nessesary
        if reverse {
            all_paths_indices.iter_mut().for_each(|p| {
                p.reverse();
            });
        }
        let timeend_reversing = timestart.elapsed();
        timestart = Instant::now();

        // Turn indexed paths into arrays of pixels
        let pixels = pick_pixels(all_pixels, all_paths_indices);
        let timeend_picking = timestart.elapsed();

        info!("TIME | [Index Pathing]:  \t+ {:?}", timeend_pathing);
        info!("TIME | [Reversing paths]:\t+ {:?}", timeend_reversing);
        info!("TIME | [Pickin pixels]:  \t+ {:?}", timeend_picking);
        info!(
            "TIME | [Creating Paths]: \t= {:?}",
            total_timestart.elapsed()
        );

        return pixels;
    }

    /// Creates the paths as lists of pixel indices (index = y * w + x).
    ///
    /// Same rules as `create_paths`: consecutive duplicates are removed, invalid indices are skipped
    /// and every pixel belongs to the first path that contains it, so the paths never overlap.
    ///
    /// If a mask is given (one byte per pixel), only pixels with a mask value >= 128 are sorted.
    /// Pixels outside the mask are left untouched and split a path into separate parts,
    /// so no pixel is ever moved across an unmasked area.
    pub fn create_index_paths(self, w: u64, h: u64, reverse: bool, mask: Option<&[u8]>, center: (f32, f32)) -> Vec<Vec<u64>> {
        let total_timestart = Instant::now();
        let mut all_paths_indices = self.index_paths(w, h, center);
        let timeend_pathing = total_timestart.elapsed();

        // Reverse spans if nessesary
        if reverse {
            all_paths_indices.par_iter_mut().for_each(|p| p.reverse());
        }

        let pixelcount = (w * h) as usize;
        // Ignore a mask that doesn't fit the image
        let mask = mask.filter(|m| m.len() >= pixelcount);
        let mut taken = vec![false; pixelcount];
        let mut paths: Vec<Vec<u64>> = Vec::with_capacity(all_paths_indices.len());

        for mut path in all_paths_indices {
            path.dedup();
            match mask {
                None => {
                    path.retain(|&i| {
                        let i = i as usize;
                        if i < pixelcount && !taken[i] {
                            taken[i] = true;
                            true
                        } else {
                            false
                        }
                    });
                    if !path.is_empty() {
                        paths.push(path);
                    }
                }
                Some(mask) => {
                    let mut part = Vec::new();
                    for i in path {
                        let iu = i as usize;
                        if iu >= pixelcount || taken[iu] {
                            continue;
                        }
                        taken[iu] = true;
                        if mask[iu] >= 128 {
                            part.push(i);
                        } else if !part.is_empty() {
                            paths.push(std::mem::take(&mut part));
                        }
                    }
                    if !part.is_empty() {
                        paths.push(part);
                    }
                }
            }
        }

        info!("TIME | [Index Pathing]:  \t+ {:?}", timeend_pathing);
        info!("TIME | [Creating Paths]: \t= {:?}", total_timestart.elapsed());
        paths
    }
}

/// Creates and returns ranges of mutable Pixels.
/// The picked pixels and their order are determined by the given vector of indices
fn pick_pixels(all_pixels: Vec<&mut Rgb<u8>>, indices: Vec<Vec<u64>>) -> Vec<Vec<&mut Rgb<u8>>> {
    let mut paths: Vec<Vec<&mut Rgb<u8>>> = Vec::new();
    let mut all_pixels: Vec<Option<&mut Rgb<u8>>> =
        all_pixels.into_iter().map(|p| Some(p)).collect();

    for mut li in indices {
        li.dedup();
        let mut path = Vec::new();
        for i in li {
            // Check if the index is valid
            if all_pixels.get(i as usize).is_some() {
                all_pixels.push(None);
                // Check if the pixel at index i is still available (not None)
                if let Some(px) = all_pixels.swap_remove(i as usize) {
                    path.push(px);
                }
            }
        }
        paths.push(path);
    }
    return paths;
}

fn is_in_bounds(x: u64, y: u64, w: u64, h: u64) -> bool {
    x > 0 && x < w && y > 0 && y < h
}

fn path_all_horizontally(w: u64, h: u64) -> Vec<Vec<u64>> {
    vec![(0..w * h).collect()]
}

fn path_all_vertically(w: u64, h: u64) -> Vec<Vec<u64>> {
    let mut path = Vec::new();

    for x in 0..w {
        for y in 0..h {
            let i = y * w + x;
            path.push(i);
        }
    }

    return vec![path];
}

fn path_horizontal_lines(w: u64, h: u64) -> Vec<Vec<u64>> {
    let mut paths: Vec<Vec<u64>> = Vec::new();

    for y in 0..h {
        paths.push((y * w..y * w + w).collect());
    }

    return paths;
}

fn path_vertical_lines(w: u64, h: u64) -> Vec<Vec<u64>> {
    let mut paths: Vec<Vec<u64>> = Vec::new();

    for x in 0..w {
        paths.push((0..h).into_iter().map(|y| y * w + x).collect());
    }

    return paths;
}

const TWO_PI: f64 = 2.0 * PI;
const HALF_PI: f64 = PI * 0.5;
fn path_x3(w: u64, h: u64, offset_y: i32, scalar_w: f64, scalar_l: f64) -> Vec<Vec<u64>> {
    let index = |x: i64, y: i64| y * w as i64 + x;
    (0..w).into_par_iter()
        .map(|x| {
            (0..h).into_par_iter().map(|y|{
                // let y2: f64 = (y - (h / 2)) as f64 / 50.0;
                let h2: f64 = h as f64/2.0;
                let y2: f64 = (y as f64 - h2 - offset_y as f64) / (h2 / scalar_l);
                let res = (y2.atan() * scalar_w / HALF_PI);
                // Some unsigned/signed stuff
                index(x as i64 + res as i64 , y as i64) as u64
            }).collect()
        }).collect()
}

fn path_diagonal_lines(w: u64, h: u64, angle: f32) -> Vec<Vec<u64>> {
    let mut paths: Vec<Vec<u64>> = Vec::new();

    // Explanation:
    // We need to iterate differently, depending on the angle
    //
    // x_line_path (a=160°)  x_line_path: (a=110°)          y_line_path: (a=110°)
    //
    //   0  1  2              0 1 2 3 4 5 6 7               0 1 2 3 4 5 6 7
    // 0 X        <Good     0 X                           0 X X
    // 1 X                  1     X                       1     X X
    // 2    X        Bad>   2         X        Correct>   2         X X
    // 3    X               3             X               3             X X
    let angle = angle % 360.0;
    let do_reverse = (angle > 45.0 && angle < 225.0) || (angle < -135.0 && angle > -315.0);
    let angle = angle % 180.0;
    let x_tan_val = angle.to_radians().tan();
    let xoverhead = (x_tan_val * h as f32).round() as i64;
    let xrange = if x_tan_val < 0.0 {
        xoverhead..w as i64
    } else {
        0..w as i64 + xoverhead
    };

    let x_line_path = |xs| {
        let mut path = Vec::new();
        for y in 0..h {
            let x = xs - (y as f32 * x_tan_val).round() as i64;
            // Prevent "overflowing" the index and selecting indices on the next line
            if x >= w as i64 || x < 0 {
                continue;
            }
            let i = y * w + x as u64;
            path.push(i);
        }
        path
    };

    let y_tan_val = (90.0 - angle).to_radians().tan();
    let yoverhead = (y_tan_val * w as f32).round() as i64;
    let yrange = if y_tan_val < 0.0 {
        yoverhead..h as i64
    } else {
        0..h as i64 + yoverhead
    };

    let y_line_path = |ys| {
        let mut path = Vec::new();
        for x in 0..w {
            let y = ys - (x as f32 * y_tan_val).round() as i64;
            // Prevent "overflowing" the index and selecting indices on the next line
            if y >= h as i64 || y < 0 {
                continue;
            }
            let i = y as u64 * w + x;
            path.push(i);
        }
        path
    };

    // Choosing the correct function
    let mut paths: Vec<Vec<u64>> = match (angle.abs() > 45.0 && angle.abs() < 135.0) {
        // THREADPOOLING WOOO
        true => yrange.into_iter().map(y_line_path).collect(),
        false => xrange.into_iter().map(x_line_path).collect(),
    };
    if (do_reverse) {
        for p in &mut paths {
            p.reverse()
        }
    }

    return paths;
}

// Helper function to return a range (including the end element), but in a binary-search-like order
fn spread_range(start: u64, end: u64) -> Vec<u64> {
    if start == end { return vec![end]; }
    if start.abs_diff(end) <= 1 { return vec![start, end]; }
    let mid = (end + start) / 2;
    let mut v = vec![mid];
    let left = spread_range(start, mid-1);
    let right = spread_range(mid+1, end);
    //println!("{{{}}} {:?} {} {:?} {{{}}}", start, left, mid, right, end);
    v.extend_from_slice(&left);
    v.extend_from_slice(&right);
    return v;
}


fn path_rays(w: u64, h: u64) -> Vec<Vec<u64>> {
    let mut cx:f32 = (w as f32) / 2.0;
    let mut cy:f32 = (h as f32) / 2.0;
    let mut dirs: Vec<(f32, f32)> = Vec::new();;
    let mut tips: Vec<(f32, f32)>;
    let  mut y = 0; let mut x = 0;
    // Uses the spread range to generate "full" rays first to prevent all rays from being uncontinuous because of overlap
    tips =      spread_range(0, w-1).into_iter().map(|x| { (x as f32, 0.0) }).collect();
    tips.extend(spread_range(0, w-1).into_iter().map(|x| { (x as f32, h as f32) }));
    tips.extend(spread_range(0, h-1).into_iter().map(|y| { (0.0, y as f32) }));
    tips.extend(spread_range(0, h-1).into_iter().map(|y| { (w as f32, y as f32) }));
    dirs = tips.into_iter().map(|(x,y)| {
        let dx = x - cx;
        let dy = y - cy;
        let m = 2.0 * dx.abs().max(dy.abs());
        (dx/m, dy/m)
    }).collect();
    let ray = |(dx, dy): (f32, f32)| {
        let mut x = cx;
        let mut y = cy;
        let mut path = Vec::new();
        loop {
            x += dx;
            y += dy;
            let i: u64 = y.round() as u64 * w + x.round() as u64;
            path.push(i);
            if x < 0.0 || x.ceil() > w as f32  || y < 0.0 || y.ceil() > h as f32 { break; }
        }
        path
    };

    return dirs.into_par_iter().map(ray).collect();
}

fn path_rect_spiral(w: u64, h: u64, square: bool) -> Vec<Vec<u64>> {
    let mut paths: Vec<Vec<u64>> = Vec::new();
    let mut x = w / 2;
    let mut y = h / 2;
    let pixelcount = w * h;
    let max_size = std::cmp::max(w, h);

    let mut path = Vec::new();
    let mut add_pixel_at = |x: u64, y: u64| {
        let i = y * w + x;
        if i < pixelcount {
            path.push(i)
        }
    };

    // Add pixel in the middle
    add_pixel_at(x, y);
    let mut reach_x = 1;
    let mut reach_y = 1;
    if !square {
        if w > h {
            reach_x = std::cmp::max(1, w - h);
        } else {
            reach_y = std::cmp::max(1, h - w);
        }
        x -= reach_x / 2;
        y -= reach_y / 2;
    }

    loop {
        for _ in 0..reach_x {
            x += 1;
            add_pixel_at(x, y);
        }
        for _ in 0..reach_y {
            y += 1;
            add_pixel_at(x, y);
        }
        reach_x += 1;
        reach_y += 1;
        if reach_y >= max_size || reach_x >= max_size {
            break;
        };
        for _ in 0..reach_x {
            x -= 1;
            add_pixel_at(x, y);
        }
        for _ in 0..reach_y {
            y -= 1;
            add_pixel_at(x, y);
        }
        reach_x += 1;
        reach_y += 1;
        if reach_y >= max_size || reach_x >= max_size {
            break;
        };
    }
    paths.push(path);

    return paths;
}

// Not really a spiral, more like connected circles
fn path_round_spiral(w: u64, h: u64) -> Vec<Vec<u64>> {
    let mut paths: Vec<Vec<u64>> = Vec::new();
    let mut x = w as f64 / 2.0;
    let mut y = h as f64 / 2.0;
    let pixelcount = w * h;
    // The max radius has to be
    let max_size = ((w * w + h * h) as f64).sqrt().ceil() as u64;

    let mut r = 1.0;
    let angle_offset = -0.5 * PI;

    // TODO: let angle_offset be set
    // TODO: make elliptic, not just circles
    // TODO: Allow to set center

    let line_path = |r| {
        let mut path = Vec::new();
        let step_amounts = 16 * r as u64;
        let step_size: f64 = 2.0 * PI / step_amounts as f64;
        for step in 0..=step_amounts {
            let angle = angle_offset + step_size * step as f64;
            let xi = x + angle.cos() * r as f64;
            let yi = y + angle.sin() * r as f64;
            if !is_in_bounds(xi as u64, yi as u64, w, h) {
                continue;
            }
            path.push(yi as u64 * w + xi as u64);
        }
        path
    };

    // THREADING, WOOO
    let path_iter = (1..max_size / 2).into_par_iter().map(line_path);
    paths = vec![path_iter.flatten().collect()];
    return paths;
}

fn path_circles(w: u64, h: u64) -> Vec<Vec<u64>> {
    let mut paths: Vec<Vec<u64>> = Vec::new();
    let mut x = w as f64 / 2.0;
    let mut y = h as f64 / 2.0;
    let pixelcount = w * h;
    // The max radius has to be
    let max_size = ((w * w + h * h) as f64).sqrt().ceil() as u64;

    let mut r = 1;
    let angle_offset = -0.5 * PI;

    // TODO: let angle_offset be set
    // TODO: make elliptic, not just circles
    // TODO: Allow to set center

    let line_path = |r| {
        let mut path_left = Vec::new();
        let mut path_right = Vec::new();
        let step_amounts = 8 * r as u64;
        let circ_step_size: f64 = PI / step_amounts as f64;
        for step in 0..=step_amounts {
            let angle = angle_offset + circ_step_size * step as f64;
            let xi = x + angle.cos() * r as f64;
            let yi = y + angle.sin() * r as f64;
            path_left.push(yi as u64 * w + xi as u64);
            let angle = angle_offset - circ_step_size * step as f64;
            let xi = x + angle.cos() * r as f64;
            let yi = y + angle.sin() * r as f64;
            path_right.push(yi as u64 * w + xi as u64);
        }
        vec![path_left, path_right]
    };
    // THREADING, WOOO
    paths.par_extend((1..max_size / 2).into_par_iter().map(line_path).flatten());
    return paths;
}


/// Wavy lines. Vertical lines (top to bottom) that swing left and right, or horizontal lines
/// (left to right) that swing up and down. All lines have the same shape, so they never overlap.
fn path_waves(w: u64, h: u64, amplitude: f32, wavelength: f32, horizontal: bool) -> Vec<Vec<u64>> {
    let amplitude = amplitude.abs() as f64;
    let wavelength = (wavelength.abs() as f64).max(1.0);
    let (along, across) = if horizontal { (w, h) } else { (h, w) };
    // Offset of the line at every position along it
    let offsets: Vec<i64> = (0..along)
        .map(|t| (amplitude * (TWO_PI * t as f64 / wavelength).sin()).round() as i64)
        .collect();
    let extra = amplitude.ceil() as i64;
    (-extra..across as i64 + extra)
        .into_par_iter()
        .map(|start| {
            let mut path = Vec::with_capacity(along as usize);
            for t in 0..along {
                let a = start + offsets[t as usize];
                if a < 0 || a >= across as i64 {
                    continue;
                }
                let (x, y) = if horizontal { (t, a as u64) } else { (a as u64, t) };
                path.push(y * w + x);
            }
            path
        })
        .filter(|p| !p.is_empty())
        .collect()
}

/// Index of (x, y) if it is inside the image
fn index_in_bounds(x: f64, y: f64, w: u64, h: u64) -> Option<u64> {
    if x < 0.0 || y < 0.0 {
        return None;
    }
    let (x, y) = (x as u64, y as u64);
    if x < w && y < h {
        Some(y * w + x)
    } else {
        None
    }
}

/// Distance from (cx, cy) to the farthest corner of the image
fn farthest_corner(w: u64, h: u64, cx: f64, cy: f64) -> f64 {
    let dx = cx.max(w as f64 - cx);
    let dy = cy.max(h as f64 - cy);
    (dx * dx + dy * dy).sqrt()
}

/// Rays from a custom center to the border of the image
fn path_rays_at(w: u64, h: u64, cx: f32, cy: f32) -> Vec<Vec<u64>> {
    let mut tips: Vec<(f32, f32)> = spread_range(0, w - 1).into_iter().map(|x| (x as f32, 0.0)).collect();
    tips.extend(spread_range(0, w - 1).into_iter().map(|x| (x as f32, h as f32)));
    tips.extend(spread_range(0, h - 1).into_iter().map(|y| (0.0, y as f32)));
    tips.extend(spread_range(0, h - 1).into_iter().map(|y| (w as f32, y as f32)));
    tips.into_par_iter()
        .map(|(tx, ty)| {
            let (dx, dy) = (tx - cx, ty - cy);
            let m = 2.0 * dx.abs().max(dy.abs());
            let mut path = Vec::new();
            if m == 0.0 {
                return path;
            }
            let (dx, dy) = (dx / m, dy / m);
            let (mut x, mut y) = (cx, cy);
            loop {
                if let Some(i) = index_in_bounds(x.round() as f64, y.round() as f64, w, h) {
                    path.push(i);
                }
                x += dx;
                y += dy;
                if x < -1.0 || x > w as f32 + 1.0 || y < -1.0 || y > h as f32 + 1.0 {
                    break;
                }
            }
            path
        })
        .collect()
}

/// Circles around a custom center (each circle is split into a left and a right half)
fn path_circles_at(w: u64, h: u64, cx: f64, cy: f64) -> Vec<Vec<u64>> {
    let max_radius = farthest_corner(w, h, cx, cy).ceil() as u64 + 1;
    let angle_offset = -0.5 * PI;
    (1..max_radius)
        .into_par_iter()
        .map(|r| {
            let mut path_left = Vec::new();
            let mut path_right = Vec::new();
            let step_amounts = 8 * r;
            let step_size = PI / step_amounts as f64;
            for step in 0..=step_amounts {
                let angle = angle_offset + step_size * step as f64;
                if let Some(i) = index_in_bounds(cx + angle.cos() * r as f64, cy + angle.sin() * r as f64, w, h) {
                    path_left.push(i);
                }
                let angle = angle_offset - step_size * step as f64;
                if let Some(i) = index_in_bounds(cx + angle.cos() * r as f64, cy + angle.sin() * r as f64, w, h) {
                    path_right.push(i);
                }
            }
            vec![path_left, path_right]
        })
        .flatten()
        .collect()
}

/// Connected circles around a custom center
fn path_round_spiral_at(w: u64, h: u64, cx: f64, cy: f64) -> Vec<Vec<u64>> {
    let max_radius = farthest_corner(w, h, cx, cy).ceil() as u64 + 1;
    let angle_offset = -0.5 * PI;
    let path: Vec<u64> = (1..max_radius)
        .into_par_iter()
        .map(|r| {
            let mut path = Vec::new();
            let step_amounts = 16 * r;
            let step_size = 2.0 * PI / step_amounts as f64;
            for step in 0..=step_amounts {
                let angle = angle_offset + step_size * step as f64;
                if let Some(i) = index_in_bounds(cx + angle.cos() * r as f64, cy + angle.sin() * r as f64, w, h) {
                    path.push(i);
                }
            }
            path
        })
        .flatten()
        .collect();
    vec![path]
}

/// A square or rectangular spiral around a custom center
fn path_rect_spiral_at(w: u64, h: u64, cx: f64, cy: f64, square: bool) -> Vec<Vec<u64>> {
    let mut x = cx.floor() as i64;
    let mut y = cy.floor() as i64;
    let (wi, hi) = (w as i64, h as i64);
    // The spiral has to reach the farthest corner
    let max_size = 2 * std::cmp::max(wi, hi) + 2;
    let mut path = Vec::new();
    let mut add_pixel_at = |x: i64, y: i64| {
        if x >= 0 && y >= 0 && x < wi && y < hi {
            path.push((y * wi + x) as u64);
        }
    };

    add_pixel_at(x, y);
    let mut reach_x = 1;
    let mut reach_y = 1;
    if !square {
        if wi > hi {
            reach_x = std::cmp::max(1, wi - hi);
        } else {
            reach_y = std::cmp::max(1, hi - wi);
        }
        x -= reach_x / 2;
        y -= reach_y / 2;
    }
    loop {
        for _ in 0..reach_x {
            x += 1;
            add_pixel_at(x, y);
        }
        for _ in 0..reach_y {
            y += 1;
            add_pixel_at(x, y);
        }
        reach_x += 1;
        reach_y += 1;
        if reach_y >= max_size || reach_x >= max_size {
            break;
        };
        for _ in 0..reach_x {
            x -= 1;
            add_pixel_at(x, y);
        }
        for _ in 0..reach_y {
            y -= 1;
            add_pixel_at(x, y);
        }
        reach_x += 1;
        reach_y += 1;
        if reach_y >= max_size || reach_x >= max_size {
            break;
        };
    }
    vec![path]
}


/// Pseudo random value in [0, 1) for a grid point
fn hash2(x: i64, y: i64, seed: u32) -> f32 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ (seed as u64).wrapping_mul(0x1656_67B1_9E37_79F9);
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    h ^= h >> 33;
    (h >> 40) as f32 / (1u64 << 24) as f32
}

/// Smooth value noise in [0, 1)
fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (ix, iy) = (x0 as i64, y0 as i64);
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    let top = lerp(hash2(ix, iy, seed), hash2(ix + 1, iy, seed), sx);
    let bottom = lerp(hash2(ix, iy + 1, seed), hash2(ix + 1, iy + 1, seed), sx);
    lerp(top, bottom, sy)
}

/// Direction of the flow field at (x, y) as an angle
fn flow_angle(x: f32, y: f32, scale: f32, seed: u32) -> f32 {
    let mut value = 0.0;
    let mut total = 0.0;
    let mut amplitude = 1.0;
    let mut frequency = 1.0 / scale;
    for octave in 0..3 {
        value += amplitude * value_noise(x * frequency, y * frequency, seed.wrapping_add(octave * 7919));
        total += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }
    (value / total) * 4.0 * std::f32::consts::PI
}

/// Lines that follow a swirling noise field. Every pixel belongs to exactly one line.
fn path_flow_field(w: u64, h: u64, scale: f32, seed: u32) -> Vec<Vec<u64>> {
    const MAX_LENGTH: usize = 4096;
    let scale = scale.abs().max(1.0);
    let n = (w * h) as usize;
    // Direction (step in x and y) at every pixel
    let directions: Vec<(f32, f32)> = (0..h)
        .into_par_iter()
        .map(|y| {
            (0..w)
                .map(|x| {
                    let angle = flow_angle(x as f32, y as f32, scale, seed);
                    (angle.cos(), angle.sin())
                })
                .collect::<Vec<(f32, f32)>>()
        })
        .flatten()
        .collect();

    let mut visited = vec![false; n];
    let mut paths = Vec::new();
    for start in 0..n {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let mut path = vec![start as u64];
        let (mut fx, mut fy) = ((start as u64 % w) as f32 + 0.5, (start as u64 / w) as f32 + 0.5);
        let mut last = start;
        let mut steps_in_same_pixel = 0;
        while path.len() < MAX_LENGTH {
            let (dx, dy) = directions[last];
            fx += dx;
            fy += dy;
            if fx < 0.0 || fy < 0.0 || fx >= w as f32 || fy >= h as f32 {
                break;
            }
            let i = (fy as u64 * w + fx as u64) as usize;
            if i == last {
                steps_in_same_pixel += 1;
                if steps_in_same_pixel > 3 {
                    break;
                }
                continue;
            }
            steps_in_same_pixel = 0;
            if visited[i] {
                break;
            }
            visited[i] = true;
            path.push(i as u64);
            last = i;
        }
        paths.push(path);
    }
    paths
}

/// Rings shaped like a star (or a polygon) around (cx, cy). Each ring starts at the top and goes clockwise.
fn path_star(w: u64, h: u64, cx: f64, cy: f64, points: u32, inner: f32) -> Vec<Vec<u64>> {
    let points = points.clamp(2, 100);
    let inner = (inner as f64).clamp(0.05, 1.0);
    // Half the angle between two points
    let sector = PI / points as f64;
    // The outline between an outer point A = (1, 0) and an inner point B
    let ab = (inner * sector.cos() - 1.0, inner * sector.sin());
    let cross = |p: (f64, f64), q: (f64, f64)| p.0 * q.1 - p.1 * q.0;
    let a_cross_ab = cross((1.0, 0.0), ab);
    // Radius of the outline (outer points have radius 1) in the direction theta
    let outline = |theta: f64| -> f64 {
        // Rotate so one point faces up, then mirror into the first half of a sector
        let mut phi = (theta + HALF_PI).rem_euclid(2.0 * sector);
        if phi > sector {
            phi = 2.0 * sector - phi;
        }
        a_cross_ab / cross((phi.cos(), phi.sin()), ab)
    };

    // Ring number and angle of every pixel
    let ring_of_pixel: Vec<(u32, f32)> = (0..h)
        .into_par_iter()
        .map(|y| {
            (0..w)
                .map(|x| {
                    let (dx, dy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy);
                    let theta = dy.atan2(dx);
                    let distance = (dx * dx + dy * dy).sqrt();
                    let ring = (distance / outline(theta)) as u32;
                    // Angle starting at the top, clockwise
                    let angle = (theta + HALF_PI).rem_euclid(TWO_PI) as f32;
                    (ring, angle)
                })
                .collect::<Vec<(u32, f32)>>()
        })
        .flatten()
        .collect();

    let ring_count = ring_of_pixel.iter().map(|r| r.0).max().map(|m| m as usize + 1).unwrap_or(0);
    let mut rings: Vec<Vec<(f32, u32)>> = vec![Vec::new(); ring_count];
    for (i, (ring, angle)) in ring_of_pixel.into_iter().enumerate() {
        rings[ring as usize].push((angle, i as u32));
    }
    rings.par_iter_mut().for_each(|ring| ring.sort_by(|a, b| a.0.total_cmp(&b.0)));
    rings
        .into_par_iter()
        .filter(|ring| !ring.is_empty())
        .map(|ring| ring.into_iter().map(|(_, i)| i as u64).collect())
        .collect()
}
