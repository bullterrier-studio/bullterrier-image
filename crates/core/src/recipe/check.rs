//! The rules of a look.

use std::collections::BTreeMap;

use serde_json::{Map, Value};

use super::{Each, Entry, Recipe, STEP_KEYS, Step};
use crate::effect::Input;
use crate::registry::Registry;
use crate::{Error, Presets};

/// The selections every picture has, which a recipe need not declare.
const BUILT_IN: [&str; 4] = ["subject", "background", "depth", "light"];

/// What an address of a step's numbers starts with besides a step's name: a step cannot be called so.
const ADDRESS_WORDS: [&str; 3] = ["region", "frame", "each"];

impl Recipe {
    /// This recipe if it follows the rules of a look, else the first it breaks. A recipe must pass this before
    /// anything runs it. The rules:
    /// - at least one step; a `size` of at least 1;
    /// - a step's effect is registered; its parameters (with a `preset`'s under them: the ones given win) are the
    ///   effect's, as [`crate::params::Table::check`] says when the core describes them (an effect it does not
    ///   describe keeps them as given, its owner checks them), none named like a step key or an input;
    /// - a number the effect declares drivable may be a drive `{by, start, end, levels}` (`levels` a whole number from 2
    ///   to 16, default 5): its `start` and `end` follow the rules of the parameter, and all drives of a step share one
    ///   `by` and one `levels`;
    /// - a step has a `partition` exactly when its effect takes that input;
    /// - `opacity` from 0 to 1; a `name` of ASCII letters, digits and underscores starting with a letter, none of
    ///   `region`, `frame`, `each`, unique among the steps and not the name of an effect a step uses;
    /// - an each step has at least one look and a `feather` from 0 to 24; a look takes no `name`;
    /// - what a step names is a region: a `mask`, a drive's `by` and a step's `partition` are one of `subject`,
    ///   `background`, `depth`, `light` or declared in `regions`; an `each` is declared. The kind a region must be
    ///   (a selection or a partition) is for the runner to check.
    ///
    /// # Errors
    /// [`Error::UnknownEffect`] for an effect that is not registered, [`Error::Invalid`] for a parameter that breaks
    /// the rule of its declaration (its `place` is the step, `step 2`, or the look of an each step, `step 2, look 1`),
    /// [`Error::Recipe`] for any other rule, naming the step the same way.
    pub fn checked(self, presets: &Presets) -> Result<Self, Error> {
        self.check_in(crate::registry::registry(), presets)?;
        Ok(self)
    }

    /// This recipe if it is a [`checked`](Self::checked) one with a `description`: a sentence, not blank, saying what
    /// the look looks like.
    ///
    /// # Errors
    /// As [`checked`](Self::checked); [`Error::Recipe`] if there is no description.
    pub fn checked_look(self, presets: &Presets) -> Result<Self, Error> {
        let look = self.checked(presets)?;
        if look.description.as_deref().is_none_or(|text| text.trim().is_empty()) {
            return Err(Error::Recipe(
                "a look needs a `description`: one sentence saying what it looks like".to_owned(),
            ));
        }
        Ok(look)
    }

    pub(crate) fn check_in(&self, registry: &Registry, presets: &Presets) -> Result<(), Error> {
        if self.steps.is_empty() {
            return Err(Error::Recipe("a recipe needs at least one step".to_owned()));
        }
        if self.size == Some(0) {
            return Err(Error::Recipe("`size` must be >= 1, got 0".to_owned()));
        }
        let rules = Rules {
            registry,
            presets,
            regions: &self.regions,
        };
        for (index, entry) in self.steps.iter().enumerate() {
            let place = format!("step {}", index + 1);
            match entry {
                Entry::Step(step) => rules.step(step, &place)?,
                Entry::Each(each) => rules.each(each, &place)?,
            }
        }
        self.check_names()
    }

