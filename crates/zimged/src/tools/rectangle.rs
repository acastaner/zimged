//! Rectangle (R): drag; Shift makes a square.

use zimged_core::{AnnotationShape, Rect, Rectangle};

use super::drag_box::DragBox;

pub fn new() -> DragBox {
    DragBox::new(|r: Rect| AnnotationShape::from(Rectangle::new(r)))
}
