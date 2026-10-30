use skia_safe as sk;
use zimged_core::annotation::text::TextSpec;
use zimged_core::{Shape, Style, Text};

use super::fill_paint;
use crate::effects::with_shadow;

/// The box's fill (if any), then the text in the text colour.
pub fn draw(canvas: &sk::Canvas, shape: &Text, style: &Style) {
    let paragraph = crate::text::paragraph(
        &TextSpec {
            text: shape.text(),
            font: &style.font,
            size: style.font_size,
            wrap: shape.width(),
        },
        crate::color(style.line),
    );
    let origin = shape.text_origin();
    with_shadow(canvas, style.shadow, |canvas| {
        if let Some(fill) = fill_paint(style) {
            canvas.draw_rect(crate::rect(shape.bounds()), &fill);
        }
        paragraph.paint(canvas, (origin.x, origin.y));
    });
}