    /// The names of the steps are unique and none is the name of an effect a step uses (it would be taken for that
    /// effect in an address).
    fn check_names(&self) -> Result<(), Error> {
        let used: Vec<&str> = self
            .steps
            .iter()
            .filter_map(|entry| match entry {
                Entry::Step(step) => Some(step.effect.as_str()),
                Entry::Each(_) => None,
            })
            .collect();
        let mut first: BTreeMap<&str, usize> = BTreeMap::new();
        for (index, entry) in self.steps.iter().enumerate() {
            let number = index + 1;
            let (Entry::Step(Step { name: Some(name), .. }) | Entry::Each(Each { name: Some(name), .. })) = entry
            else {
                continue;
            };
            if let Some(earlier) = first.get(name.as_str()) {
                return Err(Error::Recipe(format!(
                    "`name` '{name}' is the name of steps {earlier} and {number}"
                )));
            }
            if used.contains(&name.as_str()) {
                return Err(Error::Recipe(format!(
                    "step {number}: `name` '{name}' is also an effect this recipe uses; choose another"
                )));
            }
            first.insert(name, number);
        }
        Ok(())
    }
}

/// What the rules of a recipe look things up in.
struct Rules<'a> {
    registry: &'a Registry,
    presets: &'a Presets,
    regions: &'a Map<String, Value>,
}

/// A parameter that varies across the picture, as a recipe writes it: its value is `start` where the field `by` is 0
/// and `end` where it is 1.
struct Drive {
    by: String,
    start: f64,
    end: f64,
    levels: i64,
}

impl Drive {
    /// The drive a table writes, or what is wrong with it.
    fn read(table: &Map<String, Value>) -> Result<Self, String> {
        if let Some(key) = table
            .keys()
            .find(|key| !["by", "start", "end", "levels"].contains(&key.as_str()))
        {
            return Err(format!(
                "`{key}` is not a key of a drive; a drive has by, start, end and levels"
            ));
        }
        let number = |key: &str| match table.get(key) {
            None => Err(format!("`{key}` is required")),
            Some(value) => value
                .as_f64()
                .ok_or_else(|| format!("`{key}` must be a number, got {value}")),
        };
        let by = match table.get("by") {
            Some(Value::String(by)) if !by.is_empty() => by.clone(),
            other => {
                return Err(format!(
                    "`by` names the field that drives it, got {}",
                    other.unwrap_or(&Value::Null)
                ));
            }
        };
        // constant: the default `levels` of the Python `Drive`
        let levels = table.get("levels").map_or(Some(5), Value::as_i64);
        match levels {
            Some(levels @ 2..=16) => Ok(Self {
                by,
                start: number("start")?,
                end: number("end")?,
                levels,
            }),
            _ => Err(format!(
                "`levels` must be a whole number from 2 to 16, got {}",
                table.get("levels").unwrap_or(&Value::Null)
            )),
        }
    }
}

