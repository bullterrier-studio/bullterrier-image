//! A lookup table as a declared parameter, through the public API alone: what the table says of it, what a user's values
//! become and which rules they break. The table is written by hand, as a `LutName` field is declared in an effect.

use bullterrier_image::params::{Field, Kind, Scalar, Table};
use bullterrier_image::{Constraint, Error};
use serde_json::{Value, json};

fn table() -> Table {
    Table {
        doc: "The parameters of a grade.",
        fields: vec![Field {
            name: "file",
            kind: Kind::Lut,
            meaning: "The table.",
            default: Scalar::Null,
            unit: None,
            min: None,
            max: None,
            cyclic: false,
            choices: &[],
            min_length: None,
            max_length: None,
            at_most: None,
            nullable: false,
            drivable: false,
            advanced: false,
        }],
    }
}

fn check(given: &Value) -> Result<Value, Error> {
    let given = given.as_object().cloned().unwrap_or_default();
    table().check("grade", &given).map(Value::Object)
}

#[test]
fn nothing_given_is_no_table_the_identity() {
    assert_eq!(check(&json!({})).ok(), Some(json!({"file": null})));
}

#[test]
fn the_table_writes_the_kind_with_a_null_default_and_no_other_part() {
    let written = serde_json::to_value(table()).ok();
    assert_eq!(
        written.map(|table| table["fields"][0].clone()),
        Some(json!({
            "name": "file",
            "kind": "lut",
            "meaning": "The table.",
            "default": null,
            "unit": null, "min": null, "max": null, "cyclic": false, "choices": [],
            "drivable": false, "advanced": false,
        }))
    );
}

#[test]
fn a_name_a_path_or_null_is_kept_as_it_is() {
    for given in [json!("film"), json!("../assets/warm film.cube"), json!(null)] {
        assert_eq!(
            check(&json!({"file": given})).ok(),
            Some(json!({"file": given})),
            "{given}"
        );
    }
}

#[test]
fn anything_else_is_refused_naming_the_parameter_and_the_value() {
    for given in [
        json!(""),
        json!(3),
        json!(true),
        json!(["film"]),
        json!({"$object": 0}),
        json!(0.5),
    ] {
        let refused = match check(&json!({"file": given})) {
            Err(Error::Invalid {
                field,
                constraint,
                value,
                ..
            }) => Some((field, constraint, *value)),
            _ => None,
        };
        assert_eq!(
            refused,
            Some(("file".to_owned(), Constraint::LutName, given.clone())),
            "{given}"
        );
    }
}
