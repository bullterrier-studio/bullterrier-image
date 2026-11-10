//! A picture as a declared parameter, through the public API alone: what the table says of it, what a user's values
//! become and which rules they break. The table is written by hand, as a `PictureName` field is declared in an effect.

use bullterrier_image::params::{Field, Kind, Scalar, Table};
use bullterrier_image::{Constraint, Error};
use serde_json::{Value, json};

fn table() -> Table {
    Table {
        doc: "The parameters of a layer.",
        fields: vec![Field {
            name: "image",
            kind: Kind::Picture,
            meaning: "The picture.",
            default: Scalar::Null,
            unit: None,
            min: None,
            max: None,
            cyclic: false,
            choices: &[],
            min_length: None,
            max_length: None,
            at_most: None,
            nullable: true,
            drivable: false,
            advanced: false,
        }],
    }
}

fn check(given: &Value) -> Result<Value, Error> {
    let given = given.as_object().cloned().unwrap_or_default();
    table().check("layer", &given).map(Value::Object)
}

#[test]
fn nothing_given_is_no_picture() {
    assert_eq!(check(&json!({})).ok(), Some(json!({"image": null})));
}

#[test]
fn the_table_writes_the_kind_nullable_with_a_null_default() {
    let written = serde_json::to_value(table()).ok();
    assert_eq!(
        written.map(|table| table["fields"][0].clone()),
        Some(json!({
            "name": "image",
            "kind": "picture",
            "meaning": "The picture.",
            "default": null,
            "unit": null, "min": null, "max": null, "cyclic": false, "choices": [],
            "nullable": true, "drivable": false, "advanced": false,
        }))
    );
}

#[test]
fn a_name_a_path_or_null_is_kept_as_it_is() {
    for given in [json!("logo"), json!("../assets/grain texture.png"), json!(null)] {
        assert_eq!(
            check(&json!({"image": given})).ok(),
            Some(json!({"image": given})),
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
        json!(["logo"]),
        json!({"$object": 0}),
        json!([0.5, 0.5, 0.5]),
    ] {
        let refused = match check(&json!({"image": given})) {
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
            Some(("image".to_owned(), Constraint::PictureName, given.clone())),
            "{given}"
        );
    }
}
