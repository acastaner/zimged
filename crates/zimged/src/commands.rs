//! Commands: what user input (keys, menus, drops, the launch argument) turns into.
//!
//! Input handling maps events to a [`Command`]; [`crate::ZimgedApp::execute`] carries it out. Tests
//! send the same commands, or raw input through the harness. Add a variant per new action.

use std::path::PathBuf;

use zimged_core::{Property, Restack};

use crate::tools::ToolId;

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Open this file as the Image (§3.5).
    Open(PathBuf),
    /// Make this the active tool (§4.1). Ignored for unavailable tools and view-only Images.
    SelectTool(ToolId),
    /// Ctrl+Z (§4.8).
    Undo,
    /// Ctrl+Shift+Z or Ctrl+Y.
    Redo,
    /// Delete (§3.1, §4.2): remove the selected Annotations; with nothing selected, move the file
    /// to the system trash and show the next Sibling (#22).
    Delete,
    /// A property-bar change (§4.3): restyles the selection, or sets the active tool's default
    /// when nothing is selected. `merge` folds it into the previous step when that changed the same
    /// property on the same Annotations (one widget interaction is one step).
    SetProperty { property: Property, merge: bool },
    /// Ctrl+D (§4.2): duplicate the selected Annotations, offset by 10 px, without touching the
    /// clipboard.
    Duplicate,
    /// Bring to front / Send to back (property bar), Ctrl+] / Ctrl+[ (§4.2).
    Restack(Restack),
    /// Show the next Sibling (§3.1: wheel down, →).
    NextSibling,
    /// Show the previous Sibling (§3.1: wheel up, ←).
    PreviousSibling,
    /// Show the native open dialog and open the file chosen in it (Ctrl+O, §3.5).
    OpenDialog,
    /// Zoom in one step around the canvas centre (`+`, §3.1).
    ZoomIn,
    /// Zoom out one step around the canvas centre (`-`, §3.1).
    ZoomOut,
    /// Multiply the zoom by `factor`, keeping the Image point under `around` (in points) in place
    /// (Ctrl+wheel, §3.1).
    ZoomAround { factor: f32, around: egui::Pos2 },
    /// Move a zoomed Image by this many points (Space+drag, middle-drag, Shift+wheel; #43). A
    /// view change only: not an undo step, and the Image stays saved.
    Pan(egui::Vec2),
    /// Back to fit-to-window (`0`, §3.1).
    ZoomFit,
    /// Enter or leave fullscreen (F11, §2).
    ToggleFullscreen,
    /// View → *Allow multiple instances* (§8.3, §8.4). Persisted; takes effect at the next start.
    SetAllowMultipleInstances(bool),
}
