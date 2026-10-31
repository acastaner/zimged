use skia_safe as sk;
use zimged_core::{Rectangle, Style};

use super::{fill_paint, line_paint};
use crate::effects::with_shadow;

pub fn draw(canvas: &sk::Canvas, shape: &Rectangle, style: &Style) {
    let rect = crate::rect(shape.rect);
    with_shadow(canvas, style.shadow, super::outset(rect, style.thickness), |canvas| {
        if let Some(fill) = fill_paint(style) {
            canvas.draw_rect(rect, &fill);
        }
        let mut line = line_paint(style);
        line.set_stroke_join(sk::PaintJoin::Miter);
        canvas.draw_rect(rect, &line);
    });
}
