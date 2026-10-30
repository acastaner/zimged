//! Annotations (GLOSSARY): one module per type, gathered into the [`AnnotationShape`] enum.
//!
//! Adding an Annotation type (ellipse, line, arrow, text, …):
//! 1. a module here with a struct implementing [`Shape`] (geometry in Image pixels),
//! 2. one line in the `shapes!` list below,
//! 3. its drawing in `zimged-render` (`shapes/<type>.rs` plus one arm in `shapes::draw`),
//! 4. a tool that creates it in the `zimged` crate (`tools/`).

mod rectangle;
mod step_counter;

pub use rectangle::Rectangle;
pub use step_counter::StepCounter;

use crate::geometry::{Affine, Point, Rect};
use crate::style::{PropertyKind, Style};

/// What every Annotation type provides to the Document, the select tool and the property bar.
pub trait Shape {
    /// The geometric bounds, without stroke width or shadow. The selection box is drawn around it
    /// and its handles resize it.
    fn bounds(&self) -> Rect;

    /// The bounds for this style, for types whose size is a style property (Step counter).
    fn styled_bounds(&self, _style: &Style) -> Rect {
        self.bounds()
    }

    /// Is `p` on the Annotation? `tolerance` (Image pixels) widens thin strokes so they can be
    /// clicked.
    fn hit(&self, p: Point, style: &Style, tolerance: f32) -> bool;

    /// Move, scale or otherwise transform the geometry (moves, handle resizes, and the Image
    /// operations of ADR 0001).
    fn transform(&mut self, t: &Affine);

    /// The style properties the property bar shows for this type (§4.3), in display order.
    fn properties(&self) -> &'static [PropertyKind];

    /// A transformed copy.
    fn transformed(&self, t: &Affine) -> Self
    where
        Self: Clone + Sized,
    {
        let mut s = self.clone();
        s.transform(t);
        s
    }
}

/// Declares [`AnnotationShape`] with one variant per type and forwards [`Shape`] to it.
macro_rules! shapes {
    ($($variant:ident($ty:ty)),+ $(,)?) => {
        /// The geometry of an Annotation, one variant per Annotation type.
        #[derive(Clone, Debug, PartialEq)]
        pub enum AnnotationShape {
            $($variant($ty),)+
        }

        impl Shape for AnnotationShape {
            fn bounds(&self) -> Rect {
                match self { $(Self::$variant(s) => s.bounds(),)+ }
            }
            fn styled_bounds(&self, style: &Style) -> Rect {
                match self { $(Self::$variant(s) => s.styled_bounds(style),)+ }
            }
            fn hit(&self, p: Point, style: &Style, tolerance: f32) -> bool {
                match self { $(Self::$variant(s) => s.hit(p, style, tolerance),)+ }
            }
            fn transform(&mut self, t: &Affine) {
                match self { $(Self::$variant(s) => s.transform(t),)+ }
            }
            fn properties(&self) -> &'static [PropertyKind] {
                match self { $(Self::$variant(s) => s.properties(),)+ }
            }
        }

        $(impl From<$ty> for AnnotationShape {
            fn from(s: $ty) -> Self {
                Self::$variant(s)
            }
        })+
    };
}

shapes! {
    Rectangle(Rectangle),
    StepCounter(StepCounter),
}

/// Identifies an Annotation within its Document. Never reused, so it survives undo and redo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AnnotationId(pub u64);

/// An Annotation on the Image: its geometry and its style.
#[derive(Clone, Debug, PartialEq)]
pub struct Annotation {
    pub id: AnnotationId,
    pub shape: AnnotationShape,
    pub style: Style,
}

impl Annotation {
    pub fn bounds(&self) -> Rect {
        self.shape.styled_bounds(&self.style)
    }

    pub fn hit(&self, p: Point, tolerance: f32) -> bool {
        self.shape.hit(p, &self.style, tolerance)
    }
}
