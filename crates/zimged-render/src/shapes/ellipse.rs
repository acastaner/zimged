use skia_safe as sk;
use zimged_core::{Ellipse, Style};

use super::{fill_paint, line_paint};
use crate::effects::with_shadow;

pub fn draw(canvas: &sk::Canvas, shape: &Ellipse, style: &Style) {
    let oval = crate::rect(shape.rect);
    with_shadow(
        canvas,
        style.shadow,
        super::outset(oval, style.thickness),
        |canvas| {
            if let Some(fill) = fill_paint(style) {
                canvas.draw_oval(oval, &fill);
            }
            canvas.draw_oval(oval, &line_paint(style));
        },
    );
}
