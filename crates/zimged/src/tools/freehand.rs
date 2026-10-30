//! Freehand (F): drag; the stroke follows the pointer and is lightly smoothed when released.

use zimged_core::{AnnotationId, AnnotationShape, Freehand, Point};

use super::{Pointer, Tool, ToolContext};

/// Pointer moves closer than this to the last kept point are jitter, in screen points.
const MIN_STEP: f32 = 2.0;

#[derive(Default)]
pub struct FreehandTool {
    drawing: Option<Drawing>,
}

struct Drawing {
    id: AnnotationId,
    /// The pointer's path, unsmoothed.
    path: Vec<Point>,
}

impl Tool for FreehandTool {
    fn press(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        cx.doc.begin_gesture();
        let id = cx.doc.add(
            AnnotationShape::from(Freehand::new(vec![pointer.pos])),
            cx.style.clone(),
        );
        cx.doc.select([id]);
        self.drawing = Some(Drawing {
            id,
            path: vec![pointer.pos],
        });
    }

    fn drag(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        let Some(drawing) = &mut self.drawing else {
            return;
        };
        if drawing.path.last() == Some(&pointer.pos) {
            return;
        }
        drawing.path.push(pointer.pos);
        // While drawing, the raw path: smoothing a growing stroke would make its tail wobble.
        if let Some(a) = cx.doc.annotation_mut(drawing.id) {
            a.shape = AnnotationShape::from(Freehand::new(drawing.path.clone()));
        }
    }

    fn release(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        let Some(mut drawing) = self.drawing.take() else {
            return;
        };
        if drawing.path.last() != Some(&pointer.pos) {
            drawing.path.push(pointer.pos);
        }
        let start = drawing.path[0];
        if drawing.path.iter().all(|p| p.distance(start) < 1.0) {
            // A click draws nothing.
            cx.doc.cancel_gesture();
            return;
        }
        let stroke = Freehand::smoothed(&drawing.path, MIN_STEP * cx.image_per_point);
        if let Some(a) = cx.doc.annotation_mut(drawing.id) {
            a.shape = stroke.into();
        }
        cx.doc.commit_gesture();
    }

    fn cancel(&mut self, cx: &mut ToolContext<'_>) {
        if self.drawing.take().is_some() && cx.doc.in_gesture() {
            cx.doc.cancel_gesture();
        }
    }
}
