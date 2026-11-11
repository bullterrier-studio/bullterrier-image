//! A recipe read from a file on disk: the file's own text, and the files it names found beside it.

use std::path::Path;

use serde_json::Value;

use super::Recipe;
use crate::params::Kind;
use crate::{Error, Presets};

impl Recipe {
    /// The look called `name` that ships with the crate: a recipe with a `description`, named `name`.
    ///
    /// # Errors
    /// [`Error::Recipe`] saying `no look "name"`, which looks are closest (if any are) and all the looks, for any other
    /// name.
    pub fn look(name: &str) -> Result<Self, Error> {
        let looks = crate::shipped::looks();
        let Some((_, text)) = looks.iter().find(|(known, _)| *known == name) else {
            return Err(Error::Recipe(format!("no look {name:?}: {}", unknown_look(name))));
        };
        let mut recipe = Self::from_toml(text)?;
        recipe.name = Some(name.to_owned());
        Ok(recipe)
    }

    /// This recipe, read from `source` (a file's path or a look's name), held to the rules of a recipe
    /// ([`checked`](Self::checked)) with any refusal starting with `source`. A step of an effect that is not registered says
    /// which step it is and what the name is instead (see [`documentation::unknown_effect`](crate::documentation::unknown_effect)).
    ///
    /// # Errors
    /// [`Error::Recipe`] starting with `source`, then the step if the effect is unknown, then what `checked` refuses.
    pub fn checked_from(self, source: &str, presets: &Presets) -> Result<Self, Error> {
        let steps = self.steps.clone();
        self.checked(presets).map_err(|error| match error {
            Error::UnknownEffect(name) => {
                let step = steps
                    .iter()
                    .position(|entry| matches!(entry, super::Entry::Step(step) if step.effect == name))
                    .map_or_else(String::new, |at| format!("step {}: ", at + 1));
                Error::Recipe(format!(
                    "{source}: {step}{}",
                    crate::documentation::unknown_effect(&name, presets)
                ))
            }
            other => Error::Recipe(format!("{source}: {other}")),
        })
    }

    /// The recipe `spec` names: a look that ships with the crate if `spec` is exactly its name, else the recipe file at
    /// that path ([`from_file`](Self::from_file)).
    ///
    /// # Errors
    /// [`Error::Recipe`] saying `no look or file "spec"`, the closest looks and all the looks if `spec` is neither;
    /// what `from_file` refuses.
    pub fn resolve(spec: &str) -> Result<Self, Error> {
        if crate::shipped::looks().iter().any(|(known, _)| *known == spec) {
            Self::look(spec)
        } else if Path::new(spec).exists() {
            Self::from_file(Path::new(spec))
        } else {
            Err(Error::Recipe(format!(
                "no look or file {spec:?}: {}",
                unknown_look(spec)
            )))
        }
    }

    /// The recipe the file `path` holds ([`from_toml`](Self::from_toml)), with the name of the file (without `.toml`) when
    /// it names none, and each file it names found relative to the recipe file: the `file` of an svg region, and a
    /// parameter that is a picture or a lookup table given as a path. An absolute path, the name of a picture or table the
    /// photo is given (`$picture:...`, `$lut:...`) and a colour (`#rrggbb`) are left as they are.
    ///
    /// # Errors
    /// [`Error::Recipe`] starting with the file's name if it cannot be read, is not UTF-8, or is no recipe
    /// ([`from_toml`](Self::from_toml)).
    pub fn from_file(path: &Path) -> Result<Self, Error> {
        let name = path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        let refuse = |why: String| Error::Recipe(format!("{name}: {why}"));
        let bytes = std::fs::read(path).map_err(|error| refuse(format!("cannot be read: {error}")))?;
        let text =
            String::from_utf8(bytes).map_err(|error| refuse(format!("not UTF-8 text ({})", error.utf8_error())))?;
        let mut recipe = Self::from_toml(&text).map_err(|error| refuse(error.to_string()))?;
        if recipe.name.is_none() {
            recipe.name = path.file_stem().map(|stem| stem.to_string_lossy().into_owned());
        }
        let folder = path.parent().unwrap_or_else(|| Path::new("."));
        let mut written = serde_json::to_value(&recipe).map_err(|error| refuse(error.to_string()))?;
        found_beside(&mut written, folder);
        Self::from_json(&written.to_string()).map_err(|error| refuse(error.to_string()))
    }
}

/// `recipe` (in its JSON form) with each file it names relative to `folder` made `folder`'s.
fn found_beside(recipe: &mut Value, folder: &Path) {
    if let Some(Value::Object(regions)) = recipe.get_mut("regions") {
        for region in regions.values_mut() {
            if region.get("kind").and_then(Value::as_str) == Some("svg") {
                beside(region.get_mut("file"), folder);
            }
        }
    }
    let Some(Value::Array(steps)) = recipe.get_mut("steps") else {
        return;
    };
    for step in steps {
        if let Some(Value::Array(looks)) = step.get_mut("looks") {
            for look in looks.iter_mut() {
                step_files(look, folder);
            }
        } else {
            step_files(step, folder);
        }
    }
}

/// The parameters of `step` that are pictures or lookup tables given as paths, relative to `folder`.
fn step_files(step: &mut Value, folder: &Path) {
    let Some(effect) = step
        .get("effect")
        .and_then(Value::as_str)
        .and_then(|name| crate::registry::effect(name).ok())
    else {
        return;
    };
    let Some(table) = effect.params() else { return };
    let Some(Value::Object(params)) = step.get_mut("params") else {
        return;
    };
    for field in table
        .fields
        .iter()
        .filter(|field| matches!(field.kind, Kind::Picture | Kind::Lut))
    {
        beside(params.get_mut(field.name), folder);
    }
}

/// `value`, if it is a relative path, as `folder`'s own.
fn beside(value: Option<&mut Value>, folder: &Path) {
    let Some(value) = value else { return };
    let Some(text) = value.as_str() else { return };
    if text.starts_with('#') || text.starts_with('$') || Path::new(text).is_absolute() {
        return;
    }
    *value = Value::String(folder.join(text).to_string_lossy().into_owned());
}

/// What helps a person who typed `name` for a look that is not one: the closest looks (a few edits away), then all of them.
fn unknown_look(name: &str) -> String {
    let looks = crate::shipped::looks();
    let near = super::near::closest(name, looks.iter().map(|(known, _)| *known));
    let hint = if near.is_empty() {
        String::new()
    } else {
        let names: Vec<String> = near.iter().map(|known| format!("{known:?}")).collect();
        format!("did you mean {}? ", names.join(" or "))
    };
    let all: Vec<&str> = looks.iter().map(|(known, _)| *known).collect();
    format!("{hint}the looks: {}", all.join(", "))
}
