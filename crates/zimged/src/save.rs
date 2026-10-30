//! Save (§5.1, §5.3, §7.2): Flatten the Document and write it, on the worker thread (§7.3).
//!
//! The UI hands the worker a copy of what is drawn ([`zimged_core::Document::drawing`]), so editing
//! can go on meanwhile. The worker decodes the full-resolution pixels itself when editing hasn't
//! (they come back, so the Document keeps them), Flattens with the same Skia code as the screen,
//! encodes, and writes atomically ([`zimged_codec::save`]).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use zimged_codec::{SaveFormat, Size};
use zimged_core::{Document, PixelData, Pixels};
use zimged_render::{Composite, Renderer, Source, SourcePixels};

use crate::decode::{Pending, decode_full};

/// What to save, and where.
pub(crate) struct SaveJob {
    /// What is drawn: the size, the Annotations and (when decoded) the full-resolution pixels.
    pub drawing: Document,
    /// The file the Image came from: its pixels (when not yet decoded) and metadata.
    pub source: Option<PathBuf>,
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
    let (full, decoded) = match job.drawing.full_resolution() {
        Some(full) => (full.clone(), None),
        None => {
            let source = job.source.as_deref().ok_or("no pixels to save")?;
            let full = Arc::new(decode_full(source).map_err(|e| e.to_string())?);
            (full.clone(), Some(full))
        }
    };
    if (full.width, full.height) != (job.drawing.width(), job.drawing.height()) {
        return Err("the file changed on disk".into());
    }
    let mut drawing = job.drawing;
    drawing.set_full_resolution(full.clone());
    let flattened = flatten(&drawing, &full);
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
    .map_err(|e| e.to_string())?;
    Ok(Saved { decoded })
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

/// What Save As suggests: the file itself, or the same name as PNG for a read-only format.
pub(crate) fn suggestion(path: &Path) -> PathBuf {
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
