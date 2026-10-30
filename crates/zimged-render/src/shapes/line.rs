use skia_safe as sk;
use zimged_core::{Line, Style};

use super::line_paint;
use crate::effects::with_shadow;

pub fn draw(canvas: &sk::Canvas, shape: &Line, style: &Style) {
    with_shadow(canvas, style.shadow, |canvas| {
        let mut paint = line_paint(style);
        paint.set_stroke_cap(sk::PaintCap::Round);
        canvas.draw_line(crate::point(shape.from), crate::point(shape.to), &paint);
    });
}
