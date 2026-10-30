//! Select (Esc): click selects, drag moves the selection, the handles resize it.
//!
//! Shift+click and the rubber band (#27) and double-click to edit text (#28) go in `press`.

use zimged_core::{Affine, AnnotationId, AnnotationShape, Handle, Point, Rect, Shape};

use super::{Pointer, Tool, ToolContext};

/// The side of a selection handle on screen, in points.
pub const HANDLE_SIZE: f32 = 9.0;
/// How far from a handle's centre a press still grabs it, in points.
const HANDLE_GRAB: f32 = 7.0;
/// How far from a thin line a press still hits it, in points.
const HIT_SLOP: f32 = 4.0;

#[derive(Default)]
pub struct Select {
    action: Option<Action>,
}

enum Action {
    Move {
        start: Point,
        originals: Vec<(AnnotationId, AnnotationShape)>,
    },
    Resize {
        handle: Handle,
        bounds: Rect,
        originals: Vec<(AnnotationId, AnnotationShape)>,
    },
}

/// The handle of the selection box under `p`, if any.
pub fn handle_at(bounds: Rect, p: Point, image_per_point: f32) -> Option<Handle> {
    let grab = HANDLE_GRAB * image_per_point;
    Handle::ALL
        .into_iter()
        .find(|h| bounds.handle_position(*h).distance(p) <= grab)
}

fn selected_shapes(cx: &ToolContext<'_>) -> Vec<(AnnotationId, AnnotationShape)> {
    cx.doc.selected().map(|a| (a.id, a.shape.clone())).collect()
}

fn apply(cx: &mut ToolContext<'_>, originals: &[(AnnotationId, AnnotationShape)], t: &Affine) {
    for (id, shape) in originals {
        if let Some(a) = cx.doc.annotation_mut(*id) {
            a.shape = shape.transformed(t);
        }
    }
}

impl Tool for Select {
    fn press(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        let p = pointer.pos;
        if let Some(bounds) = cx.doc.selection_bounds()
            && let Some(handle) = handle_at(bounds, p, cx.image_per_point)
        {
            self.action = Some(Action::Resize {
                handle,
                bounds,
                originals: selected_shapes(cx),
            });
            cx.doc.begin_gesture();
            return;
        }
        match cx.doc.hit_test(p, HIT_SLOP * cx.image_per_point) {
            Some(id) => {
                if !cx.doc.is_selected(id) {
                    cx.doc.select([id]);
                }
                self.action = Some(Action::Move {
                    start: p,
                    originals: selected_shapes(cx),
                });
                cx.doc.begin_gesture();
            }
            None => {
                cx.doc.clear_selection();
                self.action = None;
            }
        }
    }

    fn drag(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        match &self.action {
            Some(Action::Move { start, originals }) => {
                let t = Affine::translate(pointer.pos - *start);
                apply(cx, originals, &t);
            }
            Some(Action::Resize {
                handle,
                bounds,
                originals,
            }) => {
                let to = bounds.with_handle_at(*handle, pointer.pos);
                let t = Affine::rect_to_rect(*bounds, to);
                apply(cx, originals, &t);
            }
            None => {}
        }
    }

    fn release(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        if self.action.is_some() {
            self.drag(cx, pointer);
            self.action = None;
            cx.doc.commit_gesture();
        }
    }

    fn cancel(&mut self, cx: &mut ToolContext<'_>) {
        if self.action.take().is_some() && cx.doc.in_gesture() {
            cx.doc.cancel_gesture();
        }
    }
}
