//! The menu bar (§2): File, Edit, Image, View, Help, with the quick actions on its right.
//!
//! Each entry turns into a [`Command`]; the app executes it. So far: the Image menu (rotate, flip,
//! resize, §4.7) with its quick action, and the View menu's *Allow multiple instances* toggle
//! (§8.3). The other menus and quick actions land with their tickets: quick actions go in
//! [`quick_actions`], right to left, so add them before the Image one in the order of §2.

use egui::{Color32, Stroke, vec2};

use crate::Command;

/// The Image menu's quick action (§2): its accessibility label.
pub const LABEL_IMAGE_QUICK_ACTION: &str = "Rotate, flip or resize";

pub const ROTATE_LEFT: &str = "Rotate left";
pub const ROTATE_RIGHT: &str = "Rotate right";
pub const FLIP_HORIZONTAL: &str = "Flip horizontal";
pub const FLIP_VERTICAL: &str = "Flip vertical";
pub const RESIZE: &str = "Resize…";

/// What the menus show this frame.
pub struct MenuBar {
    pub allow_multiple_instances: bool,
    /// Can Image operations run (an editable Image is open)?
    pub can_edit: bool,
}

/// Lay out the menu bar; returns the command chosen this frame, if any.
pub fn show(ui: &mut egui::Ui, bar: &MenuBar) -> Option<Command> {
    let mut command = None;
    egui::Panel::top("menu_bar").show(ui, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("Image", |ui| image_items(ui, bar, &mut command));
            ui.menu_button("View", |ui| {
                let mut allow = bar.allow_multiple_instances;
                if ui
                    .checkbox(&mut allow, "Allow multiple instances")
                    .changed()
                {
                    command = Some(Command::SetAllowMultipleInstances(allow));
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                quick_actions(ui, bar, &mut command);
            });
        });
    });
    command
}

/// The Image menu's entries (§4.7), also behind its quick action.
fn image_items(ui: &mut egui::Ui, bar: &MenuBar, command: &mut Option<Command>) {
    let items = [
        (ROTATE_LEFT, Command::Rotate { clockwise: false }),
        (ROTATE_RIGHT, Command::Rotate { clockwise: true }),
        (FLIP_HORIZONTAL, Command::Flip { horizontal: true }),
        (FLIP_VERTICAL, Command::Flip { horizontal: false }),
        (RESIZE, Command::ShowResizeDialog),
    ];
    for (i, (label, chosen)) in items.into_iter().enumerate() {
        if i == 4 {
            ui.separator();
        }
        if ui
            .add_enabled(bar.can_edit, egui::Button::new(label))
            .clicked()
        {
            *command = Some(chosen);
        }
    }
}

/// The quick actions, laid out right to left.
fn quick_actions(ui: &mut egui::Ui, bar: &MenuBar, command: &mut Option<Command>) {
    // #22's trash and #34's save / save as / copy / undo go around this one (§2).
    let button = egui::Button::new("").min_size(vec2(24.0, 20.0));
    let (response, _) = egui::containers::menu::MenuButton::from_button(button)
        .ui(ui, |ui| image_items(ui, bar, command));
    let response = response
        .on_hover_text("Rotate, flip, resize")
        .accessible_name(LABEL_IMAGE_QUICK_ACTION);
    let colour = ui.visuals().widgets.inactive.fg_stroke.color;
    paint_rotate_icon(ui.painter(), response.rect.shrink2(vec2(6.0, 4.0)), colour);
}

/// A clockwise arrow round a circle (⟳): painted, since the glyph isn't in egui's fonts.
fn paint_rotate_icon(p: &egui::Painter, r: egui::Rect, c: Color32) {
    let s = Stroke::new(1.5, c);
    let centre = r.center();
    let radius = r.width().min(r.height()) / 2.0;
    let start = -std::f32::consts::FRAC_PI_2;
    let end = start + 1.6 * std::f32::consts::PI;
    let points: Vec<_> = (0..=20)
        .map(|i| {
            let a = start + i as f32 / 20.0 * (end - start);
            centre + radius * vec2(a.cos(), a.sin())
        })
        .collect();
    let tip = points[points.len() - 1];
    p.add(egui::Shape::line(points, s));
    // The arrowhead: two strokes back from the tip, either side of the direction of travel.
    let along = vec2(-end.sin(), end.cos());
    let across = vec2(-along.y, along.x);
    p.line_segment([tip, tip - 4.0 * along + 3.0 * across], s);
    p.line_segment([tip, tip - 4.0 * along - 3.0 * across], s);
}
