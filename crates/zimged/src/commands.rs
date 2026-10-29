//! Commands: what user input (keys, menus, drops, the launch argument) turns into.
//!
//! Input handling maps events to a [`Command`]; [`crate::ZimgedApp::execute`] carries it out. Tests
//! send the same commands, or raw input through the harness. Add a variant per new action.

use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Open this file as the Image (§3.5).
    Open(PathBuf),
    /// Zoom in one step around the canvas centre (`+`, §3.1).
    ZoomIn,
    /// Zoom out one step around the canvas centre (`-`, §3.1).
    ZoomOut,
    /// Multiply the zoom by `factor`, keeping the Image point under `around` (in points) in place
    /// (Ctrl+wheel, §3.1).
    ZoomAround { factor: f32, around: egui::Pos2 },
    /// Back to fit-to-window (`0`, §3.1).
    ZoomFit,
    /// Enter or leave fullscreen (F11, §2).
    ToggleFullscreen,
}
