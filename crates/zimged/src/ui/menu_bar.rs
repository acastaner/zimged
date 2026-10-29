//! The menu bar (§2): File, Edit, Image, View, Help, with the quick actions on its right.
//!
//! Each entry turns into a [`Command`]; the app executes it. Only the View menu's *Allow multiple
//! instances* toggle (§8.3) exists so far; the other menus and the quick actions land with their
//! tickets.

use crate::Command;

/// What the menus show this frame.
pub struct MenuBar {
    pub allow_multiple_instances: bool,
}

/// Lay out the menu bar; returns the command chosen this frame, if any.
pub fn show(ui: &mut egui::Ui, bar: &MenuBar) -> Option<Command> {
    let mut command = None;
    egui::Panel::top("menu_bar").show(ui, |ui| {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("View", |ui| {
                let mut allow = bar.allow_multiple_instances;
                if ui
                    .checkbox(&mut allow, "Allow multiple instances")
                    .changed()
                {
                    command = Some(Command::SetAllowMultipleInstances(allow));
                }
            });
        });
    });
    command
}
