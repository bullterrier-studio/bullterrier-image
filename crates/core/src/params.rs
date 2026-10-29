//! How an effect declares its parameters, once, in Rust.
//!
//! An effect's `Params` struct is its declaration: each field has a type, a `///` doc comment and a `#[param(..)]`
//! attribute (default, unit, range; see [`Params`]), and a choice is a fieldless enum (see [`Choice`]). The derives
//! write `Default` and a [`Table`] describing every field, which the Python package builds the effect's parameter model
//! from (`bullterrier_image.core.params`), so the documented ranges, defaults and descriptions are written nowhere
//! else.
//!
//! ```text
//! /// The parameters of `pattern`.
//! #[derive(Deserialize, Params)]
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
//! The declaration is checked when the crate compiles: a field has a doc, a number a unit and a range that holds its
//! default, `cyclic` is for degrees spanning a turn, `drivable` for a decimal number. Values are validated by the
//! Python package, not here.

use serde::{Deserialize, Serialize};

pub use bullterrier_image_derive::{Choice, Params};

/// What a type must say to be a parameter's: its kind, and for a choice its names.
pub trait Parameter {
    /// The kind of value the type holds.
    const KIND: Kind;
    /// The names a choice picks from, in declaration order; none for any other kind.
    const CHOICES: &'static [&'static str] = &[];
}

impl Parameter for f32 {
    const KIND: Kind = Kind::Float;
}

impl Parameter for i32 {
    const KIND: Kind = Kind::Int;
}

impl Parameter for bool {
    const KIND: Kind = Kind::Bool;
}

impl Parameter for Color {
    const KIND: Kind = Kind::Color;
}

/// The parameters of an effect, declared with `#[derive(Params)]`: the table of what they are.
pub trait Declared: Default {
    /// The effect's parameters, described.
    fn table() -> Table;
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
    pub doc: &'static str,
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

/// The kind of value a parameter holds.
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

/// The unit a number is in.
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

/// A colour: red, green and blue, each 0..1. Given as a list of three numbers.
#[derive(Clone, Copy, Debug, PartialEq, Deserialize)]
#[serde(transparent)]
pub struct Color(pub [f32; 3]);

#[cfg(test)]
mod tests {
    use super::{Choice, Color, Declared, Params};
    use serde::Deserialize;
    use serde_json::json;

    /// The shapes of the probe.
    #[derive(Debug, Clone, Copy, PartialEq, Choice)]
    enum Shape {
        Stripes,
        Checks,
        FourthWay,
    }

    /// The parameters of a probe effect, which reads them as the Python package gives them.
    #[derive(Debug, Deserialize, Params)]
    #[serde(deny_unknown_fields)]
    struct Probe {
        /// The size of one cell in px, over
        /// two lines. Larger = coarser.
        #[param(default = 32.0, unit = px, min = 2, max = 512)]
        cell: f32,
        /// How many cells.
        #[param(default = 4, unit = count, min = -3, max = 32)]
        cells: i32,
        /// The direction: 'stripes' or 'checks'.
        #[param(default = 0.5, unit = degrees, min = 0, max = 360, cyclic, drivable)]
        angle: f32,
        /// The shape.
        #[param(default = FourthWay, advanced)]
        shape: Shape,
        /// Whether it is kept.
        #[param(default = true)]
        keep: bool,
        /// The colour.
        #[param(default = "#ff8000")]
        tint: Color,
    }

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
    fn the_table_describes_every_field_as_written_in_declaration_order() {
        let table = serde_json::to_value(Probe::table());
        let expected = json!({
            "doc": "The parameters of a probe effect, which reads them as the Python package gives them.",
            "fields": [
                {"name": "cell", "kind": "float", "doc": "The size of one cell in px, over two lines. Larger = coarser.",
                 "default": 32.0, "unit": "px", "min": 2, "max": 512, "cyclic": false, "choices": [],
                 "drivable": false, "advanced": false},
                {"name": "cells", "kind": "int", "doc": "How many cells.",
                 "default": 4, "unit": "count", "min": -3, "max": 32, "cyclic": false, "choices": [],
                 "drivable": false, "advanced": false},
                {"name": "angle", "kind": "float", "doc": "The direction: 'stripes' or 'checks'.",
                 "default": 0.5, "unit": "degrees", "min": 0, "max": 360, "cyclic": true, "choices": [],
                 "drivable": true, "advanced": false},
                {"name": "shape", "kind": "choice", "doc": "The shape.",
                 "default": "fourth_way", "unit": null, "min": null, "max": null, "cyclic": false,
                 "choices": ["stripes", "checks", "fourth_way"], "drivable": false, "advanced": true},
                {"name": "keep", "kind": "bool", "doc": "Whether it is kept.",
                 "default": true, "unit": null, "min": null, "max": null, "cyclic": false, "choices": [],
                 "drivable": false, "advanced": false},
                {"name": "tint", "kind": "color", "doc": "The colour.",
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
