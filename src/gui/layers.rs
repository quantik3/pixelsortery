use image::{
    imageops::{self, FilterType},
    GrayImage, Luma, RgbImage,
};
use pixelsortery::{path_creator::PathCreator, pixel_selector::PixelSelector};
use std::sync::Arc;
use pixelsortery::Pixelsorter;

use super::blend::render_layer;
use super::PixelsorterValues;

pub(crate) struct LayeredSorter {
    base_img: RgbImage,
    layers: Vec<SortingLayer>,
    current_layer: usize,
    /// Source of unique mask revisions (never repeats, also not after undo)
    revision_counter: u64,
}

/// The state of one layer, for undo/redo (without the sorted image, that is sorted again)
#[derive(Clone)]
pub(crate) struct LayerSnapshot {
    values: PixelsorterValues,
    paint_mask: Option<Arc<GrayImage>>,
    guide: Option<Arc<GrayImage>>,
    directions: Option<Arc<Vec<f32>>>,
    revision: u64,
}

/// The state of all layers, for undo/redo
#[derive(Clone)]
pub(crate) struct StackSnapshot {
    layers: Vec<LayerSnapshot>,
    current: usize,
}

pub(crate) struct SortingLayer {
    sorting_values: PixelsorterValues,
    sorted_img: RgbImage,
    needs_sorting: bool,
    /// Painted mask: where the layer is applied (white = fully, gray = partly, black = not at all).
    /// None means the whole image is sorted.
    paint_mask: Option<GrayImage>,
    /// Increased every time the painted mask or the guide changes, to detect outdated background results
    mask_revision: u64,
    /// Guide image (brightness, same size as the image) for the criteria "Guide"
    guide: Option<GrayImage>,
    /// Painted directions for the path "Drawn": one angle per pixel, NaN = not painted.
    /// Shared with background sorts, so starting one doesn't copy it.
    directions: Option<Arc<Vec<f32>>>,
}

/// Everything needed to sort some layers in a background thread
pub(crate) struct SortJob {
    /// Index of the first layer that gets sorted
    pub(crate) start: usize,
    /// The image the first layer is applied to
    input: RgbImage,
    /// Values, mask, guide and directions for every layer from `start` up to the current layer
    steps: Vec<(PixelsorterValues, Option<GrayImage>, Option<GrayImage>, Option<Arc<Vec<f32>>>)>,
    /// Values and mask revisions the job was created with
    checks: Vec<(PixelsorterValues, u64)>,
    /// Sort a smaller copy for a quick preview (only shown, not kept)
    preview: bool,
}

/// Everything needed to apply the whole layer stack to any image (frames of an animation or a video).
/// One layer can be animated: its values move from `start` to `end`.
#[derive(Clone)]
pub(crate) struct StackRenderer {
    steps: Vec<(PixelsorterValues, Option<GrayImage>, Option<GrayImage>, Option<Arc<Vec<f32>>>)>,
    /// (index of the animated layer, values at the start, values at the end)
    animation: Option<(usize, PixelsorterValues, PixelsorterValues)>,
}

impl StackRenderer {
    /// Index of the first layer that changes during the animation (all layers below stay the same)
    pub(crate) fn first_animated_layer(&self) -> usize {
        self.animation.map(|a| a.0).unwrap_or(0).min(self.steps.len())
    }

    /// Applies the layers `from..` to the image, at the animation time `t` (0 = start, 1 = end)
    pub(crate) fn render_from(&self, img: &RgbImage, from: usize, t: f32) -> RgbImage {
        let mut img = img.clone();
        for (i, (values, mask, guide, directions)) in self.steps.iter().enumerate().skip(from) {
            let values = match self.animation {
                Some((index, start, end)) if index == i => start.lerp(&end, t),
                _ => *values,
            };
            img = values.render(&img, mask.as_ref(), guide.as_ref(), directions.as_deref().map(|d| d.as_slice()));
        }
        img
    }

