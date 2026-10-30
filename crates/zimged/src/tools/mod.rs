//! The editor's tools (§4.1): the tool strip's entries and what each does with the pointer.
//!
//! Adding a tool:
//! 1. a module here with a type implementing [`Tool`] (for a drag-a-box shape, reuse
//!    [`drag_box::DragBox`] as `rectangle.rs` does),
//! 2. one arm in [`ToolId::create`] (that is what enables its button and key),
//! 3. its §4.3 property list in [`ToolId::properties`],
//! 4. for a tool that creates a new Annotation type, an arm in [`ToolId::for_shape`].
//!
//! Tools work in Image pixels ([`zimged_core::Point`]); the canvas maps the pointer for them.

mod crop;
mod drag_box;
mod drag_segment;
mod ellipse;
mod freehand;
mod rectangle;
pub mod select;
mod step_counter;
pub mod text;

use zimged_core::{
    AnnotationShape, Arrow, Document, Ellipse, Freehand, Line, Point, PropertyKind, Rect,
    Rectangle, StepCounter, Style, Text,
};

/// Every tool, in tool-strip order (§4.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolId {
    Select,
    Rectangle,
    Ellipse,
    Line,
    Arrow,
    Freehand,
    Text,
    StepCounter,
    Highlight,
    Obfuscate,
    Crop,
}

impl ToolId {
    /// §4.1 order: the tool strip shows them in this order.
    pub const ALL: [ToolId; 11] = [
        ToolId::Select,
        ToolId::Rectangle,
        ToolId::Ellipse,
        ToolId::Line,
        ToolId::Arrow,
        ToolId::Freehand,
        ToolId::Text,
        ToolId::StepCounter,
        ToolId::Highlight,
        ToolId::Obfuscate,
        ToolId::Crop,
    ];

