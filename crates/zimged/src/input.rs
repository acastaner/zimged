//! Input → commands: the keyboard shortcuts (§6.1), Sibling navigation and zoom (§3.1) and files
//! dropped onto the window (§3.5).
//!
//! Each frame, [`commands`] turns this frame's input into [`Command`]s for
//! [`crate::ZimgedApp::execute`]. Widgets don't act on keys or drops themselves.

use egui::{Event, Key, KeyboardShortcut, Modifiers, MouseWheelUnit};

use crate::Command;
use crate::tools::ToolId;

const OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
const SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);
const SAVE_AS: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::S);

/// Points of smooth (touchpad) scrolling that count as one wheel notch.
const POINTS_PER_STEP: f32 = 50.0;

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
        // Save As first: egui ignores an extra Shift when matching Ctrl+S.
        if input.consume_shortcut(&SAVE_AS) {
            commands.push(Command::SaveAs);
        }
        if input.consume_shortcut(&SAVE) {
            commands.push(Command::Save);
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
            // → / ← move through the Siblings.
            if input.consume_key(Modifiers::NONE, Key::ArrowRight) {
                commands.push(Command::NextSibling);
            }
            if input.consume_key(Modifiers::NONE, Key::ArrowLeft) {
                commands.push(Command::PreviousSibling);
            }
            editor_keys(input, &mut commands);
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
    wheel(ctx, &mut commands);
    commands
}

/// The editor's keys: History (§4.8), Delete (§4.2) and the tool keys (§4.1). Match the most
/// specific modifiers first: egui ignores extra Shift and Alt when matching.
fn editor_keys(input: &mut egui::InputState, commands: &mut Vec<Command>) {
    if input.consume_key(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)
        || input.consume_key(Modifiers::COMMAND, Key::Y)
    {
        commands.push(Command::Redo);
    }
    if input.consume_key(Modifiers::COMMAND, Key::Z) {
        commands.push(Command::Undo);
    }
    if input.consume_key(Modifiers::NONE, Key::Delete) {
        commands.push(Command::Delete);
    }
    if input.consume_key(Modifiers::NONE, Key::Enter) {
        commands.push(Command::ApplyCrop);
    }
    for tool in ToolId::ALL {
        if input.consume_key(Modifiers::NONE, tool.key()) {
            commands.push(Command::SelectTool(tool));
        }
    }
}

/// §3.1: wheel down is the next Sibling, wheel up the previous one. Ctrl+wheel zooms instead
/// (above) and Shift+wheel pans ([`crate::pan`], #43).
fn wheel(ctx: &egui::Context, commands: &mut Vec<Command>) {
    let id = egui::Id::unique("zimged_wheel_points");
    let mut points: f32 = ctx.data(|d| d.get_temp(id)).unwrap_or(0.0);
    ctx.input(|input| {
        for event in &input.events {
            let Event::MouseWheel {
                unit,
                delta,
                modifiers,
                ..
            } = event
            else {
                continue;
            };
            if modifiers.ctrl || modifiers.command || modifiers.shift || delta.y == 0.0 {
                continue;
            }
            // A positive y moves the content down: the wheel turned up.
            let steps = match unit {
                MouseWheelUnit::Line | MouseWheelUnit::Page => -delta.y.signum() as i32,
                MouseWheelUnit::Point => {
                    points += delta.y;
                    let steps = -(points / POINTS_PER_STEP).trunc() as i32;
                    points %= POINTS_PER_STEP;
                    steps
                }
            };
            let command = if steps > 0 {
                Command::NextSibling
            } else {
                Command::PreviousSibling
            };
            commands.extend(std::iter::repeat_n(command, steps.unsigned_abs() as usize));
        }
    });
    ctx.data_mut(|d| d.insert_temp(id, points));
}
