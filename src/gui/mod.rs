#![allow(unused)]
use eframe::egui::{
    self, Align, Button, Color32, Image, Key, Layout, Modifiers, RichText, ScrollArea,
    TextureFilter, TextureHandle, TextureOptions, Ui, Vec2,
};
use egui::{scroll_area::ScrollBarVisibility, style::ScrollStyle, pos2, Align2, ColorImage, FontId, Hyperlink, Id, LayerId, Modal, Order, PointerButton, Pos2, Rect, Response, Rgba, Sense, Stroke};
use egui_flex::{item, Flex, FlexAlign, FlexAlignContent, FlexJustify};
#[cfg(feature = "video")]
use egui_video::PlayerState;
use image::{GrayImage, Pixel, Rgb, RgbImage, RgbaImage};
use inflections::case::to_title_case;
use layers::{LayeredSorter, SortJobResult, StackSnapshot};
use log::{info, warn};
use pixelsortery::{
    path_creator::{PathCreator, DEFAULT_CENTER},
    pixel_selector::{
        PixelSelectCriteria,
        PixelSelector::{self, *},
    },
    span_sorter::{SortingAlgorithm, SortingCriteria},
    ChannelMode, Pixelsorter,
};
#[cfg(feature = "video")]
use pixelsortery::{Progress, ThreadPhone};
use std::{
    ffi::OsString, path::{Path, PathBuf}, sync::{atomic::{AtomicBool, Ordering}, mpsc::Receiver, Arc, Mutex}, thread::JoinHandle, time::{Duration, Instant}
};

use crate::{AUTHORS, PACKAGE_NAME, VERSION};
mod blend;
mod presets;
mod brush;
mod layers;
use blend::BlendMode;
use brush::{paint_direction_stroke, paint_mask_stroke};

/// How long the "Saved" Label should be visible before it vanishes
const SAVED_LABEL_VANISH_TIMEOUT: f32 = 2.0;
const INITIAL_WINDOW_SIZE: Vec2 = egui::vec2(1000.0, 700.0);
/// Color of the painted mask overlay (where the image gets sorted)
const MASK_OVERLAY_COLOR: [u8; 4] = [255, 40, 40, 100];

mod components;

pub fn init(ps: Option<&Pixelsorter>, img: Option<(RgbImage, PathBuf)>, video: Option<PathBuf>) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size(INITIAL_WINDOW_SIZE),
        ..Default::default()
    };

    let mut psgui = PixelsorterGui::default();
    #[cfg(feature = "video")] {
        psgui.audio_device = Some(egui_video::AudioDevice::new());
    }

    if let Some(ps) = ps {
        psgui = psgui.with_values(ps);
    }
    #[cfg(not(feature = "video"))]
    if let Some((img, img_path)) = img {
        psgui = psgui.with_image(img, img_path);
    }
    #[cfg(feature = "video")]
    if let Some((img, img_path)) = img {
        psgui = psgui.with_image(img, img_path);
    }
    else if let Some(video_path) = video {
        psgui.path = Some(video_path);
        psgui.do_open_video = true;
    }
    eframe::run_native(
        "Pixelsortery",
        options,
        Box::new(|cc| {
             // This gives us image support
            egui_extras::install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(psgui))
        }),
    )
}

/// Struct holding all the states of the gui and values of sliders etc.
struct PixelsorterGui {
    /// The path of the loaded image
    path: Option<PathBuf>,
    /// The Values and sorted Images, layered
    layered_sorter: Option<LayeredSorter>,
    /// All the adjustable values for the pixelsorter
    values: PixelsorterValues,
    show_mask: bool,
    /// The current image from the selected layer
    img: Option<RgbImage>,
    /// The image used by egui to draw every frame
    texture: Option<TextureHandle>,
    output_directory: Option<PathBuf>,
    save_into_parent_dir: bool,
    time_last_sort: Arc<Mutex<Duration>>,
    auto_sort: bool,
    do_sort: bool,
    saving_success_timeout: Option<Instant>,
    change_layer: SwitchLayerMessage,
    show_base_image: bool,
    /// Painting the mask with the mouse is active
    mask_edit: bool,
    /// Show the painted mask on top of the image while painting
    show_mask_overlay: bool,
    /// Brush radius in image pixels
    brush_radius: f32,
    /// The left mouse button erases instead of paints (the right mouse button always does the opposite)
    brush_erase: bool,
    /// 1 = hard brush edge, smaller = softer edge
    brush_hardness: f32,
    /// What the brush paints
    brush_tool: BrushTool,
    /// The tool the overlay texture was built for
    overlay_tool: BrushTool,
    /// Texture of the mask overlay
    mask_texture: Option<TextureHandle>,
    /// The mask texture has to be rebuilt (layer changed, mask replaced, ...)
    mask_texture_dirty: bool,
    /// A brush stroke is going on (mouse button held down after pressing it on the image)
    mask_stroke_active: bool,
    /// The mask was changed by the current stroke
    mask_stroke_changed: bool,
    /// Last brush position in image coordinates
    last_brush_pos: Option<(f32, f32)>,
    /// The next click on the image sets the center of rays, circles and spirals
    picking_center: bool,
    /// Zoom of the image view (1 = fit into the window)
    view_zoom: f32,
    /// Offset of the image view in screen pixels
    view_pan: Vec2,
    /// Before/after comparison: the original image is shown left of the divider
    compare: bool,
    /// Position of the comparison divider (0 - 1 of the image width)
    compare_pos: f32,
    /// Texture of the original image (for the comparison)
    original_texture: Option<TextureHandle>,
    /// Undo history (older states)
    undo_stack: Vec<StackSnapshot>,
    /// Redo history
    redo_stack: Vec<StackSnapshot>,
    /// The state the last undo entry was made for
    undo_current: Option<StackSnapshot>,
    /// Saved presets: (name, settings as text)
    presets: Vec<(String, String)>,
    /// Name typed in for a new preset
    preset_name: String,
    /// Index of the preset selected in the list
    selected_preset: Option<usize>,
    /// Settings text typed or pasted in
    preset_text: String,
    /// Sort in a background thread, so the window stays responsive
    background_sort: bool,
    /// While a slider is dragged, sort a smaller copy (needs background sorting)
    fast_preview: bool,
    /// A preview is shown: the full image has to be sorted when the mouse is released
    preview_shown: bool,
    /// The currently running background sort
    background_job: Option<BackgroundSort>,
    /// Another sort was requested while one was running (true = forced sort)
    background_pending: Option<bool>,
    /// Increased with every sort request; only the result of the newest request is kept
    sort_generation: u64,
    #[cfg(feature = "video")]
    audio_device: Option<egui_video::AudioDevice>,
    #[cfg(feature = "video")]
    video_player: Option<egui_video::Player>,
    #[cfg(feature = "video")]
    do_open_video: bool,
    /// A tuple for communicating with the current sorting thread
    #[cfg(feature = "video")]
    video_thread_phone: Option<ThreadPhone>,
}

/// Adjustable components of the pixelsorter, remembers values like diagonal angle
#[derive(Clone, Copy, PartialEq)]
struct PixelsorterValues {
    reverse: bool,
    path: PathCreator,
    selector: PixelSelector,
    criteria: SortingCriteria,
    algorithm: SortingAlgorithm,
    // Values that may not be set right now, but the values should be remembered
    /// We can select these with the real structs tbh
    path_diagonally_val: f32,
    path_x3: PathCreator,
    path_waves: PathCreator,
    path_flow: PathCreator,
    path_star: PathCreator,
    selector_random: PixelSelector,
    selector_fixed: PixelSelector,
    selector_thres: PixelSelector,
    selector_edges: PixelSelector,
    /// Center for rays, circles and spirals (relative to the image size)
    path_center: (f32, f32),
    /// Reverse every second span (herringbone)
    alternate: bool,
    /// Passes of the partial sort
    passes: u32,
    /// Strength of stretch and shift
    amount: f32,
    /// Which color channels move
    channels: ChannelMode,
    /// Posterize levels (0 = off)
    posterize: u8,
    /// How the layer is combined with the image below
    blend: BlendMode,
    /// Opacity of the layer (0 - 1)
    opacity: f32,
}

