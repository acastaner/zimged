//! An Annotation's drawn bounds and whether it is an opaque neighbourhood effect: render-crate
//! knowledge the editor needs to re-render only the region a gesture changes (#46, §7.2).
//!
//! The drawn bounds are the rectangle of Image pixels an Annotation's `draw` can touch: its
//! geometry grown for the stroke, for an arrow's head, and — when the shadow style is on — for the
//! shadow's spread. This is the same bound [`crate::effects::with_shadow`] confines each shape's
//! save-layer to, so the two must grow a shape the same way; the shape goldens and the dirty-region
//! equivalence tests guard that they do.

use zimged_core::{Annotation, AnnotationShape, Arrow, HighlightMode, Rect, Shape, Style};

use crate::effects::SHADOW_SPREAD;

/// The rectangle of Image pixels `annotation` can draw into (see the module docs). Conservative: a
/// superset of the touched pixels is always safe for the dirty region.
pub fn drawn_bounds(annotation: &Annotation) -> Rect {
    let style = &annotation.style;
    let content = content_bounds(&annotation.shape, style);
    // Effect types (Highlight, Obfuscate) never draw a shadow, whatever the style says.
    let spread = if style.shadow && !annotation.shape.needs_pixels_below() {
        SHADOW_SPREAD
    } else {
        0.0
    };
    content.expand(spread)
}

/// Does `annotation` read a neighbourhood of the pixels beneath it and hide them (Obfuscate in
/// either mode, and a Magnify Highlight)? Such an effect cannot be recomputed from a slice of the
/// canvas, so when a gesture only partly overlaps it, the editor freezes it for the frame (#46).
/// The per-pixel Highlight modes (Text, Area, Grayscale) are exact within any sub-rectangle and so
/// are not opaque in this sense.
pub fn is_opaque_effect(annotation: &Annotation) -> bool {
    match &annotation.shape {
        AnnotationShape::Obfuscate(_) => true,
        AnnotationShape::Highlight(_) => annotation.style.highlight_mode == HighlightMode::Magnify,
        _ => false,
    }
}

/// The bound each shape's `draw` grows its geometry to before the shadow — the rectangle passed to
/// [`crate::effects::with_shadow`], matched here so the dirty region covers every drawn pixel.
fn content_bounds(shape: &AnnotationShape, style: &Style) -> Rect {
    match shape {
        AnnotationShape::Rectangle(s) => s.rect.expand(style.thickness),
        AnnotationShape::Ellipse(s) => s.rect.expand(style.thickness),
        AnnotationShape::Line(s) => Rect::from_corners(s.from, s.to).expand(style.thickness),
        AnnotationShape::Arrow(s) => {
            let reach = Arrow::head_length(style.thickness).max(style.thickness);
            Rect::from_corners(s.from, s.to).expand(reach)
        }
        // The smoothed path stays within the points' convex hull, so the points' bounds are a
        // superset of the drawn curve's.
        AnnotationShape::Freehand(s) => s.bounds().expand(style.thickness),
        AnnotationShape::StepCounter(s) => s.disc(style).expand(style.thickness),
        AnnotationShape::Text(s) => s.bounds().expand(style.font_size),
        AnnotationShape::Inset(s) => s.rect.expand(style.thickness),
        // Effects draw within their own box (clipped to the Image when composited).
        AnnotationShape::Highlight(s) => s.rect,
        AnnotationShape::Obfuscate(s) => s.rect,
    }
}

#[cfg(test)]
mod tests {
    use zimged_core::{Highlight, HighlightMode, Obfuscate, Point, Rectangle, Style};

    use super::*;

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect::from_corners(Point::new(x0, y0), Point::new(x1, y1))
    }

    #[test]
    fn a_rectangles_drawn_bounds_grow_by_the_stroke_and_the_shadow_when_it_is_on() {
        let shape = Rectangle::new(rect(10.0, 20.0, 110.0, 70.0));
        let style = Style {
            thickness: 4.0,
            shadow: false,
            ..Style::default()
        };
        let a = Annotation {
            id: zimged_core::AnnotationId(1),
            shape: shape.clone().into(),
            style: style.clone(),
        };
        // Without the shadow: the geometry grown by the stroke.
        assert_eq!(drawn_bounds(&a), rect(6.0, 16.0, 114.0, 74.0));

        // With the shadow: grown further by the shadow spread on every side.
        let a = Annotation {
            id: zimged_core::AnnotationId(1),
            shape: shape.into(),
            style: Style {
                shadow: true,
                ..style
            },
        };
        let s = SHADOW_SPREAD;
        assert_eq!(
            drawn_bounds(&a),
            rect(6.0 - s, 16.0 - s, 114.0 + s, 74.0 + s)
        );
    }

    #[test]
    fn obfuscations_and_magnify_highlights_are_opaque_effects_but_a_text_highlight_is_not() {
        let region = rect(0.0, 0.0, 50.0, 50.0);
        let obf = Annotation {
            id: zimged_core::AnnotationId(1),
            shape: Obfuscate::new(region).into(),
            style: Style::default(),
        };
        assert!(is_opaque_effect(&obf));

        let hl = Annotation {
            id: zimged_core::AnnotationId(2),
            shape: Highlight::new(region).into(),
            style: Style {
                highlight_mode: HighlightMode::Magnify,
                ..Style::default()
            },
        };
        assert!(is_opaque_effect(&hl));

        let hl = Annotation {
            id: zimged_core::AnnotationId(3),
            shape: Highlight::new(region).into(),
            style: Style {
                highlight_mode: HighlightMode::Text,
                ..Style::default()
            },
        };
        assert!(!is_opaque_effect(&hl));
    }
}
