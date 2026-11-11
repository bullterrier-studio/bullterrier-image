//! A parameter table written as data (the ecosystem's parameter format, version 3) and read back: what an effect declared at
//! run time hands the core, so that the core checks, scales, documents and describes it as it does an effect it declares in
//! Rust. The oracles are the format's own fixtures, the tables the core already declares, and hand-worked examples.

use std::path::Path;

use bullterrier_image::params::{Field, Table};
use bullterrier_image::{Error, effect, effects};
use serde_json::{Value, json};

fn fixtures(name: &str) -> Result<Vec<(String, Value)>, Box<dyn std::error::Error>> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/parameter-format/fixtures/parameters")
        .join(name);
    let entries: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    Ok(entries
        .into_iter()
        .map(|entry| {
            (
                entry["label"].as_str().unwrap_or_default().to_owned(),
                entry["parameter"].clone(),
            )
        })
        .collect())
}

#[test]
fn every_valid_fixture_of_the_format_is_read() -> Result<(), Box<dyn std::error::Error>> {
    let valid = fixtures("valid.json")?;
    assert!(valid.len() > 10);
    for (label, parameter) in valid {
        Field::from_json(&parameter).map_err(|error| format!("{label}: {error}"))?;
    }
    Ok(())
}

#[test]
fn every_invalid_fixture_of_the_format_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let invalid = fixtures("invalid.json")?;
    assert!(invalid.len() > 10);
    for (label, parameter) in invalid {
        assert!(Field::from_json(&parameter).is_err(), "{label} was read");
    }
    Ok(())
}

#[test]
fn the_table_of_every_effect_the_core_declares_is_read_back_as_it_was() -> Result<(), Box<dyn std::error::Error>> {
    let mut seen = 0;
    for name in effects() {
        let Some(table) = effect(&name)?.params() else { continue };
        let written = serde_json::to_value(&table)?;
        let read = Table::from_json(&written).map_err(|error| format!("{name}: {error}"))?;
        assert_eq!(read, table, "{name}");
        seen += 1;
    }
    assert!(seen > 80, "only {seen} tables");
    Ok(())
}

fn number(name: &str, default: f64, low: f64, high: f64) -> Value {
    json!({"name": name, "kind": "float", "meaning": "A size.", "default": default, "unit": "px", "min": low, "max": high, "cyclic": false})
}

#[test]
fn a_table_made_at_run_time_checks_values_like_a_declared_one() -> Result<(), Box<dyn std::error::Error>> {
    let table = Table::from_json(&json!({"doc": "A probe.", "fields": [
        number("radius", 4.0, 0.0, 50.0),
        {"name": "mode", "kind": "choice", "meaning": "How.", "default": "soft", "unit": null, "min": null, "max": null,
         "cyclic": false, "choices": ["soft", "hard"]},
    ]}))?;
    assert_eq!(table.doc, "A probe.");
    let given = json!({"radius": 8}).as_object().cloned().unwrap_or_default();
    let checked = table.check("probe", &given)?;
    assert_eq!(checked["radius"], json!(8.0));
    assert_eq!(checked["mode"], json!("soft"));
    for (bad, said) in [
        (json!({"radius": 51}), "radius"),
        (json!({"mode": "rough"}), "mode"),
        (json!({"colour": 1}), "colour"),
    ] {
        let message = table
            .check("probe", &bad.as_object().cloned().unwrap_or_default())
            .map_or_else(|error| error.to_string(), |_| "was accepted".to_owned());
        assert!(message.contains(said), "{message}");
    }
    Ok(())
}

fn refused(fields: &[Value]) -> String {
    Table::from_json(&json!({"doc": "", "fields": fields}))
        .map_or_else(|error| error.to_string(), |_| "was read".to_owned())
}

#[test]
fn a_table_that_breaks_a_rule_of_the_format_is_refused_naming_the_parameter() {
    let at_most = |name: &str, other: &str| {
        let mut field = number(name, 1.0, 0.0, 10.0);
        field["at_most"] = json!(other);
        field
    };
    let mut other_unit = number("high", 5.0, 0.0, 10.0);
    other_unit["unit"] = json!("fraction");
    let mut drivable_int = json!({"name": "count", "kind": "int", "meaning": "", "default": 3, "unit": "count", "min": 1, "max": 9, "cyclic": false});
    drivable_int["drivable"] = json!(true);
    let twice = json!({"name": "mode", "kind": "choice", "meaning": "", "default": "a", "unit": null, "min": null, "max": null,
        "cyclic": false, "choices": ["a", "b", "a"]});
    let short_turn = json!({"name": "angle", "kind": "float", "meaning": "", "default": 0.0, "unit": "degrees", "min": 0, "max": 359, "cyclic": true});
    for (fields, said) in [
        (vec![drivable_int], "count"),
        (vec![twice], "mode"),
        (vec![short_turn], "angle"),
        (
            vec![number("size", 1.0, 0.0, 9.0), number("size", 1.0, 0.0, 9.0)],
            "size",
        ),
        (vec![number("mask", 1.0, 0.0, 9.0)], "mask"),
        (vec![number("size_ratio", 1.0, 0.0, 9.0)], "size_ratio"),
        (vec![number("1st", 1.0, 0.0, 9.0)], "1st"),
        (vec![at_most("low", "missing")], "missing"),
        (vec![at_most("low", "low")], "low"),
        (vec![at_most("low", "high"), other_unit], "low"),
        (vec![number("size", 99.0, 0.0, 9.0)], "size"),
        (vec![number("size", 1.0, 9.0, 0.0)], "size"),
    ] {
        let message = refused(&fields);
        assert!(message.contains(said), "{message}");
        assert_ne!(message, "was read", "{fields:?}");
    }
}

