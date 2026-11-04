//! A list of colours declared with `#[derive(Params)]`, end to end: the table the derive writes, the default it builds,
//! the values it reads back and the rules `Table::check` takes from it. The derive writes paths into `crate::params`, so
//! it compiles only inside the core: this is the one place a struct can be declared without being an effect. The cases
//! of the derive's own refusals are in its crate; the check rules for a table written by hand are in `tests/it`.

use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::{Color, Declared, LutName, Params};
use crate::{Constraint, Error};

/// The parameters of a gradient.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
struct Gradient {
    /// The colours of the ramp in order, at least two. Raising their number adds stops.
    #[param(default = ["#12002b", "#ff2bd6"], min_length = 2)]
    colors: Vec<Color>,
    /// The colours the marks cycle through, two to four.
    #[param(default = ["#000000", "#ffffff"], min_length = 2, max_length = 4, advanced)]
    palette: Vec<Color>,
    /// The table graded with.
    #[param()]
    file: LutName,
}

fn check(given: &Value) -> Result<Map<String, Value>, Error> {
    Gradient::table().check("gradient", given.as_object().unwrap_or(&Map::new()))
}

/// Whether `a` and `b` are the same JSON up to `f32` precision in a number: a typed value holds `f32`, a checked one `f64`.
fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => match (x.as_f64(), y.as_f64()) {
            (Some(x), Some(y)) => (x - y).abs() <= 1e-6,
            _ => false,
        },
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same(x, y)),
        _ => a == b,
    }
}

#[test]
fn the_table_describes_each_list_with_its_lengths_and_the_texts_of_its_default() {
    let written = serde_json::to_value(Gradient::table()).ok();
    let field = |at: usize| written.as_ref().map(|table| table["fields"][at].clone());
    assert_eq!(
        field(0),
        Some(json!({
            "name": "colors",
            "kind": "colors",
            "meaning": "The colours of the ramp in order, at least two. Raising their number adds stops.",
            "default": ["#12002b", "#ff2bd6"],
            "unit": null, "min": null, "max": null, "cyclic": false, "choices": [],
            "min_length": 2,
            "drivable": false, "advanced": false,
        }))
    );
    assert_eq!(
        field(1),
        Some(json!({
            "name": "palette",
            "kind": "colors",
            "meaning": "The colours the marks cycle through, two to four.",
            "default": ["#000000", "#ffffff"],
            "unit": null, "min": null, "max": null, "cyclic": false, "choices": [],
            "min_length": 2, "max_length": 4,
            "drivable": false, "advanced": true,
        }))
    );
}

#[test]
fn a_lookup_table_has_no_value_by_default_and_is_written_as_its_name() -> Result<(), Error> {
    let written = serde_json::to_value(Gradient::table()).ok();
    let field = written.map(|table| table["fields"][2].clone());
    assert_eq!(field.as_ref().map(|field| &field["kind"]), Some(&json!("lut")));
    assert_eq!(field.map(|field| field["default"].clone()), Some(Value::Null));
    assert_eq!(Gradient::default().file, LutName(None));
    assert_eq!(Gradient::default().values()["file"], Value::Null);
    let named = Gradient {
        file: LutName::new("film"),
        ..Gradient::default()
    };
    assert_eq!(named.values()["file"], json!("film"));
    // what the check makes of a name reads back into the struct
    let read: Gradient = serde_json::from_value(Value::Object(check(&json!({"file": "film"}))?))
        .map_err(|error| Error::Recipe(error.to_string()))?;
    assert_eq!(read.file, LutName::new("film"));
    Ok(())
}

#[test]
fn the_default_of_the_struct_is_the_default_of_the_table() -> Result<(), Error> {
    let from_table = check(&json!({}))?;
    let from_struct = Gradient::default().values();
    assert_eq!(
        from_struct.keys().collect::<Vec<_>>(),
        from_table.keys().collect::<Vec<_>>()
    );
    for (name, value) in &from_struct {
        assert!(
            same(value, &from_table[name]),
            "{name}: {value} against {}",
            from_table[name]
        );
    }
    // an independent reading of "#12002b": 0x12, 0x00 and 0x2b over 255
    assert!(same(
        &from_struct["colors"][0],
        &json!([18.0 / 255.0, 0.0, 43.0 / 255.0])
    ));
    Ok(())
}

#[test]
fn values_write_each_colour_as_the_three_numbers_a_person_would() {
    let gradient = Gradient {
        colors: vec![Color([0.1, 0.3, 1.0]), Color([0.0, 0.5, 0.2])],
        ..Gradient::default()
    };
    assert_eq!(gradient.values()["colors"], json!([[0.1, 0.3, 1.0], [0.0, 0.5, 0.2]]));
}

#[test]
fn what_the_check_makes_of_a_list_is_read_back_into_the_struct_in_order() -> Result<(), Error> {
    let checked = check(&json!({"colors": ["#fff", [0, 0.5, 1], "#000000"]}))?;
    let read: Gradient =
        serde_json::from_value(Value::Object(checked)).map_err(|error| Error::Recipe(error.to_string()))?;
    assert_eq!(
        read.colors,
        vec![Color([1.0, 1.0, 1.0]), Color([0.0, 0.5, 1.0]), Color([0.0, 0.0, 0.0])]
    );
    assert_eq!(read.palette, Gradient::default().palette);
    Ok(())
}

#[test]
fn the_check_takes_its_lengths_from_the_declaration() {
    let refusal = |given: Value| match check(&given) {
        Err(Error::Invalid { field, constraint, .. }) => Some((field, constraint)),
        _ => None,
    };
    assert_eq!(
        refusal(json!({"colors": ["#fff"]})),
        Some(("colors".to_owned(), Constraint::TooFew(2)))
    );
    assert_eq!(
        refusal(json!({"palette": ["#111", "#222", "#333", "#444", "#555"]})),
        Some(("palette".to_owned(), Constraint::TooMany(4)))
    );
    assert_eq!(
        refusal(json!({"palette": ["#111", "nope"]})),
        Some(("palette".to_owned(), Constraint::ColourAt(1)))
    );
    // no ceiling on `colors`
    assert!(check(&json!({"colors": vec!["#111"; 50]})).is_ok());
}
