//! Image operations (§4.7, ADR 0001): crop, and (#33) rotate, flip and resize.
//!
//! An Image operation is two things:
//! - new pixels, computed from the full-resolution [`Pixels`] (`Pixels::crop` here; #33 adds
//!   rotate/flip/resize next to it);
//! - an [`ImageTransform`] saying how old Image pixel coordinates map to new ones, which every
//!   Annotation follows through [`crate::Shape::image_transform`].
//!
//! [`crate::Document::image_operation`] applies both as one History step (pixels and Annotations
//! restored on undo, the pixel cap enforced). A new operation only needs its pixel function and its
//! `ImageTransform` variant.

use crate::document::{PixelData, Pixels};
use crate::geometry::{Affine, Point, Rect, Vector};

/// How an Image operation moves Image pixels: what Annotations follow (ADR 0001).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImageTransform {
    /// Crop: the new Image starts at `origin` (old Image pixels). Annotations keep their position
    /// in the image, so they move by `-origin`; the canvas clips what lies past the new edges.
    Crop { origin: Point },
    // #33: Rotate90 { clockwise, old size }, Flip { horizontal, old size }, Resize { sx, sy }.
    // Text and Step counters move their anchor but stay upright; resize also scales line
    // thickness and font size (a Style change: see `Annotation::image_transform`).
}

impl ImageTransform {
    /// The map from old Image pixels to new ones. Shapes whose geometry is plain points and axis
    /// aligned rects implement [`crate::Shape::image_transform`] by applying it.
    pub fn affine(&self) -> Affine {
        match *self {
            ImageTransform::Crop { origin } => Affine::translate(Vector::new(-origin.x, -origin.y)),
        }
    }
}

impl Pixels {
    /// Bytes held: what counts towards the History's pixel cap (§4.8).
    pub fn byte_len(&self) -> usize {
        match &self.data {
            PixelData::Rgb(d) | PixelData::Rgba(d) => d.len(),
        }
    }

    fn channels(&self) -> usize {
        match self.data {
            PixelData::Rgb(_) => 3,
            PixelData::Rgba(_) => 4,
        }
    }

    /// The pixels inside `region`, given in whole pixels and inside the Image.
    pub fn crop(&self, region: PixelRect) -> Pixels {
        assert!(
            region.x + region.width <= self.width && region.y + region.height <= self.height,
            "crop region outside the Image"
        );
        let n = self.channels();
        let stride = self.width as usize * n;
        let row = region.width as usize * n;
        let src = match &self.data {
            PixelData::Rgb(d) | PixelData::Rgba(d) => d,
        };
        let mut out = Vec::with_capacity(row * region.height as usize);
        for y in region.y..region.y + region.height {
            let start = y as usize * stride + region.x as usize * n;
            out.extend_from_slice(&src[start..start + row]);
        }
        Pixels {
            width: region.width,
            height: region.height,
            data: match self.data {
                PixelData::Rgb(_) => PixelData::Rgb(out),
                PixelData::Rgba(_) => PixelData::Rgba(out),
            },
        }
    }
}

/// A rectangle of whole Image pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl PixelRect {
    /// `r` rounded to whole pixels and clamped to a `width`×`height` Image. `None` when nothing
    /// of it is left.
    pub fn within(r: Rect, width: u32, height: u32) -> Option<Self> {
        let clamp = |v: f32, max: u32| v.round().clamp(0.0, max as f32) as u32;
        let (x0, x1) = (clamp(r.min.x, width), clamp(r.max.x, width));
        let (y0, y1) = (clamp(r.min.y, height), clamp(r.max.y, height));
        (x1 > x0 && y1 > y0).then_some(Self {
            x: x0,
            y: y0,
            width: x1 - x0,
            height: y1 - y0,
        })
    }

    pub fn to_rect(self) -> Rect {
        Rect::from_min_size(
            Point::new(self.x as f32, self.y as f32),
            self.width as f32,
            self.height as f32,
        )
    }
}
