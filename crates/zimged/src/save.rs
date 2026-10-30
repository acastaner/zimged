//! Save (§5.1, §5.3, §7.2): Flatten the Document and write it, on the worker thread (§7.3).
//!
//! The UI hands the worker a copy of what is drawn ([`zimged_core::Document::to_flatten`]), so editing
//! can go on meanwhile. The worker decodes the full-resolution pixels itself when editing hasn't
//! (they come back, so the Document keeps them), Flattens with the same Skia code as the screen,
//! encodes, and writes atomically ([`zimged_codec::save`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use zimged_codec::{SaveFormat, Size};
use zimged_core::{Annotation, AnnotationId, Document, PixelData, Pixels, Point, Rect, Shape};
use zimged_render::{Composite, Renderer, Source, SourcePixels, View};

use crate::decode::{Pending, decode_full};
use crate::strings;

/// What to save, and where.
pub(crate) struct SaveJob {
    /// What is drawn: the size, the Annotations and (when decoded) the full-resolution pixels.
    pub doc: Document,
    /// The file the Image came from: its pixels (when not yet decoded) and metadata.
    pub source: Option<PathBuf>,
    /// The display-size decode when it is the full resolution: used when the source can't be
    /// decoded any more (deleted or changed on disk, #21).
    pub fallback: Option<Arc<egui::ColorImage>>,
    pub target: PathBuf,
    pub format: SaveFormat,
    pub jpeg_quality: u8,
}

/// A save that went through.
pub(crate) struct Saved {
    /// The full-resolution pixels, when the worker had to decode them.
    pub decoded: Option<Arc<Pixels>>,
}

/// The `<reason>` of "Can't save <name>: <reason>" on failure.
pub(crate) type SaveResult = Result<Saved, String>;

pub(crate) type PendingSave = Pending<SaveResult>;

impl PendingSave {
    /// Start `job` on the worker thread. The pending work's path is the target.
    pub fn spawn_save(job: SaveJob) -> Self {
        let target = job.target.clone();
        Self::spawn_job(target, "worker", move |_| run(job))
    }
}

fn run(job: SaveJob) -> SaveResult {
    let (flattened, decoded) = flatten_document(job.doc, job.source.as_deref(), &job.fallback)?;
    let info = job
        .source
        .as_deref()
        .map(zimged_codec::source_info)
        .unwrap_or_default();
    let (pixels, size) = straight(&flattened);
    zimged_codec::save(
        &job.target,
        &pixels,
        size,
        job.format,
        &info,
        job.jpeg_quality,
    )
    .map_err(|e| strings::save_reason(&e))?;
    Ok(Saved { decoded })
}

/// What to copy to the clipboard (§6.2): the same as a save, without a target file.
pub(crate) struct CopyJob {
    pub doc: Document,
    pub source: Option<PathBuf>,
    pub fallback: Option<Arc<egui::ColorImage>>,
    pub clipboard: Arc<dyn zimged_platform::Clipboard>,
}

pub(crate) type PendingCopy = Pending<SaveResult>;

impl PendingCopy {
    /// Flatten, encode as PNG and put on the clipboard, on the worker thread (§7.3).
    pub fn spawn_copy(job: CopyJob) -> Self {
        Self::spawn_job(PathBuf::new(), "worker", move |_| copy(job))
    }
}

fn copy(job: CopyJob) -> SaveResult {
    let (flattened, decoded) = flatten_document(job.doc, job.source.as_deref(), &job.fallback)?;
    let (pixels, size) = straight(&flattened);
    let png =
        zimged_codec::encode_png_plain(&pixels, size).map_err(|e| strings::save_reason(&e))?;
    job.clipboard
        .write(vec![zimged_platform::ClipboardItem::new(
            zimged_platform::clipboard::IMAGE_PNG,
            png,
        )])?;
    Ok(Saved { decoded })
}

/// What copying selected Annotations puts on the clipboard (§6.2).
pub(crate) struct AnnotationsCopyJob {
    pub doc: Document,
    /// The copied Annotations (the selection).
    pub ids: Vec<AnnotationId>,
    pub clipboard: Arc<dyn zimged_platform::Clipboard>,
}

