//! Annotations (GLOSSARY): one module per type, gathered into the [`AnnotationShape`] enum.
//!
//! Adding an Annotation type (ellipse, line, arrow, text, …):
//! 1. a module here with a struct implementing [`Shape`] (geometry in Image pixels), including
//!    how it follows Image operations ([`Shape::image_transform`]),
//! 2. one line in the `shapes!` list below,
//! 3. its drawing in `zimged-render` (`shapes/<type>.rs` plus one arm in `shapes::draw`),
//! 4. a tool that creates it in the `zimged` crate (`tools/`).

mod ellipse;
mod freehand;
mod line;
mod rectangle;

pub use ellipse::Ellipse;
pub use freehand::Freehand;
pub use line::{Arrow, Line};
pub use rectangle::Rectangle;

use crate::geometry::{Affine, Point, Rect};
use crate::image_op::ImageTransform;
use crate::style::{PropertyKind, Style};

/// What every Annotation type provides to the Document, the select tool and the property bar.
pub trait Shape {
    /// The geometric bounds, without stroke width or shadow. The selection box is drawn around it
    /// and its handles resize it.
    fn bounds(&self) -> Rect;

    /// Is `p` on the Annotation? `tolerance` (Image pixels) widens thin strokes so they can be
    /// clicked.
    fn hit(&self, p: Point, style: &Style, tolerance: f32) -> bool;

    /// Move, scale or otherwise transform the geometry (moves, handle resizes, and the Image
    /// operations of ADR 0001).
    fn transform(&mut self, t: &Affine);

    /// Follow an Image operation (ADR 0001). Each type decides how: shapes, lines, arrows,
    /// freehand strokes and Insets apply `t.affine()` (as [`Rectangle`] does); text and Step
    /// counters move their anchor through it but stay upright. Style scaling on resize is done by
    /// [`Annotation::image_transform`], not here.
    fn image_transform(&mut self, t: &ImageTransform);

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
            fn hit(&self, p: Point, style: &Style, tolerance: f32) -> bool {
                match self { $(Self::$variant(s) => s.hit(p, style, tolerance),)+ }
            }
            fn transform(&mut self, t: &Affine) {
                match self { $(Self::$variant(s) => s.transform(t),)+ }
            }
            fn image_transform(&mut self, t: &ImageTransform) {
                match self { $(Self::$variant(s) => s.image_transform(t),)+ }
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
    Ellipse(Ellipse),
    Line(Line),
    Arrow(Arrow),
    Freehand(Freehand),
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
        self.shape.bounds()
    }

    pub fn hit(&self, p: Point, tolerance: f32) -> bool {
        self.shape.hit(p, &self.style, tolerance)
    }

    /// The area it covers on the Image: its bounds plus half the line. A crop removes an
    /// Annotation whose extent doesn't overlap the new Image.
    pub fn extent(&self) -> Rect {
        self.bounds().expand(self.style.thickness / 2.0)
    }

    /// Follow an Image operation (ADR 0001). #33's resize also scales the Style here (line
    /// thickness, font size).
    pub fn image_transform(&mut self, t: &ImageTransform) {
        self.shape.image_transform(t);
    }
}