/// What the overlay on the image shows
#[derive(Clone, Copy)]
enum OverlaySource<'a> {
    Mask(&'a GrayImage),
    Directions(&'a [f32]),
}

/// What the brush paints
#[derive(Clone, Copy, PartialEq, Eq)]
enum BrushTool {
    /// Where the layer is applied
    Mask,
    /// Directions for the path "Drawn"
    Direction,
}

/// A sort running in a background thread
struct BackgroundSort {
    handle: JoinHandle<SortJobResult>,
    generation: u64,
    layer_count: usize,
    current_layer: usize,
    started: Instant,
}

#[derive(PartialEq)]
enum SwitchLayerMessage {
    None,
    BaseImage,
    Layer(usize),
    DeleteLayer(usize),
}

impl PixelsorterValues {
    // Reads values and returns a Pixelsorter
    fn to_pixelsorter(&self) -> Pixelsorter {
        let mut ps = Pixelsorter::new();
        ps.path_creator = self.path;
        ps.selector = self.selector;
        ps.sorter.criteria = self.criteria;
        ps.sorter.algorithm = self.algorithm;
        ps.sorter.passes = self.passes;
        ps.sorter.amount = self.amount;
        ps.channels = self.channels;
        ps.posterize = self.posterize;
        ps.reverse = self.reverse;
        ps.path_center = self.path_center;
        ps.alternate = self.alternate;
        ps
    }

    fn read_from_pixelsorter(&mut self, ps: &Pixelsorter) {
        self.path = ps.path_creator;
        self.selector = ps.selector;
        self.criteria = ps.sorter.criteria;
        self.algorithm = ps.sorter.algorithm;
        self.passes = ps.sorter.passes;
        self.amount = ps.sorter.amount;
        self.channels = ps.channels;
        self.posterize = ps.posterize;
        self.reverse = ps.reverse;
        self.path_center = ps.path_center;
        self.alternate = ps.alternate;
        // Set the saved value, just in case
        if let PathCreator::Diagonally(a) = self.path {
            self.path_diagonally_val = a;
        }
        match self.path {
            PathCreator::Waves(..) => self.path_waves = self.path,
            PathCreator::FlowField(..) => self.path_flow = self.path,
            PathCreator::Star(..) => self.path_star = self.path,
            _ => {}
        }
        match self.selector {
            Fixed { len } => self.selector_fixed = Fixed { len },
            Random { max } => self.selector_random = Random { max },
            Threshold { min, max, criteria } => {
                self.selector_thres = Threshold { min, max, criteria }
            }
            Edges { threshold } => self.selector_edges = Edges { threshold },
            Full => warn!("The gui doesn't support the Full-Selector. Just because."),
        }
    }
}

impl Default for PixelsorterGui {
    fn default() -> Self {
        Self {
            path: None,
            layered_sorter: None,
            img: None,
            texture: None,
            show_mask: false,
            values: PixelsorterValues {
                reverse: false,
                path: PathCreator::VerticalLines,
                criteria: SortingCriteria::Brightness,
                selector: PixelSelector::Threshold {
                    min: 0,
                    max: 360,
                    criteria: PixelSelectCriteria::Hue,
                },
                algorithm: SortingAlgorithm::Mapsort,

                path_diagonally_val: 45.0,
                path_x3: PathCreator::X3(0, 300, 10.0),
                path_waves: PathCreator::Waves(20.0, 120.0, false),
                path_flow: PathCreator::FlowField(150.0, 1),
                path_star: PathCreator::Star(5, 0.5),
                amount: 0.5,
                channels: ChannelMode::All,
                posterize: 0,
                blend: BlendMode::Normal,
                opacity: 1.0,
                selector_edges: PixelSelector::Edges { threshold: 80 },
                path_center: DEFAULT_CENTER,
                alternate: false,
                passes: 8,
                selector_random: PixelSelector::Random { max: 30 },
                selector_fixed: PixelSelector::Fixed { len: 100 },
                selector_thres: PixelSelector::Threshold {
                    min: 0,
                    max: 360,
                    criteria: PixelSelectCriteria::Brightness,
                },
            },
            time_last_sort: Arc::new(Mutex::new(Duration::default())),
            auto_sort: true,
            output_directory: None,
            save_into_parent_dir: false,
            saving_success_timeout: None,
            change_layer: SwitchLayerMessage::None,
            do_sort: true,
            show_base_image: false,
            mask_edit: false,
            show_mask_overlay: true,
            brush_radius: 30.0,
            brush_erase: false,
            brush_hardness: 1.0,
            brush_tool: BrushTool::Mask,
            overlay_tool: BrushTool::Mask,
            mask_texture: None,
            mask_texture_dirty: true,
            mask_stroke_active: false,
            mask_stroke_changed: false,
            last_brush_pos: None,
            picking_center: false,
            view_zoom: 1.0,
            view_pan: Vec2::ZERO,
            compare: false,
            compare_pos: 0.5,
            original_texture: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            undo_current: None,
            presets: presets::load_presets(),
            preset_name: String::new(),
            selected_preset: None,
            preset_text: String::new(),
            background_sort: true,
            fast_preview: true,
            preview_shown: false,
            background_job: None,
            background_pending: None,
            sort_generation: 0,
            #[cfg(feature = "video")]
            audio_device: None,
            #[cfg(feature = "video")]
            video_player: None,
            #[cfg(feature = "video")]
            do_open_video: false,
            #[cfg(feature = "video")]
            video_thread_phone: None,
        }
    }
}

impl PixelsorterGui {
    // Given a pixelsorter, return a PixelsorterGui with the values set
    fn with_values(mut self, ps: &Pixelsorter) -> Self {
        self.values.read_from_pixelsorter(ps);
        self
    }
    fn with_image(mut self, img: RgbImage, image_path: PathBuf) -> Self {
        self.img = Some(img);
        self.path = Some(image_path);
        self
    }

    /// Is the threshold mask shown instead of the sorted image?
    fn showing_threshold_mask(&self) -> bool {
        self.show_mask
            && self
                .layered_sorter
                .as_ref()
                .map(|ls| selector_is_threshold(ls.get_current_layer().get_sorting_values().selector))
                .unwrap_or(false)
    }

    /// Calls sort_current_layer, sets the image and texture
    fn sort_img(&mut self, ctx: &egui::Context, force: bool) {
        if self.background_sort && self.layered_sorter.is_some() && !self.showing_threshold_mask() {
            // The image and texture are set when the result arrives
            self.request_background_sort(ctx, force);
            return;
        }
        // Results of a background sort that is still running are outdated now
        self.sort_generation += 1;
        self.background_pending = None;
        if let Some(ls) = &mut self.layered_sorter {
            // values.selector is not up-to-date with the current layer as it seems
            if selector_is_threshold(ls.get_current_layer().get_sorting_values().selector)
                && self.show_mask
            {
                // We can unwrap here, because the mask() function only fails if we don't have a threshold selector
                self.img = Some(ls.get_mask_for_current_layer().unwrap().clone());
            } else {
                let timestart = Instant::now();
                let did_sort = if force {
                    ls.sort_current_layer();
                    true
                } else {
                    ls.sort_current_layer_cached()
                };
                // Set the time only when it actually sorted something (because it might decide that it doesnt need to sort)
                if did_sort {
                    *self.time_last_sort.lock().unwrap() = timestart.elapsed();
                }
                self.img = Some(ls.get_current_layer().get_img().clone());
            }
        }
        // Display sorted image
        self.update_texture(ctx);
    }

    /// Sorts in a background thread. If a sort is already running, the new one starts after it finished.
    fn request_background_sort(&mut self, ctx: &egui::Context, force: bool) {
        self.sort_generation += 1;
        if self.background_job.is_some() {
            self.background_pending = Some(self.background_pending.unwrap_or(false) || force);
        } else {
            self.start_background_sort(ctx, force);
        }
    }

    fn start_background_sort(&mut self, ctx: &egui::Context, force: bool) {
        let Some(ls) = &self.layered_sorter else { return };
        // A quick preview while the mouse is held (dragging a slider)
        let preview = self.fast_preview && ctx.input(|i| i.pointer.any_down());
        match ls.prepare_sort_job(force, preview) {
            None => {
                // Everything is sorted already, just show it
                self.img = Some(ls.get_current_layer().get_img().clone());
                self.update_texture(ctx);
            }
            Some(job) => {
                let repaint_ctx = ctx.clone();
                let handle = std::thread::spawn(move || {
                    let result = job.run();
                    // Wake up the gui to pick up the result
                    repaint_ctx.request_repaint();
                    result
                });
                self.background_job = Some(BackgroundSort {
                    handle,
                    generation: self.sort_generation,
                    layer_count: ls.get_layers().len(),
                    current_layer: ls.get_current_index(),
                    started: Instant::now(),
                });
            }
        }
    }

    /// Checks if the background sort finished, shows and stores its result and starts the next one if requested
    fn poll_background_sort(&mut self, ctx: &egui::Context) {
        let finished = match &self.background_job {
            None => return,
            Some(job) => job.handle.is_finished(),
        };
        if !finished {
            // In case the wake up from the thread gets lost
            ctx.request_repaint_after(Duration::from_millis(100));
            return;
        }
        let job = self.background_job.take().unwrap();
        let elapsed = job.started.elapsed();
        let is_newest = job.generation == self.sort_generation;
        let more_coming = self.background_pending.is_some();
        let show_result = !self.show_base_image && !self.showing_threshold_mask();

        match job.handle.join() {
            Ok(result) => {
                if let Some(ls) = &mut self.layered_sorter {
                    let same_layers = ls.get_layers().len() == job.layer_count
                        && ls.get_current_index() == job.current_layer;
                    if same_layers {
                        // Show the result, even if it is a bit outdated and a newer sort is coming
                        if show_result && (is_newest || more_coming) {
                            if let Some(img) = result.last_image() {
                                self.img = Some(img.clone());
                            }
                        }
                        if result.is_preview() {
                            self.preview_shown = true;
                        }
                        // Only keep the newest result (previews are never kept)
                        if is_newest {
                            ls.apply_sort_result(result);
                        }
                        *self.time_last_sort.lock().unwrap() = elapsed;
                    }
                }
                if show_result {
                    self.update_texture(ctx);
                }
            }
            Err(_) => warn!("Sorting in the background failed"),
        }

        if let Some(force) = self.background_pending.take() {
            self.start_background_sort(ctx, force);
        }
    }

    /// The image that should be saved. Makes sure the current layer is completely sorted.
    fn image_for_saving(&mut self) -> Option<RgbImage> {
        if self.show_base_image || self.showing_threshold_mask() {
            return self.img.clone();
        }
        match &mut self.layered_sorter {
            Some(ls) => {
                // A background sort might not be finished yet
                ls.sort_current_layer_cached();
                Some(ls.get_current_layer().get_img().clone())
            }
            None => self.img.clone(),
        }
    }

    /// Update texture
    fn update_texture(&mut self, ctx: &egui::Context) {
        if let Some(img) = &self.img {
            let colorimg = egui::ColorImage::from_rgb(
                [img.width() as usize, img.height() as usize],
                img.as_raw(),
            );
            let mut options = TextureOptions::default();
            // Make small images stretch without blurring
            options.magnification = TextureFilter::Nearest;
            // Make big images fit without noise
            options.minification = TextureFilter::Linear;
            self.texture = Some(ctx.load_texture("Some texture name", colorimg, options));
        }
    }

    /// Tries to show the image if it exists
    fn show_img(&mut self, ui: &mut Ui) {
        let picking = self.picking_center && self.layered_sorter.is_some() && self.values.path.uses_center();
        if !picking {
            self.picking_center = false;
        }
        let painting = !picking && self.mask_edit && self.layered_sorter.is_some();
        if let Some(tex) = &self.texture {
            let (tex_id, tex_size) = (tex.id(), tex.size_vec2());
            let sense = Sense::click_and_drag();
            let (area, response) = ui.allocate_exact_size(ui.available_size(), sense);
            let img_rect = self.image_view(ui, &response, area, tex_size, painting || picking);

            let painter = ui.painter_at(area);
            let full_uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            painter.image(tex_id, img_rect, full_uv, Color32::WHITE);
            if self.compare && !self.show_base_image {
                self.draw_comparison(ui, &response, area, img_rect, painting || picking);
            }
            painter.rect_stroke(img_rect, 0.0, ui.visuals().widgets.noninteractive.bg_stroke, egui::StrokeKind::Outside);

            if painting {
                self.mask_painting(ui, &response, img_rect, area);
            }
            if picking {
                self.center_picking(ui, &response, img_rect);
            }
        }
        if !painting {
            self.mask_stroke_active = false;
            self.finish_mask_stroke(ui.ctx());
        }
    }

    /// Zoom (mouse wheel) and pan (middle mouse button, or left button when no tool is active) of the image.
    /// Returns where the image is drawn on the screen.
    fn image_view(&mut self, ui: &Ui, response: &Response, area: Rect, tex_size: Vec2, tool_active: bool) -> Rect {
        // Like before: big images are shrunk to fit, small ones are not enlarged
        let fit = (area.width() / tex_size.x).min(area.height() / tex_size.y).min(1.0);
        let image_rect = |zoom: f32, pan: Vec2| Rect::from_center_size(area.center() + pan, tex_size * fit * zoom);

        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let old_rect = image_rect(self.view_zoom, self.view_pan);
                let new_zoom = (self.view_zoom * (scroll * 0.003).exp()).clamp(1.0, 64.0);
                // Zoom towards the mouse: the point under the mouse stays where it is
                if let Some(mouse) = response.hover_pos() {
                    let relative = (mouse - old_rect.center()) / old_rect.size();
                    let new_size = tex_size * fit * new_zoom;
                    let new_center = mouse - relative * new_size;
                    self.view_pan = new_center - area.center();
                }
                self.view_zoom = new_zoom;
            }
        }
        let comparing = self.compare && !self.show_base_image;
        let panning = response.dragged_by(PointerButton::Middle)
            || (!tool_active && !comparing && response.dragged_by(PointerButton::Primary));
        if panning {
            self.view_pan += response.drag_delta();
        }
        if response.double_clicked() && !tool_active {
            self.view_zoom = 1.0;
        }
        if self.view_zoom <= 1.0 {
            // Fitted image: always centered
            self.view_pan = Vec2::ZERO;
        }
        image_rect(self.view_zoom, self.view_pan)
    }

    /// Shows the original image left of a divider. Dragging on the image moves the divider.
    fn draw_comparison(&mut self, ui: &Ui, response: &Response, area: Rect, img_rect: Rect, tool_active: bool) {
        let Some(ls) = &self.layered_sorter else { return };
        let base = ls.get_base_img();
        let size = [base.width() as usize, base.height() as usize];
        if self.original_texture.as_ref().map(|t| t.size() != size).unwrap_or(true) {
            let image = egui::ColorImage::from_rgb(size, base.as_raw());
            self.original_texture = Some(ui.ctx().load_texture("original-image", image, mask_texture_options()));
        }
        if !tool_active && (response.dragged_by(PointerButton::Primary) || response.clicked()) {
            if let Some(p) = response.interact_pointer_pos() {
                self.compare_pos = ((p.x - img_rect.min.x) / img_rect.width()).clamp(0.0, 1.0);
            }
        }
        let split_x = img_rect.min.x + self.compare_pos * img_rect.width();
        let left = Rect::from_min_max(img_rect.min, pos2(split_x, img_rect.max.y)).intersect(area);
        if let Some(tex) = &self.original_texture {
            ui.painter_at(left).image(
                tex.id(),
                img_rect,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        let painter = ui.painter_at(area);
        painter.line_segment(
            [pos2(split_x, img_rect.min.y), pos2(split_x, img_rect.max.y)],
            Stroke::new(2.0_f32, Color32::WHITE),
        );
        let label_y = img_rect.min.y.max(area.min.y) + 6.0;
        let font = FontId::proportional(13.0);
        painter.text(pos2(split_x - 6.0, label_y), Align2::RIGHT_TOP, "Original", font.clone(), Color32::WHITE);
        painter.text(pos2(split_x + 6.0, label_y), Align2::LEFT_TOP, "Sorted", font, Color32::WHITE);
    }

    /// Paints into the mask of the current layer with the mouse and draws the mask overlay and the brush
    fn mask_painting(&mut self, ui: &mut Ui, response: &Response, rect: Rect, area: Rect) {
        let Some(ls) = &mut self.layered_sorter else { return };
        let (w, h) = ls.get_base_img().dimensions();
        if w == 0 || h == 0 || rect.width() <= 0.0 || rect.height() <= 0.0 {
            return;
        }
        // Screen pixels per image pixel
        let (scale_x, scale_y) = (rect.width() / w as f32, rect.height() / h as f32);
        let to_img = |p: Pos2| ((p.x - rect.min.x) / scale_x, (p.y - rect.min.y) / scale_y);

        let (any_pressed, primary_down, secondary_down, pointer_pos, hover_pos) = ui.input(|i| {
            (
                i.pointer.any_pressed(),
                i.pointer.primary_down(),
                i.pointer.secondary_down(),
                i.pointer.interact_pos(),
                i.pointer.hover_pos(),
            )
        });

        // Start a stroke when a mouse button is pressed on the image, end it when all buttons are released
        // (the middle button moves the view)
        if any_pressed && response.hovered() && (primary_down || secondary_down) {
            self.mask_stroke_active = true;
            self.last_brush_pos = None;
        }
        if !(primary_down || secondary_down) {
            self.mask_stroke_active = false;
        }

        let mut changed_region = None;
        if self.mask_stroke_active {
            if let Some(pos) = pointer_pos {
                // The right mouse button does the opposite of the selected tool
                let erase = self.brush_erase != secondary_down;
                let current = to_img(pos);
                match self.brush_tool {
                    BrushTool::Mask => {
                        let had_mask = ls.get_current_paint_mask().is_some();
                        let from = self.last_brush_pos.unwrap_or(current);
                        let mask = ls.get_current_paint_mask_mut();
                        changed_region =
                            paint_mask_stroke(mask, from, current, self.brush_radius, erase, self.brush_hardness);
                        if !had_mask {
                            // The mask was just created, the overlay has to be built from scratch
                            self.mask_texture_dirty = true;
                        }
                        self.last_brush_pos = Some(current);
                        self.mask_stroke_changed = true;
                    }
                    BrushTool::Direction => {
                        match self.last_brush_pos {
                            // The direction is only known after the mouse moved
                            None => self.last_brush_pos = Some(current),
                            Some(from) => {
                                let had_directions = ls.has_current_directions();
                                let directions = ls.get_current_directions_mut();
                                changed_region =
                                    paint_direction_stroke(directions, w, h, from, current, self.brush_radius, erase);
                                if changed_region.is_some() {
                                    self.last_brush_pos = Some(current);
                                    self.mask_stroke_changed = true;
                                }
                                if !had_directions {
                                    self.mask_texture_dirty = true;
                                }
                                // Painting directions only makes sense with the path that uses them
                                if self.values.path != PathCreator::Drawn {
                                    self.values.path = PathCreator::Drawn;
                                    ls.update_current(self.values);
                                }
                            }
                        }
                    }
                }
            }
        } else {
            self.last_brush_pos = None;
        }

        // Keep the overlay texture up to date (it shows the mask or the directions, depending on the tool)
        if self.overlay_tool != self.brush_tool {
            self.overlay_tool = self.brush_tool;
            self.mask_texture_dirty = true;
        }
        let ls = self.layered_sorter.as_ref().unwrap();
        let overlay = match self.brush_tool {
            BrushTool::Mask => ls.get_current_paint_mask().map(OverlaySource::Mask),
            BrushTool::Direction => ls.get_current_directions().map(OverlaySource::Directions),
        };
        match overlay {
            None => self.mask_texture = None,
            Some(source) => {
                let overlay = |region: [u32; 4]| match source {
                    OverlaySource::Mask(mask) => mask_overlay_image(mask, region),
                    OverlaySource::Directions(dirs) => direction_overlay_image(dirs, w, region),
                };
                let size_matches = self
                    .mask_texture
                    .as_ref()
                    .map(|t| t.size() == [w as usize, h as usize])
                    .unwrap_or(false);
                if self.mask_texture_dirty || !size_matches {
                    let full = overlay([0, 0, w, h]);
                    self.mask_texture = Some(ui.ctx().load_texture("mask-overlay", full, mask_texture_options()));
                } else if let (Some(region), Some(tex)) = (changed_region, &mut self.mask_texture) {
                    let part = overlay(region);
                    tex.set_partial([region[0] as usize, region[1] as usize], part, mask_texture_options());
                }
                self.mask_texture_dirty = false;
            }
        }

        // Draw the overlay
        if self.show_mask_overlay {
            if let Some(tex) = &self.mask_texture {
                ui.painter_at(area).image(
                    tex.id(),
                    rect,
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
        }

        // Draw the brush
        if let Some(p) = hover_pos {
            if rect.contains(p) || self.mask_stroke_active {
                let r = (self.brush_radius * scale_x).max(1.0);
                ui.painter().circle_stroke(p, r, Stroke::new(2.0_f32, Color32::BLACK));
                ui.painter().circle_stroke(p, r, Stroke::new(1.0_f32, Color32::WHITE));
            }
        }

        if self.mask_stroke_active {
            ui.ctx().request_repaint();
        } else {
            self.finish_mask_stroke(ui.ctx());
        }
    }

    /// Sets the center of rays, circles and spirals with a click on the image
    fn center_picking(&mut self, ui: &mut Ui, response: &Response, rect: Rect) {
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            return;
        }
        let painter = ui.painter_at(response.rect);
        // Crosshair at the mouse
        if let Some(p) = ui.input(|i| i.pointer.hover_pos()) {
            if rect.contains(p) {
                let stroke = Stroke::new(1.0_f32, Color32::YELLOW);
                painter.line_segment([pos2(rect.min.x, p.y), pos2(rect.max.x, p.y)], stroke);
                painter.line_segment([pos2(p.x, rect.min.y), pos2(p.x, rect.max.y)], stroke);
            }
        }
        // The current center
        let (cx, cy) = self.values.path_center;
        let current = pos2(rect.min.x + cx * rect.width(), rect.min.y + cy * rect.height());
        painter.circle_stroke(current, 6.0, Stroke::new(2.0_f32, Color32::YELLOW));

        if response.clicked() {
            if let Some(p) = response.interact_pointer_pos() {
                self.values.path_center = (
                    ((p.x - rect.min.x) / rect.width()).clamp(0.0, 1.0),
                    ((p.y - rect.min.y) / rect.height()).clamp(0.0, 1.0),
                );
                // The values are read back from the layer every frame, so write them there
                if let Some(ls) = &mut self.layered_sorter {
                    ls.update_current(self.values);
                }
                self.picking_center = false;
                if self.auto_sort {
                    self.do_sort = true;
                }
                ui.ctx().request_repaint();
            }
        }
    }

    /// Loads a guide image for the current layer and switches the criteria to "Guide"
    fn load_guide_image(&mut self) {
        let Some(ls) = &mut self.layered_sorter else { return };
        let Some(file) = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
            .pick_file()
        else {
            return;
        };
        match image::open(&file) {
            Ok(guide) => {
                let (w, h) = ls.image_size();
                let guide = guide
                    .resize_exact(w, h, image::imageops::FilterType::Triangle)
                    .into_luma8();
                ls.set_current_guide(Some(guide));
                self.values.criteria = SortingCriteria::Guide;
                // The values are read back from the layer every frame, so write them there
                ls.update_current(self.values);
                if self.auto_sort {
                    self.do_sort = true;
                }
            }
            Err(e) => warn!("Could not open guide image '{}': {}", file.to_string_lossy(), e),
        }
    }

    /// Removes the guide image of the current layer
    fn remove_guide_image(&mut self) {
        if let Some(ls) = &mut self.layered_sorter {
            ls.set_current_guide(None);
            if self.auto_sort {
                self.do_sort = true;
            }
        }
    }

    /// Sorts again after a finished brush stroke
    fn finish_mask_stroke(&mut self, ctx: &egui::Context) {
        if self.mask_stroke_changed {
            self.mask_stroke_changed = false;
            if let Some(ls) = &mut self.layered_sorter {
                ls.paint_mask_changed();
            }
            if self.auto_sort {
                self.do_sort = true;
            }
            ctx.request_repaint();
        }
    }

    /// Replaces the mask of the current layer (None = the whole image is sorted)
    fn replace_paint_mask(&mut self, mask: Option<GrayImage>) {
        if let Some(ls) = &mut self.layered_sorter {
            ls.set_current_paint_mask(mask);
            self.mask_texture_dirty = true;
            if self.auto_sort {
                self.do_sort = true;
            }
        }
    }

    fn open_file_dialog(&mut self, ctx: &egui::Context) -> () {
        // Opening image until cancled or until valid image loaded
        loop {
            #[cfg(not(feature = "video"))]
            let mut filedialog = rfd::FileDialog::new()
                .add_filter("Images", &["png", "jpg", "jpeg", "webp"]);
            #[cfg(feature = "video")]
            let mut filedialog = rfd::FileDialog::new()
                .add_filter("Images and Videos", &["png", "jpg", "jpeg", "webp", "mp4", "mov", "webm", "avi", "mkv", "m4v", "mpg"])
                .add_filter("Videos", &["mp4", "mov", "webm", "avi", "mkv", "m4v", "mpg"]);
            let file = filedialog.pick_file();
            match file {
                None => break,
                Some(f) => {
                    if self.open_path(ctx, f) {
                        break;
                    }
                }
            }
        }
    }

    /// Opens an image (or a video) from a path. Returns false if it couldn't be opened.
    fn open_path(&mut self, ctx: &egui::Context, f: PathBuf) -> bool {
        match image::open(f.as_path()) {
            Ok(i) => {
                let img = i.into_rgb8();
                if let Some(ls) = &mut self.layered_sorter {
                    ls.set_base_img(img.clone());
                }
                self.mask_texture_dirty = true;
                self.original_texture = None;
                // Results of a running background sort belong to the old image
                self.sort_generation += 1;
                self.background_pending = None;
                // A new image: the history and the view start fresh
                self.undo_stack.clear();
                self.redo_stack.clear();
                self.undo_current = None;
                self.view_zoom = 1.0;
                self.view_pan = Vec2::ZERO;
                // I want to make layered_sorter mandatory and remove the possibility of it being None
                #[cfg(feature = "video")] {
                    self.video_player = None;
                }
                self.img = Some(img);
                self.update_texture(ctx);
                self.path = Some(f);
                self.do_sort = true;
                true
            }
            Err(e) => {
                #[cfg(feature = "video")]
                if self.init_video(ctx, &f).is_ok() {
                    // TODO: Either use layered sorter or save it for the next loaded image
                    self.layered_sorter = None;
                    self.img = None;
                    self.path = Some(f);
                    return true;
                }
                warn!("Could not open '{}': {}", f.to_string_lossy(), e);
                false
            }
        }
    }

    /// Handles files dropped onto the window and shows a hint while a file is dragged over it
    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let (dropped, hovering) = ctx.input(|i| {
            (
                i.raw.dropped_files.iter().find_map(|f| f.path.clone()),
                !i.raw.hovered_files.is_empty(),
            )
        });
        if let Some(path) = dropped {
            self.open_path(ctx, path);
        }
        if hovering {
            let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::new("file-drop-overlay")));
            let screen = ctx.screen_rect();
            painter.rect_filled(screen, 0.0, Color32::from_black_alpha(170));
            painter.text(
                screen.center(),
                Align2::CENTER_CENTER,
                "Drop the image to open it",
                FontId::proportional(28.0),
                Color32::WHITE,
            );
        }
    }

    /// Adds an undo entry when the layers changed (not while the mouse is held, so a slider drag is one entry)
    fn track_undo(&mut self, ctx: &egui::Context) {
        const MAX_UNDO: usize = 40;
        let Some(ls) = &self.layered_sorter else { return };
        if self.mask_stroke_active || ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        match &self.undo_current {
            None => self.undo_current = Some(ls.snapshot(None)),
            Some(current) => {
                if !ls.matches_snapshot(current) {
                    let new = ls.snapshot(Some(current));
                    let old = self.undo_current.replace(new).unwrap();
                    self.undo_stack.push(old);
                    if self.undo_stack.len() > MAX_UNDO {
                        self.undo_stack.remove(0);
                    }
                    self.redo_stack.clear();
                }
            }
        }
    }

    fn undo(&mut self) {
        if let Some(previous) = self.undo_stack.pop() {
            if let Some(current) = self.undo_current.take() {
                self.redo_stack.push(current);
            }
            self.restore_snapshot(previous);
        }
    }

    fn redo(&mut self) {
        if let Some(next) = self.redo_stack.pop() {
            if let Some(current) = self.undo_current.take() {
                self.undo_stack.push(current);
            }
            self.restore_snapshot(next);
        }
    }

    fn restore_snapshot(&mut self, snapshot: StackSnapshot) {
        if let Some(ls) = &mut self.layered_sorter {
            ls.restore(&snapshot);
            self.undo_current = Some(snapshot);
            self.values = ls.get_current_layer().get_sorting_values().clone();
            // Results of running sorts belong to the old state
            self.sort_generation += 1;
            self.background_pending = None;
            self.mask_texture_dirty = true;
            self.show_base_image = false;
            self.do_sort = true;
        }
    }

    /// Sets new values for the current layer from outside the options panel (presets, random)
    fn set_values(&mut self, values: PixelsorterValues) {
        self.values = values;
        if let Some(ls) = &mut self.layered_sorter {
            // The values are read back from the layer every frame, so write them there
            ls.update_current(values);
        }
        if self.auto_sort {
            self.do_sort = true;
        }
    }

    /// Sorts and saves the image to the current output directory with a given filename
    fn save_file_to_out_dir(&mut self) -> () {
        if let Some(path) = &self.path {
            let mut ps = Pixelsorter::new();
            ps.path_creator = self.values.path;
            ps.sorter.criteria = self.values.criteria;
            ps.sorter.algorithm = self.values.algorithm;
            ps.reverse = self.values.reverse;
            ps.selector = self.values.selector;

            let (basename, ext) = (
                path.file_stem()
                    .map(|s| s.to_os_string())
                    .unwrap_or(OsString::from("img")),
                path.extension()
                    .map(|s| s.to_os_string())
                    .unwrap_or(OsString::from("png")),
            );
            let filename = format!(
                "{}_sorted_[{}].{}",
                basename.to_string_lossy(),
                ps.to_compact_string(),
                ext.to_string_lossy()
            );

            let outpath = if self.save_into_parent_dir {
                Some(path.clone())
            } else {
                self.output_directory.clone()
            };

            if let Some(mut outpath) = outpath {
                if outpath.is_dir() {
                    outpath.push(filename);
                } else {
                    outpath.set_file_name(filename);
                }
                if let Some(sorted) = &self.image_for_saving() {
                    if let Ok(p) = save_image(sorted, &outpath) {
                        info!("Saved file to '{}' ...", p.to_string_lossy());
                        self.saving_success_timeout = Some(Instant::now());
                    }
                }
            } else {
                warn!("No output directory set");
            }
        }
    }

    /// Sorts and saves the current image to a location via a file picker
    fn save_file_as(&mut self) {
        let path = &self.path.clone();
        if let Some(img) = &self.image_for_saving() {
            if save_image_as(img, path.as_deref()).is_ok(){
                self.saving_success_timeout = Some(Instant::now());
            };
        }
    }

    #[cfg(feature = "video")]
    fn init_video(&mut self, ctx: &egui::Context, video_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        /* called once (creating a player) */

        let mut player = egui_video::Player::new(ctx, &video_path.to_string_lossy().to_string())
            .and_then(|player|{
                if let Some(audio) = &mut self.audio_device {
                    player.with_audio(audio)
                } else {
                    Ok(player)
                }
            });
        self.video_player = Some(player?);
        if let Some(player) = &mut self.video_player {
            player.options.looping = false;
            self.do_sort = true;
        }
        Ok(())
    }

    #[cfg(feature = "video")]
    fn video_mode(&self) -> bool {
        self.img.is_none() && self.video_player.is_some()
    }

}

