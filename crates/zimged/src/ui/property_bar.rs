//! The property bar (§2, §4.3): the active tool's controls, or the selection's.
//!
//! One control per [`PropertyKind`], in [`control`]. A new property adds one arm there (and its
//! accessibility label constant). Controls report changes as [`Property`] values; the app turns
//! them into commands.

use zimged_core::{Arrowheads, HighlightMode, Property, PropertyKind, Restack, Rgba, Style};

pub const LABEL_LINE_COLOUR: &str = "Line colour";
pub const LABEL_FILL_COLOUR: &str = "Fill colour";
pub const LABEL_NO_FILL: &str = "No fill";
pub const LABEL_THICKNESS: &str = "Thickness";
pub const LABEL_SHADOW: &str = "Shadow";
pub const LABEL_COUNTER_SIZE: &str = "Size";
pub const LABEL_ARROWHEADS: &str = "Arrowheads";
pub const LABEL_DELETE: &str = "Delete";
pub const LABEL_TEXT_COLOUR: &str = "Text colour";
pub const LABEL_FONT: &str = "Font";
pub const LABEL_FONT_SIZE: &str = "Size";
pub const LABEL_CROP_SIZE: &str = "Crop size";
pub const LABEL_APPLY: &str = "Apply";
pub const LABEL_CANCEL: &str = "Cancel";
pub const LABEL_BRING_TO_FRONT: &str = "Bring to front";
pub const LABEL_SEND_TO_BACK: &str = "Send to back";
pub const LABEL_HIGHLIGHT_COLOUR: &str = "Highlight colour";
pub const LABEL_MAGNIFICATION: &str = "Magnification";

/// The accessibility label of a highlight mode's button: "Text highlight", "Magnify highlight", …
pub fn highlight_mode_label(mode: HighlightMode) -> String {
    format!("{} highlight", mode.name())
}

/// What the property bar shows this frame.
pub struct PropertyBar<'a> {
    pub style: &'a Style,
    pub properties: &'a [PropertyKind],
    /// Annotations are selected: the controls restyle them, and Delete is offered.
    pub selection: bool,
    /// The crop tool's region size, when it has one: shown with Apply and Cancel (§2).
    pub crop: Option<zimged_codec::Size>,
    /// Shown when there are no controls.
    pub hint: &'a str,
    /// The installed font families for the font control (listed on first use of the text tool).
    pub fonts: &'a [String],
}

/// What the user did in the property bar this frame.
#[derive(Default)]
pub struct Output {
    pub changes: Vec<Property>,
    /// The property whose control is still being interacted with (a colour picker open, a value
    /// being dragged): its changes form one History step.
    pub active: Option<PropertyKind>,
    pub delete: bool,
    pub apply_crop: bool,
    pub cancel_crop: bool,
    /// Bring to front or Send to back was clicked.
    pub restack: Option<Restack>,
}

pub fn show(ui: &mut egui::Ui, bar: &PropertyBar<'_>) -> Output {
    let mut out = Output::default();
    egui::Panel::top("property_bar").show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            for kind in bar.properties {
                control(ui, *kind, bar, &mut out);
            }
            if bar.selection && ui.button("Delete").accessible_name(LABEL_DELETE).clicked() {
                out.delete = true;
            }
            if bar.selection {
                for (label, restack) in [
                    (LABEL_BRING_TO_FRONT, Restack::Front),
                    (LABEL_SEND_TO_BACK, Restack::Back),
                ] {
                    if ui.button(label).accessible_name(label).clicked() {
                        out.restack = Some(restack);
                    }
                }
            }
            if let Some(size) = bar.crop {
                ui.label(size.to_string()).accessible_name(LABEL_CROP_SIZE);
                if ui
                    .button("Apply (Enter)")
                    .accessible_name(LABEL_APPLY)
                    .clicked()
                {
                    out.apply_crop = true;
                }
                if ui
                    .button("Cancel (Esc)")
                    .accessible_name(LABEL_CANCEL)
                    .clicked()
                {
                    out.cancel_crop = true;
                }
            } else if bar.properties.is_empty() && !bar.selection {
                ui.weak(bar.hint);
            }
        });
    });
    out
}

