use super::*;

/// Represents an absolute length in pixels or rems.
///
/// `AbsoluteLength` can be either a fixed number of pixels, which is an absolute measurement not
/// affected by the current font size, or a number of rems, which is relative to the font size of
/// the root element. It is used for specifying dimensions that are either independent of or
/// related to the typographic scale.
#[derive(Clone, Copy, Neg, PartialEq)]
pub enum AbsoluteLength {
    /// A length in pixels.
    Pixels(Pixels),
    /// A length in rems.
    Rems(Rems),
}

impl AbsoluteLength {
    /// Checks if the absolute length is zero.
    pub fn is_zero(&self) -> bool {
        match self {
            AbsoluteLength::Pixels(px) => px.0 == 0.0,
            AbsoluteLength::Rems(rems) => rems.0 == 0.0,
        }
    }
}

impl From<Pixels> for AbsoluteLength {
    fn from(pixels: Pixels) -> Self {
        AbsoluteLength::Pixels(pixels)
    }
}

impl From<Rems> for AbsoluteLength {
    fn from(rems: Rems) -> Self {
        AbsoluteLength::Rems(rems)
    }
}

impl AbsoluteLength {
    /// Converts an `AbsoluteLength` to `Pixels` based on a given `rem_size`.
    ///
    /// # Arguments
    ///
    /// * `rem_size` - The size of one rem in pixels.
    ///
    /// # Returns
    ///
    /// Returns the `AbsoluteLength` as `Pixels`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use gpui::{AbsoluteLength, Pixels, Rems};
    /// let length_in_pixels = AbsoluteLength::Pixels(Pixels::from(42.0));
    /// let length_in_rems = AbsoluteLength::Rems(Rems(2.0));
    /// let rem_size = Pixels::from(16.0);
    ///
    /// assert_eq!(length_in_pixels.to_pixels(rem_size), Pixels::from(42.0));
    /// assert_eq!(length_in_rems.to_pixels(rem_size), Pixels::from(32.0));
    /// ```
    pub fn to_pixels(self, rem_size: Pixels) -> Pixels {
        match self {
            AbsoluteLength::Pixels(pixels) => pixels,
            AbsoluteLength::Rems(rems) => rems.to_pixels(rem_size),
        }
    }

    /// Converts an `AbsoluteLength` to `Rems` based on a given `rem_size`.
    ///
    /// # Arguments
    ///
    /// * `rem_size` - The size of one rem in pixels.
    ///
    /// # Returns
    ///
    /// Returns the `AbsoluteLength` as `Pixels`.
    pub fn to_rems(self, rem_size: Pixels) -> Rems {
        match self {
            AbsoluteLength::Pixels(pixels) => Rems(pixels.0 / rem_size.0),
            AbsoluteLength::Rems(rems) => rems,
        }
    }
}

impl Default for AbsoluteLength {
    fn default() -> Self {
        px(0.).into()
    }
}

impl Display for AbsoluteLength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pixels(pixels) => write!(f, "{pixels}"),
            Self::Rems(rems) => write!(f, "{rems}"),
        }
    }
}

impl Debug for AbsoluteLength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(self, f)
    }
}

const EXPECTED_ABSOLUTE_LENGTH: &str = "number with 'px' or 'rem' suffix";

impl TryFrom<&'_ str> for AbsoluteLength {
    type Error = anyhow::Error;

    fn try_from(value: &'_ str) -> Result<Self, Self::Error> {
        if let Ok(pixels) = value.try_into() {
            Ok(Self::Pixels(pixels))
        } else if let Ok(rems) = value.try_into() {
            Ok(Self::Rems(rems))
        } else {
            Err(anyhow!(
                "invalid AbsoluteLength '{value}', expected {EXPECTED_ABSOLUTE_LENGTH}"
            ))
        }
    }
}

impl JsonSchema for AbsoluteLength {
    fn schema_name() -> Cow<'static, str> {
        "AbsoluteLength".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        json_schema!({
            "type": "string",
            "pattern": r"^-?\d+(\.\d+)?(px|rem)$"
        })
    }
}

impl<'de> Deserialize<'de> for AbsoluteLength {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StringVisitor;

        impl de::Visitor<'_> for StringVisitor {
            type Value = AbsoluteLength;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "{EXPECTED_ABSOLUTE_LENGTH}")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                AbsoluteLength::try_from(value).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(StringVisitor)
    }
}

impl Serialize for AbsoluteLength {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{self}"))
    }
}

