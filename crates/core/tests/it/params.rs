//! How an effect declares its parameters and how a user's values are checked against them, through the public API
//! alone. The declaration is the core's own effects' (`grade`, `pattern`, `pixel_grid`); the checks run against
//! `common::recipe::probe_table`, a table written by hand with one parameter of every kind.

use crate::common::recipe::probe_table;
use bullterrier_image::effects::{grade, light_leak, pattern, pixel_grid};
use bullterrier_image::params::{Choice, Color, Declared, EffectParams, Scalar};
use bullterrier_image::{Constraint, Error};
use serde_json::{Map, Value, json};

/// Whether `a` and `b` are the same JSON up to `f32` precision in a number: a typed value holds `f32`, a checked one `f64`.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(x), Some(y)) => (x - y).abs() <= 1e-6 * x.abs().max(1.0),
            _ => false,
        },
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same(x, y)),
        _ => a == b,
    }
}

/// The values of `P::default()` against what its table makes of nothing given: the two places a default is declared.
fn default_agrees_with_table<P: EffectParams>() -> Result<(), Error> {
    let from_table = P::table().check(P::EFFECT, &Map::new())?;
    let from_default = P::default().values();
    assert_eq!(
        from_default.keys().collect::<Vec<_>>(),
        from_table.keys().collect::<Vec<_>>(),
        "{}: the fields",
        P::EFFECT
    );
    for (name, value) in &from_default {
        assert!(
            same(value, &from_table[name]),
            "{}.{name}: the default is {value}, the table's is {}",
            P::EFFECT,
            from_table[name]
        );
    }
    Ok(())
}

#[test]
fn a_default_is_what_the_field_declares() -> Result<(), Error> {
    // a decimal, a whole number, a switch, a choice and a colour between them
    default_agrees_with_table::<grade::Params>()?;
    default_agrees_with_table::<pixel_grid::Params>()?;
    default_agrees_with_table::<pattern::Params>()
}

#[test]
fn values_are_the_numbers_a_person_would_write() {
    // 0.2 must not widen to 0.20000000298; a whole decimal stays a decimal; a whole number stays whole; a switch is
    // a switch; a colour is three numbers; a choice its name
    let grid = pixel_grid::Params {
        cell: 7,
        gap: 0.2,
        amount: 1.0,
        keep_brightness: true,
    };
    let expected = json!({"cell": 7, "gap": 0.2, "amount": 1.0, "keep_brightness": true});
    assert_eq!(Value::Object(grid.values()), expected);
    let pattern = pattern::Params {
        kind: pattern::Kind::Checks,
        foreground: Color([0.1, 0.3, 1.0]),
        ..pattern::Params::default()
    };
    let values = pattern.values();
    assert_eq!(values["kind"], json!("checks"));
    assert_eq!(values["foreground"], json!([0.1, 0.3, 1.0]));
}

#[test]
fn a_choice_names_its_variant() {
    use pattern::Kind;
    let names = [Kind::Stripes, Kind::Checks, Kind::Dots, Kind::Grid].map(|kind| kind.name());
    assert_eq!(names, ["stripes", "checks", "dots", "grid"]);
    assert_eq!(Kind::CHOICES, names);
}

#[test]
fn each_effect_file_ties_its_params_to_the_effect_called_as_the_file() {
    assert_eq!(<grade::Params as EffectParams>::EFFECT, "grade");
    assert_eq!(<pattern::Params as EffectParams>::EFFECT, "pattern");
    assert_eq!(<pixel_grid::Params as EffectParams>::EFFECT, "pixel_grid");
}

#[test]
fn a_table_is_written_with_every_field_as_the_python_package_reads_it() {
    let table = serde_json::to_value(probe_table());
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
    use pattern::Kind;
    let read = |name: &str| serde_json::from_value::<Kind>(json!(name));
    assert_eq!(read("checks").ok(), Some(Kind::Checks));
    assert_eq!(read("grid").ok(), Some(Kind::Grid));
    let message = read("stripe").err().map(|error| error.to_string());
    assert!(
        message
            .as_deref()
            .is_some_and(|text| text.contains("stripe") && text.contains("checks") && text.contains("dots")),
        "{message:?}"
    );
    assert!(serde_json::from_value::<Kind>(json!(2)).is_err());
}