    /// Applies the layers below the animated one (they are the same for every frame)
    pub(crate) fn render_static_part(&self, img: &RgbImage) -> RgbImage {
        let mut below = self.clone();
        below.steps.truncate(self.first_animated_layer());
        below.animation = None;
        below.render_from(img, 0, 0.0)
    }
}

/// Longer side of the image in a quick preview
const PREVIEW_SIZE: u32 = 1000;

impl SortJob {
    /// Sorts all layers of the job one after another. Returns the sorted image of every layer.
    pub(crate) fn run(self) -> SortJobResult {
        let (w, h) = self.input.dimensions();
        let scale = PREVIEW_SIZE as f32 / w.max(h).max(1) as f32;
        if self.preview && scale < 1.0 {
            return self.run_preview(scale);
        }
        let mut results = Vec::with_capacity(self.steps.len());
        let mut img = self.input;
        for (values, mask, guide, directions) in &self.steps {
            img = values.render(&img, mask.as_ref(), guide.as_ref(), directions.as_deref().map(|d| d.as_slice()));
            results.push(img.clone());
        }
        SortJobResult { start: self.start, images: results, checks: self.checks, preview: false }
    }

    /// Sorts a smaller copy (everything scaled by `scale`), then enlarges the result for showing it
    fn run_preview(self, scale: f32) -> SortJobResult {
        let (w, h) = self.input.dimensions();
        let (sw, sh) = (((w as f32 * scale).round() as u32).max(1), ((h as f32 * scale).round() as u32).max(1));
        let mut img = imageops::resize(&self.input, sw, sh, FilterType::Triangle);
        for (values, mask, guide, directions) in &self.steps {
            let mask = mask.as_ref().map(|m| imageops::resize(m, sw, sh, FilterType::Triangle));
            let guide = guide.as_ref().map(|g| imageops::resize(g, sw, sh, FilterType::Triangle));
            let directions = directions.as_deref().map(|d| scale_directions(d, w, h, sw, sh));
            img = values.scaled(scale).render(&img, mask.as_ref(), guide.as_ref(), directions.as_deref());
        }
        let shown = imageops::resize(&img, w, h, FilterType::Nearest);
        SortJobResult { start: self.start, images: vec![shown], checks: Vec::new(), preview: true }
    }
}

/// Picks the painted direction for every pixel of a smaller image (nearest pixel)
fn scale_directions(directions: &[f32], w: u32, h: u32, sw: u32, sh: u32) -> Vec<f32> {
    let mut scaled = Vec::with_capacity((sw * sh) as usize);
    for y in 0..sh {
        let sy = ((y as u64 * h as u64) / sh as u64).min(h as u64 - 1);
        for x in 0..sw {
            let sx = ((x as u64 * w as u64) / sw as u64).min(w as u64 - 1);
            scaled.push(directions[(sy * w as u64 + sx) as usize]);
        }
    }
    scaled
}

pub(crate) struct SortJobResult {
    start: usize,
    images: Vec<RgbImage>,
    checks: Vec<(PixelsorterValues, u64)>,
    preview: bool,
}

impl SortJobResult {
    /// A quick preview of a smaller copy: only for showing, not kept in the layers
    pub(crate) fn is_preview(&self) -> bool {
        self.preview
    }

    /// The sorted image of the last (= the current) layer
    pub(crate) fn last_image(&self) -> Option<&RgbImage> {
        self.images.last()
    }
}

impl LayeredSorter {
    pub(crate) fn new(img: RgbImage, ps: PixelsorterValues) -> Self {
        let mut ls = LayeredSorter {
            base_img: img,
            layers: Vec::new(),
            current_layer: 0,
            revision_counter: 0,
        };
        ls.add_layer(ps);
        ls
    }