/// A non-auto length that can be defined in pixels, rems, or percent of parent.
///
/// This enum represents lengths that have a specific value, as opposed to lengths that are automatically
/// determined by the context. It includes absolute lengths in pixels or rems, and relative lengths as a
/// fraction of the parent's size.
#[derive(Clone, Copy, Neg, PartialEq)]
pub enum DefiniteLength {
    /// An absolute length specified in pixels or rems.
    Absolute(AbsoluteLength),
    /// A relative length specified as a fraction of the parent's size, between 0 and 1.
    Fraction(f32),
}

impl DefiniteLength {
    /// Converts the `DefiniteLength` to `Pixels` based on a given `base_size` and `rem_size`.
    ///
    /// If the `DefiniteLength` is an absolute length, it will be directly converted to `Pixels`.
    /// If it is a fraction, the fraction will be multiplied by the `base_size` to get the length in pixels.
    ///
    /// # Arguments
    ///
    /// * `base_size` - The base size in `AbsoluteLength` to which the fraction will be applied.
    /// * `rem_size` - The size of one rem in pixels, used to convert rems to pixels.
    ///
    /// # Returns
    ///
    /// Returns the `DefiniteLength` as `Pixels`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use gpui::{DefiniteLength, AbsoluteLength, Pixels, px, rems};
    /// let length_in_pixels = DefiniteLength::Absolute(AbsoluteLength::Pixels(px(42.0)));
    /// let length_in_rems = DefiniteLength::Absolute(AbsoluteLength::Rems(rems(2.0)));
    /// let length_as_fraction = DefiniteLength::Fraction(0.5);
    /// let base_size = AbsoluteLength::Pixels(px(100.0));
    /// let rem_size = px(16.0);
    ///
    /// assert_eq!(length_in_pixels.to_pixels(base_size, rem_size), Pixels::from(42.0));
    /// assert_eq!(length_in_rems.to_pixels(base_size, rem_size), Pixels::from(32.0));
    /// assert_eq!(length_as_fraction.to_pixels(base_size, rem_size), Pixels::from(50.0));
    /// ```
    pub fn to_pixels(self, base_size: AbsoluteLength, rem_size: Pixels) -> Pixels {
        match self {
            DefiniteLength::Absolute(size) => size.to_pixels(rem_size),
            DefiniteLength::Fraction(fraction) => match base_size {
                AbsoluteLength::Pixels(px) => px * fraction,
                AbsoluteLength::Rems(rems) => rems * rem_size * fraction,
            },
        }
    }
}

impl Debug for DefiniteLength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(self, f)
    }
}

impl Display for DefiniteLength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DefiniteLength::Absolute(length) => write!(f, "{length}"),
            DefiniteLength::Fraction(fraction) => write!(f, "{}%", (fraction * 100.0) as i32),
        }
    }
}

const EXPECTED_DEFINITE_LENGTH: &str = "expected number with 'px', 'rem', or '%' suffix";

impl TryFrom<&'_ str> for DefiniteLength {
    type Error = anyhow::Error;

    fn try_from(value: &'_ str) -> Result<Self, Self::Error> {
        if let Some(percentage) = value.strip_suffix('%') {
            let fraction: f32 = percentage.parse::<f32>().with_context(|| {
                format!("invalid DefiniteLength '{value}', expected {EXPECTED_DEFINITE_LENGTH}")
            })?;
            Ok(DefiniteLength::Fraction(fraction / 100.0))
        } else if let Ok(absolute_length) = value.try_into() {
            Ok(DefiniteLength::Absolute(absolute_length))
        } else {
            Err(anyhow!(
                "invalid DefiniteLength '{value}', expected {EXPECTED_DEFINITE_LENGTH}"
            ))
        }
    }
}

impl JsonSchema for DefiniteLength {
    fn schema_name() -> Cow<'static, str> {
        "DefiniteLength".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        json_schema!({
            "type": "string",
            "pattern": r"^-?\d+(\.\d+)?(px|rem|%)$"
        })
    }
}

impl<'de> Deserialize<'de> for DefiniteLength {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StringVisitor;

        impl de::Visitor<'_> for StringVisitor {
            type Value = DefiniteLength;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "{EXPECTED_DEFINITE_LENGTH}")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                DefiniteLength::try_from(value).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(StringVisitor)
    }
}

impl Serialize for DefiniteLength {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{self}"))
    }
}

impl From<Pixels> for DefiniteLength {
    fn from(pixels: Pixels) -> Self {
        Self::Absolute(pixels.into())
    }
}

impl From<Rems> for DefiniteLength {
    fn from(rems: Rems) -> Self {
        Self::Absolute(rems.into())
    }
}