impl eframe::App for PixelsorterGui {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let mut do_open_file = false;
        let mut do_undo = false;
        let mut do_redo = false;
        // Shortcuts
        ctx.input_mut(|i| {
            if i.consume_key(Modifiers::CTRL.plus(Modifiers::SHIFT), Key::S) {
                if let Some(img) = &self.img {
                    self.save_file_as();
                }
            }
            if i.consume_key(Modifiers::CTRL, Key::S) {
                self.save_file_to_out_dir();
            }
            if i.consume_key(Modifiers::CTRL, Key::O) {
                do_open_file = true;
            }
            if i.consume_key(Modifiers::NONE, Key::B) {
                self.mask_edit = !self.mask_edit;
            }
            if i.consume_key(Modifiers::CTRL.plus(Modifiers::SHIFT), Key::Z) || i.consume_key(Modifiers::CTRL, Key::Y) {
                do_redo = true;
            }
            if i.consume_key(Modifiers::CTRL, Key::Z) {
                do_undo = true;
            }
            if i.consume_key(Modifiers::NONE, Key::F) {
                // Fit the image into the window again
                self.view_zoom = 1.0;
                self.view_pan = Vec2::ZERO;
            }
            if i.consume_key(Modifiers::NONE, Key::M) {
                self.show_mask = !self.show_mask;
                self.do_sort = true;
            }
            if i.consume_key(Modifiers::NONE, Key::Questionmark) {
                self.layered_sorter.as_ref().unwrap().print_state();
            }
        });
        // Set default styles
        ctx.style_mut(|style| {
            style.spacing.slider_width = 170.0;
            style.spacing.combo_height = 300.0;
            // style.debug.debug_on_hover_with_all_modifiers = true;
        });

