//! Commands: what user input (keys, menus, drops, the launch argument) turns into.
//!
//! Input handling maps events to a [`Command`]; [`crate::ZimgedApp::execute`] carries it out. Tests
//! send the same commands, or raw input through the harness. Add a variant per new action.

use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Open this file as the Image (§3.5).
    Open(PathBuf),
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
