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

/// `#rrggbb`, or `#rrggbbaa` when not opaque: how colours are written in `config.toml` (§8.3).
impl std::fmt::Display for Rgba {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{:02x}{:02x}{:02x}", self.r, self.g, self.b)?;
        if self.a != 255 {
            write!(f, "{:02x}", self.a)?;
        }
        Ok(())
    }
}

impl std::str::FromStr for Rgba {
    type Err = String;

    /// `#rrggbb` or `#rrggbbaa`.
    fn from_str(s: &str) -> Result<Self, String> {
        let bad = || format!("not a colour (#rrggbb or #rrggbbaa): {s:?}");
        let hex = s.strip_prefix('#').ok_or_else(bad)?;
        if !(hex.len() == 6 || hex.len() == 8) || !hex.is_ascii() {
            return Err(bad());
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| bad());
        let a = if hex.len() == 8 { byte(6)? } else { 255 };
        Ok(Self::new(byte(0)?, byte(2)?, byte(4)?, a))
    }
}

impl serde::Serialize for Rgba {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> serde::Deserialize<'de> for Rgba {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// A fill or border colour in `config.toml`: a colour, or `"none"`.
mod fill_serde {
    use super::Rgba;
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(fill: &Option<Rgba>, s: S) -> Result<S::Ok, S::Error> {
        match fill {
            Some(c) => s.collect_str(c),
            None => s.serialize_str("none"),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Rgba>, D::Error> {
        let s = String::deserialize(d)?;
        if s == "none" {
            return Ok(None);
        }
        s.parse().map(Some).map_err(serde::de::Error::custom)
    }
}

/// Every style property an Annotation can have. Types ignore the ones they don't list.
///
/// Each tool's style is saved as `tools.<tool>` in `config.toml` (§8.3) through serde: a new field
/// is persisted automatically (its type needs `Serialize`/`Deserialize`), and a missing key takes
/// its value from [`Style::default`].
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Style {
    pub line: Rgba,
    /// `None` is "no fill" (∅).
    #[serde(with = "fill_serde")]
    pub fill: Option<Rgba>,
    /// Line thickness in Image pixels.
    pub thickness: f32,
    pub shadow: bool,
    /// Which ends of an arrow carry a head.
    pub arrowheads: Arrowheads,
    /// An Inset's border (§4.6): `None` is no border; its width is `thickness`.
    #[serde(with = "fill_serde")]
    pub border: Option<Rgba>,
}

/// Which ends of an arrow carry a head (§4.1 A).
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Arrowheads {
    #[default]
    End,
    Start,
    Both,
    None,
}

impl Arrowheads {
    /// In the order the property bar lists them.
    pub const ALL: [Arrowheads; 4] = [
        Arrowheads::End,
        Arrowheads::Start,
        Arrowheads::Both,
        Arrowheads::None,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Arrowheads::End => "End",
            Arrowheads::Start => "Start",
            Arrowheads::Both => "Both",
            Arrowheads::None => "None",
        }
    }

    pub fn at_start(self) -> bool {
        matches!(self, Arrowheads::Start | Arrowheads::Both)
    }

    pub fn at_end(self) -> bool {
        matches!(self, Arrowheads::End | Arrowheads::Both)
    }
}

impl Default for Style {
    /// Greenshot-like defaults (§8.3): red, 3 px, shadow on, no fill.
    fn default() -> Self {
        Self {
            line: Rgba::RED,
            fill: None,
            thickness: 3.0,
            shadow: true,
            arrowheads: Arrowheads::End,
            border: None,
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
    Arrowheads,
    BorderColour,
}

/// A style property with its value: one change in the property bar.
#[derive(Clone, Debug, PartialEq)]
pub enum Property {
    LineColour(Rgba),
    FillColour(Option<Rgba>),
    Thickness(f32),
    Shadow(bool),
    Arrowheads(Arrowheads),
    /// `None` is no border.
    BorderColour(Option<Rgba>),
}

impl Property {
    pub fn kind(&self) -> PropertyKind {
        match self {
            Property::LineColour(_) => PropertyKind::LineColour,
            Property::FillColour(_) => PropertyKind::FillColour,
            Property::Thickness(_) => PropertyKind::Thickness,
            Property::Shadow(_) => PropertyKind::Shadow,
            Property::Arrowheads(_) => PropertyKind::Arrowheads,
            Property::BorderColour(_) => PropertyKind::BorderColour,
        }
    }
}

/// Thickness limits of the property bar.
pub const THICKNESS_RANGE: std::ops::RangeInclusive<f32> = 1.0..=40.0;

impl Style {
    pub fn get(&self, kind: PropertyKind) -> Property {
        match kind {
            PropertyKind::LineColour => Property::LineColour(self.line),
            PropertyKind::FillColour => Property::FillColour(self.fill),
            PropertyKind::Thickness => Property::Thickness(self.thickness),
            PropertyKind::Shadow => Property::Shadow(self.shadow),
            PropertyKind::Arrowheads => Property::Arrowheads(self.arrowheads),
            PropertyKind::BorderColour => Property::BorderColour(self.border),
        }
    }

    /// Values read from `config.toml` brought back into range.
    pub fn sanitised(mut self) -> Self {
        if !self.thickness.is_finite() {
            self.thickness = Self::default().thickness;
        }
        self.set(Property::Thickness(self.thickness));
        self
    }

    pub fn set(&mut self, property: Property) {
        match property {
            Property::LineColour(c) => self.line = c,
            Property::FillColour(c) => self.fill = c,
            Property::Thickness(t) => {
                self.thickness = t.clamp(*THICKNESS_RANGE.start(), *THICKNESS_RANGE.end())
            }
            Property::Shadow(s) => self.shadow = s,
            Property::Arrowheads(a) => self.arrowheads = a,
            Property::BorderColour(c) => self.border = c,
        }
    }
}
