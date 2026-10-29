//! The canvas (§2): the Image, fit to the window.

use zimged_codec::Size;

use crate::app::ShownImage;

/// Where the canvas and the Image ended up this frame, in points.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub area: egui::Rect,
    pub image: Option<egui::Rect>,
    pub pixels_per_point: f32,
}

impl Layout {
    /// The canvas size in physical pixels.
    pub fn area_px(&self) -> Size {
        Size::new(
            (self.area.width() * self.pixels_per_point).round() as u32,
            (self.area.height() * self.pixels_per_point).round() as u32,
        )
    }
}

/// Draw the canvas in the remaining space.
pub fn show(ui: &mut egui::Ui, shown: Option<&ShownImage>, error: Option<&str>) -> Layout {
    let frame = egui::Frame::NONE.fill(ui.visuals().extreme_bg_color);
    egui::CentralPanel::default()
        .frame(frame)
        .show(ui, |ui| {
            let rect = ui.max_rect();
            let ppp = ui.ctx().pixels_per_point();
            let mut image_rect = None;
            if let Some(image) = shown {
                let size = fit(image.original_size, rect.size(), ppp);
                let dest = egui::Rect::from_center_size(rect.center(), size);
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                ui.painter()
                    .image(image.texture.id(), dest, uv, egui::Color32::WHITE);
                image_rect = Some(dest);
            } else if let Some(error) = error {
                ui.centered_and_justified(|ui| ui.label(error));
            }
            Layout {
                area: rect,
                image: image_rect,
                pixels_per_point: ppp,
            }
        })
        .inner
}

/// The on-screen size, in points, of an image of `original` pixels fit into `available` points:
/// the aspect ratio kept, and never above 100% (one image pixel per physical pixel).
fn fit(original: Size, available: egui::Vec2, pixels_per_point: f32) -> egui::Vec2 {
    let original_px = egui::vec2(original.width as f32, original.height as f32);
    let available_px = available * pixels_per_point;
    let scale = (available_px.x / original_px.x)
        .min(available_px.y / original_px.y)
        .min(1.0);
    original_px * scale / pixels_per_point
}