    pub(crate) fn get_layers(&self) -> &Vec<SortingLayer> {
        &self.layers
    }
    pub(crate) fn get_layer<T: Into<usize>>(&self, ind: T) -> Option<&SortingLayer> {
        self.layers.get(ind.into())
    }
    pub(crate) fn get_current_layer(&self) -> &SortingLayer {
        self.layers.get(self.current_layer).unwrap()
    }
    pub(crate) fn get_current_index(&self) -> usize {
        self.current_layer
    }
    pub(crate) fn get_base_img(&self) -> &RgbImage {
        &self.base_img
    }
    pub(crate) fn set_base_img(&mut self, img: RgbImage) {
        // Painted masks only fit images with the same size
        if img.dimensions() != self.base_img.dimensions() {
            for layer in self.layers.iter_mut() {
                layer.paint_mask = None;
                layer.guide = None;
                layer.directions = None;
                self.revision_counter += 1;
                layer.mask_revision = self.revision_counter;
            }
        }
        self.base_img = img;
        self.invalidate_layers_above(0usize, true);
    }

    /// The painted mask of the current layer (None = the whole image is sorted)
    pub(crate) fn get_current_paint_mask(&self) -> Option<&GrayImage> {
        self.get_current_layer().paint_mask.as_ref()
    }

    /// The painted mask of the current layer. Creates an empty mask (nothing gets sorted) if there is none yet.
    /// Call `paint_mask_changed` after changing it.
    pub(crate) fn get_current_paint_mask_mut(&mut self) -> &mut GrayImage {
        let (w, h) = self.base_img.dimensions();
        let layer = self.layers.get_mut(self.current_layer).unwrap();
        // Recreate the mask if it doesn't fit (should not happen)
        if layer.paint_mask.as_ref().map(|m| m.dimensions() != (w, h)).unwrap_or(true) {
            layer.paint_mask = Some(GrayImage::new(w, h));
        }
        layer.paint_mask.as_mut().unwrap()
    }

    /// Replace the painted mask of the current layer (None = sort the whole image)
    pub(crate) fn set_current_paint_mask(&mut self, mask: Option<GrayImage>) {
        self.layers.get_mut(self.current_layer).unwrap().paint_mask = mask;
        self.paint_mask_changed();
    }

    /// Prepares sorting the current layer (and all layers below that need it) in a background thread.
    /// Returns None if nothing needs to be sorted (and `force` is false).
    pub(crate) fn prepare_sort_job(&self, force: bool, preview: bool) -> Option<SortJob> {
        let current = self.current_layer;
        let start = self
            .layers
            .iter()
            .take(current + 1)
            .position(|l| l.needs_sorting)
            .or(if force { Some(current) } else { None })?;
        let input = if start == 0 {
            self.base_img.clone()
        } else {
            self.layers[start - 1].sorted_img.clone()
        };
        let layers = &self.layers[start..=current];
        Some(SortJob {
            start,
            input,
            steps: layers
                .iter()
                .map(|l| (l.sorting_values, l.paint_mask.clone(), l.guide.clone(), l.directions.clone()))
                .collect(),
            checks: layers.iter().map(|l| (l.sorting_values, l.mask_revision)).collect(),
            preview,
        })
    }

    /// Puts the results of a background sort into the layers.
    /// Results of layers whose values or mask changed in the meantime are thrown away (and all above).
    pub(crate) fn apply_sort_result(&mut self, result: SortJobResult) {
        if result.preview {
            return;
        }
        let start = result.start;
        let mut applied = 0;
        for (j, (img, (values, revision))) in result.images.into_iter().zip(result.checks).enumerate() {
            let Some(layer) = self.layers.get_mut(start + j) else { break };
            if layer.sorting_values != values || layer.mask_revision != revision {
                break;
            }
            layer.sorted_img = img;
            layer.needs_sorting = false;
            applied += 1;
        }
        if applied > 0 {
            self.invalidate_layers_above(start + applied - 1, false);
        }
    }

    /// The guide image of the current layer
    pub(crate) fn get_current_guide(&self) -> Option<&GrayImage> {
        self.get_current_layer().guide.as_ref()
    }