#[test]
fn a_table_with_no_parameters_and_a_pair_that_agrees_are_read() -> Result<(), Error> {
    Table::from_json(&json!({"doc": "Nothing to set.", "fields": []}))?;
    let mut low = number("low", 1.0, 0.0, 10.0);
    low["at_most"] = json!("high");
    let table = Table::from_json(&json!({"doc": "", "fields": [low, number("high", 5.0, 0.0, 10.0)]}))?;
    assert_eq!(table.fields[0].at_most, Some("high"));
    Ok(())
}

#[test]
fn the_json_schema_of_a_table_is_worked_out_from_its_parameters() -> Result<(), Box<dyn std::error::Error>> {
    let table = Table::from_json(&json!({"doc": "A probe.", "fields": [
        {"name": "radius", "kind": "float", "meaning": "Size.", "default": 4.0, "unit": "px", "min": 0, "max": 50,
         "cyclic": false, "drivable": true},
        {"name": "angle", "kind": "float", "meaning": "Turn.", "default": 0.0, "unit": "degrees", "min": 0, "max": 360,
         "cyclic": true, "advanced": true},
        {"name": "count", "kind": "int", "meaning": "How many.", "default": 3, "unit": "count", "min": 1, "max": 9, "cyclic": false},
        {"name": "on", "kind": "bool", "meaning": "Whether.", "default": true, "unit": null, "min": null, "max": null, "cyclic": false},
        {"name": "mode", "kind": "choice", "meaning": "How.", "default": "soft", "unit": null, "min": null, "max": null,
         "cyclic": false, "choices": ["soft", "hard"]},
        {"name": "stops", "kind": "colors", "meaning": "Colours.", "default": ["#000000", "#ffffff"], "unit": null, "min": null,
         "max": null, "cyclic": false, "min_length": 2, "max_length": 4},
        {"name": "label", "kind": "text", "meaning": "Words.", "default": "ab", "unit": null, "min": null, "max": null,
         "cyclic": false, "min_length": 1},
        {"name": "texture", "kind": "picture", "meaning": "Another.", "default": null, "unit": null, "min": null, "max": null,
         "cyclic": false, "nullable": true},
    ]}))?;
    let schema = table.json_schema();
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["description"], "A probe.");
    assert_eq!(schema["additionalProperties"], false);
    let p = &schema["properties"];
    assert_eq!(
        p["radius"],
        json!({"type": "number", "description": "Size.", "default": 4.0, "minimum": 0, "maximum": 50,
        "unit": "px", "cyclic": false, "drivable": true, "advanced": false})
    );
    assert_eq!(p["angle"]["cyclic"], true);
    assert_eq!(p["angle"]["advanced"], true);
    assert_eq!(
        (&p["count"]["type"], &p["count"]["minimum"], &p["count"]["default"]),
        (&json!("integer"), &json!(1), &json!(3))
    );
    assert_eq!(p["on"]["type"], "boolean");
    assert_eq!(
        (&p["mode"]["type"], &p["mode"]["enum"]),
        (&json!("string"), &json!(["soft", "hard"]))
    );
    assert_eq!(p["mode"]["unit"], Value::Null);
    assert_eq!(
        (&p["stops"]["type"], &p["stops"]["minItems"], &p["stops"]["maxItems"]),
        (&json!("array"), &json!(2), &json!(4))
    );
    assert!(
        p["stops"]["items"]["anyOf"].is_array(),
        "a colour is a text or three numbers"
    );
    assert_eq!(
        (&p["label"]["type"], &p["label"]["minLength"]),
        (&json!("string"), &json!(1))
    );
    assert!(p["label"].get("maxLength").is_none());
    assert_eq!(
        p["texture"]["anyOf"],
        json!([{"type": "string", "minLength": 1}, {"type": "null"}])
    );
    assert_eq!(p["texture"]["default"], Value::Null);
    assert_eq!(schema["required"], json!([]), "every parameter has a default");
    Ok(())
}
