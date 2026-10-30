//! Drawing each Annotation type, one module per type. The canvas is in Image pixels.
//!
//! Adding a type: a module with `draw(canvas, shape, style)` and one arm in [`draw`].

mod arrow;
mod ellipse;
mod freehand;
mod line;
mod rectangle;

use skia_safe as sk;
use zimged_core::{Annotation, AnnotationShape};

pub fn draw(canvas: &sk::Canvas, annotation: &Annotation) {
    let style = &annotation.style;
    match &annotation.shape {
        AnnotationShape::Rectangle(s) => rectangle::draw(canvas, s, style),
        AnnotationShape::Ellipse(s) => ellipse::draw(canvas, s, style),
        AnnotationShape::Line(s) => line::draw(canvas, s, style),
        AnnotationShape::Arrow(s) => arrow::draw(canvas, s, style),
        AnnotationShape::Freehand(s) => freehand::draw(canvas, s, style),
    }
}

/// An anti-aliased stroke paint for the Style's line.
pub(crate) fn line_paint(style: &zimged_core::Style) -> sk::Paint {
    let mut paint = sk::Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(sk::PaintStyle::Stroke);
    paint.set_stroke_width(style.thickness);
    paint.set_color(crate::color(style.line));
    paint
}

/// An anti-aliased fill paint, if the Style has a fill.
pub(crate) fn fill_paint(style: &zimged_core::Style) -> Option<sk::Paint> {
    let fill = style.fill?;
    let mut paint = sk::Paint::default();
    paint.set_anti_alias(true);
    paint.set_style(sk::PaintStyle::Fill);
    paint.set_color(crate::color(fill));
    Some(paint)
}