    /// Sets the guide image of the current layer (it has to have the size of the image)
    pub(crate) fn set_current_guide(&mut self, guide: Option<GrayImage>) {
        let fits = guide.as_ref().map(|g| g.dimensions() == self.base_img.dimensions()).unwrap_or(true);
        if fits {
            self.layers.get_mut(self.current_layer).unwrap().guide = guide;
            // Same handling as a changed mask: sort again, outdated results are dropped
            self.paint_mask_changed();
        }
    }

    /// The painted directions of the current layer (one angle per pixel, NaN = not painted)
    pub(crate) fn get_current_directions(&self) -> Option<&[f32]> {
        self.get_current_layer().directions.as_deref().map(|d| d.as_slice())
    }

    /// The painted directions of the current layer, created (nothing painted) if there are none yet.
    /// Call `paint_mask_changed` after changing them.
    pub(crate) fn get_current_directions_mut(&mut self) -> &mut Vec<f32> {
        let (w, h) = self.base_img.dimensions();
        let n = (w as usize) * (h as usize);
        let layer = self.layers.get_mut(self.current_layer).unwrap();
        if layer.directions.as_ref().map(|d| d.len() != n).unwrap_or(true) {
            layer.directions = Some(Arc::new(vec![f32::NAN; n]));
        }
        // Copies the directions only if a background sort is still using them
        Arc::make_mut(layer.directions.as_mut().unwrap())
    }

    /// Removes the painted directions of the current layer
    pub(crate) fn clear_current_directions(&mut self) {
        self.layers.get_mut(self.current_layer).unwrap().directions = None;
        self.paint_mask_changed();
    }

    pub(crate) fn has_current_directions(&self) -> bool {
        self.get_current_layer().directions.is_some()
    }

    /// Size of the image
    pub(crate) fn image_size(&self) -> (u32, u32) {
        self.base_img.dimensions()
    }

    /// Creates a mask with the size of the image, filled with one value (0 = nothing sorted, 255 = everything sorted)
    pub(crate) fn new_paint_mask(&self, value: u8) -> GrayImage {
        let (w, h) = self.base_img.dimensions();
        GrayImage::from_pixel(w, h, Luma([value]))
    }

    /// Marks the current layer (and all above) as needing a sort, because its mask changed
    pub(crate) fn paint_mask_changed(&mut self) {
        self.revision_counter += 1;
        let layer = self.layers.get_mut(self.current_layer).unwrap();
        layer.mask_revision = self.revision_counter;
        layer.needs_sorting = true;
        self.invalidate_layers_above(self.current_layer, false);
    }

