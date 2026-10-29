//! The canvas (§2): the Image, fit to the window, with the editor's composite and selection box
//! on top. The editor gets the pointer through the closure passed to [`show`].

use zimged_codec::Size;
use zimged_core::{Handle, Point, Rect};

use crate::app::ShownImage;
use crate::tools::select::HANDLE_SIZE;

/// Where the canvas and the Image ended up this frame, in points, and the mapping between screen
/// points and Image pixels.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub area: egui::Rect,
    pub image: Option<egui::Rect>,
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
}

impl Layout {
    /// The canvas size in physical pixels.
    pub fn area_px(&self) -> Size {
        Size::new(
            (self.area.width() * self.pixels_per_point).round() as u32,
            (self.area.height() * self.pixels_per_point).round() as u32,
        )
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
        let (Some(rect), Some(_)) = (self.image, self.image_size) else {
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
        let px = Size::new(
            (shown.width() * self.pixels_per_point).round().max(1.0) as u32,
            (shown.height() * self.pixels_per_point).round().max(1.0) as u32,
        );
        Some((shown, region, px))
    }
}

/// Draw the canvas in the remaining space. `edit` handles the pointer (the canvas response) and
/// says what to paint on top of the Image.
pub fn show(
    ui: &mut egui::Ui,
    shown: Option<&ShownImage>,
    error: Option<&str>,
    edit: impl FnOnce(&mut egui::Ui, &Layout, &egui::Response) -> EditorPaint,
) -> Layout {
    let frame = egui::Frame::NONE.fill(ui.visuals().extreme_bg_color);
    egui::CentralPanel::default()
        .frame(frame)
        .show(ui, |ui| {
            let rect = ui.max_rect();
            let ppp = ui.ctx().pixels_per_point();
            let image_rect = shown.map(|image| {
                let size = fit(image.original_size, rect.size(), ppp);
                egui::Rect::from_center_size(rect.center(), size)
            });
            let layout = Layout {
                area: rect,
                image: image_rect,
                pixels_per_point: ppp,
                image_size: shown.map(|s| s.original_size),
            };
            let response = ui.interact(
                rect,
                ui.scope_id().with("canvas"),
                egui::Sense::click_and_drag(),
            );
            let paint = edit(ui, &layout, &response);

            let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
            if let Some((texture, dest)) = paint.composite {
                ui.painter().image(texture, dest, uv, egui::Color32::WHITE);
            } else if let (Some(image), Some(dest)) = (shown, image_rect) {
                ui.painter()
                    .image(image.texture.id(), dest, uv, egui::Color32::WHITE);
            } else if let Some(error) = error {
                ui.centered_and_justified(|ui| ui.label(error));
            }
            if let Some(selection) = paint.selection {
                paint_selection(ui.painter(), selection);
            }
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
