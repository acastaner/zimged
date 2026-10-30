//! Select (Esc): click selects, Shift+click adds to the selection (or takes a selected Annotation
//! out of it), drag moves the selection, the handles resize it, and a drag on empty space draws a
//! rubber band that selects the Annotations entirely inside it (with Shift, in addition to the
//! selection).
//!
//! Double-click to edit text (#28) goes in `press`.

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
        /// Shift+press on an already selected Annotation: a click (no movement) takes it out of
        /// the selection instead of moving anything.
        deselect_on_click: Option<AnnotationId>,
        moved: bool,
    },
    Resize {
        handle: Handle,
        bounds: Rect,
        originals: Vec<(AnnotationId, AnnotationShape)>,
    },
    /// A drag on empty space. `base` stays selected besides the enclosed Annotations (the
    /// previous selection with Shift, nothing without).
    RubberBand {
        start: Point,
        to: Point,
        base: Vec<AnnotationId>,
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

/// Is `inner` entirely inside the rubber band?
fn encloses(band: Rect, inner: Rect) -> bool {
    band.contains(inner.min) && band.contains(inner.max)
}

impl Tool for Select {
    fn press(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        let p = pointer.pos;
        let shift = pointer.modifiers.shift;
        // Handles win even with Shift (Shift+corner handle unlocks an Inset's aspect ratio, §4.6).
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
                let already = cx.doc.is_selected(id);
                if shift && !already {
                    let mut ids = cx.doc.selection().to_vec();
                    ids.push(id);
                    cx.doc.select(ids);
                } else if !already {
                    cx.doc.select([id]);
                }
                self.action = Some(Action::Move {
                    start: p,
                    originals: selected_shapes(cx),
                    deselect_on_click: (shift && already).then_some(id),
                    moved: false,
                });
                cx.doc.begin_gesture();
            }
            None => {
                let base = if shift {
                    cx.doc.selection().to_vec()
                } else {
                    cx.doc.clear_selection();
                    Vec::new()
                };
                self.action = Some(Action::RubberBand {
                    start: p,
                    to: p,
                    base,
                });
            }
        }
    }

    fn drag(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        match &mut self.action {
            Some(Action::Move {
                start,
                originals,
                moved,
                ..
            }) => {
                *moved |= pointer.pos != *start;
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
            Some(Action::RubberBand { start, to, base }) => {
                *to = pointer.pos;
                let band = Rect::from_corners(*start, *to);
                let enclosed: Vec<AnnotationId> = cx
                    .doc
                    .annotations()
                    .iter()
                    .filter(|a| !base.contains(&a.id) && encloses(band, a.bounds()))
                    .map(|a| a.id)
                    .collect();
                cx.doc.select(base.iter().copied().chain(enclosed));
            }
            None => {}
        }
    }

    fn release(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer) {
        if self.action.is_none() {
            return;
        }
        self.drag(cx, pointer);
        match self.action.take() {
            Some(Action::RubberBand { .. }) | None => {}
            Some(Action::Move {
                deselect_on_click: Some(id),
                moved: false,
                ..
            }) => {
                cx.doc.cancel_gesture();
                let rest: Vec<AnnotationId> = cx
                    .doc
                    .selection()
                    .iter()
                    .copied()
                    .filter(|s| *s != id)
                    .collect();
                cx.doc.select(rest);
            }
            Some(Action::Move { .. } | Action::Resize { .. }) => {
                cx.doc.commit_gesture();
            }
        }
    }

    fn rubber_band(&self) -> Option<Rect> {
        match &self.action {
            Some(Action::RubberBand { start, to, .. }) => Some(Rect::from_corners(*start, *to)),
            _ => None,
        }
    }

    fn cancel(&mut self, cx: &mut ToolContext<'_>) {
        if self.action.take().is_some() && cx.doc.in_gesture() {
            cx.doc.cancel_gesture();
        }
    }
}
