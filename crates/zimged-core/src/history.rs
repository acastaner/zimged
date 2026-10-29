//! The History (§4.8): one linear undo/redo sequence of steps; one gesture is one step.
//!
//! A step stores the Document's editable state before and after it ([`Snapshot`]). Snapshots
//! make every edit undoable without per-edit undo code: a new Annotation type, a new property or
//! a new kind of edit gets undo and redo for free.
//!
//! Image operations (#32, #33) add the pre-operation pixels to [`Snapshot`] (behind an `Arc`, so
//! steps that don't change pixels share them) and enforce the ~512 MB cap on stored pixels by
//! dropping the oldest steps.

use crate::annotation::{Annotation, AnnotationId};
use crate::style::PropertyKind;

/// The Document's editable state, as stored in a step.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Snapshot {
    pub annotations: Vec<Annotation>,
    pub selection: Vec<AnnotationId>,
}

/// What a step was, for merging property changes made in one widget interaction.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum StepKind {
    Gesture,
    Property {
        kind: PropertyKind,
        targets: Vec<AnnotationId>,
    },
}

#[derive(Clone, Debug)]
pub(crate) struct Step {
    pub kind: StepKind,
    pub before: Snapshot,
    pub after: Snapshot,
}

#[derive(Debug, Default)]
pub(crate) struct History {
    steps: Vec<Step>,
    /// Steps `..position` are done; `position..` can be redone.
    position: usize,
    /// The position when the Image was last saved (or opened). `None`: that state is no longer
    /// reachable (it was undone and then replaced by a new step).
    saved_at: Option<usize>,
}

impl History {
    pub fn new() -> Self {
        Self {
            steps: Vec::new(),
            position: 0,
            saved_at: Some(0),
        }
    }

    /// Record a done step. Drops the redo tail.
    pub fn push(&mut self, step: Step) {
        self.steps.truncate(self.position);
        if self.saved_at.is_some_and(|s| s > self.position) {
            self.saved_at = None;
        }
        self.steps.push(step);
        self.position += 1;
    }

    /// The last done step, if it is the newest one (nothing to redo).
    pub fn last_mut(&mut self) -> Option<&mut Step> {
        if self.position == self.steps.len() {
            self.steps.last_mut()
        } else {
            None
        }
    }

    pub fn undo(&mut self) -> Option<&Step> {
        if self.position == 0 {
            return None;
        }
        self.position -= 1;
        Some(&self.steps[self.position])
    }

    pub fn redo(&mut self) -> Option<&Step> {
        let step = self.steps.get(self.position)?;
        self.position += 1;
        Some(step)
    }

    pub fn can_undo(&self) -> bool {
        self.position > 0
    }

    pub fn can_redo(&self) -> bool {
        self.position < self.steps.len()
    }

    /// The number of done steps.
    pub fn undo_len(&self) -> usize {
        self.position
    }

    pub fn redo_len(&self) -> usize {
        self.steps.len() - self.position
    }

    pub fn is_modified(&self) -> bool {
        self.saved_at != Some(self.position)
    }

    pub fn mark_saved(&mut self) {
        self.saved_at = Some(self.position);
    }
}