        // Open file on startup
        if do_open_file {
            self.open_file_dialog(ctx);
        }
        self.handle_dropped_files(ctx);
        if do_undo {
            self.undo();
        }
        if do_redo {
            self.redo();
        }

        // Load video if no image is set
        #[cfg(feature = "video")]
        if self.do_open_video {
            if let Some(video_path) = &self.path.clone() {
                self.init_video(ctx, video_path);
            }
            self.do_open_video = false;
        }
        if let Some(ls) = &self.layered_sorter {
            // Load current values
            self.values = ls.get_current_layer().get_sorting_values().clone();
        } else {
            // Create a layering thingy if we don't have one yet
            if let Some(img) = &self.img {
                self.layered_sorter = Some(LayeredSorter::new(img.clone(), self.values));
            }
        }

        // Pick up results of the background sort
        self.poll_background_sort(ctx);
        // After a preview: sort the full image once the mouse is released
        if self.preview_shown && !ctx.input(|i| i.pointer.any_down()) {
            self.preview_shown = false;
            if self.layered_sorter.is_some() {
                self.request_background_sort(ctx, false);
            }
        }

        // UI //

        #[cfg(feature = "video")]
        // Show modal if a video is currently being exported
        if let Some(phone) = &mut self.video_thread_phone {
            if phone.join_handle.is_finished() {
                self.video_thread_phone = None;
            } else {
                ctx.request_repaint();
                Modal::new("VideoInProgressModal".into())
                    .show(ctx, |ui|{
                        // Receive all progress updates and save the most recent one
                        while let Ok(prog) = phone.progress_receiver.try_recv() {
                            phone.last_progress = prog;
                        };
                        ui.style_mut().spacing.window_margin *= 2.0;
                        ui.heading("Sorting in progress...");
                        ui.vertical_centered(|ui| {
                            ui.set_width(300.0);
                            ui.separator();
                            ui.vertical(|ui|{
                                ui.label(format!("Frames sorted:\t {:<5}",  phone.last_progress.current_frame));
                                ui.label(format!("Time elapsed:\t {: <20?}", phone.last_progress.elapsed_time));
                            });
                            ui.add_space(5.0);
                            let cancel_button = Button::new(RichText::new("Stop").heading())
                                .selected(phone.cancel_signal.load(Ordering::Relaxed));
                            if ui.add(cancel_button).clicked() {
                                // Send exit "signal" to thread
                                phone.cancel_signal.store(true, Ordering::Relaxed);
                            }
                            ui.add_space(5.0);
                            ui.label(RichText::new("The destination file will not be deleted").small());
                        });
                    });
            }
        }

