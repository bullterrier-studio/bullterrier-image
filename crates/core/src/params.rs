//! How an effect declares its parameters, once, in Rust.
//!
//! An effect's `Params` struct is its declaration: each field has a type, a `///` doc comment and a `#[param(..)]`
//! attribute (default, unit, range; see [`Params`]), and a choice is a fieldless enum (see [`Choice`]). The derives
//! write `Default` and a [`Table`] describing every field, which the Python package builds the effect's parameter model
//! from (`bullterrier_image.core.params`), so the documented ranges, defaults and descriptions are written nowhere
//! else. A [`Field`] is a parameter in the ecosystem's parameter format, version 2 (`docs/parameter-format/parameters.md`
//! in the repository: keys, kinds, units, ranges), and `tests/_native/test_parameter_format.py` validates every declared
//! field against its schema.
//!
//! ```text
//! /// The parameters of `pattern`.
//! #[derive(Debug, Clone, PartialEq, Deserialize, Params)]
//! #[serde(deny_unknown_fields)]
//! pub struct Params {
//!     /// The size of one repeat in px. Larger = a coarser pattern.
//!     #[param(default = 32.0, unit = px, min = 2, max = 512)]
//!     pub cell: f32,
//!     /// The pattern: 'stripes' or 'grid'.
//!     #[param(default = Grid)]
//!     pub kind: Kind,
//! }
//! ```
//!
//! A picture laid over the photo is a `PictureName` with no default (none: the step leaves the photo as it is); like a
//! table it reaches the effect in its [`Inputs`](crate::Inputs).
//!
//! A lookup table is a `LutName` with no default (it is none: the identity table); it reaches the effect in its
//! [`Inputs`](crate::Inputs), not in the values. A field is a `FieldName` with `#[param(default = "depth")]`: the name of
//! the selection the effect works from, which it also finds as a [`Layer`](crate::Layer) in its `Inputs` under the
//! parameter's name. A colour that may be left out is an `Option<Color>` with `#[param()]`: none until a step gives one,
//! and the effect's doc says what none means. A list of colours is a `Vec<Color>` with
//! `#[param(default = ["#12002b", "#ff2bd6"], min_length = 2, max_length = 8)]`.
//!
//! The declaration is checked when the crate compiles: a field has a doc, a number a unit and a range that holds its
//! default, a list of colours a `min_length` and a default of a length inside its lengths, `cyclic` is for degrees
//! spanning a turn, `drivable` for a decimal number. The derive also writes
//! [`Declared::values`], which reads every field, so rustc cannot report a field that `render` never reads;
//! `tests/parameters_are_read.rs` does: it fails for a parameter that changes no result. A parameter that matters
//! only while another is set (`grade.pivot` needs `contrast` other than 1) gets a row in that test's `conditional`.
//!
//! A user's values are checked against the table, not against the struct: [`Table::check`] gives the complete parameters
//! an effect takes or the first rule broken. Running an effect trusts what it is given.

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

mod check;
mod scale;

#[cfg(test)]
mod derived;

pub use bullterrier_image_derive::{Choice, Params};

/// The names a choice picks from, in declaration order: what `#[derive(Choice)]` implements for a fieldless enum. The
/// other kinds need nothing from their type: the derive of `Params` knows `f32`, `i32`, `u32`, `usize`, `bool`, `Color`, `Option<Color>`,
/// `Vec<Color>`, `String`, `FieldName`, `LutName` and `PictureName` by their written names.
pub trait Choice {
    /// The names, in `snake_case`.
    const CHOICES: &'static [&'static str];

    /// The name of this variant: one of [`Choice::CHOICES`].
    fn name(&self) -> &'static str;
}

/// The parameters of an effect, declared with `#[derive(Params)]`: the table of what they are, and their values.
pub trait Declared: Default {
    /// The effect's parameters, described.
    fn table() -> Table;

