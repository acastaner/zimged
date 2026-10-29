//! Decoding and encoding (spec §5, §7.1).
//!
//! Today: display-size decode (§5.2) of every v1 read format (§5.1), with the EXIF orientation and
//! the conversion of non-sRGB ICC profiles to sRGB applied. Full-resolution decode, animation
//! frames, encoding and the safe atomic save land here with their tickets.
//!
//! The display-size path reduces first, then orients and converts colour, so the extra work is
//! done on the small buffer.

use std::fmt;
use std::path::Path;

use fast_image_resize as fr;
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader};

#[cfg(feature = "avif")]
mod avif;
mod colour;

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

    const fn transposed(self) -> Self {
        Self::new(self.height, self.width)
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
    /// The size of the file's pixels, before any reduction, as displayed (after orientation).
    pub original_size: Size,
    /// The size of `pixels`.
    pub size: Size,
    pub pixels: Pixels,
}

/// Tightly packed 8-bit sRGB pixels, row-major.
#[derive(Clone)]
pub enum Pixels {
    /// Opaque images (JPEG, and every image without an alpha channel).
    Rgb(Vec<u8>),
    /// Images with an alpha channel, not premultiplied.
    Rgba(Vec<u8>),
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

impl DecodeError {
    fn new(reason: impl fmt::Display) -> Self {
        Self(reason.to_string())
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for DecodeError {}

/// A file decoded at full resolution, not yet reduced, oriented or colour-converted. Splitting
/// [`decode_for_display`] in two lets the startup path decode before it knows the window size
/// (§1.3).
pub struct Decoded {
    /// The pixels as stored in the file.
    pixels: Pixels,
    /// The stored size of `pixels`.
    stored: Size,
    /// The EXIF orientation (§5.2), applied by [`Decoded::into_display`].
    orientation: Orientation,
    /// The embedded ICC profile, if any. Converted to sRGB by [`Decoded::into_display`] unless it
    /// already is sRGB (§5.2).
    icc: Option<Vec<u8>>,
}

impl Decoded {
    /// The size as displayed: after orientation.
    pub fn size(&self) -> Size {
        if swaps_axes(self.orientation) {
            self.stored.transposed()
        } else {
            self.stored
        }
    }

    /// Reduce to fit inside `max` (physical pixels), keeping the aspect ratio, never enlarging;
    /// then apply the orientation and convert to sRGB.
    pub fn into_display(self, max: Size) -> DisplayImage {
        let original_size = self.size();
        let target = fit_within(original_size, max);
        let stored_target = if swaps_axes(self.orientation) {
            target.transposed()
        } else {
            target
        };
        let mut pixels = resize(self.pixels, self.stored, stored_target);
        if self.orientation != Orientation::NoTransforms {
            pixels = orient(pixels, stored_target, self.orientation);
        }
        if let Some(icc) = &self.icc {
            colour::to_srgb(icc, &mut pixels);
        }
        DisplayImage {
            original_size,
            size: target,
            pixels,
        }
    }
}

/// Decode a file at its full resolution.
pub fn decode(path: &Path) -> Result<Decoded, DecodeError> {
    let reader = ImageReader::open(path)
        .map_err(DecodeError::new)?
        .with_guessed_format()
        .map_err(DecodeError::new)?;
    if reader.format() == Some(ImageFormat::Avif) {
        return decode_avif(path);
    }
    let mut decoder = reader.into_decoder().map_err(DecodeError::new)?;
    // Both come from headers the decoder has already read: nothing extra on the JPEG path (§1.3).
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let icc = decoder.icc_profile().ok().flatten();
    let image = DynamicImage::from_decoder(decoder).map_err(DecodeError::new)?;
    let stored = Size::new(image.width(), image.height());
    let pixels = if image.color().has_alpha() {
        Pixels::Rgba(image.into_rgba8().into_raw())
    } else {
        Pixels::Rgb(image.into_rgb8().into_raw())
    };
    Ok(Decoded {
        pixels,
        stored,
        orientation,
        icc,
    })
}

#[cfg(feature = "avif")]
fn decode_avif(path: &Path) -> Result<Decoded, DecodeError> {
    let data = std::fs::read(path).map_err(DecodeError::new)?;
    avif::decode(&data)
}

#[cfg(not(feature = "avif"))]
fn decode_avif(_path: &Path) -> Result<Decoded, DecodeError> {
    Err(DecodeError::new("this build of zimged can't read AVIF"))
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

fn swaps_axes(orientation: Orientation) -> bool {
    matches!(
        orientation,
        Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH
    )
}

/// Resample `pixels` from `from` to `to` (a no-op when they're equal). Alpha is premultiplied for
/// the filter and divided out again.
fn resize(pixels: Pixels, from: Size, to: Size) -> Pixels {
    if from == to {
        return pixels;
    }
    let (pixel_type, rebuild): (_, fn(Vec<u8>) -> Pixels) = match pixels {
        Pixels::Rgb(_) => (fr::PixelType::U8x3, Pixels::Rgb),
        Pixels::Rgba(_) => (fr::PixelType::U8x4, Pixels::Rgba),
    };
    let (Pixels::Rgb(buffer) | Pixels::Rgba(buffer)) = pixels;
    let src = fr::images::Image::from_vec_u8(from.width, from.height, buffer, pixel_type)
        .expect("buffer matches its dimensions");
    let mut dst = fr::images::Image::new(to.width, to.height, pixel_type);
    fr::Resizer::new()
        .resize(
            &src,
            &mut dst,
            &fr::ResizeOptions::new()
                .resize_alg(fr::ResizeAlg::Convolution(fr::FilterType::Bilinear)),
        )
        .expect("same pixel type");
    rebuild(dst.into_vec())
}

/// Apply `orientation` to `pixels` of `size` (as stored).
fn orient(pixels: Pixels, size: Size, orientation: Orientation) -> Pixels {
    let mut image = match pixels {
        Pixels::Rgb(rgb) => DynamicImage::ImageRgb8(
            image::RgbImage::from_raw(size.width, size.height, rgb)
                .expect("buffer matches its dimensions"),
        ),
        Pixels::Rgba(rgba) => DynamicImage::ImageRgba8(
            image::RgbaImage::from_raw(size.width, size.height, rgba)
                .expect("buffer matches its dimensions"),
        ),
    };
    image.apply_orientation(orientation);
    match image {
        DynamicImage::ImageRgb8(rgb) => Pixels::Rgb(rgb.into_raw()),
        DynamicImage::ImageRgba8(rgba) => Pixels::Rgba(rgba.into_raw()),
        _ => unreachable!("orientation keeps the pixel type"),
    }
}
