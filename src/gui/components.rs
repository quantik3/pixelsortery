use eframe::{
    egui::{
        self, style::HandleShape, vec2, Align, Button, Image, RichText, ScrollArea,
        SelectableLabel, Ui,
    },
    epaint::Hsva,
};
use egui::{Align2, Frame, Label, SliderClamping, Stroke, TextBuffer};
use egui_flex::FlexInstance;
use log::{info, warn};
use pixelsortery::{
    path_creator::PathCreator,
    pixel_selector::{PixelSelectCriteria, PixelSelector},
    span_sorter::{SortingAlgorithm, SortingCriteria},
};

use super::*;

/// Rows of a grid for one condition of the combined selector: criteria, min and max
fn condition_ui(ui: &mut Ui, label: &str, condition: &mut Condition) {
    ui.label(important_text(label));
    ui.horizontal(|ui| {
        for (criteria, name) in [
            (PixelSelectCriteria::Hue, "Hue"),
            (PixelSelectCriteria::Brightness, "Bright"),
            (PixelSelectCriteria::Saturation, "Sat"),
        ] {
            ui.selectable_value(&mut condition.criteria, criteria, name);
        }
    });
    ui.end_row();
    let (cap, suffix) = if condition.criteria == PixelSelectCriteria::Hue { (360, "°") } else { (256, "") };
    ui.label("Min");
    if ui
        .add(egui::Slider::new(&mut condition.min, 0..=cap).suffix(suffix).smart_aim(false))
        .dragged()
    {
        condition.max = condition.max.max(condition.min);
    }
    ui.end_row();
    ui.label("Max");
    if ui
        .add(egui::Slider::new(&mut condition.max, 0..=cap).suffix(suffix).smart_aim(false))
        .dragged()
    {
        condition.min = condition.min.min(condition.max);
    }
    ui.end_row();
}

// A style i want to apply to multiple labels
fn important_text(s: &str) -> RichText {
    //RichText::new(s).underline()
    RichText::new(s).size(14.0)
}

