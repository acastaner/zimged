//! The status bar (§2): file name, Sibling position, pixel size, zoom, animation state, message.
//!
//! Each field is a label with a fixed accessibility label (the `LABEL_*` constants), so screen
//! readers and the headless tests find it by that name and read its text as the value.

use zimged_codec::Size;

use crate::view::Zoom;

pub const LABEL_FILE_NAME: &str = "File name";
pub const LABEL_PIXEL_SIZE: &str = "Pixel size";
pub const LABEL_ZOOM: &str = "Zoom";
pub const LABEL_ANIMATION: &str = "Animation";

/// The animation state of an animated Image (§3.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnimationState {
    Playing,
    // #38: `Frozen` ("⏸ frozen at frame 1"), once an edit freezes it.
}

impl AnimationState {
    /// The spec's text (§2), which screen readers get.
    fn text(self) -> &'static str {
        match self {
            Self::Playing => "▶ animated",
        }
    }

    /// The text drawn beside the painted symbol: the default fonts have no ▶ or ⏸, and egui
    /// panics on a missing glyph.
    fn drawn_text(self) -> &'static str {
        match self {
            Self::Playing => "animated",
        }
    }
}

/// What the status bar shows this frame. `None` fields are left out.
pub struct StatusBar<'a> {
    pub file_name: Option<&'a str>,
    pub pixel_size: Option<Size>,
    pub zoom: Option<Zoom>,
    pub animation: Option<AnimationState>,
}

pub fn show(ui: &mut egui::Ui, bar: &StatusBar<'_>) {
    egui::Panel::bottom("status_bar").show(ui, |ui| {
        ui.horizontal(|ui| {
            // The same height empty or not, so the canvas (and the display-size decode box taken
            // from it) doesn't change when the first Image appears.
            ui.set_min_height(ui.spacing().interact_size.y);
            ui.spacing_mut().item_spacing.x = 16.0;
            if let Some(name) = bar.file_name {
                field(ui, LABEL_FILE_NAME, name);
            }
            // The Sibling position `n / N` goes here (#15).
            if let Some(size) = bar.pixel_size {
                field(ui, LABEL_PIXEL_SIZE, &size.to_string());
            }
            if let Some(zoom) = bar.zoom {
                field(ui, LABEL_ZOOM, &zoom.label());
            }
            if let Some(animation) = bar.animation {
                animation_field(ui, animation);
            }
            // Then the transient message slot.
        });
    });
}

fn field(ui: &mut egui::Ui, label: &str, text: &str) {
    ui.label(text).accessible_name(label);
}

/// The animation state: a painted symbol, then text. Its accessible value is the spec's text.
fn animation_field(ui: &mut egui::Ui, state: AnimationState) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        let side = ui.text_style_height(&egui::TextStyle::Body) * 0.7;
        let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
        let colour = ui.visuals().text_color();
        match state {
            AnimationState::Playing => {
                let points = vec![rect.left_top(), rect.right_center(), rect.left_bottom()];
                ui.painter()
                    .add(egui::Shape::convex_polygon(points, colour, egui::Stroke::NONE));
            }
        }
        let response = ui.label(state.drawn_text()).accessible_name(LABEL_ANIMATION);
        ui.ctx().accesskit_node_builder(response.id, |node| {
            node.set_value(state.text());
        });
    });
}
