use image::{GrayImage, Luma, RgbImage};
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
}

impl SortJob {
    /// Sorts all layers of the job one after another. Returns the sorted image of every layer.
    pub(crate) fn run(self) -> SortJobResult {
        let mut results = Vec::with_capacity(self.steps.len());
        let mut img = self.input;
        for (values, mask, guide, directions) in &self.steps {
            img = values.render(&img, mask.as_ref(), guide.as_ref(), directions.as_deref().map(|d| d.as_slice()));
            results.push(img.clone());
        }
        SortJobResult { start: self.start, images: results, checks: self.checks }
    }
}

pub(crate) struct SortJobResult {
    start: usize,
    images: Vec<RgbImage>,
    checks: Vec<(PixelsorterValues, u64)>,
}

impl SortJobResult {
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
    pub(crate) fn prepare_sort_job(&self, force: bool) -> Option<SortJob> {
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
        })
    }

    /// Puts the results of a background sort into the layers.
    /// Results of layers whose values or mask changed in the meantime are thrown away (and all above).
    pub(crate) fn apply_sort_result(&mut self, result: SortJobResult) {
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
