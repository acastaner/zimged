//! The canvas (§2): the Image, fit or zoomed (§3.1).

use zimged_codec::Size;

use crate::app::ShownImage;
use crate::view::View;

/// Where the canvas and the Image ended up this frame, in points.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub area: egui::Rect,
    pub image: Option<egui::Rect>,
    /// The size of the texture the Image was drawn from.
    pub drawn: Option<Size>,
    pub pixels_per_point: f32,
}

impl Layout {
    /// The canvas size in physical pixels.
    pub fn area_px(&self) -> Size {
        to_px(self.area.size(), self.pixels_per_point)
    }

    /// The Image's on-screen size in physical pixels.
    pub fn image_px(&self) -> Option<Size> {
        Some(to_px(self.image?.size(), self.pixels_per_point))
    }
}

fn to_px(size: egui::Vec2, ppp: f32) -> Size {
    Size::new((size.x * ppp).round() as u32, (size.y * ppp).round() as u32)
}

/// Draw the canvas in the remaining space.
pub fn show(
    ui: &mut egui::Ui,
    shown: Option<&ShownImage>,
    view: &mut View,
    error: Option<&str>,
) -> Layout {
    let frame = egui::Frame::NONE.fill(ui.visuals().extreme_bg_color);
    egui::CentralPanel::default()
        .frame(frame)
        .show(ui, |ui| {
            let rect = ui.max_rect();
            let ppp = ui.ctx().pixels_per_point();
            let mut layout = Layout {
                area: rect,
                image: None,
                drawn: None,
                pixels_per_point: ppp,
            };
            if let Some(image) = shown {
                let dest = view.image_rect(image.original_size, rect, ppp);
                let texture = image.texture_for(to_px(dest.size(), ppp));
                let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
                ui.painter().with_clip_rect(rect).image(
                    texture.id(),
                    dest,
                    uv,
                    egui::Color32::WHITE,
                );
                layout.image = Some(dest);
                layout.drawn = Some(Size::new(
                    texture.size()[0] as u32,
                    texture.size()[1] as u32,
                ));
            } else if let Some(error) = error {
                ui.centered_and_justified(|ui| ui.label(error));
            }
            layout
        })
        .inner
}