#[test]
fn parameters_as_python_gives_them_are_read_into_the_struct() {
    let read: Result<pixel_grid::Params, _> =
        serde_json::from_value(json!({"cell": 5, "gap": 0.5, "amount": 0.25, "keep_brightness": true}));
    assert_eq!(
        read.ok(),
        Some(pixel_grid::Params {
            cell: 5,
            gap: 0.5,
            amount: 0.25,
            keep_brightness: true,
        })
    );
    // a field that is not the effect's is refused
    let unknown: Result<pixel_grid::Params, _> = serde_json::from_value(json!({"cell": 5, "kelvin": 1}));
    assert!(unknown.is_err());
}

/// The probe's parameters checked for `given` (a JSON object).
fn check(given: &Value) -> Result<Value, Error> {
    let given = given.as_object().cloned().unwrap_or_default();
    probe_table().check("probe", &given).map(Value::Object)
}

#[test]
fn nothing_given_gives_every_default_in_its_normal_form() {
    // a colour is three numbers, a float default is a float, a choice is its name
    let expected = json!({
        "cell": 32.0, "cells": 4, "angle": 0.5, "shape": "fourth_way", "keep": true,
        "tint": [1.0, 128.0 / 255.0, 0.0],
    });
    assert_eq!(check(&json!({})).ok(), Some(expected));
}

#[test]
fn distinct_values_come_back_in_place() {
    let given = json!({"cell": 8, "cells": 7, "angle": 90.0, "shape": "checks", "keep": false, "tint": "#0af"});
    let expected = json!({
        "cell": 8.0, "cells": 7, "angle": 90.0, "shape": "checks", "keep": false,
        "tint": [0.0, 170.0 / 255.0, 1.0],
    });
    assert_eq!(check(&given).ok(), Some(expected));
}

#[test]
fn one_value_given_leaves_the_rest_at_their_defaults() {
    let one = check(&json!({"cells": -2})).ok();
    let field = |name: &str| one.as_ref().map(|checked| checked[name].clone());
    assert_eq!((field("cells"), field("cell")), (Some(json!(-2)), Some(json!(32.0))));
}

/// (rule, given for one field, the field, what it turns into): values that pass, at and inside each bound.
fn good() -> Vec<(&'static str, Value, &'static str, Value)> {
    vec![
        ("float at its lowest", json!({"cell": 2}), "cell", json!(2.0)),
        ("float at its highest", json!({"cell": 512.0}), "cell", json!(512.0)),
        ("float takes a whole number", json!({"cell": 100}), "cell", json!(100.0)),
        ("int at its lowest", json!({"cells": -3}), "cells", json!(-3)),
        ("int at its highest", json!({"cells": 32}), "cells", json!(32)),
        (
            "cyclic: 360 is inside the turn",
            json!({"angle": 360}),
            "angle",
            json!(360.0),
        ),
        ("cyclic: 0 is inside the turn", json!({"angle": 0}), "angle", json!(0.0)),
        (
            "cyclic: a fraction is inside",
            json!({"angle": 0.1}),
            "angle",
            json!(0.1),
        ),
        (
            "cyclic: past the turn wraps",
            json!({"angle": 725}),
            "angle",
            json!(5.0),
        ),
        (
            "cyclic: below the turn wraps",
            json!({"angle": -90}),
            "angle",
            json!(270.0),
        ),
        (
            "cyclic: a hair below wraps to the top",
            json!({"angle": -1e-20}),
            "angle",
            json!(360.0),
        ),
        (
            "a choice by name",
            json!({"shape": "stripes"}),
            "shape",
            json!("stripes"),
        ),
        (
            "colour: short hex",
            json!({"tint": "#0af"}),
            "tint",
            json!([0.0, 170.0 / 255.0, 1.0]),
        ),
        (
            "colour: capital hex, spaces around",
            json!({"tint": " #FF8000 "}),
            "tint",
            json!([1.0, 128.0 / 255.0, 0.0]),
        ),
        (
            "colour: three whole numbers",
            json!({"tint": [0, 1, 1]}),
            "tint",
            json!([0.0, 1.0, 1.0]),
        ),
        (
            "colour: three decimals",
            json!({"tint": [0.25, 0.5, 0.75]}),
            "tint",
            json!([0.25, 0.5, 0.75]),
        ),
        ("bool", json!({"keep": false}), "keep", json!(false)),
    ]
}

