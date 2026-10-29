//! The status bar (§2): file name, Sibling position, pixel size, zoom, animation state, message.
//!
//! Each field is a label with a fixed accessibility label (the `LABEL_*` constants), so screen
//! readers and the headless tests find it by that name and read its text as the value.

use zimged_codec::Size;

use crate::view::Zoom;

pub const LABEL_FILE_NAME: &str = "File name";
pub const LABEL_PIXEL_SIZE: &str = "Pixel size";
pub const LABEL_ZOOM: &str = "Zoom";

/// What the status bar shows this frame. `None` fields are left out.
pub struct StatusBar<'a> {
    pub file_name: Option<&'a str>,
    pub pixel_size: Option<Size>,
    pub zoom: Option<Zoom>,
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
            // Then the animation state and the transient message slot.
        });
    });
}

fn field(ui: &mut egui::Ui, label: &str, text: &str) {
    ui.label(text).accessible_name(label);
}