    /// Saves the state of all layers for undo/redo.
    /// Masks that didn't change since `previous` are shared with it instead of copied.
    pub(crate) fn snapshot(&self, previous: Option<&StackSnapshot>) -> StackSnapshot {
        StackSnapshot {
            current: self.current_layer,
            layers: self
                .layers
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    let unchanged = previous.and_then(|p| p.layers.get(i)).filter(|p| p.revision == l.mask_revision);
                    LayerSnapshot {
                        values: l.sorting_values,
                        paint_mask: match unchanged {
                            Some(p) => p.paint_mask.clone(),
                            None => l.paint_mask.clone().map(Arc::new),
                        },
                        guide: match unchanged {
                            Some(p) => p.guide.clone(),
                            None => l.guide.clone().map(Arc::new),
                        },
                        directions: l.directions.clone(),
                        revision: l.mask_revision,
                    }
                })
                .collect(),
        }
    }

    /// Everything needed to apply all layers to other images (animation frames, video frames).
    /// With `animation` = (start, end), the current layer is animated between these values.
    pub(crate) fn stack_renderer(&self, animation: Option<(PixelsorterValues, PixelsorterValues)>) -> StackRenderer {
        StackRenderer {
            steps: self
                .layers
                .iter()
                .map(|l| (l.sorting_values, l.paint_mask.clone(), l.guide.clone(), l.directions.clone()))
                .collect(),
            animation: animation.map(|(start, end)| (self.current_layer, start, end)),
        }
    }

    /// Is the state of the layers (not counting which one is selected) the one in the snapshot?
    pub(crate) fn matches_snapshot(&self, snapshot: &StackSnapshot) -> bool {
        self.layers.len() == snapshot.layers.len()
            && self
                .layers
                .iter()
                .zip(snapshot.layers.iter())
                .all(|(l, s)| l.sorting_values == s.values && l.mask_revision == s.revision)
    }

    /// Goes back to a saved state. All layers have to be sorted again.
    pub(crate) fn restore(&mut self, snapshot: &StackSnapshot) {
        if snapshot.layers.is_empty() {
            return;
        }
        let base = &self.base_img;
        self.layers = snapshot
            .layers
            .iter()
            .map(|s| SortingLayer {
                sorting_values: s.values,
                sorted_img: base.clone(),
                needs_sorting: true,
                paint_mask: s.paint_mask.as_deref().cloned(),
                // Same content as when the snapshot was taken, so the same revision
                mask_revision: s.revision,
                guide: s.guide.as_deref().cloned(),
                directions: s.directions.clone(),
            })
            .collect();
        self.current_layer = snapshot.current.min(self.layers.len() - 1);
    }

    pub(crate) fn print_state(&self) {
        println!("Printing Layers");
        for (i, l) in self.layers.iter().enumerate().rev() {
            println!(
                "[{}] {} {} {}",
                i,
                if l.needs_sorting { "*" } else { " " },
                l.sorting_values.to_pixelsorter().to_compact_string(),
                if i == self.current_layer { "<<<<" } else { "" },
            );
        }
    }

    /// Adds a layer, but don't sort it
    pub(crate) fn add_layer(&mut self, ps: PixelsorterValues) {
        let prev_img = if let Some(last) = self.get_layers().last() {
            last.get_img()
        } else {
            &self.base_img
        };
        // Creates a new layer at the end (which will need sorting)
        let layer = SortingLayer::new(ps, prev_img.clone());
        self.layers.push(layer);
    }

    /// Set values. Will determien if sort is needed
    pub(crate) fn update_current(&mut self, values: PixelsorterValues) {
        if self
            .layers
            .get_mut(self.current_layer)
            .unwrap()
            .set_sorting_values(values)
        {
            // Invalidate all layers above if the current one needs sorting.
            self.invalidate_layers_above(self.current_layer, false);
        }
    }

    /// Removes the layer and selects the one below, or the one above if unavailable
    pub(crate) fn remove_layer<T: Into<usize>>(&mut self, ind: T) -> bool {
        let ind = ind.into();
        // Don't allow removing if we only have one layer left
        if self.layers.len() <= 1 || ind >= self.layers.len() {
            return false;
        }

        // If the last layer was selected, adjust the selected index
        if self.current_layer == self.layers.len() - 1 || ind < self.current_layer {
            self.current_layer -= 1;
        }

        self.invalidate_layers_above(ind, false);
        self.layers.remove(ind);
        true
    }

    pub(crate) fn select_layer<T: Into<usize>>(&mut self, ind: T) -> Option<&SortingLayer> {
        let ind = ind.into();
        if ind < self.layers.len() {
            self.current_layer = ind;
            Some(&self.layers.get(ind).unwrap())
        } else {
            None
        }
    }

    /// Marks all layers above index that they need sorting
    pub(crate) fn invalidate_layers_above<T: Into<usize>>(&mut self, ind: T, inclusive: bool) {
        // If not inclusive, start at the layer above
        let ind = ind.into() + if inclusive { 0 } else { 1 };
        for layer in self.layers.iter_mut().skip(ind).rev() {
            layer.needs_sorting = true;
        }
    }

    /// Sorts the layer and all layers below if nessesary. Also marks all layers above as "needs-sorting"
    fn sort_layer<T: Into<usize>>(&mut self, ind: T) -> bool {
        let ind = ind.into();
        if ind >= self.layers.len() {
            return false;
        }

        let mut prev_img = &self.base_img;
        // As long as the layers are up to date, we don't need to sort
        let mut needs_sorting = false;
        for (i, layer) in self.layers.iter_mut().take(ind + 1).enumerate() {
            // Once we needed to sort a layer, we need to sort all of them
            needs_sorting = needs_sorting || layer.needs_sorting;
            // Sort the layer at [index] in any case
            if i == ind || needs_sorting {
                layer.sort(&prev_img);
            }
            prev_img = &layer.sorted_img;
        }
        self.invalidate_layers_above(ind, false);
        true
    }

    /// Sort the current layer, regardless if it's already sorted or not
    pub(crate) fn sort_current_layer(&mut self) {
        self.sort_layer(self.current_layer);
    }

    /// Sorts the current layer if it needs sorting
    pub(crate) fn sort_current_layer_cached(&mut self) -> bool {
        if self.get_current_layer().needs_sorting {
            self.sort_current_layer();
            true
        } else {
            false
        }
    }

    /// Sorts all layers below if needed but will not sort the current one, but instead only show the mask
    pub(crate) fn get_mask_for_current_layer(&mut self) -> Option<RgbImage> {
        let prev_index = self.current_layer - 1;
        // Bring the previous layer up to date (if nessesary)
        if let Some(layer) = self.get_layer(prev_index) {
            if layer.needs_sorting {
                self.sort_layer(prev_index);
            }
        }
        let prev_img = if let Some(layer) = self.get_layer(prev_index) {
            layer.get_img()
        } else {
            &self.base_img
        };
        self.get_current_layer().get_mask(prev_img)
    }
}

