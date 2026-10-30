//! The Resize dialog (§4.7, §6.4): width and height in pixels or as a percentage, the aspect
//! ratio locked by default; Cancel · **Resize**. Modal: while it is up, the app ignores its own
//! shortcuts and the rest of the window can't be clicked. Esc cancels, Enter resizes.

use egui::{Key, Modifiers};
use zimged_codec::Size;

pub const LABEL_WIDTH: &str = "Width";
pub const LABEL_HEIGHT: &str = "Height";
pub const LABEL_PIXELS: &str = "Pixels";
pub const LABEL_PERCENT: &str = "Percent";
pub const LABEL_LOCK: &str = "Lock aspect ratio";
/// The read-out of the size the Image will have, in pixels (`160×120`).
pub const LABEL_NEW_SIZE: &str = "New size";
pub const LABEL_CANCEL: &str = "Cancel";
pub const LABEL_RESIZE: &str = "Resize";

/// What the dialog's fields are in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Pixels,
    Percent,
}

/// How the dialog ended this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Cancel,
    Resize(Size),
}

/// The dialog's state while it is up.
pub struct ResizeDialog {
    /// The Image's size when the dialog opened: what percentages and the aspect ratio refer to.
    original: Size,
    /// The largest Image allowed (pixels): editing stops at 100 MP (§4.7).
    max_pixels: u64,
    unit: Unit,
    lock: bool,
    width: String,
    height: String,
}

impl ResizeDialog {
    pub fn new(original: Size, max_pixels: u64) -> Self {
        Self {
            original,
            max_pixels,
            unit: Unit::Pixels,
            lock: true,
            width: original.width.to_string(),
            height: original.height.to_string(),
        }
    }

    /// The size the fields ask for, if it is a valid one (at least 1×1, at most `max_pixels`).
    pub fn new_size(&self) -> Option<Size> {
        let w = self.to_pixels(&self.width, self.original.width)?;
        let h = self.to_pixels(&self.height, self.original.height)?;
        (u64::from(w) * u64::from(h) <= self.max_pixels).then_some(Size::new(w, h))
    }

    /// A field's value in pixels, for an Image side of `side` pixels.
    fn to_pixels(&self, text: &str, side: u32) -> Option<u32> {
        let v: f64 = text.trim().trim_end_matches('%').trim().parse().ok()?;
        let px = match self.unit {
            Unit::Pixels => v,
            Unit::Percent => v / 100.0 * f64::from(side),
        }
        .round();
        (1.0..=f64::from(u32::MAX))
            .contains(&px)
            .then_some(px as u32)
    }

    /// With the aspect ratio locked: the other field, following `changed` (true: the width).
    fn follow(&mut self, width_changed: bool) {
        let (from, to, from_side, to_side) = if width_changed {
            (
                &self.width,
                &mut self.height,
                self.original.width,
                self.original.height,
            )
        } else {
            (
                &self.height,
                &mut self.width,
                self.original.height,
                self.original.width,
            )
        };
        let Ok(v) = from.trim().parse::<f64>() else {
            return;
        };
        *to = match self.unit {
            Unit::Percent => from.trim().to_owned(),
            Unit::Pixels => format_pixels(v * f64::from(to_side) / f64::from(from_side)),
        };
    }

    fn set_unit(&mut self, unit: Unit) {
        if unit == self.unit {
            return;
        }
        let convert = |text: &str, side: u32| -> Option<String> {
            let v: f64 = text.trim().parse().ok()?;
            Some(match unit {
                Unit::Percent => format_percent(v / f64::from(side) * 100.0),
                Unit::Pixels => format_pixels(v / 100.0 * f64::from(side)),
            })
        };
        if let Some(w) = convert(&self.width, self.original.width) {
            self.width = w;
        }
        if let Some(h) = convert(&self.height, self.original.height) {
            self.height = h;
        }
        self.unit = unit;
    }

    /// Lay out the dialog; returns how it ended, if it did this frame.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Outcome> {
        let mut outcome = None;
        egui::Modal::new(egui::Id::unique("resize_dialog")).show(ctx, |ui| {
            ui.set_min_width(260.0);
            ui.heading("Resize");
            ui.add_space(6.0);
            egui::Grid::new("resize_fields")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Width");
                    let w = ui
                        .add(egui::TextEdit::singleline(&mut self.width).desired_width(80.0))
                        .accessible_name(LABEL_WIDTH);
                    ui.end_row();
                    ui.label("Height");
                    let h = ui
                        .add(egui::TextEdit::singleline(&mut self.height).desired_width(80.0))
                        .accessible_name(LABEL_HEIGHT);
                    ui.end_row();
                    if self.lock && w.changed() {
                        self.follow(true);
                    } else if self.lock && h.changed() {
                        self.follow(false);
                    }
                });
            ui.horizontal(|ui| {
                let mut unit = self.unit;
                ui.selectable_value(&mut unit, Unit::Pixels, "px")
                    .accessible_name(LABEL_PIXELS);
                ui.selectable_value(&mut unit, Unit::Percent, "%")
                    .accessible_name(LABEL_PERCENT);
                self.set_unit(unit);
            });
            if ui.checkbox(&mut self.lock, LABEL_LOCK).changed() && self.lock {
                self.follow(true);
            }
            let new_size = self.new_size();
            ui.horizontal(|ui| {
                ui.label("New size:");
                let text = new_size.map_or_else(|| "–".to_owned(), |s| s.to_string());
                ui.label(text).accessible_name(LABEL_NEW_SIZE);
            });
            ui.add_space(6.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let resize = egui::Button::new(egui::RichText::new("Resize").strong());
                if ui
                    .add_enabled(new_size.is_some(), resize)
                    .accessible_name(LABEL_RESIZE)
                    .clicked()
                    && let Some(size) = new_size
                {
                    outcome = Some(Outcome::Resize(size));
                }
                if ui.button("Cancel").accessible_name(LABEL_CANCEL).clicked() {
                    outcome = Some(Outcome::Cancel);
                }
            });
            // Esc means Cancel in every dialog (§6.4); Enter is the primary button.
            ui.input_mut(|i| {
                if i.consume_key(Modifiers::NONE, Key::Escape) {
                    outcome = Some(Outcome::Cancel);
                } else if i.key_pressed(Key::Enter)
                    && let Some(size) = new_size
                {
                    outcome = outcome.or(Some(Outcome::Resize(size)));
                }
            });
        });
        outcome
    }
}

fn format_pixels(v: f64) -> String {
    format!("{}", v.round().max(1.0) as u64)
}

/// A percentage with at most two decimals and no trailing zeros (`50`, `33.33`).
fn format_percent(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}