        egui::TopBottomPanel::bottom("info-bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                let tls = self.time_last_sort.lock().unwrap();
                ui.label(format!("Time of last sort:\t{:?}", tls));
                ui.label(format!(
                    "({:.3} fps)",
                    (1.0 / tls.as_secs_f32())
                ));
                if self.background_job.is_some() {
                    ui.separator();
                    ui.spinner();
                    ui.label("Sorting...");
                }
                ui.separator();
                if let Some(inst) = self.saving_success_timeout {
                    if inst.elapsed() > Duration::from_secs_f32(SAVED_LABEL_VANISH_TIMEOUT) {
                        self.saving_success_timeout = None;
                    } else {
                        ui.label(RichText::new("Saved!").color(Color32::DARK_GREEN));
                        ui.separator();
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Max), |ui| {
                    ui.horizontal(|ui|{
                        ui.spacing_mut().item_spacing.x = 0.0;
                        ui.label(format!(
                            " v{} by {}",
                            VERSION,
                            AUTHORS
                        ));
                        ui.label(" ");
                        ui.hyperlink_to(to_title_case(PACKAGE_NAME), "https://github.com/Lxtharia/pixelsortery");
                    });
                    ui.separator();
                    if let Some(tex) = &self.texture {
                        let [w, h] = tex.size();
                        ui.label(format!("{} x {} ({} pixels)", w, h, w * h));
                        ui.separator();
                    }
                    #[cfg(feature = "video")]
                    if let Some(player) = &self.video_player {
                        let (fps, (w, h)) = (player.framerate, player.size.into());
                        ui.label(format!("{fps} fps | {} x {} ({} pixels)", w, h, w * h));
                        ui.separator();
                    }
                });
            });
        });

        let prev_values = (self.values.clone(), self.show_mask);
        egui::SidePanel::left("my-left-pane")
            .resizable(false)
            // A fixed width: egui would otherwise remember the narrow default width (200 px)
            .exact_width(380.0)
            .show(ctx, |ui| {
                ui.add_space(5.0);
                // Limit the width: content that is too wide must not push the image to the right
                let panel_width = ui.available_width();
                // Scrolling in both directions: only then egui limits the width of the scroll area
                // (a vertical-only scroll area always grows with its content)
                ScrollArea::both()
                    .max_height(f32::INFINITY)
                    .max_width(panel_width)
                    .show(ui, |ui| {
                        ui.group(|ui| {
                            ui.set_width(full_width(&ui));
                            if ui.button("Open image...").clicked() {
                                self.open_file_dialog(ctx);
                            }

                            if let Some(p) = &self.path {
                                ui.label(RichText::new(p.to_string_lossy()));
                            } else {
                                ui.label(RichText::new("No image loaded... (or drop one into the window)").italics());
                            }
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(!self.undo_stack.is_empty(), Button::new("Undo"))
                                    .on_hover_text("Ctrl+Z")
                                    .clicked()
                                {
                                    self.undo();
                                }
                                if ui
                                    .add_enabled(!self.redo_stack.is_empty(), Button::new("Redo"))
                                    .on_hover_text("Ctrl+Y / Ctrl+Shift+Z")
                                    .clicked()
                                {
                                    self.redo();
                                }
                                ui.separator();
                                ui.checkbox(&mut self.compare, "Compare")
                                    .on_hover_text("Original left, sorted right. Drag on the image to move the divider");
                                if ui
                                    .button("Fit")
                                    .on_hover_text("Fit the image into the window [F]. Zoom: mouse wheel, move: middle mouse button")
                                    .clicked()
                                {
                                    self.view_zoom = 1.0;
                                    self.view_pan = Vec2::ZERO;
                                }
                                ui.label(RichText::new(format!("{:.0}%", self.view_zoom * 100.0)).small());
                            });
                        });

                        ui.add_space(5.0);

                        // SLIDERS AND BUTTONS AND STUFF
                        ui.group(|ui| {
                            self.sorting_options_panel(ui, 1);
                        });

                        ui.add_space(5.0);
                        ui.group(|ui| {
                            self.presets_panel(ui);
                        });

                        ui.add_space(5.0);

                        // SORT IMAGE button
                        ui.columns(3, |columns| {
                            let ui = &mut columns[0];
                            ui.with_layout(Layout::top_down(Align::Max), |ui| {
                                ui.checkbox(&mut self.auto_sort, "Auto-sort");
                                ui.checkbox(&mut self.background_sort, "Background")
                                    .on_hover_text("Sort in a background thread: the window stays responsive while sorting");
                                ui.add_enabled(self.background_sort, egui::Checkbox::new(&mut self.fast_preview, "Fast preview"))
                                    .on_hover_text("While a slider is dragged, a smaller copy is sorted (approximate). The full image follows when you let go");
                            });
                            let ui = &mut columns[1];
                            ui.with_layout(Layout::top_down(Align::Center), |ui| {
                                if ui.button(RichText::new("SORT IMAGE").heading()).clicked() {
                                    self.do_sort = true;
                                }
                            });
                            let ui = &mut columns[2];
                            ui.add_enabled_ui(
                                selector_is_threshold(self.values.selector),
                                |ui| {
                                    ui.checkbox(&mut self.show_mask, "Show mask");
                                },
                            );
                        });

                        ui.add_space(5.0);

                        // PAINTED MASK
                        ui.group(|ui| {
                            self.mask_panel(ui);
                        });

                        ui.add_space(5.0);

                        ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                            ui.add_space(5.0);
                            // SAVING OPTIONS
                            ui.add_enabled_ui(self.img.is_some(), |ui| {
                                ui.group(|ui| {
                                    self.save_options_panel(ui);
                                });
                            });
                            ui.add_space(5.0);
                            ui.group(|ui| {
                                // LAYERING
                                ui.style_mut().spacing.scroll.floating = false;
                                ui.add_enabled_ui(self.layered_sorter.is_some(), |ui|{
                                    ScrollArea::vertical()
                                        // .scroll_bar_visibility(ScrollBarVisibility::AlwaysVisible)
                                        .id_salt("LayerScrollArea")
                                        .show(ui, |ui| {
                                            Flex::vertical()
                                                .h_full()
                                                .w_full()
                                                .wrap(false)
                                                .justify(FlexJustify::End)
                                                .align_items(FlexAlign::Stretch)
                                                .show(ui, |flex| {
                                                    // flex.grow();
                                                    //for i in 0..8 {
                                                    //    let b = Button::new(format!("LMAO: {}", i));
                                                    //    flex.add(item().basis(30.0), b);
                                                    //}
                                                    self.layering_panel(flex);
                                                });
                                        });
                                });
                            });
                        });
                    });
            });

        if let Some(ls) = &mut self.layered_sorter {
            // info!("Setting values for current: {}", self.values.to_pixelsorter().to_compact_string());
            // Write any changes back to the layered sorter
            ls.update_current(self.values.clone());
        }
        self.track_undo(ctx);

        // Auto-Sort current image on changes or if image needs sorting
        let values_changed = self.values != prev_values.0 || self.show_mask != prev_values.1;
        if (self.do_sort || (self.auto_sort && values_changed)) {
            self.do_sort = false;
            self.sort_img(&ctx, true);
            #[cfg(feature = "video")]
            // Update the video filter function to use the new values
            if let Some(player) = &mut self.video_player {
                let sorter = self.values.to_pixelsorter();
                let time_last_sort_arc = self.time_last_sort.clone();
                player.video_streamer.lock().filter_video_frame_fn = Some(create_frame_filter(sorter, time_last_sort_arc));
                if player.player_state.get() != PlayerState::Playing {
                    let current_frame = player.video_streamer.lock().current_frame();
                    if let Some(current_frame) = current_frame {
                        player.set_current_frame(current_frame);
                    }
                }
            }
        }

        //
        if let Some(ls) = &mut self.layered_sorter {
            // We are switching layers!!
            // Sort after switching
            if self.change_layer != SwitchLayerMessage::None {
                // Another layer might have another mask
                self.mask_texture_dirty = true;
            }
            if let SwitchLayerMessage::Layer(i) = self.change_layer {
                self.change_layer = SwitchLayerMessage::None;
                ls.select_layer(i);
                self.show_base_image = false;
                self.sort_img(ctx, false);
                ctx.request_repaint();
            } else if SwitchLayerMessage::BaseImage == self.change_layer {
                self.change_layer = SwitchLayerMessage::None;
                self.show_base_image = true;
                self.img = Some(ls.get_base_img().clone());
                self.update_texture(ctx);
            } else if let SwitchLayerMessage::DeleteLayer(i) = self.change_layer {
                self.change_layer = SwitchLayerMessage::None;
                // We are deleting the current selected layer
                if i == ls.get_current_index() && !self.show_base_image {
                    ls.remove_layer(i);
                    // Trigger a select on the (now) current layer (remove_layer may have reselected one)
                    self.change_layer = SwitchLayerMessage::Layer(ls.get_current_index());
                } else {
                    // We are deleting another layer than the current one
                    // Or we are looking at the base image
                    ls.remove_layer(i);
                }
            }
        }
        #[cfg(not(feature = "video"))]
        egui::CentralPanel::default().show(ctx, |ui| {
                self.show_img(ui);
        });
        #[cfg(feature = "video")]
        // Display the image or video!
        egui::CentralPanel::default().show(ctx, |ui| {
            use egui_video;
            if self.video_player.is_some() {
                self.video_panel(ui);
            } else {
                self.show_img(ui);
            }
        });
    }
}