impl SortingLayer {
    /// Creates a new layer with an unsorted image (the one below it in the best case) and mark it as needs_sorting
    pub(crate) fn new(ps: PixelsorterValues, img: RgbImage) -> Self {
        SortingLayer {
            sorting_values: ps,
            sorted_img: img,
            needs_sorting: true,
            paint_mask: None,
            mask_revision: 0,
            guide: None,
            directions: None,
        }
    }

    pub(crate) fn get_sorting_values(&self) -> &PixelsorterValues {
        &self.sorting_values
    }
    /// Sets new values and if they differ from the current values, marks this layer as needs_sorting and returns true
    pub(crate) fn set_sorting_values(&mut self, ps: PixelsorterValues) -> bool {
        let mut changed = false;
        // Only set as needs_sorting when the new values are actually different?
        if self.sorting_values != ps {
            self.needs_sorting = true;
            changed = true;
        }
        self.sorting_values = ps;
        changed
    }

    pub(crate) fn get_img(&self) -> &RgbImage {
        &self.sorted_img
    }

    pub(crate) fn has_paint_mask(&self) -> bool {
        self.paint_mask.is_some()
    }

    fn sort(&mut self, img: &RgbImage) {
        self.sorted_img = self.sorting_values.render(
            img,
            self.paint_mask.as_ref(),
            self.guide.as_ref(),
            self.directions.as_deref().map(|d| d.as_slice()),
        );
        self.needs_sorting = false;
    }

    fn get_mask(&self, img: &RgbImage) -> Option<RgbImage> {
        let mut masked_img = img.clone();
        if self.sorting_values.to_pixelsorter().mask(&mut masked_img) {
            return Some(masked_img);
        }
        None
    }
}