    /// Every parameter with its value, as a recipe writes it: a decimal as the number a person would write (`0.2`, not
    /// the `f32` widened to `0.20000000298`), a colour as three such numbers, a choice as its name.
    fn values(&self) -> Map<String, Value>;
}

/// The parameters of the effect called [`EffectParams::EFFECT`]: the build script writes the impl for the `Params` of
/// each file in `src/effects/`, so a step can be built from the typed parameters (see `Step::of`).
pub trait EffectParams: Declared {
    /// The name of the effect, as its file is called.
    const EFFECT: &'static str;
}

/// `value` as the number a person would write: the shortest decimal that reads back as the same `f32`. A non-finite
/// value is JSON `null`, which `Recipe::checked` refuses.
pub(crate) fn decimal(value: f32) -> Value {
    Value::from(value.to_string().parse::<f64>().unwrap_or_else(|_| f64::from(value)))
}

/// An effect's parameters, described: the Python package builds the effect's model from it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Table {
    /// What the parameters are, as a whole (the doc comment of the struct).
    pub doc: &'static str,
    /// Every parameter, in declaration order.
    pub fields: Vec<Field>,
}

/// One parameter, described.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the flags are the keys of the parameter format, each written as it is"
)]
pub struct Field {
    /// The name, as the effect's parameters are given by name.
    pub name: &'static str,
    /// What kind of value it holds.
    pub kind: Kind,
    /// What it does, its unit and range in words, what raising it does (its doc comment).
    pub meaning: &'static str,
    /// The value an effect uses when none is given: a colour as its `'#rrggbb'` text, a list of colours as those texts,
    /// a choice as its name.
    pub default: Scalar,
    /// The unit of a number.
    pub unit: Option<Unit>,
    /// The lowest value of a number, inclusive.
    pub min: Option<Scalar>,
    /// The highest value of a number, inclusive.
    pub max: Option<Scalar>,
    /// A direction in degrees whose value and the value a turn further are the same: `min` to `max` is one turn.
    pub cyclic: bool,
    /// The names of a choice, in declaration order; none for any other kind.
    pub choices: &'static [&'static str],
    /// The fewest elements of a list of colours, or characters of a text; none for any other kind, and not written then.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_length: Option<usize>,
    /// The most elements of a list of colours, or characters of a text; none when it has no limit, and not written then.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<usize>,
    /// The fewest different characters of a text (`"aab"` has two); none for no such rule and for any other kind, and not
    /// written then.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_distinct: Option<usize>,
    /// The name of another number this one may not exceed (`band_min_height` below `band_max_height`); none when it has no
    /// such limit, and not written then. Both are checked together, whichever is given; both are in the same unit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at_most: Option<&'static str>,
    /// The value may be left out, and the default is `null` (an `Option<Color>`); not written when false. A lookup table
    /// is always so and does not say it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub nullable: bool,
    /// A pixel-by-pixel drive may vary it.
    pub drivable: bool,
    /// It tunes the technique rather than chooses the look.
    pub advanced: bool,
}

/// The kind of value a parameter holds. Python's `_PLAIN_KINDS` (`core/params.py`) maps each to its annotation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A decimal number (`f32`).
    Float,
    /// A whole number (`i32`, `u32` or `usize`).
    Int,
    /// A switch.
    Bool,
    /// One of a few names (a fieldless enum with `#[derive(Choice)]`).
    Choice,
    /// A colour: three numbers 0..1.
    Color,
    /// A list of colours (`Vec<Color>`), at least `min_length` long.
    Colors,
    /// The name of a selection (`FieldName`): `depth`, `light`, `subject` or a region the recipe declares. The effect finds
    /// the selection as a [`Layer`](crate::Layer) in its inputs, under the parameter's name.
    Field,
    /// A 3D colour lookup table (`LutName`): the name of a table the photo was given, or the path of a `.cube` file;
    /// none is the identity table. The default is always none.
    Lut,
    /// Free text (`String`), such as the characters of a palette: any string within `min_length` .. `max_length` characters
    /// and with at least `min_distinct` different ones, when the field has those rules.
    Text,
    /// A picture laid over the photo (`PictureName`): the name of a picture the photo was given, or the path of an image
    /// file; none leaves the photo as it is. The default is always none.
    Picture,
}

