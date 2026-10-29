//! Decoding and encoding (spec §5, §7.1).
//!
//! Today: display-size decode (§5.2) of the formats the `image` crate is built with (JPEG only so
//! far). Full-resolution decode, EXIF orientation, lcms2 colour conversion, animation frames,
//! encoding and the safe atomic save land here with their tickets.

use std::fmt;
use std::path::Path;

use fast_image_resize as fr;

/// Pixel dimensions, width × height.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

impl fmt::Display for Size {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}×{}", self.width, self.height)
    }
}

/// An image decoded for display: reduced to fit a box, never above its original size.
#[derive(Clone)]
pub struct DisplayImage {
    /// The size of the file's pixels, before any reduction.
    pub original_size: Size,
    /// The size of `rgb`.
    pub size: Size,
    /// Tightly packed 8-bit RGB, row-major.
    pub rgb: Vec<u8>,
}

impl fmt::Debug for DisplayImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DisplayImage")
            .field("original_size", &self.original_size)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

/// Why an image couldn't be decoded. The `Display` text is the `<reason>` of
/// "Can't open <name>: <reason>" (§5.2).
#[derive(Debug)]
pub struct DecodeError(String);

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DecodeError {}

/// A file decoded at full resolution, not yet reduced. Splitting [`decode_for_display`] in two lets
/// the startup path decode before it knows the window size (§1.3).
pub struct Decoded(image::RgbImage);

impl Decoded {
    pub fn size(&self) -> Size {
        let (w, h) = self.0.dimensions();
        Size::new(w, h)
    }

    /// Reduce to fit inside `max` (physical pixels), keeping the aspect ratio, never enlarging.
    pub fn into_display(self, max: Size) -> DisplayImage {
        let original_size = self.size();
        let target = fit_within(original_size, max);
        let rgb = if target == original_size {
            self.0.into_raw()
        } else {
            let src = fr::images::Image::from_vec_u8(
                original_size.width,
                original_size.height,
                self.0.into_raw(),
                fr::PixelType::U8x3,
            )
            .expect("buffer matches its dimensions");
            let mut dst = fr::images::Image::new(target.width, target.height, fr::PixelType::U8x3);
            fr::Resizer::new()
                .resize(
                    &src,
                    &mut dst,
                    &fr::ResizeOptions::new()
                        .resize_alg(fr::ResizeAlg::Convolution(fr::FilterType::Bilinear)),
                )
                .expect("same pixel type");
            dst.into_vec()
        };
        DisplayImage {
            original_size,
            size: target,
            rgb,
        }
    }
}

/// Decode a file at its full resolution.
pub fn decode(path: &Path) -> Result<Decoded, DecodeError> {
    let reader = image::ImageReader::open(path)
        .map_err(|e| DecodeError(e.to_string()))?
        .with_guessed_format()
        .map_err(|e| DecodeError(e.to_string()))?;
    let img = reader.decode().map_err(|e| DecodeError(e.to_string()))?;
    Ok(Decoded(img.into_rgb8()))
}

/// Decode a file for display inside `max` physical pixels (§5.2).
pub fn decode_for_display(path: &Path, max: Size) -> Result<DisplayImage, DecodeError> {
    Ok(decode(path)?.into_display(max))
}

/// The largest size with `size`'s aspect ratio that fits in `max`, never above `size` itself.
fn fit_within(size: Size, max: Size) -> Size {
    let scale = (f64::from(max.width) / f64::from(size.width))
        .min(f64::from(max.height) / f64::from(size.height))
        .min(1.0);
    if scale >= 1.0 {
        return size;
    }
    let w = ((f64::from(size.width) * scale).round() as u32).max(1);
    let h = ((f64::from(size.height) * scale).round() as u32).max(1);
    Size::new(w, h)
}