impl PixelsorterGui {
    pub(super) fn path_combo_box(&mut self, ui: &mut Ui, id: u64) {
        let available_paths = vec![
            PathCreator::AllHorizontally,
            PathCreator::AllVertically,
            PathCreator::HorizontalLines,
            PathCreator::VerticalLines,
            PathCreator::Rays,
            PathCreator::Circles,
            PathCreator::Spiral,
            PathCreator::SquareSpiral,
            PathCreator::RectSpiral,
            PathCreator::Diagonally(self.values.path_diagonally_val),
            PathCreator::Hilbert,
            self.values.path_x3.clone(),
            self.values.path_waves.clone(),
            self.values.path_flow.clone(),
            PathCreator::Drawn,
            self.values.path_star.clone(),
        ];
        // Only the name, the numbers are shown on the sliders below (keeps the combo box short)
        let short_name = |p: &PathCreator| p.to_string().split(" (").next().unwrap_or_default().to_string();
        let selected_text = short_name(&self.values.path);

        ui.horizontal(|ui| {
            // Build ComboBox from entries in the path_name_mappings
            egui::ComboBox::from_id_salt(format!("path_combo_{}", id))
                .selected_text(selected_text)
                .show_ui(ui, |ui| {
                    for p in available_paths {
                        ui.selectable_value(&mut self.values.path, p, short_name(&p));
                    }
                });

            // Reverse checkbox
            ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                ui.checkbox(&mut self.values.reverse, "Reverse?");
            });
        });
        ui.end_row();

        // Path specific tweaks for some pathings
        // Nested Grid for sub-options
        let current_path = self.values.path.clone();
        let h2 = self.img.as_ref().and_then(|i| Some(i.height())).unwrap_or(200) as i32 / 2;
        match self.values.path {
            PathCreator::Diagonally(ref mut angle) => {
                ui.label("");
                egui::Grid::new(format!("path_options_grid_{}", id))
                    .num_columns(2)
                    .min_row_height(25.0)
                    .show(ui, |ui| {
                        // Narrower sliders, so the longer labels still fit into the side panel
                        ui.spacing_mut().slider_width = 120.0;
                        ui.label(important_text("Angle"));
                        let slider = egui::Slider::new(angle, 0.0..=360.0)
                            .suffix("°")
                            .clamping(SliderClamping::Edits)
                            .drag_value_speed(0.2)
                            .max_decimals(1)
                            .smart_aim(false);
                        ui.add(slider);
                        ui.end_row();
                        // Save for when we reselect diagonally
                        self.values.path_diagonally_val = angle.clone();
                    });
                ui.end_row();
            },
            PathCreator::X3(ref mut offset_y, ref mut scalar_w, ref mut scalar_l) => {
                ui.label("");
                egui::Grid::new(format!("path_options_grid_{}", id))
                    .num_columns(2)
                    .min_row_height(25.0)
                    .show(ui, |ui| {
                        // Narrower sliders, so the longer labels still fit into the side panel
                        ui.spacing_mut().slider_width = 120.0;
                        let slider_a = egui::Slider::new(offset_y, -h2..=h2)
                            .clamping(SliderClamping::Never)
                            .drag_value_speed(0.2)
                            .max_decimals(1);
                            let slider_w = egui::Slider::new(scalar_w, -500..=500)
                            .clamping(SliderClamping::Never)
                            .drag_value_speed(0.2)
                            .max_decimals(1);
                        let slider_l = egui::Slider::new(scalar_l, 0.0..=20.0)
                            .clamping(SliderClamping::Never)
                            .drag_value_speed(0.2)
                            .max_decimals(1);
                        ui.label(important_text("Y-Offset"));
                        ui.add(slider_a);
                        ui.end_row();
                        ui.label(important_text("Width"));
                        ui.add(slider_w);
                        ui.end_row();
                        ui.label(important_text("Slope"));
                        ui.add(slider_l);
                        ui.end_row();
                        // Save for when we reselect diagonally
                        self.values.path_x3 = current_path;
                    });
                ui.end_row();
            }
            PathCreator::Waves(ref mut amplitude, ref mut wavelength, ref mut horizontal) => {
                ui.label("");
                egui::Grid::new(format!("path_options_grid_{}", id))
                    .num_columns(2)
                    .min_row_height(25.0)
                    .show(ui, |ui| {
                        // Narrower sliders, so the longer labels still fit into the side panel
                        ui.spacing_mut().slider_width = 120.0;
                        ui.label(important_text("Amplitude"));
                        ui.add(
                            egui::Slider::new(amplitude, 0.0..=500.0)
                                .logarithmic(true)
                                // Without this, half of the slider is filled with values below 1 px
                                .smallest_positive(1.0)
                                .clamping(SliderClamping::Never)
                                .max_decimals(1)
                                .suffix(" px"),
                        );
                        ui.end_row();
                        ui.label(important_text("Wavelength"));
                        ui.add(
                            egui::Slider::new(wavelength, 2.0..=3000.0)
                                .logarithmic(true)
                                .clamping(SliderClamping::Never)
                                .max_decimals(1)
                                .suffix(" px"),
                        );
                        ui.end_row();
                        ui.label(important_text("Direction"));
                        ui.horizontal(|ui| {
                            ui.selectable_value(horizontal, false, "Vertical");
                            ui.selectable_value(horizontal, true, "Horizontal");
                        });
                        ui.end_row();
                    });
                ui.end_row();
            }
            PathCreator::FlowField(ref mut scale, ref mut seed) => {
                ui.label("");
                egui::Grid::new(format!("path_options_grid_{}", id))
                    .num_columns(2)
                    .min_row_height(25.0)
                    .show(ui, |ui| {
                        // Narrower sliders, so the longer labels still fit into the side panel
                        ui.spacing_mut().slider_width = 120.0;
                        ui.label(important_text("Swirl size"));
                        ui.add(
                            egui::Slider::new(scale, 5.0..=3000.0)
                                .logarithmic(true)
                                .clamping(SliderClamping::Never)
                                .max_decimals(0)
                                .suffix(" px"),
                        )
                        .on_hover_text("Size of the swirls. Small = chaotic, big = calm flowing lines");
                        ui.end_row();
                        ui.label(important_text("Seed"));
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(seed));
                            if ui.button("New").on_hover_text("Another random flow field").clicked() {
                                *seed = rand::random::<u32>() % 100_000;
                            }
                        });
                        ui.end_row();
                    });
                ui.end_row();
            }
            PathCreator::Star(ref mut points, ref mut inner) => {
                ui.label("");
                egui::Grid::new(format!("path_options_grid_{}", id))
                    .num_columns(2)
                    .min_row_height(25.0)
                    .show(ui, |ui| {
                        // Narrower sliders, so the longer labels still fit into the side panel
                        ui.spacing_mut().slider_width = 120.0;
                        ui.label(important_text("Points"));
                        ui.add(egui::Slider::new(points, 2..=24));
                        ui.end_row();
                        ui.label(important_text("Inner"));
                        ui.add(egui::Slider::new(inner, 0.05..=1.0).fixed_decimals(2))
                            .on_hover_text("Size of the inner corners. Small = spiky star, 1 = flower-like");
                        ui.end_row();
                        ui.label("");
                        if ui
                            .button("Polygon")
                            .on_hover_text("Make the star a regular polygon (triangle, square, pentagon, ...)")
                            .clicked()
                        {
                            *inner = (std::f32::consts::PI / (*points).max(2) as f32).cos();
                        }
                        ui.end_row();
                    });
                ui.end_row();
            }
            _ => {}
        };
        // Remember the settings for when we reselect these paths
        match self.values.path {
            PathCreator::Waves(..) => self.values.path_waves = self.values.path,
            PathCreator::FlowField(..) => self.values.path_flow = self.values.path,
            PathCreator::Star(..) => self.values.path_star = self.values.path,
            _ => {}
        }

        // Center of rays, circles and spirals
        if self.values.path.uses_center() {
            ui.label("");
            egui::Grid::new(format!("path_center_grid_{}", id))
                .num_columns(2)
                .min_row_height(25.0)
                .show(ui, |ui| {
                    // Narrower sliders, so the longer labels still fit into the side panel
                    ui.spacing_mut().slider_width = 120.0;
                    ui.label(important_text("Center X"));
                    ui.add(egui::Slider::new(&mut self.values.path_center.0, 0.0..=1.0).fixed_decimals(2));
                    ui.end_row();
                    ui.label(important_text("Center Y"));
                    ui.add(egui::Slider::new(&mut self.values.path_center.1, 0.0..=1.0).fixed_decimals(2));
                    ui.end_row();
                    ui.label("");
                    ui.horizontal(|ui| {
                        if ui
                            .add(Button::new("Pick on image").selected(self.picking_center))
                            .on_hover_text("Click on the image to set the center")
                            .clicked()
                        {
                            self.picking_center = !self.picking_center;
                        }
                        if ui.button("Reset").clicked() {
                            self.values.path_center = DEFAULT_CENTER;
                        }
                    });
                    ui.end_row();
                });
            ui.end_row();
        }
    }

    pub(super) fn selector_combo_box(&mut self, ui: &mut Ui, id: u64) {
        ui.visuals_mut().weak_text_color();
        // Two fixed rows: all five don't fit into one row of the side panel
        let rows = [
            vec![
                (self.values.selector_fixed, "Fixed"),
                (self.values.selector_random, "Random"),
                (self.values.selector_thres, "Threshold"),
            ],
            vec![(self.values.selector_edges, "Edges"), (self.values.selector_combined, "Combined")],
        ];
        ui.vertical(|ui| {
            for row in rows {
                ui.horizontal(|ui| {
                    for (s, n) in row {
                        ui.selectable_value(&mut self.values.selector, s, n);
                    }
                });
            }
        });
        ui.end_row();

        // Nested Grid for sub-options
        ui.label("");
        egui::Grid::new(format!("selector_options_grid_{}", id))
            .num_columns(2)
            .min_row_height(25.0)
            .show(ui, |ui| {
                match &mut self.values.selector {
                    PixelSelector::Fixed { len } => {
                        ui.label(important_text("Length"));
                        let slider = egui::Slider::new(len, 0..=2000)
                            .logarithmic(true)
                            .clamping(SliderClamping::Never)
                            .drag_value_speed(0.2)
                            .smart_aim(false);
                        ui.add(slider);
                        ui.end_row();
                        // Save selector state
                        self.values.selector_fixed = self.values.selector;
                    }
                    PixelSelector::Random { max } => {
                        ui.label(important_text("Max"));
                        let slider = egui::Slider::new(max, 0..=2000)
                            .logarithmic(true)
                            .clamping(SliderClamping::Never)
                            .drag_value_speed(0.2)
                            .smart_aim(false)
                            .step_by(1.0);
                        ui.add(slider);
                        ui.end_row();
                        // Save selector state
                        self.values.selector_random = self.values.selector;
                    }
                    PixelSelector::Threshold { min, max, criteria } => {
                        ui.label(important_text("Criteria"));
                        ui.horizontal(|ui| {
                            vec![
                                PixelSelectCriteria::Hue,
                                PixelSelectCriteria::Brightness,
                                PixelSelectCriteria::Saturation,
                            ]
                            .into_iter()
                            .for_each(|c| {
                                ui.selectable_value(criteria, c, format!("{:?}", c));
                            });
                        });
                        ui.end_row();

                        let (cap, selector_suffix) = if *criteria == PixelSelectCriteria::Hue {
                            (360, "°")
                        } else {
                            (256, "")
                        };

                        // Get slider colors and image
                        // HSVA::new(hue, saturation, brightness, alpha)
                        let (mincol, maxcol, criteria_image) = match criteria {
                            PixelSelectCriteria::Hue => (
                                Hsva::new(*min as f32 / 360.0, 1.0, 1.0, 1.0).into(),
                                Hsva::new(*max as f32 / 360.0, 1.0, 1.0, 1.0).into(),
                                Image::new(egui::include_image!("../../assets/hue-bar.png")),
                            ),
                            PixelSelectCriteria::Brightness => (
                                Hsva::new(1.0, 0.0, *min as f32 / 256.0, 1.0).into(),
                                Hsva::new(1.0, 0.0, *max as f32 / 256.0, 1.0).into(),
                                Image::new(egui::include_image!("../../assets/brightness-bar.png")),
                            ),
                            PixelSelectCriteria::Saturation => (
                                Hsva::new(1.0, *min as f32 / 256.0, 1.0, 1.0).into(),
                                Hsva::new(1.0, *max as f32 / 256.0, 1.0, 1.0).into(),
                                Image::new(egui::include_image!("../../assets/saturation-bar.png")),
                            ),
                        };

                        ui.label(important_text("Min"));
                        ui.scope(|ui| {
                            ui.style_mut().visuals.selection.bg_fill = mincol;
                            let min_slider = egui::Slider::new(min, 0..=cap)
                                .handle_shape(HandleShape::Rect { aspect_ratio: 0.5 })
                                .trailing_fill(true)
                                .suffix(selector_suffix)
                                .drag_value_speed(0.2)
                                .smart_aim(false);
                            if ui.add(min_slider).dragged() {
                                *max = (*max).clamp(*min, u64::MAX);
                            };
                        });
                        ui.end_row();

                        ui.label("");
                        ui.add(
                            criteria_image
                                .maintain_aspect_ratio(false)
                                .fit_to_exact_size([ui.style().spacing.slider_width, 15.0].into()),
                        );
                        ui.end_row();

                        ui.label(important_text("Max"));
                        ui.scope(|ui| {
                            ui.style_mut().visuals.selection.bg_fill = maxcol;
                            let max_slider = egui::Slider::new(max, 0..=cap)
                                .handle_shape(HandleShape::Rect { aspect_ratio: 0.5 })
                                .trailing_fill(true)
                                .suffix(selector_suffix)
                                .drag_value_speed(0.2)
                                .smart_aim(false);

                            if ui.add(max_slider).dragged() {
                                *min = (*min).clamp(u64::MIN, *max);
                            };
                        });
                        ui.end_row();

                        // Save selector state
                        self.values.selector_thres = self.values.selector;
                    }
                    PixelSelector::Edges { threshold } => {
                        ui.label(important_text("Edge"));
                        let slider = egui::Slider::new(threshold, 0..=765)
                            .clamping(SliderClamping::Never)
                            .drag_value_speed(0.5)
                            .smart_aim(false);
                        ui.add(slider)
                            .on_hover_text("A span ends where neighbouring pixels differ more than this. Lower = shorter spans, objects stay visible");
                        ui.end_row();
                        // Save selector state
                        self.values.selector_edges = self.values.selector;
                    }
                    PixelSelector::Combined { first, second, op } => {
                        ui.spacing_mut().slider_width = 120.0;
                        condition_ui(ui, "First", first);
                        ui.label("");
                        ui.horizontal(|ui| {
                            ui.selectable_value(op, CombineOp::And, "AND")
                                .on_hover_text("Pixels have to match both conditions");
                            ui.selectable_value(op, CombineOp::Or, "OR")
                                .on_hover_text("Pixels have to match at least one condition");
                        });
                        ui.end_row();
                        condition_ui(ui, "Second", second);
                        // Save selector state
                        self.values.selector_combined = self.values.selector;
                    }
                    // We don't expose the Full Selector to the gui, so I don't wanna support it
                    PixelSelector::Full => {
                        self.values.selector = PixelsorterGui::default().values.selector
                    }
                }
            });
        ui.end_row();
    }

    pub(super) fn criteria_combo_box(&mut self, ui: &mut Ui, id: u64) {
        egui::ComboBox::from_id_salt(format!("criteria_combo_{}", id))
            .selected_text(format!("{:?}", self.values.criteria))
            .show_ui(ui, |ui| {
                vec![
                    SortingCriteria::Brightness,
                    SortingCriteria::Lightness,
                    SortingCriteria::Saturation,
                    SortingCriteria::Hue,
                    SortingCriteria::Warmth,
                    SortingCriteria::Red,
                    SortingCriteria::Green,
                    SortingCriteria::Blue,
                    SortingCriteria::Guide,
                ]
                .into_iter()
                .for_each(|c| {
                    ui.selectable_value(&mut self.values.criteria, c, format!("{:?}", c));
                });
            });
    }

    pub(super) fn algorithmn_combo_box(&mut self, ui: &mut Ui, id: u64) {
        egui::ComboBox::from_id_salt(format!("algorithm_combo_{}", id))
            .selected_text(format!("{:?}", self.values.algorithm))
            .show_ui(ui, |ui| {
                vec![
                    SortingAlgorithm::Mapsort,
                    SortingAlgorithm::Shellsort,
                    SortingAlgorithm::Glitchsort,
                    SortingAlgorithm::Partial,
                    SortingAlgorithm::Stretch,
                    SortingAlgorithm::Shift,
                    SortingAlgorithm::DebugColor,
                ]
                .into_iter()
                .for_each(|a| {
                    ui.selectable_value(&mut self.values.algorithm, a, format!("{:?}", a));
                });
            });
    }

    pub(super) fn sorting_options_panel(&mut self, ui: &mut Ui, id: u64) {
        // ui.vertical_centered(|ui| {
        // ui.colored_label(Color32::GOLD, "Sorting Options");
        // });

        egui::Grid::new(format!("sorting_options_grid_{}", id))
            .num_columns(2)
            .spacing([20.0, 4.0])
            .min_row_height(25.0)
            .striped(true)
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Options").size(16.0));
                });
                ui.separator();
                ui.end_row();

                // PATH
                ui.label(important_text("Path"));
                self.path_combo_box(ui, id);

                // SELECTOR
                ui.label(important_text("Selector"));
                self.selector_combo_box(ui, id);

                // SORTER
                // SORTING ALGORITHM
                ui.label(important_text("Algorithm"));
                self.algorithmn_combo_box(ui, id);
                ui.end_row();
                if self.values.algorithm == SortingAlgorithm::Partial {
                    ui.label(important_text("Passes"));
                    ui.add(
                        egui::Slider::new(&mut self.values.passes, 1..=1000)
                            .logarithmic(true)
                            .clamping(SliderClamping::Never),
                    )
                    .on_hover_text("How far pixels move towards their place. Few passes = dripping look");
                    ui.end_row();
                }
                if matches!(self.values.algorithm, SortingAlgorithm::Stretch | SortingAlgorithm::Shift) {
                    ui.label(important_text("Amount"));
                    ui.add(egui::Slider::new(&mut self.values.amount, 0.0..=1.0).fixed_decimals(2))
                        .on_hover_text(if self.values.algorithm == SortingAlgorithm::Stretch {
                            "How much of each span gets overwritten by its stretched beginning"
                        } else {
                            "Maximum shift, relative to the span length"
                        });
                    ui.end_row();
                }
                // SORTING CRITERIA
                ui.label(important_text("Criteria"));
                ui.horizontal(|ui| {
                    ui.label("by");
                    self.criteria_combo_box(ui, id);
                });
                ui.end_row();
                // GUIDE IMAGE
                if self.values.criteria == SortingCriteria::Guide {
                    let has_guide = self
                        .layered_sorter
                        .as_ref()
                        .map(|ls| ls.get_current_guide().is_some())
                        .unwrap_or(false);
                    ui.label(important_text("Guide"));
                    ui.horizontal(|ui| {
                        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                        if ui
                            .add_enabled(self.layered_sorter.is_some(), Button::new("Load..."))
                            .on_hover_text("Pixels are sorted by the brightness of this image at their position")
                            .clicked()
                        {
                            self.load_guide_image();
                        }
                        if ui.add_enabled(has_guide, Button::new("Remove")).clicked() {
                            self.remove_guide_image();
                        }
                        ui.label(
                            RichText::new(if has_guide { "loaded" } else { "none: sorted by brightness" }).small(),
                        );
                    });
                    ui.end_row();
                }
                // CHANNELS
                ui.label(important_text("Channels"));
                ui.horizontal(|ui| {
                    for (c, name) in [
                        (ChannelMode::All, "All"),
                        (ChannelMode::Red, "R"),
                        (ChannelMode::Green, "G"),
                        (ChannelMode::Blue, "B"),
                    ] {
                        ui.selectable_value(&mut self.values.channels, c, name);
                    }
                })
                .response
                .on_hover_text("Move only one color channel, the others stay in place. Try a layer per channel with different paths");
                ui.end_row();
                // POSTERIZE
                ui.label(important_text("Posterize"));
                ui.add(
                    egui::Slider::new(&mut self.values.posterize, 0..=16)
                        .custom_formatter(|v, _| if v < 2.0 { "Off".to_string() } else { format!("{}", v as u32) }),
                )
                .on_hover_text("Reduce the colors of the sorted pixels to this many levels per channel");
                ui.end_row();
                // PATTERN
                ui.label(important_text("Pattern"));
                ui.checkbox(&mut self.values.alternate, "Alternate direction")
                    .on_hover_text("Every second span is sorted the other way round (herringbone)");
                ui.end_row();
                // BLENDING
                ui.label(important_text("Blend"));
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt(format!("blend_combo_{}", id))
                        .selected_text(format!("{:?}", self.values.blend))
                        .width(90.0)
                        .show_ui(ui, |ui| {
                            for mode in BlendMode::ALL {
                                ui.selectable_value(&mut self.values.blend, mode, format!("{:?}", mode));
                            }
                        })
                        .response
                        .on_hover_text("How this layer is combined with the image below it");
                    ui.spacing_mut().slider_width = 100.0;
                    ui.add(
                        egui::Slider::new(&mut self.values.opacity, 0.0..=1.0)
                            .custom_formatter(|v, _| format!("{:.0}%", v * 100.0))
                            .custom_parser(|s| s.trim().trim_end_matches('%').parse::<f64>().ok().map(|v| v / 100.0)),
                    )
                    .on_hover_text("Opacity of this layer");
                });
                ui.end_row();
            });
    }

    /// The settings of the current layer as preset text (including blend mode and opacity)
    fn current_preset_text(&self) -> String {
        format!(
            "{};blend={:?};opacity={}",
            self.values.to_pixelsorter().to_preset_string(),
            self.values.blend,
            self.values.opacity
        )
    }

    /// Applies preset text to the current layer. Returns an error message if it can't be read.
    fn apply_preset_text(&mut self, text: &str) -> Result<(), String> {
        let ps = Pixelsorter::from_preset_string(text)?;
        let mut values = self.values;
        values.read_from_pixelsorter(&ps);
        // The layer settings that only the gui knows
        values.blend = BlendMode::Normal;
        values.opacity = 1.0;
        for part in text.split(';') {
            if let Some((key, value)) = part.split_once('=') {
                match key.trim() {
                    "blend" => {
                        if let Some(mode) = BlendMode::ALL.iter().find(|m| format!("{:?}", m).eq_ignore_ascii_case(value.trim())) {
                            values.blend = *mode;
                        }
                    }
                    "opacity" => values.opacity = value.trim().parse().unwrap_or(1.0),
                    _ => {}
                }
            }
        }
        self.set_values(values);
        Ok(())
    }

    pub(super) fn presets_panel(&mut self, ui: &mut Ui) {
        ui.set_width(full_width(&ui));
        let enabled = self.layered_sorter.is_some();
        egui::CollapsingHeader::new(important_text("Presets"))
            .id_salt("presets_header")
            .default_open(false)
            .show(ui, |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    // Saved presets
                    ui.horizontal(|ui| {
                        let selected_name = self
                            .selected_preset
                            .and_then(|i| self.presets.get(i))
                            .map(|p| p.0.clone())
                            .unwrap_or_else(|| "Choose a preset...".to_string());
                        egui::ComboBox::from_id_salt("preset_combo")
                            .selected_text(selected_name)
                            .width(170.0)
                            .truncate()
                            .show_ui(ui, |ui| {
                                for (i, (name, _)) in self.presets.iter().enumerate() {
                                    ui.selectable_value(&mut self.selected_preset, Some(i), name);
                                }
                            });
                        let selected = self.selected_preset.filter(|&i| i < self.presets.len());
                        if ui.add_enabled(selected.is_some(), Button::new("Apply")).clicked() {
                            if let Some(i) = selected {
                                let text = self.presets[i].1.clone();
                                if let Err(e) = self.apply_preset_text(&text) {
                                    warn!("Invalid preset '{}': {}", self.presets[i].0, e);
                                }
                            }
                        }
                        if ui.add_enabled(selected.is_some(), Button::new("Delete")).clicked() {
                            if let Some(i) = selected {
                                self.presets.remove(i);
                                self.selected_preset = None;
                                presets::save_presets(&self.presets);
                            }
                        }
                    });
                    // Save the current settings
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut self.preset_name).hint_text("Name").desired_width(170.0));
                        let name = presets::clean_name(&self.preset_name);
                        if ui
                            .add_enabled(!name.is_empty(), Button::new("Save"))
                            .on_hover_text("Save the settings of this layer (a preset with the same name is replaced)")
                            .clicked()
                        {
                            let text = self.current_preset_text();
                            match self.presets.iter().position(|p| p.0 == name) {
                                Some(i) => {
                                    self.presets[i].1 = text;
                                    self.selected_preset = Some(i);
                                }
                                None => {
                                    self.presets.push((name, text));
                                    self.selected_preset = Some(self.presets.len() - 1);
                                }
                            }
                            presets::save_presets(&self.presets);
                            self.preset_name.clear();
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui
                            .button("Random")
                            .on_hover_text("Random settings for this layer. Ctrl+Z goes back")
                            .clicked()
                        {
                            let mut ps = self.values.to_pixelsorter();
                            ps.randomize();
                            let mut values = self.values;
                            values.read_from_pixelsorter(&ps);
                            self.set_values(values);
                        }
                        if ui
                            .button("Copy as text")
                            .on_hover_text("Copy the settings, e.g. for the command line: --preset \"...\"")
                            .clicked()
                        {
                            ui.ctx().copy_text(self.current_preset_text());
                        }
                    });
                    // Settings text, e.g. copied from someone else
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.preset_text)
                                .hint_text("Paste settings text here")
                                .desired_width(170.0),
                        );
                        if ui.add_enabled(!self.preset_text.trim().is_empty(), Button::new("Apply")).clicked() {
                            let text = self.preset_text.trim().to_string();
                            match self.apply_preset_text(&text) {
                                Ok(()) => self.preset_text.clear(),
                                Err(e) => warn!("Invalid settings text: {}", e),
                            }
                        }
                    });
                });
            });
    }

    pub(super) fn animation_panel(&mut self, ui: &mut Ui) {
        ui.set_width(full_width(&ui));
        let enabled = self.layered_sorter.is_some() && self.anim_job.is_none();
        let current_layer = self.layered_sorter.as_ref().map(|ls| ls.get_current_index()).unwrap_or(0);
        egui::CollapsingHeader::new(important_text("Animation"))
            .id_salt("animation_header")
            .default_open(false)
            .show(ui, |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    ui.label(
                        RichText::new(format!(
                            "Layer [{}] moves from the start to the end settings. Set up the layer, press 'Set', change it, press 'Set' at the end.",
                            current_layer
                        ))
                        .small(),
                    );
                    egui::Grid::new("animation_grid").num_columns(2).min_row_height(22.0).show(ui, |ui| {
                        ui.spacing_mut().slider_width = 120.0;
                        for (label, is_start) in [("Start", true), ("End", false)] {
                            ui.label(important_text(label));
                            ui.horizontal(|ui| {
                                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                                if ui.button("Set").on_hover_text("Use the current settings of the layer").clicked() {
                                    if is_start {
                                        self.anim_start = Some(self.values);
                                    } else {
                                        self.anim_end = Some(self.values);
                                    }
                                }
                                let values = if is_start { self.anim_start } else { self.anim_end };
                                let text = values
                                    .map(|v| v.to_pixelsorter().to_pretty_short_string())
                                    .unwrap_or_else(|| "not set".to_string());
                                ui.label(RichText::new(text).small().monospace());
                            });
                            ui.end_row();
                        }
                        ui.label(important_text("Frames"));
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut self.anim_frames).range(2..=1000));
                            ui.label("at");
                            ui.add(egui::DragValue::new(&mut self.anim_fps).range(1..=60).suffix(" fps"));
                            ui.checkbox(&mut self.anim_pingpong, "Ping-pong")
                                .on_hover_text("Play forwards and then backwards, so the loop has no jump");
                        });
                        ui.end_row();
                        ui.label(important_text("GIF size"));
                        ui.add(
                            egui::DragValue::new(&mut self.anim_max_size)
                                .range(0..=8000)
                                .custom_formatter(|v, _| if v < 1.0 { "full size".to_string() } else { format!("{} px", v as u32) }),
                        )
                        .on_hover_text("Longer side of the GIF (0 = full size). Big GIFs get huge and slow");
                        ui.end_row();
                        let both_set = self.anim_start.is_some() && self.anim_end.is_some();
                        ui.label(important_text("Try"));
                        let response = ui.add_enabled(both_set, egui::Slider::new(&mut self.anim_t, 0.0..=1.0).fixed_decimals(2));
                        if response.changed() {
                            if let Some(values) = self.animation_values(self.anim_t) {
                                self.set_values(values);
                            }
                        }
                        response.on_hover_text("Shows the layer at this point of the animation (Ctrl+Z goes back)");
                        ui.end_row();
                    });
                    let both_set = self.anim_start.is_some() && self.anim_end.is_some();
                    ui.horizontal(|ui| {
                        if ui.add_enabled(both_set, Button::new("Export GIF...")).clicked() {
                            let suggested = self
                                .path
                                .as_ref()
                                .and_then(|p| p.file_stem())
                                .map(|s| format!("{}-animated.gif", s.to_string_lossy()))
                                .unwrap_or_else(|| "animation.gif".to_string());
                            if let Some(file) = rfd::FileDialog::new().add_filter("GIF", &["gif"]).set_file_name(suggested).save_file() {
                                self.start_animation_export(AnimationTarget::Gif(file));
                            }
                        }
                        if ui
                            .add_enabled(both_set, Button::new("Export PNG frames..."))
                            .on_hover_text("Numbered PNG files in full quality, e.g. to make a video from them")
                            .clicked()
                        {
                            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                                self.start_animation_export(AnimationTarget::Pngs(dir));
                            }
                        }
                    });
                    if let Some(message) = &self.anim_message {
                        ui.label(RichText::new(message).small());
                    }
                });
            });
    }

    /// Shown when the image is a frame of a video: apply the layers to the whole video
    #[cfg(feature = "video")]
    pub(super) fn video_layers_panel(&mut self, ui: &mut Ui) {
        let Some(source) = self.video_source.clone() else { return };
        ui.set_width(full_width(&ui));
        ui.horizontal(|ui| {
            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
            ui.label(important_text("Video"));
            ui.label(
                RichText::new(source.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())
                    .small(),
            );
        });
        ui.label(RichText::new("You are editing one frame. Layers, masks and directions are applied to every frame.").small());
        ui.horizontal(|ui| {
            let can_animate = self.anim_start.is_some() && self.anim_end.is_some();
            ui.add_enabled(can_animate, egui::Checkbox::new(&mut self.video_animate, "Animate"))
                .on_hover_text("The current layer moves from the animation start to the end settings over the video");
            if ui.add_enabled(self.video_thread_phone.is_none(), Button::new("Export video...")).clicked() {
                self.start_video_export_with_layers();
            }
            if ui.button("Back to video").clicked() {
                let ctx = ui.ctx().clone();
                self.open_path(&ctx, source.clone());
            }
        });
    }

    pub(super) fn mask_panel(&mut self, ui: &mut Ui) {
        ui.set_width(full_width(&ui));
        let enabled = self.layered_sorter.is_some();
        let has_mask = self
            .layered_sorter
            .as_ref()
            .map(|ls| ls.get_current_paint_mask().is_some())
            .unwrap_or(false);
        let is_threshold = selector_is_threshold(self.values.selector);
        let has_directions = self
            .layered_sorter
            .as_ref()
            .map(|ls| ls.has_current_directions())
            .unwrap_or(false);
        let mut clear_directions = false;
        let previous_tool = self.brush_tool;
        // What to do with the mask after the buttons were drawn
        let mut new_mask: Option<Option<GrayImage>> = None;
        let mut save_mask = false;
        let mut load_mask = false;
        let mut mask_from_threshold = false;

        ui.add_enabled_ui(enabled, |ui| {
            ui.horizontal(|ui| {
                ui.label(important_text("Brush"));
                ui.checkbox(&mut self.mask_edit, "Draw [B]");
                ui.checkbox(&mut self.show_mask_overlay, "Show overlay");
            });
            ui.horizontal(|ui| {
                ui.label("Paints");
                ui.selectable_value(&mut self.brush_tool, BrushTool::Mask, "Mask")
                    .on_hover_text("Where this layer is applied");
                ui.selectable_value(&mut self.brush_tool, BrushTool::Direction, "Direction")
                    .on_hover_text("Draw lines the sorting should follow (path 'Drawn'). The color shows the direction");
            });
            ui.add_enabled_ui(self.mask_edit, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.brush_erase, false, "Paint");
                    ui.selectable_value(&mut self.brush_erase, true, "Erase");
                    ui.label("Size");
                    ui.spacing_mut().slider_width = 130.0;
                    let slider = egui::Slider::new(&mut self.brush_radius, 1.0..=1000.0)
                        .logarithmic(true)
                        .clamping(SliderClamping::Never)
                        .max_decimals(0)
                        .suffix(" px");
                    ui.add(slider);
                });
                if self.brush_tool == BrushTool::Mask { ui.horizontal(|ui| {
                    ui.label("Hardness");
                    ui.spacing_mut().slider_width = 130.0;
                    ui.add(egui::Slider::new(&mut self.brush_hardness, 0.0..=1.0).fixed_decimals(2))
                        .on_hover_text("1 = hard edge. Lower values give soft edges that blend the sorted layer smoothly into the image");
                }); }
            });
            if self.brush_tool == BrushTool::Direction {
                ui.horizontal(|ui| {
                    if ui.add_enabled(has_directions, Button::new("Clear directions")).clicked() {
                        clear_directions = true;
                    }
                });
                let hint = if has_directions {
                    "Paint strokes along the image: the sorting follows them, between strokes the directions blend. Right click erases."
                } else {
                    "Turn on 'Draw' and paint strokes on the image. The path switches to 'Drawn' automatically."
                };
                ui.label(RichText::new(hint).small());
            } else {
            ui.horizontal(|ui| {
                if let Some(ls) = &self.layered_sorter {
                    if ui.button("Fill").on_hover_text("Sort the whole image (mask everything)").clicked() {
                        new_mask = Some(Some(ls.new_paint_mask(255)));
                    }
                    if ui.button("Empty").on_hover_text("Sort nothing, then paint where to sort").clicked() {
                        new_mask = Some(Some(ls.new_paint_mask(0)));
                    }
                    if ui.add_enabled(has_mask, Button::new("Invert")).clicked() {
                        if let Some(mask) = ls.get_current_paint_mask() {
                            let mut inverted = mask.clone();
                            let raw: &mut [u8] = &mut inverted;
                            raw.iter_mut().for_each(|v| *v = 255 - *v);
                            new_mask = Some(Some(inverted));
                        }
                    }
                    if ui.add_enabled(has_mask, Button::new("Remove")).on_hover_text("Remove the mask: the whole image is sorted").clicked() {
                        new_mask = Some(None);
                    }
                }
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(has_mask, Button::new("Save...")).on_hover_text("Save the mask as a black and white image").clicked() {
                    save_mask = true;
                }
                if ui.button("Load...").on_hover_text("Load a black and white image as mask (white = sorted)").clicked() {
                    load_mask = true;
                }
                if ui
                    .add_enabled(is_threshold, Button::new("From threshold"))
                    .on_hover_text("Create the mask from the threshold selector (what 'Show mask' shows), then paint on it")
                    .clicked()
                {
                    mask_from_threshold = true;
                }
            });
            let hint = if has_mask {
                "Only the red area is sorted. Left click paints, right click erases."
            } else if self.mask_edit {
                "No mask yet: the whole image is sorted. Paint on the image to sort only there."
            } else {
                "No mask: the whole image is sorted."
            };
            ui.label(RichText::new(hint).small());
            }
        });

        if self.brush_tool != previous_tool {
            self.mask_texture_dirty = true;
        }
        if clear_directions {
            if let Some(ls) = &mut self.layered_sorter {
                ls.clear_current_directions();
                self.mask_texture_dirty = true;
                if self.auto_sort {
                    self.do_sort = true;
                }
            }
        }

        if save_mask {
            self.save_paint_mask();
        }
        if load_mask {
            if let Some(mask) = self.load_paint_mask() {
                new_mask = Some(Some(mask));
            }
        }
        if mask_from_threshold {
            if let Some(ls) = &mut self.layered_sorter {
                // White where the threshold selects pixels
                if let Some(threshold_mask) = ls.get_mask_for_current_layer() {
                    let (w, h) = threshold_mask.dimensions();
                    let values: Vec<u8> = threshold_mask.pixels().map(|p| p.0[0]).collect();
                    new_mask = GrayImage::from_raw(w, h, values).map(Some);
                }
            }
        }
        if let Some(mask) = new_mask {
            self.replace_paint_mask(mask);
        }
    }

    /// Saves the mask of the current layer as an image file
    fn save_paint_mask(&self) {
        let Some(mask) = self.layered_sorter.as_ref().and_then(|ls| ls.get_current_paint_mask()) else { return };
        let suggested = self
            .path
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| format!("{}-mask.png", s.to_string_lossy()))
            .unwrap_or_else(|| "mask.png".to_string());
        if let Some(file) = rfd::FileDialog::new().add_filter("PNG", &["png"]).set_file_name(suggested).save_file() {
            match mask.save(&file) {
                Ok(_) => info!("Saved mask to '{}'", file.to_string_lossy()),
                Err(e) => warn!("Saving the mask failed: {}", e),
            }
        }
    }

    /// Lets the user pick an image and returns it as a mask with the size of the current image
    fn load_paint_mask(&self) -> Option<GrayImage> {
        let ls = self.layered_sorter.as_ref()?;
        let file = rfd::FileDialog::new()
            .add_filter("Images", &["png", "jpg", "jpeg", "webp"])
            .pick_file()?;
        match image::open(&file) {
            Ok(img) => {
                let (w, h) = ls.image_size();
                Some(img.resize_exact(w, h, image::imageops::FilterType::Triangle).into_luma8())
            }
            Err(e) => {
                warn!("Could not open mask '{}': {}", file.to_string_lossy(), e);
                None
            }
        }
    }

    pub(super) fn save_options_panel(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.set_width(full_width(&ui));
            // let w = ui.max_rect().max.x - ui.max_rect().min.x;
            // ui.set_width(w);
            if ui.button("Save as...").clicked() {
                info!("Saving image...");
                if let Some(img) = &self.img {
                    self.save_file_as();
                }
            }

            // Enable Save button, if a dir is set, or if we are saving into the parent directory
            ui.add_enabled_ui(
                self.output_directory.is_some() || self.save_into_parent_dir,
                |ui| {
                    if ui.button("Save").clicked() {
                        info!("Saving image...");
                        self.save_file_to_out_dir();
                    }
                },
            );
            if ui.button("Choose destination...").clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    self.output_directory = Some(dir);
                    self.save_into_parent_dir = false;
                }
            }
            ui.checkbox(&mut self.save_into_parent_dir, "Same directory");
        });
        ui.add_space(5.0);
        ui.horizontal_wrapped(|ui| {
            ui.label("Saving into: ");
            let text = if self.save_into_parent_dir {
                let mut parent_dir = self.path.as_ref().unwrap().clone();
                parent_dir.pop();
                RichText::new(parent_dir.to_string_lossy()).monospace()
            } else if let Some(output_dir) = &self.output_directory {
                RichText::new(output_dir.to_string_lossy()).monospace()
            } else {
                RichText::new("No output directory set").italics()
            };
            ui.label(text);
        });
    }

    #[cfg(feature = "video")]
    pub(super) fn video_panel(&mut self, ui: &mut Ui) {
        let mut current_frame = None;
        let mut do_export = false;
        let mut edit_frame_at = None;
        if let Some(player) = &mut self.video_player {
            ui.with_layout(Layout::bottom_up(Align::Center).with_cross_justify(true), |ui|{
                ui.group(|ui| {
                    ui.horizontal(|ui|{
                        if ui.button("Save Frame").clicked() {
                            current_frame = player.video_streamer.lock().current_frame().and_then(|frame|{
                                Some(colimg_to_rgbimg(&frame))
                            });
                        }
                        if ui.button("Start Export").clicked() {
                            do_export = true;
                            player.pause();
                        }
                        if ui
                            .button("Edit frame with layers")
                            .on_hover_text("Edit this frame with layers, masks and the brush, then export the whole video with them")
                            .clicked()
                        {
                            player.pause();
                            edit_frame_at = Some(player.elapsed_ms());
                        }
                        ui.checkbox(&mut player.options.looping, "Loop");

                        if ui.button("Mute/Unmute").clicked() {
                            if player.options.audio_volume.get() != 0. {
                                player.options.audio_volume.set(0.)
                            } else {
                                player.options
                                    .audio_volume
                                    .set(player.options.max_audio_volume / 2.)
                            }
                        }
                        ui.add_space(ui.available_width());
                    });
                });
                ui.with_layout(Layout::top_down(Align::Min), |ui|{
                    let scale_to_fit = (ui.available_width() / player.size.x).min(ui.available_height() / player.size.y) ;
                    player.ui(ui, player.size * scale_to_fit);
                });
            });
        }

        if let Some(frame) = current_frame {
            save_image_as(&frame, self.path.as_deref());
        }
        if let Some(ms) = edit_frame_at {
            let ctx = ui.ctx().clone();
            self.edit_video_frame(&ctx, ms);
            return;
        }
        if do_export {
            if let Some(path) = self.path.clone() {
                // Choose file
                let file = rfd::FileDialog::new()
                    .add_filter("Videos", &["mp4", "mov", "webm", "avi", "mkv", "m4v", "mpg"])
                    .save_file();
                if let Some(output) = file {
                    // Start sorting the video in the background
                    let ps = self.values.to_pixelsorter();
                    self.video_thread_phone = Some(
                        ps.sort_video_threaded(
                            path.to_string_lossy().as_str(),
                            output.to_string_lossy().as_str()
                        )
                    );
                };
            } else {
                panic!("Video opened, but no path set");
            }
        }
    }

    pub(super) fn layering_panel(&mut self, flex: &mut FlexInstance) {
        if let Some(ls) = &mut self.layered_sorter {
            let mut layer_to_select = None;
            let mut layer_to_delete = None;

            let item_frame = Frame::default()
                .inner_margin(0.0)
                .stroke(Stroke::new(1.0_f32, Color32::DARK_GRAY));

            // ADD LAYER (+)
            let button = Button::new(RichText::new("+").heading());
            if flex.add(item().basis(30.0).grow(0.0), button).clicked() {
                ls.add_layer(ls.get_current_layer().get_sorting_values().clone());
                // self.change_layer = SwitchLayerMessage::Layer(ls.get_layers().len() - 1);
                layer_to_select = Some(ls.get_layers().len() - 1);
            }

            let only_one_layer = ls.get_layers().len() == 1;

            for (i, layer) in ls.get_layers().iter().enumerate().rev() {
                let values = layer.get_sorting_values();

                let layer_name = RichText::new(format!(
                    "[{}] {}{}",
                    i,
                    layer
                        .get_sorting_values()
                        .to_pixelsorter()
                        .to_pretty_short_string(),
                    if layer.has_paint_mask() { " [M]" } else { "" }
                ));

                let button = SelectableLabel::new(
                    ls.get_current_index() == i && !self.show_base_image,
                    layer_name.monospace().size(10.5),
                );

                let delete_button = Button::new("X").corner_radius(0.0);

                // Add a layer button
                flex.add_flex(item().basis(30.0).frame(item_frame), Flex::horizontal(), |flex| {
                    flex.add_ui(item(), |ui|{
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                            // Long layer names are cut off with "…" instead of widening the side panel
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);

                            let res_del = ui.add_enabled_ui(!only_one_layer, |ui|{
                                ui.add_sized(vec2(20.0, 30.0), delete_button)
                            }).inner;
                            let res = ui.add_sized(ui.available_size(), button);
                            //let res = ui.add_sized(vec2(ui.available_width(), 30.0), button);

                            // Adding and removing on clicks
                            if res.clicked() {
                                layer_to_select = Some(i);
                            }
                            if res.middle_clicked() || res_del.clicked() {
                                layer_to_delete = Some(i);
                            }
                        });
                    });
                });
            }

            // Show-base-image button
            let base_image_button = SelectableLabel::new(
                self.show_base_image,
                RichText::new("[Original Image]").underline().monospace(),
            );
            flex.add_ui(item().basis(30.0).frame(item_frame), |ui| {
                if ui
                    .add_sized(ui.available_size(), base_image_button)
                    .clicked()
                {
                    self.change_layer = SwitchLayerMessage::BaseImage;
                }
            });

            // The loop told us something has to be selected/deleted
            // We set a flag so that is done at the end of the gui update function
            if let Some(i) = layer_to_select {
                self.change_layer = SwitchLayerMessage::Layer(i);
            }
            if let Some(i) = layer_to_delete {
                self.change_layer = SwitchLayerMessage::DeleteLayer(i);
            }
        } else {
            // When there is no layered sorter, f.e. when sorting videos
            let item_frame = Frame::default()
                .inner_margin(0.0)
                .stroke(Stroke::new(1.0_f32, Color32::DARK_GRAY));

            flex.add_ui(item().basis(30.0).frame(item_frame), |ui| {
                ui.set_min_width(ui.available_width());
                ui.add_sized(ui.available_size(), Label::new("[No Layers]"));
            });
        }
    }
}
