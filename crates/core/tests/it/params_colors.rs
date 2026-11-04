//! A list of colours as a declared parameter, through the public API alone: what the table says of it, what a user's
//! values become and which rules they break. The table is written by hand, one list with a floor (`stops`, at least one)
//! and one with both ends (`palette`, two to four), as `Vec<Color>` fields are declared in an effect.

use bullterrier_image::params::{Field, Kind, Scalar, Table};
use bullterrier_image::{Constraint, Error};
use serde_json::{Value, json};

/// A list of colours of `least` to `most` (none: no limit) elements, with `default`.
fn colors(name: &'static str, default: &'static [&'static str], (least, most): (usize, Option<usize>)) -> Field {
    Field {
        name,
        kind: Kind::Colors,
        meaning: "Colours.",
        default: Scalar::List(default),
        unit: None,
        min: None,
        max: None,
        cyclic: false,
        choices: &[],
        min_length: Some(least),
        max_length: most,
        nullable: false,
        drivable: false,
        advanced: false,
    }
}

fn table() -> Table {
    Table {
        doc: "The parameters of a gradient.",
        fields: vec![
            colors("stops", &["#12002b", "#ff2bd6"], (1, None)),
            colors("palette", &["#000000", "#ffffff"], (2, Some(4))),
        ],
    }
}

/// `given` (a JSON object) checked against the table.
fn check(given: &Value) -> Result<Value, Error> {
    let given = given.as_object().cloned().unwrap_or_default();
    table().check("gradient", &given).map(Value::Object)
}

/// What the check says of `given`: the field blamed, the rule broken and the value shown.
fn refusal(given: &Value) -> Option<(String, Constraint, Value)> {
    match check(given) {
        Err(Error::Invalid {
            effect,
            field,
            constraint,
            value,
            ..
        }) if effect == "gradient" => Some((field, constraint, *value)),
        _ => None,
    }
}

/// A colour of 0..1 channels from its 8-bit levels.
fn levels(red: u8, green: u8, blue: u8) -> Value {
    json!([
        f64::from(red) / 255.0,
        f64::from(green) / 255.0,
        f64::from(blue) / 255.0
    ])
}

#[test]
fn nothing_given_gives_each_list_its_default_as_channels() {
    let expected = json!({
        "stops": [levels(0x12, 0x00, 0x2b), levels(0xff, 0x2b, 0xd6)],
        "palette": [levels(0, 0, 0), levels(255, 255, 255)],
    });
    assert_eq!(check(&json!({})).ok(), Some(expected));
}

#[test]
fn the_table_writes_a_list_with_its_lengths_and_a_default_as_the_texts_of_its_colours() {
    let written = serde_json::to_value(table()).ok();
    let field = |at: usize| written.as_ref().map(|table| table["fields"][at].clone());
    assert_eq!(
        field(0),
        Some(json!({
            "name": "stops", "kind": "colors", "meaning": "Colours.", "default": ["#12002b", "#ff2bd6"],
            "unit": null, "min": null, "max": null, "cyclic": false, "choices": [], "min_length": 1,
            "drivable": false, "advanced": false,
        })),
        "no max_length is written for a list with no limit"
    );
    assert_eq!(field(1).map(|palette| palette["max_length"].clone()), Some(json!(4)));
}

#[test]
fn every_spelling_of_a_colour_is_read_as_a_scalar_colour_is_and_they_mix() {
    let given = json!({"stops": ["#0af", " #FF8000 ", [0.25, 0.5, 0.75], "#ffffff"]});
    let expected = json!([
        [0.0, 170.0 / 255.0, 1.0],
        [1.0, 128.0 / 255.0, 0.0],
        [0.25, 0.5, 0.75],
        [1.0, 1.0, 1.0],
    ]);
    assert_eq!(
        check(&given).ok().map(|checked| checked["stops"].clone()),
        Some(expected)
    );
}

#[test]
fn the_order_of_the_colours_is_kept() {
    let given = json!({"stops": ["#ff0000", "#00ff00", "#0000ff"]});
    let expected = json!([levels(255, 0, 0), levels(0, 255, 0), levels(0, 0, 255)]);
    assert_eq!(
        check(&given).ok().map(|checked| checked["stops"].clone()),
        Some(expected)
    );
}

#[test]
fn the_lengths_are_inclusive_at_both_ends() {
    // one colour is the floor of `stops`; two and four are both ends of `palette`; `stops` has no ceiling
    let many = json!(vec!["#101010"; 64]);
    for given in [
        json!({"stops": ["#101010"]}),
        json!({"stops": many}),
        json!({"palette": ["#101010", "#202020"]}),
        json!({"palette": ["#101010", "#202020", "#303030", "#404040"]}),
    ] {
        assert!(check(&given).is_ok(), "{given}");
    }
}

#[test]
fn a_list_outside_its_lengths_is_refused_naming_the_field_the_limit_and_the_list() {
    // (rule, given, the field blamed, the rule broken)
    let rows = [
        (
            "empty, floor of one",
            json!({"stops": []}),
            "stops",
            Constraint::TooFew(1),
        ),
        (
            "one, floor of two",
            json!({"palette": ["#101010"]}),
            "palette",
            Constraint::TooFew(2),
        ),
        (
            "five, ceiling of four",
            json!({"palette": ["#101010", "#202020", "#303030", "#404040", "#505050"]}),
            "palette",
            Constraint::TooMany(4),
        ),
    ];
    for (rule, given, field, constraint) in rows {
        let expected = given[field].clone();
        assert_eq!(
            refusal(&given),
            Some((field.to_owned(), constraint, expected)),
            "{rule}"
        );
    }
}

#[test]
fn an_element_that_is_no_colour_is_refused_naming_its_place() {
    let rows = [
        ("first", json!({"stops": ["red", "#ffffff"]}), 0),
        ("last", json!({"stops": ["#ffffff", "#fff", "#ggg"]}), 2),
        ("three hex digits missing", json!({"stops": ["#ffffff", "#ff"]}), 1),
        ("a channel above 1", json!({"stops": ["#ffffff", [0, 0, 2]]}), 1),
        ("two channels", json!({"stops": ["#ffffff", [0, 0]]}), 1),
        ("a bool channel", json!({"stops": [[true, 0, 0]]}), 0),
        ("a number", json!({"stops": ["#ffffff", 7]}), 1),
        ("null", json!({"stops": [null]}), 0),
        ("a list in a list", json!({"stops": [["#ffffff"]]}), 0),
    ];
    for (rule, given, at) in rows {
        let expected = given["stops"].clone();
        assert_eq!(
            refusal(&given),
            Some(("stops".to_owned(), Constraint::ColourAt(at), expected)),
            "{rule}"
        );
    }
}

#[test]
fn a_value_that_is_no_list_is_refused() {
    // one colour alone is not a list of one, and neither is a number, a switch or a map
    for given in [
        json!("#ffffff"),
        json!(0.5),
        json!(null),
        json!(true),
        json!({"0": "#ffffff"}),
    ] {
        let given = json!({"stops": given});
        let expected = given["stops"].clone();
        assert_eq!(
            refusal(&given),
            Some(("stops".to_owned(), Constraint::ColourList, expected)),
            "{given}"
        );
    }
}

#[test]
fn a_refusal_reads_in_one_sentence() {
    let message = |given: Value| check(&given).err().map(|error| error.to_string());
    assert_eq!(
        message(json!({"palette": ["#fff"]})).as_deref(),
        Some("effect 'gradient': `palette` must have at least 2 elements, got [\"#fff\"]")
    );
    assert_eq!(
        message(json!({"stops": ["#fff", "red"]})).as_deref(),
        Some(
            "effect 'gradient': `stops` must be a list of colours, and element 1 is not '#rrggbb', '#rgb' or three \
             numbers from 0 to 1, got [\"#fff\",\"red\"]"
        )
    );
}

#[test]
fn a_list_given_is_not_changed_and_the_other_list_keeps_its_default() {
    let given = json!({"stops": ["#ff0000"]});
    let before = given.clone();
    let checked = check(&given).ok();
    assert_eq!(given, before);
    assert_eq!(
        checked.map(|checked| checked["palette"].clone()),
        Some(json!([levels(0, 0, 0), levels(255, 255, 255)]))
    );
}