impl PendingCopy {
    /// Serialise the Annotations and render their PNG, on the worker thread (§7.3).
    pub fn spawn_copy_annotations(job: AnnotationsCopyJob) -> Self {
        Self::spawn_job(PathBuf::new(), "worker", move |_| copy_annotations(job))
    }
}

/// How far drawing reaches beyond an Annotation's bounds (line, arrowheads, shadow), at most;
/// the PNG is trimmed to what was drawn afterwards.
fn drawn_margin(style: &zimged_core::Style) -> f32 {
    zimged_core::Arrow::head_length(style.thickness) + style.thickness + 12.0
}

/// Publish the Annotations `job.ids` as clipboard JSON and, together, a PNG of them on
/// transparency (§6.2). The PNG leaves out types that draw from the pixels under them
/// ([`Shape::needs_pixels_below`]: highlight, obfuscate); it is omitted when nothing is left.
fn copy_annotations(job: AnnotationsCopyJob) -> SaveResult {
    let copied: Vec<&Annotation> = job
        .doc
        .annotations()
        .iter()
        .filter(|a| job.ids.contains(&a.id))
        .collect();
    if copied.is_empty() {
        return Err(strings::NOTHING_SELECTED.into());
    }
    let json = zimged_core::clipboard::to_json(copied.iter().copied());
    let in_png: Vec<&Annotation> = copied
        .into_iter()
        .filter(|a| !a.shape.needs_pixels_below())
        .collect();
    let mut items = Vec::new();
    if let Some(png) = annotations_png(&job.doc, &in_png)? {
        // The PNG first: on Windows, setting an image clears the clipboard (see OsClipboard).
        items.push(zimged_platform::ClipboardItem::new(
            zimged_platform::clipboard::IMAGE_PNG,
            png,
        ));
    }
    items.push(zimged_platform::ClipboardItem::new(
        zimged_core::clipboard::MIME,
        json,
    ));
    job.clipboard.write(items)?;
    Ok(Saved { decoded: None })
}

/// `annotations` of `doc` drawn at 100% on transparency, trimmed to what was drawn.
fn annotations_png(doc: &Document, annotations: &[&Annotation]) -> Result<Option<Vec<u8>>, String> {
    let Some(region) = annotations
        .iter()
        .map(|a| a.bounds().expand(drawn_margin(&a.style)))
        .reduce(|a, b| a.union(&b))
    else {
        return Ok(None);
    };
    let min = Point::new(region.min.x.floor(), region.min.y.floor());
    let max = Point::new(region.max.x.ceil(), region.max.y.ceil());
    let region = Rect::from_corners(min, max);
    let view = View {
        region,
        width: region.width() as u32,
        height: region.height() as u32,
    };
    let ids: Vec<AnnotationId> = annotations.iter().map(|a| a.id).collect();
    let composite = Renderer::new().annotations_only(doc, &ids, view);
    let Some(trimmed) = trim_transparent(&composite) else {
        return Ok(None);
    };
    let (pixels, size) = straight(&trimmed);
    zimged_codec::encode_png_plain(&pixels, size)
        .map(Some)
        .map_err(|e| strings::save_reason(&e))
}

/// The composite cropped to its pixels that aren't fully transparent; `None` when all are.
fn trim_transparent(c: &Composite) -> Option<Composite> {
    let (w, h) = (c.width as usize, c.height as usize);
    let alpha = |x: usize, y: usize| c.rgba_premultiplied[(y * w + x) * 4 + 3];
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if alpha(x, y) != 0 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    if x0 >= x1 {
        return None;
    }
    let mut rgba = Vec::with_capacity((x1 - x0) * (y1 - y0) * 4);
    for y in y0..y1 {
        rgba.extend_from_slice(&c.rgba_premultiplied[(y * w + x0) * 4..(y * w + x1) * 4]);
    }
    Some(Composite {
        width: (x1 - x0) as u32,
        height: (y1 - y0) as u32,
        rgba_premultiplied: rgba,
    })
}

