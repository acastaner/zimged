//! Commands: what user input (keys, menus, drops, the launch argument) turns into.
//!
//! Input handling maps events to a [`Command`]; [`crate::ZimgedApp::execute`] carries it out. Tests
//! send the same commands, or raw input through the harness. Add a variant per new action.

use std::path::PathBuf;

use zimged_core::Property;

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
    /// Delete (§4.2): remove the selected Annotations. With nothing selected it trashes the file
    /// (#22).
    Delete,
    /// A property-bar change (§4.3): restyles the selection, or sets the active tool's default
    /// when nothing is selected. `merge` folds it into the previous step when that changed the same
    /// property on the same Annotations (one widget interaction is one step).
    SetProperty { property: Property, merge: bool },
}
