//! The rules of a look.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::needs::{Use, Uses};
use super::references::{self, Form};
use super::{Drive, Each, Entry, Recipe, Regions, STEP_KEYS, Step, drive};
use crate::effect::Input;
use crate::registry::Registry;
use crate::{Error, Presets};

/// What an address of a step's numbers starts with besides a step's name: a step cannot be called so.
const ADDRESS_WORDS: [&str; 3] = ["region", "frame", "each"];

impl Recipe {
    /// This recipe if it follows the rules of a look, else the first it breaks. A recipe must pass this before
    /// anything runs it. The rules:
    /// - at least one step; a `size` of at least 1; a `frame` whose numbers lie within their ranges (see [`Frame`](super::Frame));
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
    /// - what a step names is a region: a `mask`, a drive's `by`, the field an effect's field parameter names (its
    ///   default if the step sets none) and a step's `partition` are one of `subject`, `background`, `depth`, `light` or
    ///   declared in `regions`; an `each` is declared. The kind a region must be
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

    /// The rules of [`checked`](Self::checked) against `registry`, and the names of the regions the steps refer to.
    pub(crate) fn check_in(&self, registry: &Registry, presets: &Presets) -> Result<Uses, Error> {
        if self.steps.is_empty() {
            return Err(Error::Recipe("a recipe needs at least one step".to_owned()));
        }
        if self.size == Some(0) {
            return Err(Error::Recipe("`size` must be >= 1, got 0".to_owned()));
        }
        self.frame.check()?;
        for (name, region) in self.regions.iter() {
            region.check(name, &self.regions)?;
        }
        references::check_regions(&self.regions)?;
        let mut rules = Rules {
            registry,
            presets,
            regions: &self.regions,
            at: 0,
            uses: Vec::new(),
        };
        for (index, entry) in self.steps.iter().enumerate() {
            rules.at = index;
            let place = format!("step {}", index + 1);
            match entry {
                Entry::Step(step) => rules.step(step, &place)?,
                Entry::Each(each) => rules.each(each, &place)?,
            }
        }
        self.check_names()?;
        Ok(Uses(rules.uses))
    }

    /// The names of the steps are unique and none is the name of an effect a step uses (it would be taken for that
    /// effect in an address).
    fn check_names(&self) -> Result<(), Error> {
        let used: BTreeSet<&str> = self
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
            if used.contains(name.as_str()) {
                return Err(Error::Recipe(format!(
                    "step {number}: `name` '{name}' is also an effect this recipe uses; choose another"
                )));
            }
            first.insert(name, number);
        }
        Ok(())
    }
}

/// Why `value`, a number of the recipe, is not within `min` and `max` (both inclusive), as words that follow the number's
/// name (`must be >= 0, got -1`); none if it is. A number that is not finite is outside any range.
pub(super) fn outside(value: f64, min: f64, max: f64) -> Option<String> {
    if !value.is_finite() {
        Some(format!("must be a finite number, got {value}"))
    } else if value < min {
        Some(format!("must be >= {min}, got {value}"))
    } else if value > max {
        Some(format!("must be <= {max}, got {value}"))
    } else {
        None
    }
}

/// What the rules of a recipe look things up in.
struct Rules<'a> {
    registry: &'a Registry,
    presets: &'a Presets,
    regions: &'a Regions,
    /// The step being checked, counting from 0.
    at: usize,
    /// The places the steps checked so far name a region.
    uses: Vec<Use>,
}

impl Rules<'_> {
    fn step(&mut self, step: &Step, place: &str) -> Result<(), Error> {
        let effect = self.registry.effect(&step.effect)?;
        self.layer(place, step.mask.as_deref(), step.opacity, step.name.as_deref())?;
        let params = self.presets.under(step, place)?;
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
        let drives = drive::in_params(&step.effect, &params, place)?;
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
            self.refer(place, "by", &drive.by, Some(Form::Selection))?;
        }
        for field in effect.fields() {
            let name = field
                .field_in(&params)
                .map_err(|why| Error::Recipe(format!("{place}: {why}")))?;
            self.refer(place, &field.parameter, name, Some(Form::Selection))?;
        }
        if let Some(partition) = &step.partition {
            self.refer(place, "partition", partition, None)?;
        }
        Ok(())
    }

    fn each(&mut self, each: &Each, place: &str) -> Result<(), Error> {
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
        self.refer(place, "each", &each.each, Some(Form::Partition))?;
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
    fn layer(
        &mut self,
        place: &str,
        mask: Option<&str>,
        opacity: Option<f64>,
        name: Option<&str>,
    ) -> Result<(), Error> {
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
            Some(mask) => self.refer(place, "mask", mask, Some(Form::Selection)),
            None => Ok(()),
        }
    }

    /// `name`, what `key` of the step at `place` refers to, is a region of the `wanted` form (none: either), and is
    /// remembered as used.
    fn refer(&mut self, place: &str, key: &str, name: &str, wanted: Option<Form>) -> Result<(), Error> {
        references::refer(self.regions, place, key, name, wanted)?;
        self.uses.push(Use {
            step: self.at,
            place: place.to_owned(),
            key: key.to_owned(),
            name: name.to_owned(),
        });
        Ok(())
    }
}