#[test]
fn values_the_declaration_allows_are_accepted_in_normal_form() {
    for (rule, given, field, expected) in good() {
        let result = check(&given);
        assert_eq!(
            result.as_ref().ok().map(|checked| checked[field].clone()),
            Some(expected),
            "{rule}: {result:?}"
        );
    }
}

/// (rule, given, the field blamed, the rule broken, the value shown).
#[expect(clippy::too_many_lines, reason = "a table of rows, one rule each")]
fn bad() -> Vec<(&'static str, Value, &'static str, Constraint, Value)> {
    let shapes = &["stripes", "checks", "fourth_way"][..];
    let fields = vec!["cell", "cells", "angle", "shape", "keep", "tint"];
    vec![
        (
            "unknown key",
            json!({"kelvin": 5000}),
            "kelvin",
            Constraint::Unknown(fields),
            json!(5000),
        ),
        (
            "float below its lowest",
            json!({"cell": 1.5}),
            "cell",
            Constraint::AtLeast(Scalar::Int(2)),
            json!(1.5),
        ),
        (
            "float above its highest",
            json!({"cell": 512.5}),
            "cell",
            Constraint::AtMost(Scalar::Int(512)),
            json!(512.5),
        ),
        (
            "float as text",
            json!({"cell": "8"}),
            "cell",
            Constraint::Number,
            json!("8"),
        ),
        (
            "float as bool",
            json!({"cell": true}),
            "cell",
            Constraint::Number,
            json!(true),
        ),
        (
            "float as null",
            json!({"cell": null}),
            "cell",
            Constraint::Number,
            json!(null),
        ),
        (
            "int below its lowest",
            json!({"cells": -4}),
            "cells",
            Constraint::AtLeast(Scalar::Int(-3)),
            json!(-4),
        ),
        (
            "int above its highest",
            json!({"cells": 33}),
            "cells",
            Constraint::AtMost(Scalar::Int(32)),
            json!(33),
        ),
        (
            "int as a whole decimal",
            json!({"cells": 3.0}),
            "cells",
            Constraint::WholeNumber,
            json!(3.0),
        ),
        (
            "int as a fraction",
            json!({"cells": 2.5}),
            "cells",
            Constraint::WholeNumber,
            json!(2.5),
        ),
        (
            "int as bool",
            json!({"cells": true}),
            "cells",
            Constraint::WholeNumber,
            json!(true),
        ),
        (
            "choice not listed",
            json!({"shape": "stripe"}),
            "shape",
            Constraint::OneOf(shapes),
            json!("stripe"),
        ),
        (
            "choice as number",
            json!({"shape": 2}),
            "shape",
            Constraint::OneOf(shapes),
            json!(2),
        ),
        (
            "bool as number",
            json!({"keep": 1}),
            "keep",
            Constraint::TrueOrFalse,
            json!(1),
        ),
        (
            "bool as text",
            json!({"keep": "true"}),
            "keep",
            Constraint::TrueOrFalse,
            json!("true"),
        ),
        (
            "colour: two hex digits",
            json!({"tint": "#12"}),
            "tint",
            Constraint::Colour,
            json!("#12"),
        ),
        (
            "colour: not hex",
            json!({"tint": "#gggggg"}),
            "tint",
            Constraint::Colour,
            json!("#gggggg"),
        ),
        (
            "colour: no hash",
            json!({"tint": "ff8000"}),
            "tint",
            Constraint::Colour,
            json!("ff8000"),
        ),
        (
            "colour: a channel above 1",
            json!({"tint": [0, 0, 2]}),
            "tint",
            Constraint::Colour,
            json!([0, 0, 2]),
        ),
        (
            "colour: a channel below 0",
            json!({"tint": [-0.5, 0, 1]}),
            "tint",
            Constraint::Colour,
            json!([-0.5, 0, 1]),
        ),
        (
            "colour: two channels",
            json!({"tint": [0, 0]}),
            "tint",
            Constraint::Colour,
            json!([0, 0]),
        ),
        (
            "colour: a bool channel",
            json!({"tint": [true, 0, 0]}),
            "tint",
            Constraint::Colour,
            json!([true, 0, 0]),
        ),
        (
            "colour: a number",
            json!({"tint": 7}),
            "tint",
            Constraint::Colour,
            json!(7),
        ),
    ]
}

