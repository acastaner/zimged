//! The Document: the open Image, its Annotations, the Selection and the History.
//!
//! Every change goes through a step, so it lands in the History:
//! - a gesture (a drag that draws, moves or resizes): [`Document::begin_gesture`], then any number
//!   of edits while the pointer moves, then [`Document::commit_gesture`] (or
//!   [`Document::cancel_gesture`]);
//! - a one-shot edit (delete, paste, duplicate, …): [`Document::step`] wraps a closure in a
//!   gesture;
//! - a property change: [`Document::set_property`].
//!
//! Edit methods ([`Document::add`], [`Document::annotation_mut`], [`Document::remove`], …) may only
//! be called inside a gesture; they panic otherwise, so no edit can bypass the History.

use std::sync::Arc;

use crate::annotation::{Annotation, AnnotationId, AnnotationShape};
use crate::geometry::{Point, Rect};
use crate::history::{History, Snapshot, Step, StepKind};
use crate::style::{Property, Style};

/// Full-resolution Image pixels: 8-bit sRGB, tightly packed, row-major.
#[derive(Clone)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub data: PixelData,
}

/// The channels of [`Pixels`].
#[derive(Clone)]
pub enum PixelData {
    /// Opaque images.
    Rgb(Vec<u8>),
    /// Images with an alpha channel, not premultiplied.
    Rgba(Vec<u8>),
}

impl std::fmt::Debug for Pixels {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Pixels({}×{})", self.width, self.height)
    }
}

#[derive(Debug)]
pub struct Document {
    width: u32,
    height: u32,
    /// The full-resolution pixels, once decoded (§4.7: decoded when editing starts).
    full_resolution: Option<Arc<Pixels>>,
    /// Bottom to top: the last one is drawn on top.
    annotations: Vec<Annotation>,
    selection: Vec<AnnotationId>,
    history: History,
    next_id: u64,
    /// The state when the open gesture began.
    gesture: Option<Snapshot>,
    /// Bumped on every visible change, so renderers know when to redraw.
    revision: u64,
}