/// The unit a number is in. Python's `UNITS` (`core/params.py`) lists the same names; a model checks its unit against them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    /// Pixels at the design size.
    Px,
    /// A share of something, 0..1.
    Fraction,
    /// Degrees.
    Degrees,
    /// Photographic stops.
    Stops,
    /// A count of marks.
    Count,
    /// A ratio: 1 leaves things as they are.
    Ratio,
    /// Turns of a cycle.
    Cycles,
    /// Marks per 1000 design px².
    Density,
}

/// A default or a bound as the declaration writes it: a whole number stays whole, a decimal stays decimal. (A list of
/// colours is not a scalar, but it is a default all the same, so it is one more way to write one.)
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Scalar {
    /// A whole number.
    Int(i64),
    /// A decimal number.
    Float(f64),
    /// A switch.
    Bool(bool),
    /// A colour's `'#rrggbb'` text, a choice's name, or a field's name.
    Text(&'static str),
    /// The `'#rrggbb'` texts of a list of colours.
    List(&'static [&'static str]),
    /// No value: the default of a lookup table (the identity table), of a picture or of an optional colour.
    Null,
}

impl fmt::Display for Scalar {
    /// A number as a person writes it: `0`, `500`, `0.5`; a whole decimal loses its point.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(whole) => write!(formatter, "{whole}"),
            Self::Float(real) if real.fract() == 0.0 && real.abs() < 1e15 => write!(formatter, "{real:.0}"),
            Self::Float(real) => write!(formatter, "{real}"),
            Self::Bool(flag) => write!(formatter, "{flag}"),
            Self::Text(text) => formatter.write_str(text),
            Self::List(items) => write!(formatter, "[{}]", items.join(", ")),
            Self::Null => formatter.write_str("null"),
        }
    }
}

/// The value of a lookup table parameter: the name of a table the photo was given (see
/// `Photo::with_lut`), else the path of a `.cube` file; none is the identity table. A recipe
/// writes it as a string or `null`. The table itself never travels in the parameters: a render hands it to the effect in
/// its [`Inputs`](crate::Inputs).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LutName(pub Option<String>);

impl LutName {
    /// The table called `name`: a name given to the photo, or a path.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(Some(name.into()))
    }

    /// As a recipe writes it: the name, or `null` for none.
    #[must_use]
    pub fn json(&self) -> Value {
        self.0.clone().map_or(Value::Null, Value::from)
    }
}

/// The value of a picture parameter: the name of a picture the photo was given (see `Photo::with_image`), else the path
/// of an image file read once per render; none means no picture. A recipe writes it as a string or `null`. The picture
/// itself never travels in the parameters: a render hands it to the effect in its [`Inputs`](crate::Inputs), as an
/// [`Image`](crate::Image) with straight alpha. (It is not the [`Picture`](crate::Picture) a step reads: that is the photo.)
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PictureName(pub Option<String>);

impl PictureName {
    /// The picture called `name`: a name given to the photo, or a path.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(Some(name.into()))
    }

    /// As a recipe writes it: the name, or `null` for none.
    #[must_use]
    pub fn json(&self) -> Value {
        self.0.clone().map_or(Value::Null, Value::from)
    }
}

/// The value of a field parameter: the name of a selection (`depth`, `light`, `subject` or a region of the recipe) that an
/// effect works from. A recipe writes it as a string; the selection itself reaches the effect in its
/// [`Inputs`](crate::Inputs).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FieldName(pub String);

impl FieldName {
    /// The selection called `name`.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
}

/// A colour: red, green and blue, each 0..1. Given as a list of three numbers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Color(pub [f32; 3]);
