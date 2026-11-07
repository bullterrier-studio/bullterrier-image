//! A parameter that may not exceed another (`at_most`), through the public API alone: what the table says of it, which
//! values `Table::check` and `Recipe::checked` accept, and what a refusal names. The table is written by hand, a lowest
//! (`low`, at most `high`) and a highest (`high`) height, as a declared pair of limits is.

use crate::common::Outcome;
use crate::common::recipe::{Double, setup};
use bullterrier_image::params::{Field, Kind, Scalar, Table, Unit};
use bullterrier_image::{Constraint, Effect, Error, ErrorKind, Presets, Recipe};
use serde_json::{Value, json};

/// A height in px from 0 to 2000, with `default`, that may not exceed the parameter `at_most` if one is named.
fn height(name: &'static str, default: i64, at_most: Option<&'static str>) -> Field {
    Field {
        name,
        kind: Kind::Float,
        meaning: "A height.",
        default: Scalar::Int(default),
        unit: Some(Unit::Px),
        min: Some(Scalar::Int(0)),
        max: Some(Scalar::Int(2000)),
        cyclic: false,
        choices: &[],
        min_length: None,
        max_length: None,
        at_most,
        nullable: false,
        drivable: false,
        advanced: false,
    }
}

fn table() -> Table {
    Table {
        doc: "The band heights.",
        fields: vec![height("low", 10, Some("high")), height("high", 100, None)],
    }
}

/// `given` (a JSON object) checked against the table.
fn check(given: &Value) -> Result<Value, Error> {
    let given = given.as_object().cloned().unwrap_or_default();
    table().check("bands", &given).map(Value::Object)
}

/// The field, the rule and the value a refusal of `given` shows; none if it is accepted or refused for another reason.
fn refusal(given: &Value) -> Option<(String, Constraint, Value)> {
    match check(given) {
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
fn the_table_names_the_limit_of_the_field_that_has_one_and_no_key_for_the_others() {
    let written = serde_json::to_value(table()).ok();
    let field = |at: usize| written.as_ref().map(|table| table["fields"][at].clone());
    assert_eq!(field(0).map(|low| low["at_most"].clone()), Some(json!("high")));
    assert_eq!(field(1).map(|high| high.get("at_most").cloned()), Some(None));
}

#[test]
fn a_lowest_at_or_below_the_highest_is_accepted_and_one_above_is_refused() {
    let accepted = [
        (json!({}), (10.0, 100.0)),
        (json!({"low": 0, "high": 0}), (0.0, 0.0)),
        (json!({"low": 50, "high": 50}), (50.0, 50.0)),
        (json!({"low": 20.5, "high": 2000}), (20.5, 2000.0)),
        (json!({"low": 100}), (100.0, 100.0)),
    ];
    for (given, (low, high)) in accepted {
        let checked = check(&given).ok();
        let read = |name: &str| checked.as_ref().and_then(|values| values[name].as_f64());
        assert_eq!((read("low"), read("high")), (Some(low), Some(high)), "{given}");
    }
    let refused = [
        json!({"low": 101}),
        json!({"low": 51, "high": 50}),
        json!({"high": 9}),
        json!({"low": 2000, "high": 1999.5}),
    ];
    for given in refused {
        assert!(
            matches!(refusal(&given), Some((field, Constraint::AtMostField("high", _), _)) if field == "low"),
            "{given} is refused for `low` exceeding `high`"
        );
    }
}

#[test]
fn a_refusal_names_the_effect_both_parameters_and_both_values() {
    let message = check(&json!({"low": 60, "high": 50}))
        .err()
        .map(|error| error.to_string());
    for word in ["bands", "low", "high", "60", "50"] {
        assert!(
            message.as_deref().is_some_and(|text| text.contains(word)),
            "{word} is missing from {message:?}"
        );
    }
    // the limit shown is the one in force, here the default of `high`
    assert_eq!(
        refusal(&json!({"low": 101})),
        Some((
            "low".to_owned(),
            Constraint::AtMostField("high", Scalar::Float(100.0)),
            json!(101.0)
        ))
    );
}

#[test]
fn a_value_out_of_its_own_range_is_refused_for_its_own_rule_first() {
    assert_eq!(
        refusal(&json!({"low": 3000})),
        Some(("low".to_owned(), Constraint::AtMost(Scalar::Int(2000)), json!(3000)))
    );
}

#[test]
fn a_step_of_a_registered_effect_is_held_to_the_rule_like_a_table_is() -> Outcome {
    setup()?;
    let double = Double::unique().with_table(table()).register()?;
    let recipe = |params: Value| {
        Recipe::from_json(&json!({"steps": [{"effect": double.name(), "params": params}]}).to_string())
            .and_then(|recipe| recipe.checked(&Presets::new()))
    };
    assert!(recipe(json!({"low": 20, "high": 30})).is_ok());
    let refused = recipe(json!({"low": 40, "high": 30})).err();
    assert_eq!(refused.as_ref().map(Error::kind), Some(ErrorKind::Recipe));
    let message = refused.map(|error| error.to_string()).unwrap_or_default();
    assert!(
        ["step 1", "low", "high", "40", "30"]
            .iter()
            .all(|word| message.contains(word)),
        "{message}"
    );
    Ok(())
}
