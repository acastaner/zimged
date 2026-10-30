//! Saving an animation over its own file (§3.4, §6.4): "Overwrite animated image?" / "Saving will
//! replace the animated file <name> with a still image. All frames except the first will be lost.",
//! with Cancel · Save As… · **Overwrite**. Modal; Esc is Cancel.

pub const TITLE: &str = "Overwrite animated image?";

/// The button the user picked, if any this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Answer {
    Cancel,
    SaveAs,
    Overwrite,
}

pub fn show(ctx: &egui::Context, file_name: &str) -> Option<Answer> {
    let mut answer = None;
    egui::Modal::new(egui::Id::unique("zimged_overwrite_animation")).show(ctx, |ui| {
        ui.set_max_width(360.0);
        ui.heading(TITLE);
        ui.label(format!(
            "Saving will replace the animated file {file_name} with a still image. \
             All frames except the first will be lost."
        ));
        ui.add_space(8.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Right to left: the primary button (bold) ends up last, on the right.
            if ui
                .button(egui::RichText::new("Overwrite").strong())
                .clicked()
            {
                answer = Some(Answer::Overwrite);
            }
            if ui.button("Save As…").clicked() {
                answer = Some(Answer::SaveAs);
            }
            if ui.button("Cancel").clicked() {
                answer = Some(Answer::Cancel);
            }
        });
    });
    // Esc means Cancel in every dialog. A click on the backdrop does nothing: the dialog is modal.
    if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
        answer = Some(Answer::Cancel);
    }
    answer
}