impl Document {
    /// A Document for an Image of `width`×`height` full-resolution pixels, with no Annotations.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            full_resolution: None,
            annotations: Vec::new(),
            selection: Vec::new(),
            history: History::new(),
            next_id: 1,
            gesture: None,
            revision: 0,
        }
    }

    // ---- The Image ----

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// The Image's bounds in Image pixels.
    pub fn bounds(&self) -> Rect {
        Rect::from_min_size(Point::default(), self.width as f32, self.height as f32)
    }

    pub fn full_resolution(&self) -> Option<&Arc<Pixels>> {
        self.full_resolution.as_ref()
    }

    pub fn set_full_resolution(&mut self, pixels: Arc<Pixels>) {
        self.full_resolution = Some(pixels);
    }

    /// A copy of what is drawn (the size, the pixels and the Annotations), with no Selection and
    /// no History: what a worker thread Flattens (§7.2) while editing goes on.
    pub fn drawing(&self) -> Document {
        Document {
            full_resolution: self.full_resolution.clone(),
            annotations: self.annotations.clone(),
            revision: self.revision,
            ..Document::new(self.width, self.height)
        }
    }

    // ---- Reading ----

    /// Bottom to top.
    pub fn annotations(&self) -> &[Annotation] {
        &self.annotations
    }

    pub fn annotation(&self, id: AnnotationId) -> Option<&Annotation> {
        self.annotations.iter().find(|a| a.id == id)
    }

    /// The topmost Annotation under `p`.
    pub fn hit_test(&self, p: Point, tolerance: f32) -> Option<AnnotationId> {
        self.annotations
            .iter()
            .rev()
            .find(|a| a.hit(p, tolerance))
            .map(|a| a.id)
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    // ---- Selection (not a History step on its own) ----

    pub fn selection(&self) -> &[AnnotationId] {
        &self.selection
    }

    pub fn is_selected(&self, id: AnnotationId) -> bool {
        self.selection.contains(&id)
    }

    pub fn select(&mut self, ids: impl IntoIterator<Item = AnnotationId>) {
        self.selection = ids
            .into_iter()
            .filter(|id| self.annotation(*id).is_some())
            .collect();
    }

    pub fn clear_selection(&mut self) {
        self.selection.clear();
    }

    /// The selected Annotations, bottom to top.
    pub fn selected(&self) -> impl Iterator<Item = &Annotation> {
        self.annotations
            .iter()
            .filter(|a| self.selection.contains(&a.id))
    }

    /// The bounds of the whole selection: what the dashed box and its handles surround.
    pub fn selection_bounds(&self) -> Option<Rect> {
        self.selected()
            .map(|a| a.bounds())
            .reduce(|a, b| a.union(&b))
    }

    // ---- Steps ----

    pub fn in_gesture(&self) -> bool {
        self.gesture.is_some()
    }

    /// Start a gesture: the edits until [`Self::commit_gesture`] become one History step.
    pub fn begin_gesture(&mut self) {
        assert!(self.gesture.is_none(), "a gesture is already open");
        self.gesture = Some(self.snapshot());
    }

    /// End the gesture. Records a step if anything changed; returns whether it did.
    pub fn commit_gesture(&mut self) -> bool {
        let before = self.gesture.take().expect("no open gesture");
        let after = self.snapshot();
        if before.annotations == after.annotations {
            return false;
        }
        self.history.push(Step {
            kind: StepKind::Gesture,
            before,
            after,
        });
        true
    }

    /// Abandon the gesture and put everything back as it was.
    pub fn cancel_gesture(&mut self) {
        let before = self.gesture.take().expect("no open gesture");
        self.restore(&before);
    }

    /// Run `edit` as one step.
    pub fn step<R>(&mut self, edit: impl FnOnce(&mut Self) -> R) -> R {
        self.begin_gesture();
        let r = edit(self);
        self.commit_gesture();
        r
    }

    // ---- Edits (inside a gesture) ----

    /// Add an Annotation on top of the others; returns its id.
    pub fn add(&mut self, shape: impl Into<AnnotationShape>, style: Style) -> AnnotationId {
        self.assert_in_gesture();
        let id = AnnotationId(self.next_id);
        self.next_id += 1;
        self.annotations.push(Annotation {
            id,
            shape: shape.into(),
            style,
        });
        self.revision += 1;
        id
    }

    pub fn annotation_mut(&mut self, id: AnnotationId) -> Option<&mut Annotation> {
        self.assert_in_gesture();
        self.revision += 1;
        self.annotations.iter_mut().find(|a| a.id == id)
    }

    /// Remove these Annotations (and drop them from the selection).
    pub fn remove(&mut self, ids: &[AnnotationId]) {
        self.assert_in_gesture();
        self.annotations.retain(|a| !ids.contains(&a.id));
        self.selection.retain(|id| !ids.contains(id));
        self.revision += 1;
    }

    // ---- Edits (each one step) ----

    /// Delete the selected Annotations as one step. Returns false when nothing was selected.
    pub fn delete_selection(&mut self) -> bool {
        if self.selection.is_empty() {
            return false;
        }
        let ids = self.selection.clone();
        self.step(|doc| doc.remove(&ids));
        true
    }

    /// Restyle the selected Annotations as one step. With `merge`, the change is folded into the
    /// previous step when that was a change of the same property on the same Annotations (one
    /// widget interaction, e.g. dragging in a colour picker, is one step). Returns false when
    /// nothing was selected.
    pub fn set_property(&mut self, property: Property, merge: bool) -> bool {
        assert!(self.gesture.is_none(), "set_property inside a gesture");
        if self.selection.is_empty() {
            return false;
        }
        let kind = StepKind::Property {
            kind: property.kind(),
            targets: self.selection.clone(),
        };
        let before = self.snapshot();
        let selection = self.selection.clone();
        for a in &mut self.annotations {
            if selection.contains(&a.id) {
                a.style.set(property.clone());
            }
        }
        self.revision += 1;
        let after = self.snapshot();
        if before.annotations == after.annotations {
            return true;
        }
        if merge && let Some(last) = self.history.last_mut().filter(|s| s.kind == kind) {
            last.after = after;
            return true;
        }
        self.history.push(Step {
            kind,
            before,
            after,
        });
        true
    }

    // ---- History ----

    pub fn undo(&mut self) -> bool {
        if self.gesture.is_some() {
            return false;
        }
        let Some(before) = self.history.undo().map(|s| s.before.clone()) else {
            return false;
        };
        self.restore(&before);
        true
    }

    pub fn redo(&mut self) -> bool {
        if self.gesture.is_some() {
            return false;
        }
        let Some(after) = self.history.redo().map(|s| s.after.clone()) else {
            return false;
        };
        self.restore(&after);
        true
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// How many steps can be undone.
    pub fn undo_len(&self) -> usize {
        self.history.undo_len()
    }

    /// How many steps can be redone.
    pub fn redo_len(&self) -> usize {
        self.history.redo_len()
    }

    /// Unsaved changes: the state differs from the one last saved (or opened).
    pub fn is_modified(&self) -> bool {
        self.history.is_modified()
    }

    /// Record that the current state was saved (§4.8: saving doesn't clear the History).
    pub fn mark_saved(&mut self) {
        self.history.mark_saved();
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            annotations: self.annotations.clone(),
            selection: self.selection.clone(),
        }
    }

    fn restore(&mut self, s: &Snapshot) {
        self.annotations = s.annotations.clone();
        self.selection = s.selection.clone();
        self.revision += 1;
    }

    fn assert_in_gesture(&self) {
        assert!(
            self.gesture.is_some(),
            "Document edits must be inside a step"
        );
    }
}
