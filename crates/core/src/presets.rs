//! Presets: named parameter sets of one effect.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::Error;
use crate::registry::Registry;

/// One preset: what it looks like in a sentence, and the parameters it sets.
#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    /// One sentence saying what the look is.
    pub description: String,
    /// The parameters it sets, as written.
    pub params: Map<String, Value>,
}

/// The presets of the effects, by effect and name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Presets {
    by_effect: BTreeMap<String, BTreeMap<String, Preset>>,
}

impl Presets {
    /// No presets.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add the presets of `effect`, replacing any added for it before. `table` has one table for each preset, named
    /// as the preset is: its `description` (one sentence saying what the look is, not blank) and the effect's
    /// parameters. For an effect the core describes the parameters are checked as a step's would be; for any other,
    /// including one that is not registered yet, they are kept as given.
    ///
    /// # Errors
    /// [`Error::Recipe`] if `table` is not a table of tables, a preset has no description, or its parameters break a
    /// rule of the effect (the message names the preset).
    pub fn add(&mut self, effect: &str, table: &Value) -> Result<(), Error> {
        self.add_in(crate::registry::registry(), effect, table)
    }

    pub(crate) fn add_in(&mut self, registry: &Registry, effect: &str, table: &Value) -> Result<(), Error> {
        let Value::Object(written) = table else {
            return Err(Error::Recipe(format!(
                "the presets of '{effect}' are a table with one table for each preset, got {table}"
            )));
        };
        let described = registry.effect(effect).ok().and_then(|found| found.params());
        let mut presets = BTreeMap::new();
        for (name, preset) in written {
            let owner = format!("preset '{name}' of '{effect}'");
            let Value::Object(fields) = preset else {
                return Err(Error::Recipe(format!(
                    "{owner} is a table of its description and parameters, got {preset}"
                )));
            };
            let mut params = fields.clone();
            let description = match params.remove("description") {
                Some(Value::String(text)) if !text.trim().is_empty() => text,
                _ => {
                    return Err(Error::Recipe(format!(
                        "{owner} needs a `description`: one sentence saying what the look is"
                    )));
                }
            };
            if let Some(table) = &described {
                table
                    .check(effect, &params)
                    .map_err(|error| Error::Recipe(format!("{owner}: {error}")))?;
            }
            presets.insert(name.clone(), Preset { description, params });
        }
        self.by_effect.insert(effect.to_owned(), presets);
        Ok(())
    }

    /// The preset called `preset` of `effect`.
    #[must_use]
    pub fn get(&self, effect: &str, preset: &str) -> Option<&Preset> {
        self.by_effect.get(effect)?.get(preset)
    }

    /// The names of the presets of `effect`, sorted.
    #[must_use]
    pub fn names(&self, effect: &str) -> Vec<&str> {
        self.by_effect
            .get(effect)
            .map_or_else(Vec::new, |presets| presets.keys().map(String::as_str).collect())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{Preset, Presets};
    use crate::Error;
    use crate::testing::registry;

    /// `presets` with the table `table` added for `effect`, or why not.
    fn added(effect: &str, table: &Value) -> Result<Presets, Error> {
        let mut presets = Presets::new();
        presets.add_in(&registry()?, effect, table)?;
        Ok(presets)
    }

    fn params(value: &Value) -> serde_json::Map<String, Value> {
        value.as_object().cloned().unwrap_or_default()
    }

    #[test]
    fn presets_are_kept_as_written_by_effect_and_name() -> Result<(), Error> {
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
        assert_eq!(presets.names("probe"), ["plain", "wide"]);
        assert_eq!(presets.get("probe", "narrow"), None);
        assert_eq!(presets.get("grade", "wide"), None);
        assert_eq!(presets.names("grade"), Vec::<&str>::new());
        Ok(())
    }

    #[test]
    fn adding_the_presets_of_an_effect_again_replaces_them() -> Result<(), Error> {
        let mut presets = added("probe", &json!({"old": {"description": "Old."}}))?;
        presets.add_in(&registry()?, "probe", &json!({"new": {"description": "New."}}))?;
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
    fn a_preset_of_an_effect_with_no_table_or_not_known_is_kept_as_given() -> Result<(), Error> {
        let table = json!({"p": {"description": "D.", "anything": [1, "two"]}});
        for effect in ["seams", "not_registered_yet"] {
            let presets = added(effect, &table)?;
            assert_eq!(
                presets.get(effect, "p").map(|preset| preset.params.clone()),
                Some(params(&json!({"anything": [1, "two"]}))),
                "{effect}"
            );
        }
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
}
