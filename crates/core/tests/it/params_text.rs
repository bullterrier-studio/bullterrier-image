//! Text as a declared parameter, through the public API alone: what the table says of it, what a user's values become and
//! which rules they break. The table is written by hand, as a `String` field is declared in an effect.

use bullterrier_image::params::{Field, Kind, Scalar, Table};
use bullterrier_image::{Constraint, Error};
use serde_json::{Value, json};

fn table(min_length: Option<usize>, max_length: Option<usize>, min_distinct: Option<usize>) -> Table {
    Table {
        doc: "The parameters of a caption.",
        fields: vec![Field {
            name: "chars",
            kind: Kind::Text,
            meaning: "The characters.",
            default: Scalar::Text(" .:#"),
            unit: None,
            min: None,
            max: None,
            cyclic: false,
            choices: &[],
            min_length,
            max_length,
            min_distinct,
            at_most: None,
            nullable: false,
            drivable: false,
            advanced: false,
        }],
    }
}

fn check(table: &Table, given: &Value) -> Result<Value, Error> {
    let given = given.as_object().cloned().unwrap_or_default();
    table.check("caption", &given).map(Value::Object)
}

fn refused(table: &Table, given: &Value) -> Option<(String, Constraint, Value)> {
    match check(table, &json!({"chars": given})) {
        Err(Error::Invalid {
            field,
            constraint,
            value,
            ..
        }) => Some((field, constraint, *value)),
        _ => None,
    }
}

#[test]
fn nothing_given_is_the_default() {
    assert_eq!(
        check(&table(None, None, None), &json!({})).ok(),
        Some(json!({"chars": " .:#"}))
    );
}

#[test]
fn the_table_writes_the_kind_and_each_rule_it_has() {
    let written = serde_json::to_value(table(Some(1), Some(8), Some(2))).ok();
    assert_eq!(
        written.map(|table| table["fields"][0].clone()),
        Some(json!({
            "name": "chars",
            "kind": "text",
            "meaning": "The characters.",
            "default": " .:#",
            "unit": null, "min": null, "max": null, "cyclic": false, "choices": [],
            "min_length": 1, "max_length": 8, "min_distinct": 2,
            "drivable": false, "advanced": false,
        }))
    );
    let bare = serde_json::to_value(table(None, None, None)).ok();
    let field = bare.map(|table| table["fields"][0].clone());
    assert_eq!(
        field.map(|field| ["min_length", "max_length", "min_distinct"].map(|key| field.get(key).cloned())),
        Some([None, None, None])
    );
}

#[test]
fn text_inside_every_rule_is_kept_as_it_is() {
    let rules = table(Some(2), Some(4), Some(2));
    for given in ["ab", "abcd", "aab", " .:#", "░▒▓█"] {
        assert_eq!(
            check(&rules, &json!({"chars": given})).ok(),
            Some(json!({"chars": given})),
            "{given}"
        );
    }
}

#[test]
fn length_counts_characters_not_bytes() {
    // four characters of three bytes each: inside a ceiling of 4, over one of 3
    assert!(check(&table(None, Some(4), None), &json!({"chars": "░▒▓█"})).is_ok());
    assert_eq!(
        refused(&table(None, Some(3), None), &json!("░▒▓█")),
        Some(("chars".to_owned(), Constraint::TooLong(3), json!("░▒▓█")))
    );
}

#[test]
fn distinct_characters_are_counted_not_positions() {
    let rules = table(None, None, Some(2));
    for given in ["", "a", "aaaa", "    "] {
        assert_eq!(
            refused(&rules, &json!(given)),
            Some(("chars".to_owned(), Constraint::TooFewDistinct(2), json!(given))),
            "{given:?}"
        );
    }
    assert!(check(&rules, &json!({"chars": "aaab"})).is_ok());
}

#[test]
fn too_short_text_is_refused_naming_the_least() {
    assert_eq!(
        refused(&table(Some(3), None, None), &json!("ab")),
        Some(("chars".to_owned(), Constraint::TooShort(3), json!("ab")))
    );
}

#[test]
fn anything_but_text_is_refused() {
    let rules = table(None, None, None);
    for given in [
        json!(3),
        json!(true),
        json!(null),
        json!(["a", "b"]),
        json!({"$object": 0}),
    ] {
        assert_eq!(
            refused(&rules, &given),
            Some(("chars".to_owned(), Constraint::Text, given.clone())),
            "{given}"
        );
    }
}
