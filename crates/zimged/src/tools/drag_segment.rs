//! A tool that creates an Annotation by dragging from one end to the other: line and arrow.
//! Shift snaps the angle to 15° steps (§4.1).

use zimged_core::{AnnotationId, AnnotationShape, Arrow, Line, Point};

use super::{Pointer, Tool, ToolContext};

/// The angle step Shift snaps to.
const SNAP_DEGREES: f32 = 15.0;

pub struct DragSegment {
    /// Builds the Annotation's shape from the dragged ends.
    make: fn(Point, Point) -> AnnotationShape,
    drawing: Option<Drawing>,
}

struct Drawing {
    start: Point,
    id: AnnotationId,
    moved: bool,
}

/// Line (L): drag; Shift snaps to 15° steps.
pub fn line() -> DragSegment {
    DragSegment::new(|a, b| Line::new(a, b).into())
}

/// Arrow (A): drag; Shift snaps to 15° steps.
pub fn arrow() -> DragSegment {
    DragSegment::new(|a, b| Arrow::new(a, b).into())
}

impl DragSegment {
    pub fn new(make: fn(Point, Point) -> AnnotationShape) -> Self {
        Self {
            make,
            drawing: None,
        }
    }
}

/// The end of a segment dragged from `start` to `to`; with Shift, at the same length but on the
/// nearest multiple of 15°.
fn dragged_end(start: Point, to: Pointer) -> Point {
    if !to.modifiers.shift {
        return to.pos;
    }
    let d = to.pos - start;
    let step = SNAP_DEGREES.to_radians();
    let angle = (d.y.atan2(d.x) / step).round() * step;
    let length = d.length();
    Point::new(
        start.x + length * angle.cos(),
        start.y + length * angle.sin(),
    )
}

impl Tool for DragSegment {
    fn press(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        cx.doc.begin_gesture();
        let id = cx
            .doc
            .add((self.make)(pointer.pos, pointer.pos), cx.style.clone());
        cx.doc.select([id]);
        self.drawing = Some(Drawing {
            start: pointer.pos,
            id,
            moved: false,
        });
    }

    fn drag(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        let Some(drawing) = &mut self.drawing else {
            return;
        };
        drawing.moved = true;
        let shape = (self.make)(drawing.start, dragged_end(drawing.start, pointer));
        if let Some(a) = cx.doc.annotation_mut(drawing.id) {
            a.shape = shape;
        }
    }

    fn release(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        let Some(drawing) = self.drawing.take() else {
            return;
        };
        let end = dragged_end(drawing.start, pointer);
        if !drawing.moved || drawing.start.distance(end) < 1.0 {
            // A click draws nothing.
            cx.doc.cancel_gesture();
            return;
        }
        if let Some(a) = cx.doc.annotation_mut(drawing.id) {
            a.shape = (self.make)(drawing.start, end);
        }
        cx.doc.commit_gesture();
    }

    fn cancel(&mut self, cx: &mut ToolContext<'_>) {
        if self.drawing.take().is_some() && cx.doc.in_gesture() {
            cx.doc.cancel_gesture();
        }
    }
}
