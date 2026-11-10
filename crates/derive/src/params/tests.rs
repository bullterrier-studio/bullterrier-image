//! Tests of `#[derive(Params)]`: what the macro accepts and what it refuses, naming the field.

use super::expand;
use syn::{DeriveInput, parse_quote};

/// A documented `Params` struct whose one field, documented, is written in the call.
macro_rules! documented {
    ($($field:tt)*) => {{
        let input: DeriveInput = parse_quote! {
            /// The parameters of a probe.
            struct Params {
                /// A field.
                $($field)*
            }
        };
        input
    }};
}

fn expanded(input: &DeriveInput) -> Result<String, String> {
    expand(input)
        .map(|code| code.to_string())
        .map_err(|error| error.to_string())
}

/// Each row must be refused with an error that names every word of `named`.
fn assert_refused(rows: &[(&str, DeriveInput, &[&str])]) {
    for (rule, input, named) in rows {
        let error = expanded(input).err();
        assert!(
            error
                .as_deref()
                .is_some_and(|text| named.iter().all(|name| text.contains(name))),
            "{rule}: {error:?}"
        );
    }
}

#[test]
fn a_complete_declaration_is_accepted_for_every_kind_of_field() {
    let good = [
        documented! { #[param(default = 4.0, unit = px, min = 0, max = 32, drivable, advanced)] cell: f32, },
        documented! { #[param(default = 4, unit = count, min = 3, max = 32)] cells: i32, },
        // a whole number may span all of i32
        documented! { #[param(default = 0, unit = count, min = -2147483648, max = 2147483647)] cells: i32, },
        // an unsigned whole number may span all of u32
        documented! { #[param(default = 0, unit = count, min = 0, max = 4294967295)] seed: u32, },
        // the bounds are inclusive: a default may sit on either
        documented! { #[param(default = 0.0, unit = px, min = 0, max = 32)] low: f32, },
        documented! { #[param(default = 32.0, unit = px, min = 0, max = 32)] high: f32, },
        documented! { #[param(default = false)] keep: bool, },
        documented! { #[param(default = "#101018")] tint: Color, },
        documented! { #[param(default = ["#12002b", "#ff2bd6"], min_length = 2)] stops: Vec<Color>, },
        // a floor and a ceiling are inclusive: a default may sit on either, a trailing comma is fine
        documented! { #[param(default = ["#000000", "#ffffff",], min_length = 1, max_length = 2)] stops: Vec<Color>, },
        documented! { #[param(default = ["#000000"], min_length = 1, max_length = 1, advanced)] stops: Vec<Color>, },
        documented! { #[param()] file: LutName, },
        documented! { #[param(advanced)] file: LutName, },
        documented! { #[param()] image: PictureName, },
        documented! { #[param(default = "depth")] field: FieldName, },
        documented! { #[param(default = "depth", advanced)] field: FieldName, },
        documented! { #[param()] color: Option<Color>, },
        documented! { #[param(advanced)] color: Option<Color>, },
        documented! { #[param(default = Grid)] kind: Kind, },
        documented! { #[param(default = 0.0, unit = degrees, min = 0, max = 360, cyclic)] angle: f32, },
        // as rustfmt wraps a long attribute
        documented! {
            #[param(
                default = -90.0,
                unit = degrees,
                min = -180,
                max = 180,
                cyclic,
            )]
            lean: f32,
        },
    ];
    for input in &good {
        let code = expanded(input);
        assert!(code.as_ref().is_ok_and(|code| code.contains("Declared")), "{code:?}");
    }
}

#[test]
fn a_declaration_missing_a_part_is_refused_naming_the_field() {
    assert_refused(&[
        (
            "no doc",
            parse_quote! { /// The parameters.
            struct Params { #[param(default = 4.0, unit = px, min = 0, max = 32)] cell: f32, } },
            &["cell", "doc"],
        ),
        ("no param attribute", documented! { cell: f32, }, &["cell", "param"]),
        (
            "no default",
            documented! { #[param(unit = px, min = 0, max = 32)] cell: f32, },
            &["cell", "default"],
        ),
        (
            "no unit",
            documented! { #[param(default = 4.0, min = 0, max = 32)] cell: f32, },
            &["cell", "unit"],
        ),
        (
            "no min",
            documented! { #[param(default = 4.0, unit = px, max = 32)] cell: f32, },
            &["cell", "min"],
        ),
        (
            "no max",
            documented! { #[param(default = 4.0, unit = px, min = 0)] cell: f32, },
            &["cell", "max"],
        ),
        (
            "a value given twice",
            documented! { #[param(default = 4.0, unit = px, min = 0, max = 32, min = 1)] cell: f32, },
            &["cell", "`min`", "twice"],
        ),
        (
            "a flag given twice",
            documented! { #[param(default = 4.0, unit = px, min = 0, max = 32, drivable, drivable)] cell: f32, },
            &["cell", "`drivable`", "twice"],
        ),
        (
            "a second param attribute",
            documented! { #[param(default = 4.0, unit = px, min = 0, max = 32)] #[param(drivable)] cell: f32, },
            &["cell", "one #[param"],
        ),
        (
            "a setting nobody knows",
            documented! { #[param(default = 4.0, unit = px, min = 0, max = 32, strength = 2)] cell: f32, },
            &["cell", "strength"],
        ),
    ]);
}

#[test]
fn a_number_that_breaks_its_range_is_refused_naming_the_field() {
    assert_refused(&[
        (
            "min above max",
            documented! { #[param(default = 4.0, unit = px, min = 32, max = 0)] cell: f32, },
            &["cell", "min"],
        ),
        (
            "default below the range",
            documented! { #[param(default = -1.0, unit = px, min = 0, max = 32)] cell: f32, },
            &["cell", "default"],
        ),
        (
            "default above the range",
            documented! { #[param(default = 33, unit = px, min = 0, max = 32)] cell: f32, },
            &["cell", "default"],
        ),
        (
            "cyclic, not in degrees",
            documented! { #[param(default = 0.0, unit = px, min = 0, max = 360, cyclic)] cell: f32, },
            &["cell", "cyclic"],
        ),
        (
            "cyclic, not a turn",
            documented! { #[param(default = 0.0, unit = degrees, min = 0, max = 180, cyclic)] cell: f32, },
            &["cell", "cyclic"],
        ),
        (
            "a decimal default for a whole number",
            documented! { #[param(default = 4.5, unit = count, min = 3, max = 32)] cells: i32, },
            &["cells", "whole"],
        ),
        (
            "a decimal bound for a whole number",
            documented! { #[param(default = 4, unit = count, min = 3, max = 32.0)] cells: i32, },
            &["cells", "whole"],
        ),
        (
            "a whole number whose max is beyond i32",
            documented! { #[param(default = 4, unit = count, min = 3, max = 5000000000)] cells: i32, },
            &["cells", "max", "i32"],
        ),
        (
            "a whole number whose min is beyond i32",
            documented! { #[param(default = 4, unit = count, min = -2147483649, max = 32)] cells: i32, },
            &["cells", "min", "i32"],
        ),
        (
            "a whole number whose default is beyond i32",
            documented! { #[param(default = 5000000000, unit = count, min = 3, max = 6000000000)] cells: i32, },
            &["cells", "i32"],
        ),
        (
            "an unsigned whole number whose max is beyond u32",
            documented! { #[param(default = 0, unit = count, min = 0, max = 4294967296)] seed: u32, },
            &["seed", "max", "u32"],
        ),
        (
            "an unsigned whole number whose min is negative",
            documented! { #[param(default = 0, unit = count, min = -1, max = 32)] seed: u32, },
            &["seed", "min", "u32"],
        ),
        (
            "an unsigned whole number whose default is beyond u32",
            documented! { #[param(default = 4294967296, unit = count, min = 0, max = 4294967297)] seed: u32, },
            &["seed", "u32"],
        ),
        (
            "a whole number that is drivable",
            documented! { #[param(default = 4, unit = count, min = 3, max = 32, drivable)] cells: i32, },
            &["cells", "drivable"],
        ),
    ]);
}

#[test]
fn a_switch_colour_or_choice_with_a_setting_or_default_of_another_kind_is_refused_naming_the_field() {
    assert_refused(&[
        (
            "a unit on a switch",
            documented! { #[param(default = false, unit = px)] keep: bool, },
            &["keep", "unit"],
        ),
        (
            "a bound on a colour",
            documented! { #[param(default = "#101018", min = 0)] tint: Color, },
            &["tint", "min"],
        ),
        (
            "a bound on a choice",
            documented! { #[param(default = Grid, max = 3)] kind: Kind, },
            &["kind", "max"],
        ),
        (
            "cyclic on a switch",
            documented! { #[param(default = false, cyclic)] keep: bool, },
            &["keep", "cyclic"],
        ),
        (
            "drivable on a switch",
            documented! { #[param(default = false, drivable)] keep: bool, },
            &["keep", "drivable"],
        ),
        (
            "a colour that is not #rrggbb",
            documented! { #[param(default = "red")] tint: Color, },
            &["tint", "#rrggbb"],
        ),
        (
            "a number for a switch",
            documented! { #[param(default = 1.0)] keep: bool, },
            &["keep", "true or false"],
        ),
        (
            "a colour for a switch",
            documented! { #[param(default = "#ffffff")] keep: bool, },
            &["keep", "true or false"],
        ),
        (
            "a switch for a colour",
            documented! { #[param(default = true)] tint: Color, },
            &["tint", "#rrggbb"],
        ),
        (
            "a number for a colour",
            documented! { #[param(default = 1.0)] tint: Color, },
            &["tint", "#rrggbb"],
        ),
        (
            "a number for a choice",
            documented! { #[param(default = 1)] kind: Kind, },
            &["kind", "variants"],
        ),
        (
            "a switch for a choice",
            documented! { #[param(default = true)] kind: Kind, },
            &["kind", "variants"],
        ),
        (
            "a colour for a choice",
            documented! { #[param(default = "#ffffff")] kind: Kind, },
            &["kind", "variants"],
        ),
        (
            "a variant for a number",
            documented! { #[param(default = Grid, unit = px, min = 0, max = 32)] cell: f32, },
            &["cell", "number"],
        ),
        (
            "a colour whose digits are not all hex",
            documented! { #[param(default = "#+f+f+f")] tint: Color, },
            &["tint", "#rrggbb"],
        ),
    ]);
}

#[test]
#[expect(clippy::too_many_lines, reason = "a table of rows, one rule each")]
fn a_list_of_colours_that_breaks_its_rules_is_refused_naming_the_field() {
    assert_refused(&[
        (
            "no min_length",
            documented! { #[param(default = ["#000000"])] stops: Vec<Color>, },
            &["stops", "min_length"],
        ),
        (
            "a min_length of zero",
            documented! { #[param(default = ["#000000"], min_length = 0)] stops: Vec<Color>, },
            &["stops", "min_length", "1"],
        ),
        (
            "a min_length that is no whole number",
            documented! { #[param(default = ["#000000"], min_length = 1.5)] stops: Vec<Color>, },
            &["stops", "min_length", "whole"],
        ),
        (
            "a max_length that is no whole number",
            documented! { #[param(default = ["#000000"], min_length = 1, max_length = "two")] stops: Vec<Color>, },
            &["stops", "max_length", "whole"],
        ),
        (
            "min_length above max_length",
            documented! { #[param(default = ["#000000", "#ffffff"], min_length = 3, max_length = 2)] stops: Vec<Color>, },
            &["stops", "min_length", "max_length"],
        ),
        (
            "a default shorter than min_length",
            documented! { #[param(default = ["#000000"], min_length = 2)] stops: Vec<Color>, },
            &["stops", "default", "min_length"],
        ),
        (
            "a default longer than max_length",
            documented! { #[param(default = ["#000000", "#ffffff"], min_length = 1, max_length = 1)] stops: Vec<Color>, },
            &["stops", "default", "max_length"],
        ),
        (
            "an empty default",
            documented! { #[param(default = [], min_length = 1)] stops: Vec<Color>, },
            &["stops", "default"],
        ),
        (
            "an element that is not #rrggbb",
            documented! { #[param(default = ["#000000", "red"], min_length = 1)] stops: Vec<Color>, },
            &["stops", "#rrggbb"],
        ),
        (
            "an element in capitals, which the format does not write",
            documented! { #[param(default = ["#FFFFFF"], min_length = 1)] stops: Vec<Color>, },
            &["stops", "#rrggbb"],
        ),
        (
            "an element that is not text (rustc's error points at it)",
            documented! { #[param(default = ["#000000", 1], min_length = 1)] stops: Vec<Color>, },
            &["string literal"],
        ),
        (
            "one colour for a list",
            documented! { #[param(default = "#000000", min_length = 1)] stops: Vec<Color>, },
            &["stops", "list of"],
        ),
        (
            "a list for a colour",
            documented! { #[param(default = ["#000000"])] tint: Color, },
            &["tint", "#rrggbb"],
        ),
        (
            "a list for a switch",
            documented! { #[param(default = ["#000000"])] keep: bool, },
            &["keep", "true or false"],
        ),
        (
            "a unit on a list",
            documented! { #[param(default = ["#000000"], min_length = 1, unit = px)] stops: Vec<Color>, },
            &["stops", "unit"],
        ),
        (
            "a bound on a list",
            documented! { #[param(default = ["#000000"], min_length = 1, min = 0)] stops: Vec<Color>, },
            &["stops", "min"],
        ),
        (
            "drivable on a list",
            documented! { #[param(default = ["#000000"], min_length = 1, drivable)] stops: Vec<Color>, },
            &["stops", "drivable"],
        ),
        (
            "min_length on a colour",
            documented! { #[param(default = "#000000", min_length = 1)] tint: Color, },
            &["tint", "min_length", "list of colours"],
        ),
        (
            "max_length on a number",
            documented! { #[param(default = 4.0, unit = px, min = 0, max = 32, max_length = 3)] cell: f32, },
            &["cell", "max_length", "list of colours"],
        ),
        (
            "a default on a lookup table",
            documented! { #[param(default = "film.cube")] file: LutName, },
            &["file", "default", "identity"],
        ),
        (
            "a unit on a lookup table",
            documented! { #[param(unit = px)] file: LutName, },
            &["file", "unit"],
        ),
        (
            "a bound on a lookup table",
            documented! { #[param(max = 1)] file: LutName, },
            &["file", "max"],
        ),
        (
            "drivable on a lookup table",
            documented! { #[param(drivable)] file: LutName, },
            &["file", "drivable"],
        ),
        (
            "a default on a picture",
            documented! { #[param(default = "logo.png")] image: PictureName, },
            &["image", "default", "none"],
        ),
        (
            "a unit on a picture",
            documented! { #[param(unit = px)] image: PictureName, },
            &["image", "unit"],
        ),
        (
            "min_length on a lookup table",
            documented! { #[param(min_length = 1)] file: LutName, },
            &["file", "min_length"],
        ),
        (
            "no default on a field",
            documented! { #[param()] field: FieldName, },
            &["field", "default"],
        ),
        (
            "a field default that is no name",
            documented! { #[param(default = 3)] field: FieldName, },
            &["field", "name"],
        ),
        (
            "an empty field default",
            documented! { #[param(default = "")] field: FieldName, },
            &["field", "name"],
        ),
        (
            "a unit on a field",
            documented! { #[param(default = "depth", unit = px)] field: FieldName, },
            &["field", "unit"],
        ),
        (
            "drivable on a field",
            documented! { #[param(default = "depth", drivable)] field: FieldName, },
            &["field", "drivable"],
        ),
        (
            "a default on an optional colour",
            documented! { #[param(default = "#000000")] color: Option<Color>, },
            &["color", "default", "none"],
        ),
        (
            "a bound on an optional colour",
            documented! { #[param(max = 1)] color: Option<Color>, },
            &["color", "max"],
        ),
        (
            "min_length on an optional colour",
            documented! { #[param(min_length = 1)] color: Option<Color>, },
            &["color", "min_length"],
        ),
        (
            "a list of anything but colours",
            documented! { #[param(default = ["#000000"], min_length = 1)] stops: Vec<f32>, },
            &["stops", "Vec<Color>"],
        ),
    ]);
}

#[test]
fn a_type_the_derive_does_not_know_is_refused_naming_the_field_and_the_type() {
    assert_refused(&[
        (
            "f64 is not a decimal the derive knows",
            documented! { #[param(default = 4.0, unit = px, min = 0, max = 32)] cell: f64, },
            &["cell", "`f64`"],
        ),
        (
            "u64 is not a whole number the derive knows",
            documented! { #[param(default = 4, unit = count, min = 3, max = 32)] cells: u64, },
            &["cells", "`u64`"],
        ),
        (
            "an optional number is not a number",
            documented! { #[param(default = 4.0, unit = px, min = 0, max = 32)] cell: Option<f32>, },
            &["cell", "`Option<f32>`"],
        ),
        (
            "an alias of f32 is not recognised by its name",
            documented! { #[param(default = 4.0, unit = px, min = 0, max = 32)] cell: Real, },
            &["cell", "`Real`", "(a number is f32, i32 or u32)"],
        ),
        (
            "an unknown type with a number for its default",
            documented! { #[param(default = 4.0)] cell: f64, },
            &["cell", "`f64`", "variants"],
        ),
        (
            "an unknown type that is drivable",
            documented! { #[param(default = Grid, drivable)] kind: Kind, },
            &["kind", "`Kind`", "f32"],
        ),
    ]);
}

#[test]
fn what_is_not_a_documented_struct_of_named_fields_is_refused_naming_it() {
    let undocumented: DeriveInput = parse_quote! {
        struct Params {
            /// A size.
            #[param(default = 4.0, unit = px, min = 0, max = 32)]
            cell: f32,
        }
    };
    let tuple: DeriveInput = parse_quote! {
        /// The parameters.
        struct Params(f32);
    };
    let choice: DeriveInput = parse_quote! {
        /// The parameters.
        enum Params { A }
    };
    assert_refused(&[
        ("no doc on the struct", undocumented, &["Params", "doc"]),
        ("a tuple struct", tuple, &["Params", "named fields"]),
        ("an enum", choice, &["Params", "struct"]),
    ]);
}

/// A struct of two fields: `first`, `second` (each `#[param(..)] name: type,`), documented.
macro_rules! pair {
    ({ $($first:tt)* }, { $($second:tt)* }) => {{
        let input: DeriveInput = parse_quote! {
            /// The parameters of a probe.
            struct Params {
                /// The first.
                $($first)*
                /// The second.
                $($second)*
            }
        };
        input
    }};
}

#[test]
fn a_number_may_be_limited_by_another_in_its_unit_whose_default_it_does_not_exceed() {
    let good = [
        pair! {
            { #[param(default = 10.0, unit = px, min = 0, max = 2000, at_most = high)] low: f32, },
            { #[param(default = 100.0, unit = px, min = 0, max = 2000)] high: f32, }
        },
        // equal defaults, a whole number limited by a decimal, and the limit declared before the field
        pair! {
            { #[param(default = 5.0, unit = count, min = 0, max = 9)] high: f32, },
            { #[param(default = 5, unit = count, min = 0, max = 9, at_most = high)] low: i32, }
        },
    ];
    for input in &good {
        let code = expanded(input);
        assert!(code.as_ref().is_ok_and(|code| code.contains("at_most")), "{code:?}");
    }
}

#[test]
fn an_at_most_that_names_no_other_number_of_its_unit_or_a_lower_default_is_refused_naming_the_field() {
    let refused = [
        (
            "a field that does not exist",
            pair! {
                { #[param(default = 10.0, unit = px, min = 0, max = 99, at_most = tall)] low: f32, },
                { #[param(default = 90.0, unit = px, min = 0, max = 99)] high: f32, }
            },
            &["low", "tall"][..],
        ),
        (
            "the field itself",
            pair! {
                { #[param(default = 10.0, unit = px, min = 0, max = 99, at_most = low)] low: f32, },
                { #[param(default = 90.0, unit = px, min = 0, max = 99)] high: f32, }
            },
            &["low", "another number"],
        ),
        (
            "a switch",
            pair! {
                { #[param(default = 10.0, unit = px, min = 0, max = 99, at_most = high)] low: f32, },
                { #[param(default = true)] high: bool, }
            },
            &["low", "high", "another number"],
        ),
        (
            "another unit",
            pair! {
                { #[param(default = 10.0, unit = px, min = 0, max = 99, at_most = high)] low: f32, },
                { #[param(default = 90.0, unit = degrees, min = 0, max = 99)] high: f32, }
            },
            &["low", "high", "degrees"],
        ),
        (
            "a default above the limit's",
            pair! {
                { #[param(default = 50.0, unit = px, min = 0, max = 99, at_most = high)] low: f32, },
                { #[param(default = 40.0, unit = px, min = 0, max = 99)] high: f32, }
            },
            &["low", "high", "default"],
        ),
        (
            "a field that is no number",
            pair! {
                { #[param(default = true, at_most = high)] low: bool, },
                { #[param(default = 90.0, unit = px, min = 0, max = 99)] high: f32, }
            },
            &["low", "at_most"],
        ),
    ];
    for (rule, input, named) in &refused {
        let error = expanded(input).err();
        assert!(
            error
                .as_deref()
                .is_some_and(|text| named.iter().all(|name| text.contains(name))),
            "{rule}: {error:?}"
        );
    }
}