#[test]
fn values_that_break_a_rule_are_refused_naming_the_effect_the_field_the_rule_and_the_value() {
    for (rule, given, field, constraint, value) in bad() {
        let result = check(&given);
        assert!(
            matches!(
                &result,
                Err(Error::Invalid { place: None, effect, field: blamed, constraint: broken, value: shown })
                    if effect == "probe" && blamed == field && *broken == constraint && **shown == value
            ),
            "{rule}: {result:?}"
        );
    }
}

#[test]
fn a_refusal_reads_in_one_sentence_like_the_python_package_says_it() {
    let message = |given: Value| check(&given).err().map(|error| error.to_string());
    assert_eq!(
        message(json!({"cell": 1.5})).as_deref(),
        Some("effect 'probe': `cell` must be >= 2, got 1.5")
    );
    assert_eq!(
        message(json!({"cells": 33})).as_deref(),
        Some("effect 'probe': `cells` must be <= 32, got 33")
    );
    assert_eq!(
        message(json!({"shape": "stripe"})).as_deref(),
        Some("effect 'probe': `shape` must be one of stripes, checks, fourth_way, got \"stripe\"")
    );
    assert_eq!(
        message(json!({"kelvin": 1})).as_deref(),
        Some("effect 'probe': `kelvin` is not a parameter; the parameters are: cell, cells, angle, shape, keep, tint")
    );
}

#[test]
fn the_given_map_is_left_as_it_was() {
    let given: Map<String, Value> = json!({"cell": 8}).as_object().cloned().unwrap_or_default();
    let before = given.clone();
    assert!(probe_table().check("probe", &given).is_ok());
    assert_eq!(given, before);
}

#[test]
fn a_seed_takes_every_unsigned_32_bit_number_and_nothing_below_or_above() {
    let seed = |value: Value| {
        let given = json!({ "seed": value }).as_object().cloned().unwrap_or_default();
        light_leak::Params::table()
            .check("light_leak", &given)
            .map(|checked| checked["seed"].clone())
    };
    assert_eq!(seed(json!(0)).ok(), Some(json!(0)));
    assert_eq!(seed(json!(u32::MAX)).ok(), Some(json!(u32::MAX)));
    for beyond in [json!(i64::from(u32::MAX) + 1), json!(-1)] {
        assert!(
            matches!(seed(beyond.clone()), Err(Error::Invalid { field, .. }) if field == "seed"),
            "{beyond} is no seed"
        );
    }
    // the typed struct holds the highest seed and writes it back as written
    let params = light_leak::Params {
        seed: u32::MAX,
        ..light_leak::Params::default()
    };
    assert_eq!(params.values()["seed"], json!(u32::MAX));
}

#[test]
fn a_count_held_as_usize_is_a_whole_number_in_the_table_and_the_values() {
    let given = |value: Value| json!({ "pieces": value }).as_object().cloned().unwrap_or_default();
    let checked = |value: Value| {
        bullterrier_image::effects::shatter::Params::table()
            .check("shatter", &given(value))
            .map(|values| values["pieces"].clone())
    };
    assert_eq!(checked(json!(7)).ok(), Some(json!(7)));
    for bad in [json!(-1), json!(1.5), json!(1_000_000)] {
        assert!(
            matches!(checked(bad.clone()), Err(Error::Invalid { field, .. }) if field == "pieces"),
            "{bad} is no count of pieces"
        );
    }
    // the typed struct holds a usize and writes it back as written
    let params = bullterrier_image::effects::shatter::Params {
        pieces: 7_usize,
        ..Default::default()
    };
    assert_eq!(params.values()["pieces"], json!(7));
}
