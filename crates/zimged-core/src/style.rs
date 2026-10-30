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
    pub const YELLOW: Rgba = Rgba::rgb(255, 255, 0);
}

/// The highlight's modes (§4.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum HighlightMode {
    /// A translucent colour multiplied over the region.
    #[default]
    Text,
    /// Everything outside the region is dimmed.
    Area,
    /// Everything outside the region is shown in grayscale.
    Grayscale,
    /// The region shows what's under it, enlarged.
    Magnify,
}

impl HighlightMode {
    pub const ALL: [HighlightMode; 4] = [
        HighlightMode::Text,
        HighlightMode::Area,
        HighlightMode::Grayscale,
        HighlightMode::Magnify,
    ];

    pub fn name(self) -> &'static str {
        match self {
            HighlightMode::Text => "Text",
            HighlightMode::Area => "Area",
            HighlightMode::Grayscale => "Grayscale",
            HighlightMode::Magnify => "Magnify",
        }
    }
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
    pub highlight_mode: HighlightMode,
    /// The text highlight's colour (multiplied over the region).
    pub highlight: Rgba,
    /// The magnify highlight's factor. The app seeds the highlight tool's default from the
    /// `magnify_factor` setting (§8.3).
    pub magnification: f32,
}

impl Default for Style {
    /// Greenshot-like defaults (§8.3): red, 3 px, shadow on, no fill.
    fn default() -> Self {
        Self {
            line: Rgba::RED,
            fill: None,
            thickness: 3.0,
            shadow: true,
            highlight_mode: HighlightMode::Text,
            highlight: Rgba::YELLOW,
            magnification: 2.0,
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
    HighlightMode,
    HighlightColour,
    Magnification,
}

/// A style property with its value: one change in the property bar.
#[derive(Clone, Debug, PartialEq)]
pub enum Property {
    LineColour(Rgba),
    FillColour(Option<Rgba>),
    Thickness(f32),
    Shadow(bool),
    HighlightMode(HighlightMode),
    HighlightColour(Rgba),
    Magnification(f32),
}

impl Property {
    pub fn kind(&self) -> PropertyKind {
        match self {
            Property::LineColour(_) => PropertyKind::LineColour,
            Property::FillColour(_) => PropertyKind::FillColour,
            Property::Thickness(_) => PropertyKind::Thickness,
            Property::Shadow(_) => PropertyKind::Shadow,
            Property::HighlightMode(_) => PropertyKind::HighlightMode,
            Property::HighlightColour(_) => PropertyKind::HighlightColour,
            Property::Magnification(_) => PropertyKind::Magnification,
        }
    }
}

/// Thickness limits of the property bar.
pub const THICKNESS_RANGE: std::ops::RangeInclusive<f32> = 1.0..=40.0;

/// Magnification limits of the property bar.
pub const MAGNIFICATION_RANGE: std::ops::RangeInclusive<f32> = 1.0..=10.0;

impl Style {
    pub fn get(&self, kind: PropertyKind) -> Property {
        match kind {
            PropertyKind::LineColour => Property::LineColour(self.line),
            PropertyKind::FillColour => Property::FillColour(self.fill),
            PropertyKind::Thickness => Property::Thickness(self.thickness),
            PropertyKind::Shadow => Property::Shadow(self.shadow),
            PropertyKind::HighlightMode => Property::HighlightMode(self.highlight_mode),
            PropertyKind::HighlightColour => Property::HighlightColour(self.highlight),
            PropertyKind::Magnification => Property::Magnification(self.magnification),
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
            Property::HighlightMode(m) => self.highlight_mode = m,
            Property::HighlightColour(c) => self.highlight = c,
            Property::Magnification(f) => {
                self.magnification = f.clamp(
                    *MAGNIFICATION_RANGE.start(),
                    *MAGNIFICATION_RANGE.end(),
                )
            }
        }
    }
}
