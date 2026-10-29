//! Input → commands: the keyboard shortcuts (§6.1), zoom (§3.1) and files dropped onto the window
//! (§3.5).
//!
//! Each frame, [`commands`] turns this frame's input into [`Command`]s for
//! [`crate::ZimgedApp::execute`]. Widgets don't act on keys or drops themselves.

use egui::{Key, KeyboardShortcut, Modifiers};

use crate::Command;

const OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);

/// The commands this frame's input asks for, in order. `canvas` is the canvas as of the last frame:
/// Ctrl+wheel only zooms while the pointer is over it.
pub fn commands(ctx: &egui::Context, canvas: Option<egui::Rect>) -> Vec<Command> {
    let mut commands = Vec::new();
    // Plain keys belong to a focused text field (the text tool, a dialog) when there is one.
    let plain_keys = !ctx.egui_wants_keyboard_input();
    ctx.input_mut(|input| {
        if input.consume_shortcut(&OPEN) {
            commands.push(Command::OpenDialog);
        }
        if plain_keys {
            // `+` is Shift+= on many layouts; `=` alone also zooms in, as in most viewers.
            if input.consume_key(Modifiers::NONE, Key::Plus)
                || input.consume_key(Modifiers::NONE, Key::Equals)
            {
                commands.push(Command::ZoomIn);
            }
            if input.consume_key(Modifiers::NONE, Key::Minus) {
                commands.push(Command::ZoomOut);
            }
            if input.consume_key(Modifiers::NONE, Key::Num0) {
                commands.push(Command::ZoomFit);
            }
            if input.consume_key(Modifiers::NONE, Key::F11) {
                commands.push(Command::ToggleFullscreen);
            }
        }
        // Ctrl+wheel (egui's zoom modifier) and touchpad pinch.
        let factor = input.zoom_delta();
        if factor != 1.0
            && let (Some(around), Some(canvas)) = (input.pointer.hover_pos(), canvas)
            && canvas.contains(around)
        {
            commands.push(Command::ZoomAround { factor, around });
        }
        // Several files dropped at once: only the first is opened, as the new Image.
        let dropped = input.raw.dropped_files.first().map(|f| f.path().to_owned());
        if let Some(path) = dropped {
            // #36: Ctrl+drop (`input.modifiers.command`) inserts the file as an Inset instead
            // (§4.6). Until Insets exist, it opens the file like a plain drop.
            commands.push(Command::Open(path));
        }
    });
    commands
}
