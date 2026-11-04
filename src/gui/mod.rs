#![allow(unused)]
use eframe::egui::{
    self, Align, Button, Color32, Image, Key, Layout, Modifiers, RichText, ScrollArea,
    TextureFilter, TextureHandle, TextureOptions, Ui, Vec2,
};
use egui::{scroll_area::ScrollBarVisibility, style::ScrollStyle, pos2, ColorImage, Hyperlink, Modal, Pos2, Rect, Response, Rgba, Sense, Stroke};
use egui_flex::{item, Flex, FlexAlign, FlexAlignContent, FlexJustify};
#[cfg(feature = "video")]
use egui_video::PlayerState;
use image::{GrayImage, Pixel, Rgb, RgbImage, RgbaImage};
use inflections::case::to_title_case;
use layers::LayeredSorter;
use log::{info, warn};
use pixelsortery::{
    path_creator::PathCreator,
    pixel_selector::{
        PixelSelectCriteria,
        PixelSelector::{self, *},
    },
    span_sorter::{SortingAlgorithm, SortingCriteria},
    Pixelsorter,
};
#[cfg(feature = "video")]
use pixelsortery::{Progress, ThreadPhone};
use std::{
    ffi::OsString, path::{Path, PathBuf}, sync::{atomic::{AtomicBool, Ordering}, mpsc::Receiver, Arc, Mutex}, thread::JoinHandle, time::{Duration, Instant}
};