/// Flatten `doc` at full resolution, decoding its pixels from `source` (or taking `fallback`)
/// when the Document hasn't got them yet; those decoded pixels come back too.
fn flatten_document(
    mut doc: Document,
    source: Option<&Path>,
    fallback: &Option<Arc<egui::ColorImage>>,
) -> Result<(Composite, Option<Arc<Pixels>>), String> {
    let (full, decoded) = match doc.full_resolution() {
        Some(full) => (full.clone(), None),
        None => {
            let decoded = source
                .ok_or_else(|| strings::NO_PIXELS_TO_SAVE.to_owned())
                .and_then(|source| decode_full(source).map_err(|e| e.to_string()))
                .ok()
                .filter(|p| (p.width, p.height) == (doc.width(), doc.height()));
            let full = match (decoded, fallback) {
                (Some(pixels), _) => Arc::new(pixels),
                (None, Some(fallback)) => Arc::new(from_color_image(fallback)),
                (None, None) => return Err(strings::FILE_CHANGED_OR_GONE.into()),
            };
            (full.clone(), Some(full))
        }
    };
    if (full.width, full.height) != (doc.width(), doc.height()) {
        return Err(strings::FILE_CHANGED.into());
    }
    doc.set_full_resolution(full.clone());
    Ok((flatten(&doc, &full), decoded))
}

/// Flatten (§7.2): the same renderer as the canvas, over the whole Image at full resolution.
fn flatten(doc: &Document, full: &Pixels) -> Composite {
    let source = Source {
        key: 0,
        width: full.width,
        height: full.height,
        pixels: match &full.data {
            PixelData::Rgb(rgb) => SourcePixels::Rgb(rgb),
            PixelData::Rgba(rgba) => SourcePixels::Rgba(rgba),
        },
    };
    Renderer::new().flatten(doc, &source)
}

/// egui's (premultiplied) pixels as full-resolution Image pixels.
fn from_color_image(image: &egui::ColorImage) -> Pixels {
    let [width, height] = image.size;
    let opaque = image.pixels.iter().all(|p| p.a() == 255);
    let data = if opaque {
        PixelData::Rgb(
            image
                .pixels
                .iter()
                .flat_map(|p| [p.r(), p.g(), p.b()])
                .collect(),
        )
    } else {
        PixelData::Rgba(
            image
                .pixels
                .iter()
                .flat_map(|p| p.to_srgba_unmultiplied())
                .collect(),
        )
    };
    Pixels {
        width: width as u32,
        height: height as u32,
        data,
    }
}

/// The composite (premultiplied RGBA) as the encoders take it: RGB when opaque, straight RGBA
/// otherwise.
fn straight(composite: &Composite) -> (zimged_codec::Pixels, Size) {
    let size = Size::new(composite.width, composite.height);
    let premultiplied = &composite.rgba_premultiplied;
    if premultiplied.chunks_exact(4).all(|px| px[3] == 255) {
        let rgb = premultiplied
            .chunks_exact(4)
            .flat_map(|px| [px[0], px[1], px[2]])
            .collect();
        return (zimged_codec::Pixels::Rgb(rgb), size);
    }
    let mut rgba = premultiplied.clone();
    for px in rgba.chunks_exact_mut(4) {
        let a = u32::from(px[3]);
        if a != 0 && a != 255 {
            for c in &mut px[..3] {
                *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    (zimged_codec::Pixels::Rgba(rgba), size)
}

/// Where Ctrl+S writes without asking: the file itself, when zimged writes its format (§5.1).
pub(crate) fn in_place_format(path: &Path) -> Option<SaveFormat> {
    SaveFormat::from_path(path)
}

/// What Save As suggests: the file itself, or the same name as PNG for a read-only format. A
/// clipboard Image (no file) suggests "Clipboard image.png".
pub(crate) fn suggestion(path: Option<&Path>) -> PathBuf {
    let Some(path) = path else {
        return PathBuf::from(format!("{}.png", strings::CLIPBOARD_IMAGE));
    };
    if in_place_format(path).is_some() {
        path.to_owned()
    } else {
        path.with_extension("png")
    }
}

/// The file Save As writes for the name chosen in the dialog, and its format: a name without a
/// writable extension gets `.png` added (the default format).
pub(crate) fn chosen_target(chosen: PathBuf) -> (PathBuf, SaveFormat) {
    match SaveFormat::from_path(&chosen) {
        Some(format) => (chosen, format),
        None => {
            let mut name = chosen.into_os_string();
            name.push(".png");
            (PathBuf::from(name), SaveFormat::Png)
        }
    }
}
