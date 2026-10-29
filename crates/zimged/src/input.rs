//! Input → commands. Keys, the wheel and drops are mapped to a [`Command`] here, at the top of
//! each frame, never inside widgets.

use egui::{Event, Key, Modifiers, MouseWheelUnit};

use crate::Command;

/// Points of smooth (touchpad) scrolling that count as one wheel notch.
const POINTS_PER_STEP: f32 = 50.0;

/// The commands this frame's input asks for, in order.
pub fn commands(ctx: &egui::Context) -> Vec<Command> {
    let mut commands = Vec::new();
    ctx.input_mut(|input| {
        // §3.1: → / ← move through the Siblings.
        if input.consume_key(Modifiers::NONE, Key::ArrowRight) {
            commands.push(Command::NextSibling);
        }
        if input.consume_key(Modifiers::NONE, Key::ArrowLeft) {
            commands.push(Command::PreviousSibling);
        }
    });
    wheel(ctx, &mut commands);
    commands
}

/// §3.1: wheel down is the next Sibling, wheel up the previous one. Ctrl+wheel is zoom, not here.
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
            if modifiers.ctrl || modifiers.command || delta.y == 0.0 {
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