impl Rules<'_> {
    fn step(&self, step: &Step, place: &str) -> Result<(), Error> {
        let effect = self.registry.effect(&step.effect)?;
        self.layer(place, step.mask.as_deref(), step.opacity, step.name.as_deref())?;
        let params = self.with_preset(step, place)?;
        for key in Input::ALL.iter().map(|input| input.name()) {
            if params.contains_key(key) {
                return Err(Error::Recipe(format!(
                    "{place}: `{key}` is supplied by the recipe itself and cannot be set"
                )));
            }
        }
        for key in STEP_KEYS {
            if params.contains_key(*key) {
                return Err(Error::Recipe(format!(
                    "{place}: `{key}` is a key of the step, not a parameter of '{}': give it as the step's `{key}`",
                    step.effect
                )));
            }
        }
        let drives = drives(&step.effect, &params, place)?;
        if let Some(table) = effect.params() {
            for (name, _) in &drives {
                if table.fields.iter().any(|field| field.name == name && !field.drivable) {
                    let drivable: Vec<&str> = table
                        .fields
                        .iter()
                        .filter(|field| field.drivable)
                        .map(|f| f.name)
                        .collect();
                    return Err(Error::Recipe(format!(
                        "{place}: effect '{}': `{name}` cannot be driven by a field; its drivable parameters: {}",
                        step.effect,
                        drivable.join(", ")
                    )));
                }
            }
            // the values between the ends of a drive lie between two values the parameter takes, so they take it too
            let mut ends: Vec<fn(&Drive) -> f64> = vec![|drive| drive.start];
            if !drives.is_empty() {
                ends.push(|drive| drive.end);
            }
            for end in ends {
                let mut values = params.clone();
                for (name, drive) in &drives {
                    values.insert(name.clone(), Value::from(end(drive)));
                }
                table.check(&step.effect, &values).map_err(|error| error.at(place))?;
            }
        }
        match (effect.inputs().contains(&Input::Partition), &step.partition) {
            (true, None) => {
                return Err(Error::Recipe(format!(
                    "{place}: effect '{}' draws along the borders of a partition: give `partition`, a region",
                    step.effect
                )));
            }
            (false, Some(_)) => {
                return Err(Error::Recipe(format!(
                    "{place}: effect '{}' takes no partition: remove `partition`",
                    step.effect
                )));
            }
            _ => {}
        }
        for (_, drive) in &drives {
            self.region(place, "by", &drive.by, true)?;
        }
        if let Some(partition) = &step.partition {
            self.region(place, "partition", partition, true)?;
        }
        Ok(())
    }

    fn each(&self, each: &Each, place: &str) -> Result<(), Error> {
        self.layer(place, each.mask.as_deref(), each.opacity, each.name.as_deref())?;
        if each.looks.is_empty() {
            return Err(Error::Recipe(format!("{place}: an each step needs at least one look")));
        }
        if !(0.0..=24.0).contains(&each.feather) {
            return Err(Error::Recipe(format!(
                "{place}: `feather` must be from 0 to 24 px, got {}",
                each.feather
            )));
        }
        self.region(place, "each", &each.each, false)?;
        for (index, look) in each.looks.iter().enumerate() {
            let Some(look) = look else { continue };
            let place = format!("{place}, look {}", index + 1);
            if look.name.is_some() {
                return Err(Error::Recipe(format!(
                    "{place}: a look takes no `name`: its numbers are addressed by place, `looks[{index}]`"
                )));
            }
            self.step(look, &place)?;
        }
        Ok(())
    }

    /// What a step and an each step share: how the result is laid over the picture below.
    fn layer(&self, place: &str, mask: Option<&str>, opacity: Option<f64>, name: Option<&str>) -> Result<(), Error> {
        if let Some(opacity) = opacity
            && !(0.0..=1.0).contains(&opacity)
        {
            return Err(Error::Recipe(format!(
                "{place}: `opacity` must be from 0 to 1, got {opacity}"
            )));
        }
        if let Some(name) = name {
            let mut characters = name.chars();
            let word = characters.next().is_some_and(|first| first.is_ascii_alphabetic())
                && characters.all(|character| character.is_ascii_alphanumeric() || character == '_');
            if !word {
                return Err(Error::Recipe(format!(
                    "{place}: `name` must be letters, digits and underscores, starting with a letter, got '{name}'"
                )));
            }
            if ADDRESS_WORDS.contains(&name) {
                return Err(Error::Recipe(format!(
                    "{place}: `name` '{name}' is reserved: an address starts with `region`, `frame` or `each`"
                )));
            }
        }
        match mask {
            Some(mask) => self.region(place, "mask", mask, true),
            None => Ok(()),
        }
    }

    /// `name` (what `key` of the step at `place` refers to) is a region: one the recipe declares, or, when
    /// `built_in` allows, one every picture has.
    fn region(&self, place: &str, key: &str, name: &str, built_in: bool) -> Result<(), Error> {
        if self.regions.contains_key(name) || (built_in && BUILT_IN.contains(&name)) {
            return Ok(());
        }
        let valid: Vec<&str> = built_in
            .then_some(BUILT_IN.iter().copied())
            .into_iter()
            .flatten()
            .chain(self.regions.keys().map(String::as_str))
            .collect();
        Err(Error::Recipe(format!(
            "{place}: `{key}` names '{name}', which is no region; valid regions: {}",
            valid.join(", ")
        )))
    }

    /// The parameters of `step` with its preset's under them: the ones given win.
    fn with_preset(&self, step: &Step, place: &str) -> Result<Map<String, Value>, Error> {
        let Some(name) = &step.preset else {
            return Ok(step.params.clone());
        };
        let Some(preset) = self.presets.get(&step.effect, name) else {
            let names = self.presets.names(&step.effect);
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
}

/// The parameters in `params` that are drives (a table with `by`), with their names, once their shape and the sharing
/// of one `by` and one `levels` are checked.
fn drives(effect: &str, params: &Map<String, Value>, place: &str) -> Result<Vec<(String, Drive)>, Error> {
    let mut drives = Vec::new();
    for (name, value) in params {
        let Some(table) = value.as_object().filter(|table| table.contains_key("by")) else {
            continue;
        };
        let drive = Drive::read(table)
            .map_err(|problem| Error::Recipe(format!("{place}: effect '{effect}': `{name}`: {problem}")))?;
        drives.push((name.clone(), drive));
    }
    if let [(_, first), rest @ ..] = drives.as_slice() {
        if rest.iter().any(|(_, other)| other.by != first.by) {
            return Err(Error::Recipe(format!(
                "{place}: effect '{effect}': the drives of one step must vary by one field"
            )));
        }
        if rest.iter().any(|(_, other)| other.levels != first.levels) {
            return Err(Error::Recipe(format!(
                "{place}: effect '{effect}': the drives of one step must vary by one number of levels"
            )));
        }
    }
    Ok(drives)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::testing::registry;
    use crate::{BlendMode, Each, Entry, Error, Presets, Recipe, Step};

    /// A step of the probe effect: its parameters are checked against its table.
    fn probe() -> Step {
        Step::new("probe")
    }

    /// A probe step called `name`.
    fn named(name: &str) -> Step {
        let mut step = probe();
        step.name = Some(name.to_owned());
        step
    }

    /// A `seams` step (the effect that takes a partition and describes no parameters) drawing along `partition`.
    fn seams(partition: Option<&str>) -> Step {
        let mut step = Step::new("seams");
        step.partition = partition.map(str::to_owned);
        step
    }

    /// A drive, as a recipe file writes it.
    fn drive(by: &str, start: f64, end: f64) -> Value {
        json!({"by": by, "start": start, "end": end})
    }

    /// `recipe` declaring the regions `panes` (a partition) and `cutout` (a selection).
    fn with_regions(mut recipe: Recipe) -> Recipe {
        recipe.regions = json!({"panes": {"kind": "grid"}, "cutout": {"kind": "path"}})
            .as_object()
            .cloned()
            .unwrap_or_default();
        recipe
    }

    /// Presets of the probe: `wide` sets a cell and a count, `plain` only the count.
    fn presets() -> Result<Presets, Error> {
        let mut presets = Presets::new();
        let table = json!({
            "wide": {"description": "Wide cells.", "cell": 8, "cells": 7},
            "plain": {"description": "Few cells.", "cells": 2},
        });
        presets.add_in(&registry()?, "probe", &table)?;
        Ok(presets)
    }

    /// The error `recipe` is refused with by the test registry and `presets`; None if it is accepted.
    fn refusal(recipe: &Recipe, presets: &Presets) -> Result<Option<Error>, Error> {
        Ok(recipe.check_in(&registry()?, presets).err())
    }

    /// The recipes that follow every rule, one for each thing a rule might wrongly refuse.
    fn good() -> Vec<(&'static str, Recipe)> {
        let mut sized = Recipe::new([probe()]);
        sized.size = Some(1);
        let mut each = Each::new("panes", [Some(probe().param("cells", 7)), None]);
        each.feather = 24.0;
        each.mask = Some("depth".to_owned());
        let mut each_at_zero = Each::new("panes", [None]);
        each_at_zero.feather = 0.0;
        let mut looks_reuse_a_name = Each::new("panes", [Some(Step::new("probe"))]);
        looks_reuse_a_name.name = Some("probe_2".to_owned());
        let all_set = probe()
            .param("cell", 8)
            .param("cells", -3)
            .param("angle", 725)
            .param("shape", "checks")
            .param("keep", false)
            .param("tint", "#0af");
        vec![
            ("a plain step", Recipe::new([probe()])),
            ("size 1", sized),
            ("every parameter set, one past the turn", Recipe::new([all_set])),
            (
                "opacity 0 and 1",
                Recipe::new([probe().opacity(0.0), probe().opacity(1.0)]),
            ),
            ("a blend mode", Recipe::new([probe().blend_mode(BlendMode::SoftLight)])),
            (
                "each built-in selection as a mask",
                Recipe::new(["subject", "background", "depth", "light"].map(|mask| probe().mask(mask))),
            ),
            (
                "a declared region as a mask",
                with_regions(Recipe::new([probe().mask("cutout")])),
            ),
            (
                "names with digits and underscores",
                Recipe::new([named("pulse_2"), named("A1")]),
            ),
            (
                "a name that is an effect used only inside an each step",
                with_regions(Recipe::new([Entry::from(looks_reuse_a_name), named("x").into()])),
            ),
            (
                "a step that takes a partition, given a declared one",
                with_regions(Recipe::new([seams(Some("panes"))])),
            ),
            (
                "a built-in selection as a step's partition",
                Recipe::new([seams(Some("subject"))]),
            ),
            (
                "an effect with no table keeps its parameters as given",
                Recipe::new([seams(Some("subject")).param("anything", json!([1, "two"]))]),
            ),
            ("an each step over a declared region", with_regions(Recipe::new([each]))),
            (
                "an each step with hard borders",
                with_regions(Recipe::new([each_at_zero])),
            ),
            (
                "a drive on a drivable parameter, by a built-in selection",
                Recipe::new([probe().param("cell", drive("depth", 4.0, 100.0))]),
            ),
            (
                "two drives sharing field and levels, a cyclic one running past the turn",
                Recipe::new([probe()
                    .param("cell", json!({"by": "light", "start": 2, "end": 512, "levels": 16}))
                    .param("angle", json!({"by": "light", "start": -90, "end": 725, "levels": 16}))]),
            ),
            (
                "a drive by a declared region",
                with_regions(Recipe::new([probe().param("cell", drive("cutout", 2.0, 3.0))])),
            ),
        ]
    }

    #[test]
    fn recipes_that_follow_the_rules_are_accepted() -> Result<(), Error> {
        for (rule, recipe) in good() {
            let refused = refusal(&recipe, &Presets::new())?;
            assert!(refused.is_none(), "{rule}: {refused:?}");
        }
        Ok(())
    }

    /// (the rule, a recipe that breaks it, a word of the message that names the trouble).
    #[expect(clippy::too_many_lines, reason = "a table of rows, one rule each")]
    fn bad() -> Vec<(&'static str, Recipe, &'static str)> {
        let nothing = Vec::<Step>::new();
        let mut size_zero = Recipe::new([probe()]);
        size_zero.size = Some(0);
        let in_params = |key: &str, value: Value| Recipe::new([probe().param(key, value)]);
        let drives = |cell: Value, angle: Value| Recipe::new([probe().param("cell", cell).param("angle", angle)]);
        let each = |feather: f64| {
            let mut each = Each::new("panes", [None]);
            each.feather = feather;
            with_regions(Recipe::new([each]))
        };
        let mut each_named = Each::new("panes", [None]);
        each_named.name = Some("pulse".to_owned());
        let mut each_opaque = Each::new("panes", [None]);
        each_opaque.opacity = Some(2.0);
        let mut nameless_look = named("inner");
        nameless_look.mask = None;
        let mut preset_missing = probe();
        preset_missing.preset = Some("nope".to_owned());
        let mut partition_on_probe = probe();
        partition_on_probe.partition = Some("panes".to_owned());
        let mut opacity_nan = probe();
        opacity_nan.opacity = Some(f64::NAN);
        vec![
            ("a recipe needs a step", Recipe::new(nothing), "at least one step"),
            ("size is at least 1", size_zero, "`size`"),
            ("the effect exists", Recipe::new([Step::new("nope")]), "nope"),
            (
                "a parameter the effect declares",
                in_params("kelvin", json!(1)),
                "`kelvin` is not a parameter",
            ),
            (
                "a parameter within its range",
                in_params("cell", json!(1)),
                "`cell` must be >= 2",
            ),
            (
                "a parameter of its type",
                in_params("cells", json!(3.0)),
                "`cells` must be a whole number",
            ),
            (
                "a step key is no parameter",
                in_params("mask", json!("subject")),
                "`mask` is a key of the step",
            ),
            (
                "an input is no parameter",
                in_params("size_ratio", json!(2)),
                "`size_ratio` is supplied by the recipe",
            ),
            (
                "the partition is no parameter",
                in_params("partition", json!("p")),
                "`partition` is supplied by the recipe",
            ),
            ("a preset the effect has", Recipe::new([preset_missing]), "no preset"),
            (
                "a partition for the effect that draws along one",
                Recipe::new([seams(None)]),
                "`partition`",
            ),
            (
                "a partition only for that effect",
                with_regions(Recipe::new([partition_on_probe])),
                "takes no partition",
            ),
            (
                "the partition is a region",
                Recipe::new([seams(Some("nowhere"))]),
                "nowhere",
            ),
            ("opacity at most 1", Recipe::new([probe().opacity(1.5)]), "`opacity`"),
            ("opacity at least 0", Recipe::new([probe().opacity(-0.1)]), "`opacity`"),
            ("opacity is a number", Recipe::new([opacity_nan]), "`opacity`"),
            (
                "a name starts with a letter",
                Recipe::new([named("1st")]),
                "letters, digits and underscores",
            ),
            (
                "a name has no space",
                Recipe::new([named("a b")]),
                "letters, digits and underscores",
            ),
            (
                "a name is not empty",
                Recipe::new([named("")]),
                "letters, digits and underscores",
            ),
            (
                "a name is ascii",
                Recipe::new([named("caf\u{e9}")]),
                "letters, digits and underscores",
            ),
            ("a name is not `region`", Recipe::new([named("region")]), "reserved"),
            ("a name is not `frame`", Recipe::new([named("frame")]), "reserved"),
            ("a name is not `each`", Recipe::new([named("each")]), "reserved"),
            (
                "names are unique",
                Recipe::new([named("x"), probe(), named("x")]),
                "steps 1 and 3",
            ),
            (
                "a name is not an effect the recipe uses",
                Recipe::new([named("probe")]),
                "also an effect",
            ),
            ("a mask is a region", Recipe::new([probe().mask("nowhere")]), "nowhere"),
            (
                "a drive is on a drivable parameter",
                Recipe::new([probe().param("cells", drive("depth", 1.0, 2.0))]),
                "cannot be driven",
            ),
            (
                "a drive is not on a colour",
                Recipe::new([probe().param("tint", drive("depth", 0.0, 1.0))]),
                "cannot be driven",
            ),
            (
                "a drive is on a parameter the effect has",
                Recipe::new([probe().param("kelvin", drive("depth", 0.0, 1.0))]),
                "`kelvin` is not a parameter",
            ),
            (
                "the drives of a step share a field",
                drives(drive("depth", 4.0, 9.0), drive("light", 0.0, 90.0)),
                "one field",
            ),
            (
                "the drives of a step share the number of levels",
                drives(
                    json!({"by": "depth", "start": 4, "end": 9, "levels": 3}),
                    drive("depth", 0.0, 90.0),
                ),
                "number of levels",
            ),
            (
                "a drive has at least 2 levels",
                in_params("cell", json!({"by": "depth", "start": 4, "end": 9, "levels": 1})),
                "`levels`",
            ),
            (
                "a drive has at most 16 levels",
                in_params("cell", json!({"by": "depth", "start": 4, "end": 9, "levels": 17})),
                "`levels`",
            ),
            (
                "a drive has whole levels",
                in_params("cell", json!({"by": "depth", "start": 4, "end": 9, "levels": 2.5})),
                "levels",
            ),
            (
                "a drive has an end",
                in_params("cell", json!({"by": "depth", "start": 4})),
                "end",
            ),
            (
                "a drive has no other key",
                in_params("cell", json!({"by": "depth", "start": 4, "end": 9, "ease": 1})),
                "ease",
            ),
            (
                "a drive names its field",
                in_params("cell", drive("", 4.0, 9.0)),
                "`by`",
            ),
            (
                "a drive starts in range",
                in_params("cell", drive("depth", 1.0, 9.0)),
                "`cell` must be >= 2",
            ),
            (
                "a drive ends in range",
                in_params("cell", drive("depth", 4.0, 513.0)),
                "`cell` must be <= 512",
            ),
            (
                "a drive starts with a number",
                in_params("cell", json!({"by": "depth", "start": "low", "end": 9})),
                "start",
            ),
            (
                "a drive's field is a region",
                in_params("cell", drive("nowhere", 4.0, 9.0)),
                "nowhere",
            ),
            (
                "an each step has a look",
                with_regions(Recipe::new([Each::new("panes", [])])),
                "at least one look",
            ),
            ("feather at most 24", each(24.5), "`feather`"),
            ("feather at least 0", each(-0.5), "`feather`"),
            ("feather is a number", each(f64::NAN), "`feather`"),
            (
                "an each step names a region",
                Recipe::new([Each::new("nowhere", [None])]),
                "nowhere",
            ),
            (
                "an each step names a partition region, not a built-in selection",
                Recipe::new([Each::new("subject", [None])]),
                "subject",
            ),
            (
                "a look takes no name",
                with_regions(Recipe::new([Each::new("panes", [Some(nameless_look)])])),
                "a look takes no `name`",
            ),
            (
                "a look's effect exists",
                with_regions(Recipe::new([Each::new("panes", [Some(Step::new("nope"))])])),
                "nope",
            ),
            (
                "a look's parameters are in range",
                with_regions(Recipe::new([Each::new(
                    "panes",
                    [None, Some(probe().param("cell", 1))],
                )])),
                "`cell` must be >= 2",
            ),
            (
                "a look's mask is a region",
                with_regions(Recipe::new([Each::new("panes", [Some(probe().mask("nowhere"))])])),
                "nowhere",
            ),
            (
                "an each step's opacity is at most 1",
                with_regions(Recipe::new([each_opaque])),
                "`opacity`",
            ),
            (
                "an each step's name is taken",
                with_regions(Recipe::new([Entry::from(named("pulse")), each_named.into()])),
                "steps 1 and 2",
            ),
        ]
    }

    #[test]
    fn a_rule_broken_is_refused_naming_the_trouble() -> Result<(), Error> {
        for (rule, recipe, word) in bad() {
            let refused = refusal(&recipe, &Presets::new())?.map(|error| error.to_string());
            assert!(
                refused.as_deref().is_some_and(|text| text.contains(word)),
                "{rule}: wanted {word:?}, got {refused:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn the_refusal_of_a_later_step_names_its_place() -> Result<(), Error> {
        let recipe = Recipe::new([probe(), probe(), probe().mask("nowhere")]);
        let refused = refusal(&recipe, &Presets::new())?.map(|error| error.to_string());
        assert!(
            refused.as_deref().is_some_and(|text| text.contains("step 3")),
            "{refused:?}"
        );
        Ok(())
    }

    #[test]
    fn an_unknown_effect_is_the_registrys_own_error() -> Result<(), Error> {
        let refused = refusal(&Recipe::new([Step::new("nope")]), &Presets::new())?;
        assert!(matches!(refused, Some(Error::UnknownEffect(name)) if name == "nope"));
        Ok(())
    }

    #[test]
    fn a_parameter_out_of_range_is_the_tables_own_error() -> Result<(), Error> {
        let refused = refusal(&Recipe::new([probe().param("cell", 1)]), &Presets::new())?;
        assert!(
            matches!(&refused, Some(Error::Invalid { effect, field, value, .. }) if effect == "probe" && field == "cell" && **value == json!(1)),
            "{refused:?}"
        );
        Ok(())
    }

    #[test]
    fn a_preset_lies_under_the_given_parameters() -> Result<(), Error> {
        let presets = presets()?;
        let with = |preset: &str, step: Step| {
            let mut step = step;
            step.preset = Some(preset.to_owned());
            Recipe::new([step])
        };
        // the preset's values are checked as the step's own would be, so the merge is what counts
        assert!(refusal(&with("wide", probe()), &presets)?.is_none());
        assert!(refusal(&with("plain", probe().param("cell", 512)), &presets)?.is_none());
        // a parameter given wins over the preset's, and is the one checked
        let refused = refusal(&with("wide", probe().param("cell", 1)), &presets)?.map(|error| error.to_string());
        assert!(
            refused
                .as_deref()
                .is_some_and(|text| text.contains("`cell` must be >= 2, got 1")),
            "{refused:?}"
        );
        // a preset the effect lacks is refused, listing the ones it has
        let refused = refusal(&with("narrow", probe()), &presets)?.map(|error| error.to_string());
        assert!(
            refused
                .as_deref()
                .is_some_and(|text| text.contains("narrow") && text.contains("plain, wide")),
            "{refused:?}"
        );
        // an effect with no presets says so
        let mut step = seams(Some("subject"));
        step.preset = Some("any".to_owned());
        let refused = refusal(&Recipe::new([step]), &presets)?.map(|error| error.to_string());
        assert!(
            refused.as_deref().is_some_and(|text| text.contains("no presets")),
            "{refused:?}"
        );
        Ok(())
    }

    #[test]
    fn a_parameter_refused_in_a_later_step_names_the_step_and_keeps_its_typed_information() {
        let grade = |step: Step| Recipe::new([Step::new("grade"), step]);
        let refused = grade(Step::new("grade").param("subject", 1)).checked(&Presets::new());
        assert!(
            matches!(&refused, Err(Error::Invalid { place: Some(place), effect, field, .. })
                if place == "step 2" && effect == "grade" && field == "subject"),
            "{refused:?}"
        );
        let text = refused.err().map(|error| error.to_string());
        assert!(
            text.as_deref()
                .is_some_and(|text| text.starts_with("step 2: effect 'grade': `subject` is not")),
            "{text:?}"
        );
    }

    #[test]
    fn a_parameter_refused_in_a_look_names_the_step_and_the_look() -> Result<(), Error> {
        let with_regions = with_regions(Recipe::new([
            probe().into(),
            Entry::from(Each::new("panes", [None, Some(probe().param("cell", 1))])),
        ]));
        let refused = refusal(&with_regions, &Presets::new())?;
        assert!(
            matches!(&refused, Some(Error::Invalid { place: Some(place), field, .. })
                if place == "step 2, look 2" && field == "cell"),
            "{refused:?}"
        );
        // the first step is named too, and a table checked on its own has no place
        let first = refusal(&Recipe::new([probe().param("cell", 1)]), &Presets::new())?;
        assert!(
            matches!(&first, Some(Error::Invalid { place: Some(place), .. }) if place == "step 1"),
            "{first:?}"
        );
        Ok(())
    }

    #[test]
    fn the_public_check_uses_the_core_effects_and_hands_the_recipe_back_unchanged() {
        let good = Recipe::new([Step::new("grade").param("temperature", 0.25), Step::new("pixel_grid")]);
        assert_eq!(good.clone().checked(&Presets::new()).ok(), Some(good));
        let bad = Recipe::new([Step::new("grade").param("kelvin", 5000)]);
        assert!(matches!(bad.checked(&Presets::new()), Err(Error::Invalid { effect, .. }) if effect == "grade"));
    }

    #[test]
    fn a_look_needs_a_description_besides_following_the_rules() {
        let described = |text: Option<&str>| {
            let mut recipe = Recipe::new([Step::new("grade")]);
            recipe.description = text.map(str::to_owned);
            recipe.checked_look(&Presets::new())
        };
        assert!(described(Some("A warm evening.")).is_ok());
        for missing in [None, Some(""), Some("  \n")] {
            assert!(
                matches!(described(missing), Err(Error::Recipe(message)) if message.contains("description")),
                "{missing:?}"
            );
        }
        let mut broken = Recipe::new([Step::new("nope")]);
        broken.description = Some("A look.".to_owned());
        assert!(matches!(
            broken.checked_look(&Presets::new()),
            Err(Error::UnknownEffect(_))
        ));
    }
}
