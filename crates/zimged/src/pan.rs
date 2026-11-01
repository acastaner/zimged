//! Panning a zoomed Image (#43): Space+left-drag, middle-drag and Shift+wheel.
//!
//! Self-contained so it composes with the editor tools: a left-drag on the canvas belongs to the
//! active tool unless Space is held, and middle-drag is always a pan. Tools ask
//! [`PanGesture::owns_pointer`] before acting on the pointer. Plain wheel is left alone (Sibling
//! navigation, §3.1); Ctrl+wheel is zoom ([`crate::input`]).

use egui::{Key, PointerButton};

use crate::Command;

/// The drag in progress, if any. Lives across frames, so it is held by the app.
#[derive(Default)]
pub struct PanGesture {
    /// The button of the pan drag in progress.
    dragging: Option<PointerButton>,
}

impl PanGesture {
    /// The pan this frame's input asks for. `canvas` is the canvas as of the last frame: a pan drag
    /// only starts on it, and Shift+wheel only pans over it.
    pub fn commands(&mut self, ctx: &egui::Context, canvas: Option<egui::Rect>) -> Vec<Command> {
        let space = space_held(ctx);
        let line = ctx.options(|o| o.input_options.line_scroll_speed);
        let mut commands = Vec::new();
        ctx.input(|input| {
            let pointer = &input.pointer;
            if let Some(button) = self.dragging {
                if !pointer.button_down(button) {
                    self.dragging = None;
                }
            } else if let (Some(origin), Some(canvas)) = (pointer.press_origin(), canvas)
                && canvas.contains(origin)
            {
                if pointer.button_pressed(PointerButton::Middle) {
                    self.dragging = Some(PointerButton::Middle);
                } else if space && pointer.button_pressed(PointerButton::Primary) {
                    self.dragging = Some(PointerButton::Primary);
                }
            }
            if self.dragging.is_some() && pointer.delta() != egui::Vec2::ZERO {
                commands.push(Command::Pan(pointer.delta()));
            }

            // Shift+wheel scrolls horizontally. Raw events, so the step isn't smoothed over frames
            // and the plain wheel stays untouched for Sibling navigation.
            let over_canvas = matches!(
                (input.pointer.hover_pos(), canvas),
                (Some(at), Some(canvas)) if canvas.contains(at)
            );
            if !over_canvas {
                return;
            }
            for event in &input.raw.events {
                if let egui::Event::MouseWheel {
                    unit,
                    delta,
                    modifiers,
                    ..
                } = event
                    && modifiers.shift_only()
                {
                    let step = match unit {
                        egui::MouseWheelUnit::Point => 1.0,
                        egui::MouseWheelUnit::Line => line,
                        egui::MouseWheelUnit::Page => canvas.map_or(0.0, |c| c.width()),
                    };
                    // Some platforms already turn Shift+wheel into a horizontal delta.
                    let dx = (delta.x + delta.y) * step;
                    if dx != 0.0 {
                        commands.push(Command::Pan(egui::vec2(dx, 0.0)));
                    }
                }
            }
        });
        commands
    }

    /// Does the pointer belong to panning (a pan drag in progress, or Space held so a left-drag
    /// would start one)? The active tool leaves the pointer alone while it does.
    pub fn owns_pointer(&self, ctx: &egui::Context) -> bool {
        self.dragging.is_some() || space_held(ctx)
    }

    /// Is a pan drag actually under way (Space+left or middle button down)? The cursor shows the
    /// closed grabbing hand while it is (#48).
    pub fn is_dragging(&self) -> bool {
        self.dragging.is_some()
    }
}

/// Space is the pan modifier, except while a text field (the text tool, a dialog) has the keyboard.
fn space_held(ctx: &egui::Context) -> bool {
    !ctx.egui_wants_keyboard_input() && ctx.input(|i| i.key_down(Key::Space))
}