fn control(ui: &mut egui::Ui, kind: PropertyKind, bar: &PropertyBar<'_>, out: &mut Output) {
    let style = bar.style;
    match kind {
        PropertyKind::LineColour => {
            ui.label("Line");
            if let Some(c) = colour(ui, style.line, LABEL_LINE_COLOUR, kind, out) {
                out.changes.push(Property::LineColour(c));
            }
        }
        PropertyKind::FillColour => {
            ui.label("Fill");
            let shown = style.fill.unwrap_or(Rgba::WHITE);
            if let Some(c) = colour(ui, shown, LABEL_FILL_COLOUR, kind, out) {
                out.changes.push(Property::FillColour(Some(c)));
            }
            let none = style.fill.is_none();
            if ui
                .add(egui::Button::selectable(none, "None"))
                .on_hover_text("No fill")
                .accessible_name(LABEL_NO_FILL)
                .clicked()
            {
                out.changes
                    .push(Property::FillColour(if none { Some(shown) } else { None }));
            }
        }
        PropertyKind::Thickness => {
            ui.label("Thickness");
            let mut value = style.thickness;
            let range = zimged_core::style::THICKNESS_RANGE;
            let response = ui
                .add(egui::DragValue::new(&mut value).range(range).speed(0.1))
                .accessible_name(LABEL_THICKNESS);
            if response.dragged() || response.has_focus() {
                out.active = Some(kind);
            }
            if response.changed() && value != style.thickness {
                out.changes.push(Property::Thickness(value));
            }
        }
        PropertyKind::CounterSize => {
            ui.label("Size");
            let mut value = style.counter_size;
            let range = zimged_core::style::COUNTER_SIZE_RANGE;
            let response = ui
                .add(egui::DragValue::new(&mut value).range(range).speed(0.5))
                .accessible_name(LABEL_COUNTER_SIZE);
            if response.dragged() || response.has_focus() {
                out.active = Some(kind);
            }
            if response.changed() && value != style.counter_size {
                out.changes.push(Property::CounterSize(value));
            }
        }
        PropertyKind::Shadow => {
            let mut on = style.shadow;
            if ui
                .checkbox(&mut on, "Shadow")
                .accessible_name(LABEL_SHADOW)
                .changed()
            {
                out.changes.push(Property::Shadow(on));
            }
        }
        PropertyKind::TextColour => {
            ui.label("Text");
            if let Some(c) = colour(ui, style.line, LABEL_TEXT_COLOUR, kind, out) {
                out.changes.push(Property::TextColour(c));
            }
        }
        PropertyKind::Font => {
            let mut chosen = None;
            let response = egui::ComboBox::from_id_salt("font")
                .selected_text(style.font.as_str())
                .width(160.0)
                .height(400.0)
                .show_ui(ui, |ui| {
                    for family in bar.fonts {
                        if ui
                            .selectable_label(*family == style.font, family.as_str())
                            .clicked()
                        {
                            chosen = Some(family.clone());
                        }
                    }
                })
                .response;
            response.accessible_name(LABEL_FONT);
            if let Some(family) = chosen.filter(|f| *f != style.font) {
                out.changes.push(Property::Font(family));
            }
        }
        PropertyKind::FontSize => {
            ui.label("Size");
            let mut value = style.font_size;
            let range = zimged_core::style::FONT_SIZE_RANGE;
            let response = ui
                .add(egui::DragValue::new(&mut value).range(range).speed(0.25))
                .accessible_name(LABEL_FONT_SIZE);
            if response.dragged() || response.has_focus() {
                out.active = Some(kind);
            }
            if response.changed() && value != style.font_size {
                out.changes.push(Property::FontSize(value));
            }
        }
        PropertyKind::Arrowheads => {
            ui.label("Arrowheads");
            let mut value = style.arrowheads;
            let _ = egui::ComboBox::from_id_salt(LABEL_ARROWHEADS)
                .selected_text(value.name())
                .show_ui(ui, |ui| {
                    for option in Arrowheads::ALL {
                        ui.selectable_value(&mut value, option, option.name());
                    }
                })
                .response
                .accessible_name(LABEL_ARROWHEADS);
            if value != style.arrowheads {
                out.changes.push(Property::Arrowheads(value));
            }
        }
        PropertyKind::HighlightMode => {
            ui.label("Mode");
            for mode in HighlightMode::ALL {
                let on = style.highlight_mode == mode;
                if ui
                    .add(egui::Button::selectable(on, mode.name()))
                    .accessible_name(highlight_mode_label(mode))
                    .clicked()
                    && !on
                {
                    out.changes.push(Property::HighlightMode(mode));
                }
            }
        }
        // §4.3: the colour applies to text mode only, the magnification to magnify mode only.
        PropertyKind::HighlightColour => {
            if style.highlight_mode == HighlightMode::Text {
                ui.label("Colour");
                if let Some(c) = colour(ui, style.highlight, LABEL_HIGHLIGHT_COLOUR, kind, out) {
                    out.changes.push(Property::HighlightColour(c));
                }
            }
        }
        PropertyKind::Magnification => {
            if style.highlight_mode == HighlightMode::Magnify {
                ui.label("Magnification");
                let mut value = style.magnification;
                let range = zimged_core::style::MAGNIFICATION_RANGE;
                let response = ui
                    .add(egui::DragValue::new(&mut value).range(range).speed(0.05))
                    .accessible_name(LABEL_MAGNIFICATION);
                if response.dragged() || response.has_focus() {
                    out.active = Some(kind);
                }
                if response.changed() && value != style.magnification {
                    out.changes.push(Property::Magnification(value));
                }
            }
        }
    }
}

/// A colour button with a picker. Returns the new colour when it changed.
fn colour(
    ui: &mut egui::Ui,
    c: Rgba,
    label: &str,
    kind: PropertyKind,
    out: &mut Output,
) -> Option<Rgba> {
    // The picker's popup id, as egui's colour button derives it, to tell whether it is open.
    let popup_id = ui.auto_id_with("popup");
    let mut srgba = [c.r, c.g, c.b, c.a];
    let response = ui
        .color_edit_button_srgba_unmultiplied(&mut srgba)
        .accessible_name(label);
    if egui::Popup::is_id_open(ui.ctx(), popup_id) {
        out.active = Some(kind);
    }
    let new = Rgba::new(srgba[0], srgba[1], srgba[2], srgba[3]);
    (response.changed() && new != c).then_some(new)
}