fn set_full_width(ui: &mut Ui) -> () {
    ui.set_width(ui.max_rect().max.x - ui.max_rect().min.x);
}

fn full_width(ui: &Ui) -> f32 {
    ui.max_rect().max.x - ui.max_rect().min.x
}

fn full_height(ui: &Ui) -> f32 {
    ui.max_rect().max.y - ui.max_rect().min.y
}
fn selector_is_threshold(sel: PixelSelector) -> bool {
    matches!(
        sel,
        Threshold {
            min: _,
            max: _,
            criteria: _
        }
    )
}

/// opens a file dialog to let the user choose a destination for a file
/// an optional path can be provided that is used to suggest a new filename
pub fn save_image_as(img: &RgbImage, original_path: Option<&Path>) -> Result<(), image::ImageError> {
        let mut suggested_filename = String::from("");
        if let Some(p) = original_path {
            if let (Some(stem), Some(ext)) = (p.file_stem(), p.extension()) {
                suggested_filename = format!(
                    "{}-sorted.{}",
                    stem.to_string_lossy(),
                    ext.to_string_lossy()
                );
            };
        };
        let file = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
            .set_file_name(suggested_filename)
            .save_file();
        if let Some(f) = file {
            let res = img.save(&f);
            if let Err(err_msg) = &res {
                warn!(
                    "Saving image to {} failed: {}",
                    f.to_string_lossy(),
                    err_msg
                );
            } else {
                info!("Saved file to '{}' ...", f.to_string_lossy());
            };
            res
        } else {
            Ok(())
        }
}


