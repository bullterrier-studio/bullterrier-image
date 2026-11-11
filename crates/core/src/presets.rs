//! Presets: named parameter sets of one effect.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::recipe::file::model_of;
use crate::registry::Registry;
use crate::{Error, Step};

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
    by_effect: BTreeMap<String, Vec<(String, Preset)>>,
}

impl Presets {
    /// No presets.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The presets that ship with the crate ([`shipped::presets`](crate::shipped::presets)), for every effect that has some.
    ///
    /// # Errors
    /// [`Error::Recipe`] if a shipped file breaks a rule of its effect: a defect of the crate, which its tests exclude.
    pub fn shipped() -> Result<Self, Error> {
        let mut presets = Self::new();
        for (effect, text) in crate::shipped::presets() {
            presets.add_toml(effect, text)?;
        }
        Ok(presets)
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

    /// As [`add`](Self::add), for the table that the TOML text `text` writes (one `[name]` table per preset):
    ///
    /// ```text
    /// [sepia]
    /// description = "Warm brown monochrome."
    /// stops = ["#1a0f08", "#f4e4c1"]
    /// ```
    ///
    /// # Errors
    /// [`Error::Recipe`] if the text is not TOML (the message keeps its line and column) or holds a value a recipe has no use for (`nan`, `inf`, a date or time),
    /// as a recipe file's would; or as [`add`](Self::add).
    pub fn add_toml(&mut self, effect: &str, text: &str) -> Result<(), Error> {
        let table: toml::Table = toml::from_str(text)
            .map_err(|error: toml::de::Error| Error::Recipe(error.to_string().trim_end().to_owned()))?;
        self.add(effect, &Value::Object(model_of(table)?))
    }

    pub(crate) fn add_in(&mut self, registry: &Registry, effect: &str, table: &Value) -> Result<(), Error> {
        let Value::Object(written) = table else {
            return Err(Error::Recipe(format!(
                "the presets of '{effect}' are a table with one table for each preset, got {table}"
            )));
        };
        let described = registry.effect(effect).ok().map(|found| found.params());
        let mut presets = Vec::new();
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
            presets.push((name.clone(), Preset { description, params }));
        }
        self.by_effect.insert(effect.to_owned(), presets);
        Ok(())
    }

    /// The preset called `preset` of `effect`.
    #[must_use]
    pub fn get(&self, effect: &str, preset: &str) -> Option<&Preset> {
        self.by_effect
            .get(effect)?
            .iter()
            .find_map(|(name, found)| (name == preset).then_some(found))
    }

    /// The parameters of `step` with its preset's under them: the ones given win. `place` (`step 2`) leads the message
    /// when the step names a preset its effect lacks.
    pub(crate) fn under(&self, step: &Step, place: &str) -> Result<Map<String, Value>, Error> {
        let Some(name) = &step.preset else {
            return Ok(step.params.clone());
        };
        let Some(preset) = self.get(&step.effect, name) else {
            let names = self.names(&step.effect);
            let has = if names.is_empty() {
                "it has no presets".to_owned()
            } else {
                format!("its presets: {}", names.join(", "))
            };
            return Err(Error::Recipe(format!(
                "{place}: effect '{}' has no preset '{name}'; {has}",
                step.effect
            )));
        };
        let mut params = preset.params.clone();
        params.extend(step.params.clone());
        Ok(params)
    }

    /// The names of the presets of `effect`, in the order they were written.
    #[must_use]
    pub fn names(&self, effect: &str) -> Vec<&str> {
        self.by_effect.get(effect).map_or_else(Vec::new, |presets| {
            presets.iter().map(|(name, _)| name.as_str()).collect()
        })
    }
}
