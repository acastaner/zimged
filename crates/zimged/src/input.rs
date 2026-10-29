//! Keyboard input → [`Command`]s (§6.1, §4.1). Pointer input on the canvas goes to the active
//! tool instead (see `editor.rs`).
//!
//! Add a shortcut here as a `consume_key` producing a command; match the most specific modifier
//! combination first (egui ignores extra Shift and Alt when matching).

use egui::{Key, Modifiers};

use crate::Command;
use crate::tools::ToolId;

/// The commands for this frame's key presses. Nothing while a text field has keyboard focus.
pub fn keyboard_commands(ctx: &egui::Context) -> Vec<Command> {
    if ctx.egui_wants_keyboard_input() {
        return Vec::new();
    }
    ctx.input_mut(|i| {
        let mut commands = Vec::new();
        if i.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
            || i.consume_key(Modifiers::COMMAND, Key::Y)
        {
            commands.push(Command::Redo);
        }
        if i.consume_key(Modifiers::COMMAND, Key::Z) {
            commands.push(Command::Undo);
        }
        if i.consume_key(Modifiers::NONE, Key::Delete) {
            commands.push(Command::Delete);
        }
        for tool in ToolId::ALL {
            if i.consume_key(Modifiers::NONE, tool.key()) {
                commands.push(Command::SelectTool(tool));
            }
        }
        commands
    })
}