use crate::{AUTHORS, PACKAGE_NAME, VERSION};
mod layers;

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
    selector_random: PixelSelector,
    selector_fixed: PixelSelector,
    selector_thres: PixelSelector,
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
        ps.reverse = self.reverse;
        ps
    }

    fn read_from_pixelsorter(&mut self, ps: &Pixelsorter) {
        self.path = ps.path_creator;
        self.selector = ps.selector;
        self.criteria = ps.sorter.criteria;
        self.algorithm = ps.sorter.algorithm;
        self.reverse = ps.reverse;
        // Set the saved value, just in case
        if let PathCreator::Diagonally(a) = self.path {
            self.path_diagonally_val = a;
        }
        match self.selector {
            Fixed { len } => self.selector_fixed = Fixed { len },
            Random { max } => self.selector_random = Random { max },
            Threshold { min, max, criteria } => {
                self.selector_thres = Threshold { min, max, criteria }
            }
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
            mask_texture: None,
            mask_texture_dirty: true,
            mask_stroke_active: false,
            mask_stroke_changed: false,
            last_brush_pos: None,
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
        self.values.path = ps.path_creator;
        self.values.criteria = ps.sorter.criteria;
        self.values.algorithm = ps.sorter.algorithm;
        self.values.reverse = ps.reverse;
        self.values.selector = ps.selector;
        self
    }
    fn with_image(mut self, img: RgbImage, image_path: PathBuf) -> Self {
        self.img = Some(img);
        self.path = Some(image_path);
        self
    }

    /// Calls sort_current_layer, sets the image and texture
    fn sort_img(&mut self, ctx: &egui::Context, force: bool) {
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
        let painting = self.mask_edit && self.layered_sorter.is_some();
        if let Some(tex) = &self.texture {
            let (tex_id, tex_size) = (tex.id(), tex.size_vec2());
            let sense = if painting { Sense::click_and_drag() } else { Sense::hover() };
            let response = egui::Frame::group(ui.style_mut())
                .inner_margin(0)
                .show(ui, |ui| {
                    ui.add(Image::new((tex_id, tex_size)).shrink_to_fit().sense(sense))
                })
                .inner;
            if painting {
                self.mask_painting(ui, &response);
            }
        }
        if !painting {
            self.mask_stroke_active = false;
            self.finish_mask_stroke(ui.ctx());
        }
    }

    /// Paints into the mask of the current layer with the mouse and draws the mask overlay and the brush
    fn mask_painting(&mut self, ui: &mut Ui, response: &Response) {
        let Some(ls) = &mut self.layered_sorter else { return };
        let (w, h) = ls.get_base_img().dimensions();
        let rect = response.rect;
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
        if any_pressed && response.hovered() {
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
                let had_mask = ls.get_current_paint_mask().is_some();
                let current = to_img(pos);
                let from = self.last_brush_pos.unwrap_or(current);
                let mask = ls.get_current_paint_mask_mut();
                changed_region = paint_mask_stroke(mask, from, current, self.brush_radius, if erase { 0 } else { 255 });
                if !had_mask {
                    // The mask was just created, the overlay has to be built from scratch
                    self.mask_texture_dirty = true;
                }
                self.last_brush_pos = Some(current);
                self.mask_stroke_changed = true;
            }
        } else {
            self.last_brush_pos = None;
        }

        // Keep the overlay texture up to date
        let mask = self.layered_sorter.as_ref().and_then(|ls| ls.get_current_paint_mask());
        match mask {
            None => self.mask_texture = None,
            Some(mask) => {
                let size_matches = self
                    .mask_texture
                    .as_ref()
                    .map(|t| t.size() == [mask.width() as usize, mask.height() as usize])
                    .unwrap_or(false);
                if self.mask_texture_dirty || !size_matches {
                    let full = mask_overlay_image(mask, [0, 0, mask.width(), mask.height()]);
                    self.mask_texture = Some(ui.ctx().load_texture("mask-overlay", full, mask_texture_options()));
                } else if let (Some(region), Some(tex)) = (changed_region, &mut self.mask_texture) {
                    let part = mask_overlay_image(mask, region);
                    tex.set_partial([region[0] as usize, region[1] as usize], part, mask_texture_options());
                }
                self.mask_texture_dirty = false;
            }
        }

        // Draw the overlay
        if self.show_mask_overlay {
            if let Some(tex) = &self.mask_texture {
                ui.painter().image(
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
                ui.painter().circle_stroke(p, r, Stroke::new(2.0, Color32::BLACK));
                ui.painter().circle_stroke(p, r, Stroke::new(1.0, Color32::WHITE));
            }
        }

        if self.mask_stroke_active {
            ui.ctx().request_repaint();
        } else {
            self.finish_mask_stroke(ui.ctx());
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
                Some(f) => match image::open(f.as_path()) {
                    Ok(i) => {
                        let img = i.into_rgb8();
                        if let Some(ls) = &mut self.layered_sorter {
                            ls.set_base_img(img.clone());
                        }
                        self.mask_texture_dirty = true;
                        // I want to make layered_sorter mandatory and remove the possibility of it being None
                        #[cfg(feature = "video")] {
                            self.video_player = None;
                        }
                        self.img = Some(img);
                        self.update_texture(ctx);
                        self.path = Some(f);
                        break;
                    }
                    Err(_) => {
                        #[cfg(feature = "video")]
                        if self.init_video(ctx, &f).is_ok() {
                            // TODO: Either use layered sorter or save it for the next loaded image
                            self.layered_sorter = None;
                            self.img = None;
                            self.path = Some(f);
                            break;
                        }
                    }
                },
            }
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
                if let Some(sorted) = &self.img {
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
        if let Some(img) = &self.img {
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
            //.exact_width(380.0)
            .max_width(380.0)
            .show(ctx, |ui| {
                ui.add_space(5.0);
                ScrollArea::vertical()
                    .max_height(f32::INFINITY)
                    .max_width(f32::INFINITY)
                    .show(ui, |ui| {
                        ui.group(|ui| {
                            ui.set_width(full_width(&ui));
                            if ui.button("Open image...").clicked() {
                                self.open_file_dialog(ctx);
                            }

                            if let Some(p) = &self.path {
                                ui.label(RichText::new(p.to_string_lossy()));
                            } else {
                                ui.label(RichText::new("No image loaded...").italics());
                            }
                        });

                        ui.add_space(5.0);

                        // SLIDERS AND BUTTONS AND STUFF
                        ui.group(|ui| {
                            self.sorting_options_panel(ui, 1);
                        });

                        ui.add_space(5.0);

                        // SORT IMAGE button
                        ui.columns(3, |columns| {
                            let ui = &mut columns[0];
                            ui.with_layout(Layout::right_to_left(Align::TOP), |ui| {
                                ui.checkbox(&mut self.auto_sort, "Auto-sort");
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
            if raw[row + x as usize] >= 128 {
                rgba.extend_from_slice(&MASK_OVERLAY_COLOR);
            } else {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            }
        }
    }
    ColorImage::from_rgba_unmultiplied([(x1 - x0) as usize, (y1 - y0) as usize], &rgba)
}

/// Paints a line of circles from `from` to `to` (image coordinates) into the mask.
/// Returns the changed region `[x0, y0, x1, y1]` (end exclusive), or None if nothing inside the image was touched.
fn paint_mask_stroke(mask: &mut GrayImage, from: (f32, f32), to: (f32, f32), radius: f32, value: u8) -> Option<[u32; 4]> {
    let (w, h) = (mask.width() as i64, mask.height() as i64);
    let r = radius.max(0.5);
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let distance = (dx * dx + dy * dy).sqrt();
    // Place the circles close enough to get a smooth line
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
                if px * px + py * py <= r * r {
                    raw[(y * w + x) as usize] = value;
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
