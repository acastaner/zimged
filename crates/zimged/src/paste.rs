//! Images coming in to be pasted (§6.3) or Ctrl+dropped (§4.6): read and decoded off the UI thread
//! (another application serves the clipboard's data, and a large image takes a while to decode).
//!
//! #37 adds zimged Annotations: their MIME type goes before [`IMAGE_FORMATS`] in the formats
//! [`PendingIncoming::paste`] asks the clipboard for, and they get a variant in [`Incoming`].

use std::path::PathBuf;
use std::sync::Arc;

use zimged_codec::{DisplayImage, Size};
use zimged_core::{PixelData, Pixels};
use zimged_platform::Clipboard;
use zimged_platform::clipboard::IMAGE_FORMATS;

use crate::decode::Pending;

/// What came in.
pub(crate) enum Incoming {
    /// An image at full resolution (oriented, in sRGB), and its display-size pixels when asked for
    /// (it becomes the Image).
    Image {
        full: Pixels,
        display: Option<DisplayImage>,
    },
    /// The clipboard holds nothing zimged can paste.
    Nothing,
}

/// Where an incoming image goes, decided when it was asked for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Destination {
    /// Into the open Image, as an Inset.
    Inset,
    /// The empty window: it becomes the Image (§3.5).
    Window,
}

/// An incoming image in flight. The error is the status-bar reason.
pub(crate) struct PendingIncoming {
    pub pending: Pending<Result<Incoming, String>>,
    pub destination: Destination,
    /// The file Ctrl+dropped; `None` for a paste.
    pub file: Option<PathBuf>,
}

impl PendingIncoming {
    /// Read the clipboard and decode the image on it. `display_box`: the size display pixels are
    /// made for, when it becomes the Image.
    pub fn paste(
        clipboard: Arc<dyn Clipboard>,
        destination: Destination,
        display_box: Size,
    ) -> Self {
        let pending = Pending::spawn_job(PathBuf::new(), "paste", move |_| {
            let Some(item) = clipboard.read(IMAGE_FORMATS)? else {
                return Ok(Incoming::Nothing);
            };
            match zimged_codec::decode_bytes(&item.data) {
                Ok(decoded) => Ok(image(decoded, destination, display_box)),
                // An image format zimged can't decode is still nothing to paste.
                Err(_) => Ok(Incoming::Nothing),
            }
        });
        Self {
            pending,
            destination,
            file: None,
        }
    }

    /// Decode a Ctrl+dropped file.
    pub fn file(path: PathBuf, destination: Destination, display_box: Size) -> Self {
        let pending = Pending::spawn_job(path.clone(), "decode", move |path| {
            let decoded = zimged_codec::decode(path).map_err(|e| e.to_string())?;
            Ok(image(decoded, destination, display_box))
        });
        Self {
            pending,
            destination,
            file: Some(path),
        }
    }
}

fn image(decoded: zimged_codec::Decoded, destination: Destination, display_box: Size) -> Incoming {
    let size = decoded.size();
    let full = decoded.into_display(size);
    let display = (destination == Destination::Window).then(|| full.reduced(display_box));
    Incoming::Image {
        full: Pixels {
            width: full.size.width,
            height: full.size.height,
            data: match full.pixels {
                zimged_codec::Pixels::Rgb(rgb) => PixelData::Rgb(rgb),
                zimged_codec::Pixels::Rgba(rgba) => PixelData::Rgba(rgba),
            },
        },
        display,
    }
}
