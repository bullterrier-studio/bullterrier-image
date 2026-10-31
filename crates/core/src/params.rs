//! How an effect declares its parameters, once, in Rust.
//!
//! An effect's `Params` struct is its declaration: each field has a type, a `///` doc comment and a `#[param(..)]`
//! attribute (default, unit, range; see [`Params`]), and a choice is a fieldless enum (see [`Choice`]). The derives
//! write `Default` and a [`Table`] describing every field, which the Python package builds the effect's parameter model
//! from (`bullterrier_image.core.params`), so the documented ranges, defaults and descriptions are written nowhere
//! else. A [`Field`] is a parameter in the ecosystem's parameter format, version 1 (`docs/parameter-format/parameters.md`
//! in the repository: keys, kinds, units, ranges), and `tests/_native/test_parameter_format.py` validates every declared
//! field against its schema.
//!
//! ```text
//! /// The parameters of `pattern`.
//! #[derive(Deserialize, Params)]
//! #[serde(deny_unknown_fields)]
//! pub(crate) struct Params {
//!     /// The size of one repeat in px. Larger = a coarser pattern.
//!     #[param(default = 32.0, unit = px, min = 2, max = 512)]
//!     pub(crate) cell: f32,
//!     /// The pattern: 'stripes' or 'grid'.
//!     #[param(default = Grid)]
//!     pub(crate) kind: Kind,
//! }
//! ```
//!
//! The declaration is checked when the crate compiles: a field has a doc, a number a unit and a range that holds its
//! default, `cyclic` is for degrees spanning a turn, `drivable` for a decimal number. The derive also writes
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

pub use bullterrier_image_derive::{Choice, Params};

/// The names a choice picks from, in declaration order: what `#[derive(Choice)]` implements for a fieldless enum. The
/// four other kinds need nothing from their type: the derive of `Params` knows `f32`, `i32`, `bool` and `Color` by their
/// written names.
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
pub struct Field {
    /// The name, as the effect's parameters are given by name.
    pub name: &'static str,
    /// What kind of value it holds.
    pub kind: Kind,
    /// What it does, its unit and range in words, what raising it does (its doc comment).
    pub meaning: &'static str,
    /// The value an effect uses when none is given: a colour as its `'#rrggbb'` text, a choice as its name.
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
    /// A whole number (`i32`).
    Int,
    /// A switch.
    Bool,
    /// One of a few names (a fieldless enum with `#[derive(Choice)]`).
    Choice,
    /// A colour: three numbers 0..1.
    Color,
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

/// A default or a bound as the declaration writes it: a whole number stays whole, a decimal stays decimal.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Scalar {
    /// A whole number.
    Int(i64),
    /// A decimal number.
    Float(f64),
    /// A switch.
    Bool(bool),
    /// A colour's `'#rrggbb'` text, or a choice's name.
    Text(&'static str),
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
        }
    }
}

/// A colour: red, green and blue, each 0..1. Given as a list of three numbers.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(transparent)]
pub struct Color(pub [f32; 3]);

/// A probe effect's parameters, one of every kind: what the tests of the declaration and of the validation read.
#[cfg(test)]
pub(crate) mod probe {
    use super::{Choice, Color, Params};
    use serde::Deserialize;

    /// The shapes of the probe.
    #[derive(Debug, Clone, Copy, PartialEq, Choice)]
    pub(crate) enum Shape {
        Stripes,
        Checks,
        FourthWay,
    }

    /// The parameters of a probe effect, which reads them as the Python package gives them.
    #[derive(Debug, Deserialize, Params)]
    #[serde(deny_unknown_fields)]
    pub(crate) struct Probe {
        /// The size of one cell in px, over
        /// two lines. Larger = coarser.
        #[param(default = 32.0, unit = px, min = 2, max = 512, drivable)]
        pub(crate) cell: f32,
        /// How many cells.
        #[param(default = 4, unit = count, min = -3, max = 32)]
        pub(crate) cells: i32,
        /// The direction: 'stripes' or 'checks'.
        #[param(default = 0.5, unit = degrees, min = 0, max = 360, cyclic, drivable)]
        pub(crate) angle: f32,
        /// The shape.
        #[param(default = FourthWay, advanced)]
        pub(crate) shape: Shape,
        /// Whether it is kept.
        #[param(default = true)]
        pub(crate) keep: bool,
        /// The colour.
        #[param(default = "#ff8000")]
        pub(crate) tint: Color,
    }
}

#[cfg(test)]
mod tests {
    use super::probe::{Probe, Shape};
    use super::{Choice as _, Color, Declared, EffectParams};
    use crate::effects::{grade, pattern, pixel_grid};
    use serde_json::{Value, json};