impl From<AbsoluteLength> for DefiniteLength {
    fn from(length: AbsoluteLength) -> Self {
        Self::Absolute(length)
    }
}

impl Default for DefiniteLength {
    fn default() -> Self {
        Self::Absolute(AbsoluteLength::default())
    }
}

/// A length that can be defined in pixels, rems, percent of parent, or auto.
#[derive(Clone, Copy, PartialEq)]
pub enum Length {
    /// A definite length specified either in pixels, rems, or as a fraction of the parent's size.
    Definite(DefiniteLength),
    /// An automatic length that is determined by the context in which it is used.
    Auto,
}

impl Debug for Length {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(self, f)
    }
}

impl Display for Length {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Length::Definite(definite_length) => write!(f, "{}", definite_length),
            Length::Auto => write!(f, "auto"),
        }
    }
}

const EXPECTED_LENGTH: &str = "expected 'auto' or number with 'px', 'rem', or '%' suffix";

impl TryFrom<&'_ str> for Length {
    type Error = anyhow::Error;

    fn try_from(value: &'_ str) -> Result<Self, Self::Error> {
        if value == "auto" {
            Ok(Length::Auto)
        } else if let Ok(definite_length) = value.try_into() {
            Ok(Length::Definite(definite_length))
        } else {
            Err(anyhow!(
                "invalid Length '{value}', expected {EXPECTED_LENGTH}"
            ))
        }
    }
}

impl JsonSchema for Length {
    fn schema_name() -> Cow<'static, str> {
        "Length".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        json_schema!({
            "type": "string",
            "pattern": r"^(auto|-?\d+(\.\d+)?(px|rem|%))$"
        })
    }
}

impl<'de> Deserialize<'de> for Length {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StringVisitor;

        impl de::Visitor<'_> for StringVisitor {
            type Value = Length;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                write!(f, "{EXPECTED_LENGTH}")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Length::try_from(value).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(StringVisitor)
    }
}

impl Serialize for Length {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{self}"))
    }
}

/// Constructs a `DefiniteLength` representing a relative fraction of a parent size.
///
/// This function creates a `DefiniteLength` that is a specified fraction of a parent's dimension.
/// The fraction should be a floating-point number between 0.0 and 1.0, where 1.0 represents 100% of the parent's size.
///
/// # Arguments
///
/// * `fraction` - The fraction of the parent's size, between 0.0 and 1.0.
///
/// # Returns
///
/// A `DefiniteLength` representing the relative length as a fraction of the parent's size.
pub const fn relative(fraction: f32) -> DefiniteLength {
    DefiniteLength::Fraction(fraction)
}

/// Returns the Golden Ratio, i.e. `~(1.0 + sqrt(5.0)) / 2.0`.
pub const fn phi() -> DefiniteLength {
    relative(1.618_034)
}

/// Constructs a `Rems` value representing a length in rems.
///
/// # Arguments
///
/// * `rems` - The number of rems for the length.
///
/// # Returns
///
/// A `Rems` representing the specified number of rems.
pub const fn rems(rems: f32) -> Rems {
    Rems(rems)
}

/// Constructs a `Pixels` value representing a length in pixels.
///
/// # Arguments
///
/// * `pixels` - The number of pixels for the length.
///
/// # Returns
///
/// A `Pixels` representing the specified number of pixels.
pub const fn px(pixels: f32) -> Pixels {
    Pixels(pixels)
}

/// Returns a `Length` representing an automatic length.
///
/// The `auto` length is often used in layout calculations where the length should be determined
/// by the layout context itself rather than being explicitly set. This is commonly used in CSS
/// for properties like `width`, `height`, `margin`, `padding`, etc., where `auto` can be used
/// to instruct the layout engine to calculate the size based on other factors like the size of the
/// container or the intrinsic size of the content.
///
/// # Returns
///
/// A `Length` variant set to `Auto`.
pub const fn auto() -> Length {
    Length::Auto
}

impl From<Pixels> for Length {
    fn from(pixels: Pixels) -> Self {
        Self::Definite(pixels.into())
    }
}

impl From<Rems> for Length {
    fn from(rems: Rems) -> Self {
        Self::Definite(rems.into())
    }
}

impl From<DefiniteLength> for Length {
    fn from(length: DefiniteLength) -> Self {
        Self::Definite(length)
    }
}

impl From<AbsoluteLength> for Length {
    fn from(length: AbsoluteLength) -> Self {
        Self::Definite(length.into())
    }
}

impl Default for Length {
    fn default() -> Self {
        Self::Definite(DefiniteLength::default())
    }
}