/// Saves image to a path. If the file already exists, increment the name.
/// Returns the path the image finally gets saved to
/// Returns an Error if the path is a directory
pub fn save_image(img: &RgbImage, path: &PathBuf) -> Result<PathBuf, String> {
    if path.is_dir() {
        return Err(String::from("Destination is a directory"));
    }
    let org_stem = path.file_stem().unwrap_or_default().to_os_string();
    let org_ext = path.extension().unwrap_or_default().to_os_string();
    let mut new_path = path.clone();

    let mut counter = 1;
    while let Ok(true) = new_path.try_exists() {
        counter += 1;
        let mut new_name = OsString::new();
        new_name.push(&org_stem);
        new_name.push(format!("-{}.", counter));
        new_name.push(&org_ext);
        new_path.set_file_name(new_name);
    }
    if let Err(e) = img.save(&new_path) {
        warn!(
            "Saving image to '{}' failed: {}",
            new_path.to_string_lossy(),
            e
        );
        return Err(e.to_string());
    }
    Ok(new_path)
}

pub fn create_frame_filter(sorter: Pixelsorter, time_last_sort_arc: Arc<Mutex<Duration>> )
    -> Box<dyn FnMut(&mut ColorImage) + Send>
{
    Box::new(move |frame| {
        let timer = Instant::now();
        let mut img = colimg_to_rgbaimg(frame);
        let (w, h) = (img.width() as u64, img.height() as u64);
        // Sort the RGBA data directly (4 bytes per pixel, alpha stays untouched)
        let raw: &mut [u8] = &mut img;
        sorter.sort_raw(raw, 4, w, h, None);
        let newimg = ColorImage::from_rgba_unmultiplied(frame.size, img.as_raw());
        *frame = newimg;
        *time_last_sort_arc.lock().unwrap() = timer.elapsed();
    })
}

