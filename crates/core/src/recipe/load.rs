//! A recipe read from a file on disk: the file's own text, and the files it names found beside it.

use std::path::Path;

use serde_json::Value;

use super::Recipe;
use crate::Error;
use crate::params::Kind;

impl Recipe {
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
            looks.iter_mut().for_each(|look| step_files(look, folder));
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