impl PixelsorterValues {
    /// Values between `self` (t = 0) and `end` (t = 1). Numbers move smoothly (if both use the same
    /// kind of path / selector), everything else switches in the middle.
    pub(crate) fn lerp(&self, end: &PixelsorterValues, t: f32) -> PixelsorterValues {
        let t = t.clamp(0.0, 1.0);
        let f = |a: f32, b: f32| a + (b - a) * t;
        let int = |a: f64, b: f64| (a + (b - a) * t as f64).round();
        // Everything that can't move smoothly: start values in the first half, end values in the second
        let mut v = if t < 0.5 { *self } else { *end };

        v.path = match (self.path, end.path) {
            (PathCreator::Diagonally(a), PathCreator::Diagonally(b)) => PathCreator::Diagonally(f(a, b)),
            (PathCreator::Waves(a1, l1, _), PathCreator::Waves(a2, l2, _)) => {
                let horizontal = matches!(v.path, PathCreator::Waves(_, _, true));
                PathCreator::Waves(f(a1, a2), f(l1, l2), horizontal)
            }
            (PathCreator::FlowField(s1, _), PathCreator::FlowField(s2, _)) => {
                let seed = if let PathCreator::FlowField(_, seed) = v.path { seed } else { 0 };
                PathCreator::FlowField(f(s1, s2), seed)
            }
            (PathCreator::Star(p1, i1), PathCreator::Star(p2, i2)) => {
                PathCreator::Star(int(p1 as f64, p2 as f64) as u32, f(i1, i2))
            }
            (PathCreator::X3(o1, w1, s1), PathCreator::X3(o2, w2, s2)) => PathCreator::X3(
                int(o1 as f64, o2 as f64) as i32,
                int(w1 as f64, w2 as f64) as i64,
                s1 + (s2 - s1) * t as f64,
            ),
            _ => v.path,
        };
        v.selector = match (self.selector, end.selector) {
            (PixelSelector::Fixed { len: a }, PixelSelector::Fixed { len: b }) => {
                PixelSelector::Fixed { len: int(a as f64, b as f64) as u64 }
            }
            (PixelSelector::Random { max: a }, PixelSelector::Random { max: b }) => {
                PixelSelector::Random { max: int(a as f64, b as f64) as u32 }
            }
            (PixelSelector::Edges { threshold: a }, PixelSelector::Edges { threshold: b }) => {
                PixelSelector::Edges { threshold: int(a as f64, b as f64) as u64 }
            }
            (
                PixelSelector::Threshold { min: min1, max: max1, criteria: c1 },
                PixelSelector::Threshold { min: min2, max: max2, criteria: c2 },
            ) if c1 == c2 => PixelSelector::Threshold {
                min: int(min1 as f64, min2 as f64) as u64,
                max: int(max1 as f64, max2 as f64) as u64,
                criteria: c1,
            },
            _ => v.selector,
        };
        v.path_center = (f(self.path_center.0, end.path_center.0), f(self.path_center.1, end.path_center.1));
        v.passes = int(self.passes as f64, end.passes as f64) as u32;
        v.amount = f(self.amount, end.amount);
        v.opacity = f(self.opacity, end.opacity);
        v.posterize = int(self.posterize as f64, end.posterize as f64) as u8;
        v
    }

    /// The values for an image scaled by `scale`: lengths in pixels are scaled too
    pub(crate) fn scaled(&self, scale: f32) -> PixelsorterValues {
        let px = |v: f32| v * scale;
        let mut values = *self;
        values.selector = match self.selector {
            // 0 means "no spans" for these selectors, keep that
            PixelSelector::Fixed { len } if len > 0 => PixelSelector::Fixed { len: (px(len as f32).round() as u64).max(1) },
            PixelSelector::Random { max } if max > 1 => PixelSelector::Random { max: (px(max as f32).round() as u32).max(2) },
            other => other,
        };
        values.path = match self.path {
            PathCreator::Waves(amplitude, wavelength, horizontal) => {
                PathCreator::Waves(px(amplitude), px(wavelength).max(2.0), horizontal)
            }
            PathCreator::FlowField(size, seed) => PathCreator::FlowField(px(size).max(1.0), seed),
            PathCreator::X3(offset, width, slope) => PathCreator::X3(px(offset as f32) as i32, px(width as f32) as i64, slope),
            other => other,
        };
        values
    }

    /// Sorts the image with these values and blends the result over it (blend mode, opacity, mask)
    pub(crate) fn render(
        &self,
        img: &RgbImage,
        mask: Option<&GrayImage>,
        guide: Option<&GrayImage>,
        directions: Option<&[f32]>,
    ) -> RgbImage {
        render_layer(&self.to_pixelsorter(), self.blend, self.opacity, img, mask, guide, directions)
    }
}
