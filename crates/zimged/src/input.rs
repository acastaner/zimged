//! Raw input → [`Command`]s (§3.1). Widgets don't act on keys themselves.

use egui::{Key, Modifiers};

use crate::Command;

/// The commands this frame's input asks for. `canvas` is the canvas as of the last frame: Ctrl+wheel
/// only zooms while the pointer is over it.
pub fn commands(ctx: &egui::Context, canvas: Option<egui::Rect>) -> Vec<Command> {
    let mut out = Vec::new();
    if ctx.egui_wants_keyboard_input() {
        return out;
    }
    ctx.input_mut(|i| {
        // `+` is Shift+= on many layouts; `=` alone also zooms in, as in most viewers.
        if i.consume_key(Modifiers::NONE, Key::Plus) || i.consume_key(Modifiers::NONE, Key::Equals)
        {
            out.push(Command::ZoomIn);
        }
        if i.consume_key(Modifiers::NONE, Key::Minus) {
            out.push(Command::ZoomOut);
        }
        if i.consume_key(Modifiers::NONE, Key::Num0) {
            out.push(Command::ZoomFit);
        }
        if i.consume_key(Modifiers::NONE, Key::F11) {
            out.push(Command::ToggleFullscreen);
        }
        // Ctrl+wheel (egui's zoom modifier) and touchpad pinch.
        let factor = i.zoom_delta();
        if factor != 1.0
            && let (Some(around), Some(canvas)) = (i.pointer.hover_pos(), canvas)
            && canvas.contains(around)
        {
            out.push(Command::ZoomAround { factor, around });
        }
    });
    out
}
