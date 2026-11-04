//! A field and an optional colour as declared parameters, through the public API alone: what the table says of them,
//! what a user's values become and which rules they break. The table is written by hand, as `FieldName` and
//! `Option<Color>` fields are declared in an effect. The same values are tried on the Python models of such a table
//! (`tests/core/test_params.py`).

use bullterrier_image::params::{Field, Kind, Scalar, Table};
use bullterrier_image::{Constraint, Error};
use serde_json::{Value, json};

fn table() -> Table {
    let plain = Field {
        name: "",
        kind: Kind::Field,
        meaning: "",
        default: Scalar::Null,
        unit: None,
        min: None,
        max: None,
        cyclic: false,
        choices: &[],
        min_length: None,
        max_length: None,
        nullable: false,
        drivable: false,
        advanced: false,
    };
    Table {
        doc: "The parameters of a blur.",
        fields: vec![
            Field {
                name: "field",
                meaning: "The selection to follow.",
                default: Scalar::Text("depth"),
                ..plain.clone()
            },
            Field {
                name: "color",
                kind: Kind::Color,
                meaning: "The glow's colour.",
                nullable: true,
                ..plain
            },
        ],
    }
}

fn check(given: &Value) -> Result<Value, Error> {
    let given = given.as_object().cloned().unwrap_or_default();
    table().check("blur", &given).map(Value::Object)
}

fn refused(given: &Value) -> Option<(String, Constraint)> {
    match check(given) {
        Err(Error::Invalid { field, constraint, .. }) => Some((field, constraint)),
        _ => None,
    }
}

#[test]
fn nothing_given_is_the_declared_field_and_no_colour() {
    assert_eq!(check(&json!({})).ok(), Some(json!({"field": "depth", "color": null})));
}

#[test]
fn the_table_writes_nullable_only_where_it_applies() {
    let written = serde_json::to_value(table()).ok();
    let field = |at: usize| written.as_ref().map(|table| table["fields"][at].clone());
    assert_eq!(field(0).map(|field| field["kind"].clone()), Some(json!("field")));
    assert_eq!(field(0).map(|field| field.get("nullable").cloned()), Some(None));
    assert_eq!(field(1).map(|field| field["nullable"].clone()), Some(json!(true)));
    assert_eq!(field(1).map(|field| field["default"].clone()), Some(Value::Null));
}

#[test]
fn a_name_is_kept_and_a_colour_is_read_in_any_spelling() {
    let blue = json!([0.0, 0.0, 1.0]);
    for colour in [json!("#00f"), json!(" #0000FF "), json!([0, 0, 1])] {
        assert_eq!(
            check(&json!({"field": "near", "color": colour})).ok(),
            Some(json!({"field": "near", "color": blue})),
            "{colour}"
        );
    }
}

#[test]
fn null_is_no_colour_where_the_field_allows_it() {
    assert_eq!(
        check(&json!({"color": null})).ok(),
        Some(json!({"field": "depth", "color": null}))
    );
}

#[test]
fn null_is_refused_where_the_field_is_not_nullable() {
    assert_eq!(
        refused(&json!({"field": null})),
        Some(("field".to_owned(), Constraint::FieldName))
    );
}

#[test]
fn what_is_no_name_or_no_colour_is_refused_naming_the_parameter() {
    for given in [
        json!(""),
        json!(3),
        json!(true),
        json!(["depth"]),
        json!({"by": "depth"}),
    ] {
        assert_eq!(
            refused(&json!({ "field": given })),
            Some(("field".to_owned(), Constraint::FieldName)),
            "{given}"
        );
    }
    for given in [json!("red"), json!([2, 0, 0]), json!(1), json!(false), json!("")] {
        assert_eq!(
            refused(&json!({ "color": given })),
            Some(("color".to_owned(), Constraint::Colour)),
            "{given}"
        );
    }
}
