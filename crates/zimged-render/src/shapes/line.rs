use skia_safe as sk;
use zimged_core::{Line, Style};

use super::line_paint;
use crate::effects::with_shadow;

pub fn draw(canvas: &sk::Canvas, shape: &Line, style: &Style) {
    let bounds = sk::Rect::from_ltrb(
        shape.from.x.min(shape.to.x),
        shape.from.y.min(shape.to.y),
        shape.from.x.max(shape.to.x),
        shape.from.y.max(shape.to.y),
    );
    with_shadow(
        canvas,
        style.shadow,
        super::outset(bounds, style.thickness),
        |canvas| {
            let mut paint = line_paint(style);
            paint.set_stroke_cap(sk::PaintCap::Round);
            canvas.draw_line(crate::point(shape.from), crate::point(shape.to), &paint);
        },
    );
}
