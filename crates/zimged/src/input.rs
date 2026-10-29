//! Input → commands: the keyboard shortcuts (§6.1) and files dropped onto the window (§3.5).
//!
//! Each frame, [`commands`] turns this frame's input into [`Command`]s for
//! [`crate::ZimgedApp::execute`]. Widgets don't act on keys or drops themselves.

use egui::{Key, KeyboardShortcut, Modifiers};

use crate::Command;

const OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);

/// The commands this frame's input asks for, in order.
pub fn commands(ctx: &egui::Context) -> Vec<Command> {
    let mut commands = Vec::new();
    ctx.input_mut(|input| {
        if input.consume_shortcut(&OPEN) {
            commands.push(Command::OpenDialog);
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
