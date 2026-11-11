//! Presets, through the public API alone: named parameter sets of one effect, kept as written, checked by the effect's
//! table when the core describes it, added from a table or from TOML text.

use crate::common::Outcome;
use crate::common::recipe::setup;
use bullterrier_image::{Error, Preset, Presets};
use serde_json::{Value, json};

/// `presets` with the table `table` added for `effect` (`probe` and `bordered` are registered), or why not.
fn added(effect: &str, table: &Value) -> Result<Presets, Error> {
    setup()?;
    let mut presets = Presets::new();
    presets.add(effect, table)?;
    Ok(presets)
}

fn params(value: &Value) -> serde_json::Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

#[test]
fn presets_are_kept_as_written_by_effect_and_name() -> Outcome {
    let table = json!({
        "wide": {"description": "Wide cells.", "cell": 8, "cells": 7, "tint": "#0af"},
        "plain": {"description": "Few cells.", "cells": 2},
    });
    let presets = added("probe", &table)?;
    assert_eq!(
        presets.get("probe", "wide"),
        Some(&Preset {
            description: "Wide cells.".to_owned(),
            params: params(&json!({"cell": 8, "cells": 7, "tint": "#0af"})),
        })
    );
    assert_eq!(presets.names("probe"), ["wide", "plain"], "in the order written");
    assert_eq!(presets.get("probe", "narrow"), None);
    assert_eq!(presets.get("grade", "wide"), None);
    assert_eq!(presets.names("grade"), Vec::<&str>::new());
    Ok(())
}

#[test]
fn adding_the_presets_of_an_effect_again_replaces_them() -> Outcome {
    let mut presets = added("probe", &json!({"old": {"description": "Old."}}))?;
    presets.add("probe", &json!({"new": {"description": "New."}}))?;
    assert_eq!(presets.names("probe"), ["new"]);
    Ok(())
}

#[test]
fn a_preset_of_an_effect_the_core_describes_is_checked_by_its_table() {
    for (rule, table, word) in [
        (
            "a parameter the effect has",
            json!({"p": {"description": "D.", "kelvin": 1}}),
            "kelvin",
        ),
        (
            "a parameter in range",
            json!({"p": {"description": "D.", "cell": 1}}),
            "`cell` must be >= 2",
        ),
        (
            "a parameter of its type",
            json!({"p": {"description": "D.", "cells": 2.5}}),
            "whole number",
        ),
    ] {
        let refused = added("probe", &table).err().map(|error| error.to_string());
        // it names the preset, the effect and the trouble
        assert!(
            refused
                .as_deref()
                .is_some_and(|text| text.contains("preset 'p'") && text.contains("'probe'") && text.contains(word)),
            "{rule}: {refused:?}"
        );
    }
}

#[test]
fn a_preset_of_an_effect_not_known_yet_is_kept_as_given() -> Outcome {
    let table = json!({"p": {"description": "D.", "anything": [1, "two"]}});
    let effect = "not_registered_yet";
    let presets = added(effect, &table)?;
    assert_eq!(
        presets.get(effect, "p").map(|preset| preset.params.clone()),
        Some(params(&json!({"anything": [1, "two"]}))),
    );
    Ok(())
}

#[test]
fn a_preset_needs_a_sentence_saying_what_it_looks_like() {
    for (rule, table) in [
        ("a description", json!({"p": {"cell": 8}})),
        (
            "a description that is not blank",
            json!({"p": {"description": "  ", "cell": 8}}),
        ),
        (
            "a description that is text",
            json!({"p": {"description": 3, "cell": 8}}),
        ),
    ] {
        let refused = added("probe", &table).err().map(|error| error.to_string());
        assert!(
            refused
                .as_deref()
                .is_some_and(|text| text.contains("preset 'p'") && text.contains("`description`")),
            "{rule}: {refused:?}"
        );
    }
}

#[test]
fn presets_are_a_table_of_tables() {
    for (rule, table) in [
        ("a table", json!([{"description": "D."}])),
        ("a table for each preset", json!({"p": "sepia"})),
    ] {
        assert!(matches!(added("probe", &table), Err(Error::Recipe(_))), "{rule}");
    }
}

#[test]
fn presets_are_read_from_toml_text_and_checked_like_any() -> Outcome {
    let mut presets = Presets::new();
    presets.add_toml(
        "grade",
        "# the warm one\n[warm]\ndescription = \"Golden hour.\"\ntemperature = 0.25\nexposure = 1\n\n[plain]\ndescription = \"Nothing.\"\n",
    )?;
    assert_eq!(presets.names("grade"), ["warm", "plain"]);
    assert_eq!(
        presets
            .get("grade", "warm")
            .map(|preset| (preset.description.as_str(), preset.params.clone())),
        Some(("Golden hour.", params(&json!({"temperature": 0.25, "exposure": 1}))))
    );
    for (rule, text, word) in [
        ("text that is TOML", "[warm", "line 1"),
        ("a description", "[warm]\ntemperature = 0.25\n", "`description`"),
        (
            "a parameter the effect has",
            "[warm]\ndescription = \"D.\"\nkelvin = 5000\n",
            "kelvin",
        ),
    ] {
        let refused = Presets::new()
            .add_toml("grade", text)
            .err()
            .map(|error| error.to_string());
        assert!(
            refused.as_deref().is_some_and(|message| message.contains(word)),
            "{rule}: {refused:?}"
        );
    }
    Ok(())
}

#[test]
fn toml_a_recipe_has_no_use_for_is_refused_as_a_recipe_file_does() {
    for (rule, effect, text, words) in [
        (
            "nan, effect described",
            "grade",
            "[w]\ndescription = \"D.\"\nexposure = nan\n",
            ["w.exposure", "finite"],
        ),
        (
            "inf, effect described",
            "grade",
            "[w]\ndescription = \"D.\"\nexposure = inf\n",
            ["w.exposure", "finite"],
        ),
        (
            "nan, effect not described",
            "bordered",
            "[w]\ndescription = \"D.\"\nradius = nan\n",
            ["w.radius", "finite"],
        ),
        (
            "a date, effect described",
            "grade",
            "[w]\ndescription = \"D.\"\nexposure = 2024-01-01\n",
            ["w.exposure", "date"],
        ),
        (
            "a date, effect not described",
            "bordered",
            "[w]\ndescription = \"D.\"\nwhen = 2024-01-01\n",
            ["w.when", "date"],
        ),
        (
            "nan in a list",
            "bordered",
            "[w]\ndescription = \"D.\"\nstops = [1.0, nan]\n",
            ["w.stops[1]", "finite"],
        ),
    ] {
        let refused = Presets::new()
            .add_toml(effect, text)
            .err()
            .map(|error| error.to_string());
        assert!(
            refused
                .as_deref()
                .is_some_and(|message| words.iter().all(|word| message.contains(word))),
            "{rule}: {refused:?}"
        );
    }
}

#[test]
fn the_presets_of_an_effect_are_listed_in_the_order_the_file_wrote_them() -> Result<(), bullterrier_image::Error> {
    let mut presets = bullterrier_image::Presets::new();
    presets.add_toml(
        "grade",
        "[warm]\ndescription = \"Warmer.\"\ntemperature = 1\n[cool]\ndescription = \"Cooler.\"\ntemperature = -1\n[alpha]\ndescription = \"Last.\"\n",
    )?;
    assert_eq!(presets.names("grade"), ["warm", "cool", "alpha"]);
    Ok(())
}