    /// The tool's name: its button's accessibility label.
    pub fn name(self) -> &'static str {
        match self {
            ToolId::Select => "Select",
            ToolId::Rectangle => "Rectangle",
            ToolId::Ellipse => "Ellipse",
            ToolId::Line => "Line",
            ToolId::Arrow => "Arrow",
            ToolId::Freehand => "Freehand",
            ToolId::Text => "Text",
            ToolId::StepCounter => "Step counter",
            ToolId::Highlight => "Highlight",
            ToolId::Obfuscate => "Obfuscate",
            ToolId::Crop => "Crop",
        }
    }

    /// The tool's key under `tools` in `config.toml` (§8.3): `tools.rectangle`, ….
    pub fn config_key(self) -> &'static str {
        match self {
            ToolId::Select => "select",
            ToolId::Rectangle => "rectangle",
            ToolId::Ellipse => "ellipse",
            ToolId::Line => "line",
            ToolId::Arrow => "arrow",
            ToolId::Freehand => "freehand",
            ToolId::Text => "text",
            ToolId::StepCounter => "step_counter",
            ToolId::Highlight => "highlight",
            ToolId::Obfuscate => "obfuscate",
            ToolId::Crop => "crop",
        }
    }

    /// The tool that creates this kind of Annotation: restyling an Annotation also sets that
    /// tool's default (§4.3). A new Annotation type adds an arm here.
    pub fn for_shape(shape: &AnnotationShape) -> ToolId {
        match shape {
            AnnotationShape::Rectangle(_) => ToolId::Rectangle,
            AnnotationShape::Ellipse(_) => ToolId::Ellipse,
            AnnotationShape::Line(_) => ToolId::Line,
            AnnotationShape::Arrow(_) => ToolId::Arrow,
            AnnotationShape::Freehand(_) => ToolId::Freehand,
            AnnotationShape::Text(_) => ToolId::Text,
            AnnotationShape::StepCounter(_) => ToolId::StepCounter,
        }
    }

    /// The single-key shortcut (Greenshot's).
    pub fn key(self) -> egui::Key {
        use egui::Key;
        match self {
            ToolId::Select => Key::Escape,
            ToolId::Rectangle => Key::R,
            ToolId::Ellipse => Key::E,
            ToolId::Line => Key::L,
            ToolId::Arrow => Key::A,
            ToolId::Freehand => Key::F,
            ToolId::Text => Key::T,
            ToolId::StepCounter => Key::I,
            ToolId::Highlight => Key::H,
            ToolId::Obfuscate => Key::O,
            ToolId::Crop => Key::C,
        }
    }

    /// The tool's behaviour, or `None` while it isn't built yet (its button is disabled).
    pub fn create(self) -> Option<Box<dyn Tool>> {
        match self {
            ToolId::Select => Some(Box::new(select::Select::default())),
            ToolId::Rectangle => Some(Box::new(rectangle::new())),
            ToolId::StepCounter => Some(Box::new(step_counter::PlaceCounter::default())),
            ToolId::Text => Some(Box::new(text::TextTool)),
            ToolId::Ellipse => Some(Box::new(ellipse::new())),
            ToolId::Line => Some(Box::new(drag_segment::line())),
            ToolId::Arrow => Some(Box::new(drag_segment::arrow())),
            ToolId::Freehand => Some(Box::new(freehand::FreehandTool::default())),
            ToolId::Crop => Some(Box::new(crop::Crop::default())),
            _ => None,
        }
    }

    pub fn is_available(self) -> bool {
        self.create().is_some()
    }

    /// The property-bar controls for this tool's defaults when nothing is selected (§4.3).
    pub fn properties(self) -> &'static [PropertyKind] {
        match self {
            ToolId::Rectangle => Rectangle::PROPERTIES,
            ToolId::StepCounter => StepCounter::PROPERTIES,
            ToolId::Text => Text::PROPERTIES,
            ToolId::Ellipse => Ellipse::PROPERTIES,
            ToolId::Line => Line::PROPERTIES,
            ToolId::Arrow => Arrow::PROPERTIES,
            ToolId::Freehand => Freehand::PROPERTIES,
            _ => &[],
        }
    }

    /// The tool's style before the user changes any of its properties (§4.3, §8.3).
    pub fn default_style(self) -> Style {
        match self {
            ToolId::StepCounter => step_counter::default_style(),
            _ => Style::default(),
        }
    }

    /// The property bar's hint when the tool has no controls to show.
    pub fn hint(self) -> &'static str {
        match self {
            ToolId::Select => "Click an Annotation to select it",
            ToolId::Crop => "Drag the region to keep",
            _ => "",
        }
    }
}

/// The pointer, in Image pixels.
#[derive(Clone, Copy, Debug)]
pub struct Pointer {
    pub pos: Point,
    pub modifiers: egui::Modifiers,
}

/// What a tool gets to work with during a pointer event.
pub struct ToolContext<'a> {
    pub doc: &'a mut Document,
    /// This tool's current style (its defaults, §4.3), for the Annotations it creates.
    pub style: &'a Style,
    /// Image pixels per screen point: converts on-screen sizes (handles, click slop) to Image
    /// pixels.
    pub image_per_point: f32,
    /// The crop tool's region (§4.1 C), in Image pixels. Not a Document edit until applied.
    pub crop: &'a mut Option<Rect>,
}

/// A tool's pointer handling. The canvas calls `press`, then `drag` for each move while the
/// button is held, then `release`. A click without movement is `press` then `release`.
///
/// A tool that edits opens a gesture on the Document in `press` and commits it in `release`, so
/// one drag is one History step (§4.8).
pub trait Tool {
    fn press(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer);
    fn drag(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer);
    fn release(&mut self, cx: &mut ToolContext<'_>, pointer: Pointer);

    /// The rubber band being dragged out, in Image pixels (the select tool).
    fn rubber_band(&self) -> Option<zimged_core::Rect> {
        None
    }

    /// Abandon an unfinished gesture (the tool is being switched away from mid-drag).
    fn cancel(&mut self, cx: &mut ToolContext<'_>) {
        if cx.doc.in_gesture() {
            cx.doc.cancel_gesture();
        }
    }
}
