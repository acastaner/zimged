//! The canvas (§2): the Image, fit or zoomed (§3.1), with the editor's composite and selection box
//! on top. The editor gets the pointer through the closure passed to [`show`].

use zimged_codec::Size;
use zimged_core::{Handle, Point, Rect};

use crate::app::ShownImage;
use crate::tools::select::HANDLE_SIZE;
use crate::view::View;

/// Where the canvas and the Image ended up this frame, in points, and the mapping between screen
/// points and Image pixels.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub area: egui::Rect,
    pub image: Option<egui::Rect>,
    /// The size of the texture the Image was drawn from.
    pub drawn: Option<Size>,
    pub pixels_per_point: f32,
    /// The Image's full-resolution size, when one is shown.
    pub image_size: Option<Size>,
}

/// What the editor wants painted over (or instead of) the Image this frame.
#[derive(Default)]
pub struct EditorPaint {
    /// The Skia composite of the visible region, and where it goes (§7.2).
    pub composite: Option<(egui::TextureId, egui::Rect)>,
    /// The selection's bounds, in points: drawn as a dashed box with 8 handles.
    pub selection: Option<egui::Rect>,
    /// Painted last, on top of everything (the text caret, #28).
    pub overlay: Vec<egui::Shape>,
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

    /// Image pixels per screen point at the current zoom.
    pub fn image_per_point(&self) -> f32 {
        match (self.image, self.image_size) {
            (Some(rect), Some(size)) if rect.width() > 0.0 => size.width as f32 / rect.width(),
            _ => 1.0,
        }
    }

    /// The Image pixel under a screen position.
    pub fn image_point(&self, pos: egui::Pos2) -> Point {
        let Some(rect) = self.image else {
            return Point::default();
        };
        let s = self.image_per_point();
        Point::new((pos.x - rect.min.x) * s, (pos.y - rect.min.y) * s)
    }

    /// Where an Image pixel is on screen.
    pub fn screen_point(&self, p: Point) -> egui::Pos2 {
        let Some(rect) = self.image else {
            return egui::Pos2::ZERO;
        };
        let s = self.image_per_point();
        egui::pos2(rect.min.x + p.x / s, rect.min.y + p.y / s)
    }

    pub fn rect_to_screen(&self, r: Rect) -> egui::Rect {
        egui::Rect::from_two_pos(self.screen_point(r.min), self.screen_point(r.max))
    }

    /// The part of the Image that is on screen: where it is (points), which Image pixels it
    /// shows, and its size in physical pixels. What the Skia composite renders (§7.2).
    pub fn visible(&self) -> Option<(egui::Rect, Rect, Size)> {
        let shown = self.image?.intersect(self.area);
        if shown.width() <= 0.0 || shown.height() <= 0.0 {
            return None;
        }
        let region = Rect::from_corners(self.image_point(shown.min), self.image_point(shown.max));
        let px = to_px(shown.size(), self.pixels_per_point);
        let px = Size::new(px.width.max(1), px.height.max(1));
        Some((shown, region, px))
    }
}

fn to_px(size: egui::Vec2, ppp: f32) -> Size {
    Size::new((size.x * ppp).round() as u32, (size.y * ppp).round() as u32)
}

/// Draw the canvas in the remaining space. Without an Image, `message` (an error or the
/// empty-window hint) is centred on it. `edit` handles the pointer (the canvas response) and says
/// what to paint on top of the Image.
pub fn show(
    ui: &mut egui::Ui,
    shown: Option<&ShownImage>,
    view: &mut View,
    message: Option<&str>,
    edit: impl FnOnce(&mut egui::Ui, &Layout, &egui::Response) -> EditorPaint,
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
                image_size: shown.map(|s| s.original_size),
            };
            let texture = shown.map(|image| {
                let dest = view.image_rect(image.original_size, rect, ppp);
                layout.image = Some(dest);
                image.texture_for(to_px(dest.size(), ppp))
            });
            let response = ui.interact(
                rect,
                ui.scope_id().with("canvas"),
                egui::Sense::click_and_drag(),
            );
            let paint = edit(ui, &layout, &response);

            let painter = ui.painter().with_clip_rect(rect);
            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            if let (Some(texture), Some(dest)) = (texture, layout.image) {
                if let Some((composite, dest)) = paint.composite {
                    painter.image(composite, dest, uv, egui::Color32::WHITE);
                } else {
                    painter.image(texture.id(), dest, uv, egui::Color32::WHITE);
                }
                layout.drawn = Some(Size::new(
                    texture.size()[0] as u32,
                    texture.size()[1] as u32,
                ));
            } else if let Some(message) = message {
                ui.centered_and_justified(|ui| ui.label(message));
            }
            if let Some(selection) = paint.selection {
                paint_selection(&painter, selection);
            }
            painter.extend(paint.overlay);
            layout
        })
        .inner
}

/// The selection box (§2): dashed, with 8 handles.
fn paint_selection(painter: &egui::Painter, r: egui::Rect) {
    let dark = egui::Stroke::new(1.0, egui::Color32::from_rgb(20, 20, 20));
    let light = egui::Stroke::new(1.0, egui::Color32::WHITE);
    let corners = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    painter.add(egui::Shape::line(corners.to_vec(), dark));
    painter.extend(egui::Shape::dashed_line(&corners, light, 5.0, 4.0));
    for handle in Handle::ALL {
        let (u, v) = handle.anchor();
        let center = egui::pos2(r.min.x + u * r.width(), r.min.y + v * r.height());
        let square = egui::Rect::from_center_size(center, egui::vec2(HANDLE_SIZE, HANDLE_SIZE));
        painter.rect(
            square,
            0.0,
            egui::Color32::WHITE,
            dark,
            egui::StrokeKind::Inside,
        );
    }
}