fn colimg_to_rgbimg(cimg: &ColorImage) -> RgbImage  {
    let (w, h) = (cimg.width(), cimg.height());
    let mut buffer = Vec::with_capacity(w*h*3);
    for c in cimg.pixels.iter() {
        buffer.push(c.r());
        buffer.push(c.g());
        buffer.push(c.b());
    }
    RgbImage::from_raw(w as u32, h as u32, buffer).unwrap()
}

fn colimg_to_rgbaimg(cimg: &ColorImage) -> RgbaImage  {
    let (w, h) = (cimg.width(), cimg.height());
    let mut buffer = Vec::with_capacity(w*h*3);
    for c in cimg.pixels.iter() {
        buffer.push(c.r());
        buffer.push(c.g());
        buffer.push(c.b());
        buffer.push(c.a());
    }
    RgbaImage::from_raw(w as u32, h as u32, buffer).unwrap()
}

fn mask_texture_options() -> TextureOptions {
    let mut options = TextureOptions::default();
    options.magnification = TextureFilter::Nearest;
    options.minification = TextureFilter::Linear;
    options
}

/// Creates the overlay image for a region `[x0, y0, x1, y1]` (end exclusive) of the mask
fn mask_overlay_image(mask: &GrayImage, region: [u32; 4]) -> ColorImage {
    let [x0, y0, x1, y1] = region;
    let w = mask.width();
    let raw = mask.as_raw();
    let mut rgba = Vec::with_capacity(((x1 - x0) * (y1 - y0) * 4) as usize);
    for y in y0..y1 {
        let row = (y * w) as usize;
        for x in x0..x1 {
            let v = raw[row + x as usize] as u16;
            let [r, g, b, a] = MASK_OVERLAY_COLOR;
            // Soft mask: the overlay fades out with the mask
            rgba.extend_from_slice(&[r, g, b, (a as u16 * v / 255) as u8]);
        }
    }
    ColorImage::from_rgba_unmultiplied([(x1 - x0) as usize, (y1 - y0) as usize], &rgba)
}

/// Creates the overlay image for a region `[x0, y0, x1, y1]` (end exclusive) of the painted directions.
/// The color shows the direction (like a color wheel), unpainted pixels are transparent.
fn direction_overlay_image(directions: &[f32], w: u32, region: [u32; 4]) -> ColorImage {
    let [x0, y0, x1, y1] = region;
    let mut rgba = Vec::with_capacity(((x1 - x0) * (y1 - y0) * 4) as usize);
    for y in y0..y1 {
        let row = (y * w) as usize;
        for x in x0..x1 {
            let angle = directions[row + x as usize];
            if angle.is_nan() {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                let hue = (angle / std::f32::consts::TAU).rem_euclid(1.0);
                let color: Color32 = egui::epaint::Hsva::new(hue, 0.85, 1.0, 1.0).into();
                rgba.extend_from_slice(&[color.r(), color.g(), color.b(), 120]);
            }
        }
    }
    ColorImage::from_rgba_unmultiplied([(x1 - x0) as usize, (y1 - y0) as usize], &rgba)
}
