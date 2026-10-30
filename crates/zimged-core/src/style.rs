//! Style properties (§4.3): what the property bar edits.
//!
//! One [`Style`] struct holds every property any Annotation type can have; each type lists the
//! ones it uses ([`crate::Shape::properties`]), and the property bar shows exactly those.
//!
//! Adding a property (arrowheads, font, size, mode, strength, …):
//! 1. a field on [`Style`] with its default in `Style::default`,
//! 2. a variant in [`PropertyKind`] and in [`Property`], and an arm in `Style::get`/`Style::set`,
//! 3. a control in the `zimged` crate's `ui/property_bar.rs`,
//! 4. the kind in the `properties()` list of the Annotation types (and tools) that use it.

/// A colour, 8-bit sRGB with straight (not premultiplied) alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const RED: Rgba = Rgba::rgb(255, 0, 0);
    pub const WHITE: Rgba = Rgba::rgb(255, 255, 255);
    pub const BLACK: Rgba = Rgba::rgb(0, 0, 0);
}

/// Every style property an Annotation can have. Types ignore the ones they don't list.
#[derive(Clone, Debug, PartialEq)]
pub struct Style {
    pub line: Rgba,
    /// `None` is "no fill" (∅).
    pub fill: Option<Rgba>,
    /// Line thickness in Image pixels.
    pub thickness: f32,
    pub shadow: bool,
    /// The text font's family name (§4.4).
    pub font: String,
    /// The text size in Image pixels.
    pub font_size: f32,
}

impl Default for Style {
    /// Greenshot-like defaults (§8.3): red, 3 px, shadow on, no fill.
    fn default() -> Self {
        Self {
            line: Rgba::RED,
            fill: None,
            thickness: 3.0,
            shadow: true,
            font: DEFAULT_FONT.to_owned(),
            font_size: 20.0,
        }
    }
}

/// Names a style property, without a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PropertyKind {
    LineColour,
    FillColour,
    Thickness,
    Shadow,
    /// The text colour: the same field as the line colour (`Style::line`).
    TextColour,
    Font,
    FontSize,
}

/// A style property with its value: one change in the property bar.
#[derive(Clone, Debug, PartialEq)]
pub enum Property {
    LineColour(Rgba),
    FillColour(Option<Rgba>),
    Thickness(f32),
    Shadow(bool),
    TextColour(Rgba),
    Font(String),
    FontSize(f32),
}

impl Property {
    pub fn kind(&self) -> PropertyKind {
        match self {
            Property::LineColour(_) => PropertyKind::LineColour,
            Property::FillColour(_) => PropertyKind::FillColour,
            Property::Thickness(_) => PropertyKind::Thickness,
            Property::Shadow(_) => PropertyKind::Shadow,
            Property::TextColour(_) => PropertyKind::TextColour,
            Property::Font(_) => PropertyKind::Font,
            Property::FontSize(_) => PropertyKind::FontSize,
        }
    }
}

/// Thickness limits of the property bar.
pub const THICKNESS_RANGE: std::ops::RangeInclusive<f32> = 1.0..=40.0;

/// Font size limits of the property bar, in Image pixels.
pub const FONT_SIZE_RANGE: std::ops::RangeInclusive<f32> = 6.0..=400.0;

/// The text tool's default font: the platform's sans-serif (fontconfig resolves the alias; where
/// nothing matches, Skia falls back to the system's default font).
pub const DEFAULT_FONT: &str = "sans-serif";

impl Style {
    pub fn get(&self, kind: PropertyKind) -> Property {
        match kind {
            PropertyKind::LineColour => Property::LineColour(self.line),
            PropertyKind::FillColour => Property::FillColour(self.fill),
            PropertyKind::Thickness => Property::Thickness(self.thickness),
            PropertyKind::Shadow => Property::Shadow(self.shadow),
            PropertyKind::TextColour => Property::TextColour(self.line),
            PropertyKind::Font => Property::Font(self.font.clone()),
            PropertyKind::FontSize => Property::FontSize(self.font_size),
        }
    }

    pub fn set(&mut self, property: Property) {
        match property {
            Property::LineColour(c) => self.line = c,
            Property::FillColour(c) => self.fill = c,
            Property::Thickness(t) => {
                self.thickness = t.clamp(*THICKNESS_RANGE.start(), *THICKNESS_RANGE.end())
            }
            Property::Shadow(s) => self.shadow = s,
            Property::TextColour(c) => self.line = c,
            Property::Font(f) => self.font = f,
            Property::FontSize(s) => {
                self.font_size = s.clamp(*FONT_SIZE_RANGE.start(), *FONT_SIZE_RANGE.end())
            }
        }
    }
}