    #[test]
    fn a_default_is_what_the_field_declares() {
        let probe = Probe::default();
        assert_eq!(probe.cell.to_bits(), 32.0_f32.to_bits());
        assert_eq!(probe.cells, 4);
        assert_eq!(probe.angle.to_bits(), 0.5_f32.to_bits());
        assert_eq!(probe.shape, Shape::FourthWay);
        assert!(probe.keep);
        assert_eq!(probe.tint, Color([1.0, 128.0 / 255.0, 0.0]));
    }

    #[test]
    fn values_are_the_numbers_a_person_would_write() {
        // 0.2 must not widen to 0.20000000298; a whole decimal stays a decimal; a colour is three numbers; a choice its name
        let probe = Probe {
            cell: 0.2,
            cells: -7,
            angle: 90.0,
            shape: Shape::FourthWay,
            keep: false,
            tint: Color([0.1, 0.3, 1.0]),
        };
        let expected = json!({
            "cell": 0.2, "cells": -7, "angle": 90.0, "shape": "fourth_way", "keep": false,
            "tint": [0.1, 0.3, 1.0],
        });
        assert_eq!(Value::Object(probe.values()), expected);
    }

    #[test]
    fn the_values_of_the_defaults_are_the_declared_defaults() {
        let expected = json!({
            "cell": 32.0, "cells": 4, "angle": 0.5, "shape": "fourth_way", "keep": true,
            "tint": [1.0, 0.501_960_8, 0.0],
        });
        assert_eq!(Value::Object(Probe::default().values()), expected);
    }

    #[test]
    fn a_choice_names_its_variant() {
        let names = [Shape::Stripes, Shape::Checks, Shape::FourthWay].map(|shape| shape.name());
        assert_eq!(names, ["stripes", "checks", "fourth_way"]);
    }

    #[test]
    fn each_effect_file_ties_its_params_to_the_effect_called_as_the_file() {
        assert_eq!(<grade::Params as EffectParams>::EFFECT, "grade");
        assert_eq!(<pattern::Params as EffectParams>::EFFECT, "pattern");
        assert_eq!(<pixel_grid::Params as EffectParams>::EFFECT, "pixel_grid");
    }

    #[test]
    fn the_table_describes_every_field_as_written_in_declaration_order() {
        let table = serde_json::to_value(Probe::table());
        let expected = json!({
            "doc": "The parameters of a probe effect, which reads them as the Python package gives them.",
            "fields": [
                {"name": "cell", "kind": "float", "meaning": "The size of one cell in px, over two lines. Larger = coarser.",
                 "default": 32.0, "unit": "px", "min": 2, "max": 512, "cyclic": false, "choices": [],
                 "drivable": true, "advanced": false},
                {"name": "cells", "kind": "int", "meaning": "How many cells.",
                 "default": 4, "unit": "count", "min": -3, "max": 32, "cyclic": false, "choices": [],
                 "drivable": false, "advanced": false},
                {"name": "angle", "kind": "float", "meaning": "The direction: 'stripes' or 'checks'.",
                 "default": 0.5, "unit": "degrees", "min": 0, "max": 360, "cyclic": true, "choices": [],
                 "drivable": true, "advanced": false},
                {"name": "shape", "kind": "choice", "meaning": "The shape.",
                 "default": "fourth_way", "unit": null, "min": null, "max": null, "cyclic": false,
                 "choices": ["stripes", "checks", "fourth_way"], "drivable": false, "advanced": true},
                {"name": "keep", "kind": "bool", "meaning": "Whether it is kept.",
                 "default": true, "unit": null, "min": null, "max": null, "cyclic": false, "choices": [],
                 "drivable": false, "advanced": false},
                {"name": "tint", "kind": "color", "meaning": "The colour.",
                 "default": "#ff8000", "unit": null, "min": null, "max": null, "cyclic": false, "choices": [],
                 "drivable": false, "advanced": false},
            ],
        });
        assert_eq!(table.ok(), Some(expected));
    }

    #[test]
    fn a_choice_is_read_by_its_name_and_a_wrong_name_lists_the_choices() {
        let read = |name: &str| serde_json::from_value::<Shape>(json!(name));
        assert_eq!(read("checks").ok(), Some(Shape::Checks));
        assert_eq!(read("fourth_way").ok(), Some(Shape::FourthWay));
        let message = read("stripe").err().map(|error| error.to_string());
        assert!(
            message
                .as_deref()
                .is_some_and(|text| text.contains("stripe") && text.contains("fourth_way")),
            "{message:?}"
        );
        assert!(serde_json::from_value::<Shape>(json!(2)).is_err());
    }

    #[test]
    fn parameters_as_python_gives_them_are_read_into_the_struct() {
        let probe: Result<Probe, _> = serde_json::from_value(json!({
            "cell": 8.0, "cells": 5, "angle": 90.0, "shape": "stripes", "keep": false, "tint": [0.0, 0.5, 1.0],
        }));
        assert!(matches!(
            probe,
            Ok(Probe {
                cells: 5,
                shape: Shape::Stripes,
                keep: false,
                tint: Color([_, _, _]),
                ..
            })
        ));
    }
}
