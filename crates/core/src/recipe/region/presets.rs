//! The shipped presets of the `shards` region kind: named sets of its fields, laid under the ones a declaration gives.
//!
//! The presets are data, one file inside the crate (`data/regions/shards.toml`), compiled in so that the core needs no
//! file at run time.

use std::sync::OnceLock;

use serde_json::{Map, Value};

use crate::{Error, Preset};

/// The text of the presets file.
const SHIPPED: &str = include_str!("../../../data/regions/shards.toml");

/// The region kinds that have shipped presets.
const KINDS: [&str; 1] = ["shards"];

/// The presets of the `shards` kind in file order.
fn shipped() -> Result<&'static [(String, Preset)], &'static str> {
    static PRESETS: OnceLock<Result<Vec<(String, Preset)>, &'static str>> = OnceLock::new();
    PRESETS
        .get_or_init(|| {
            let Ok(Value::Object(table)) = toml::from_str::<Value>(SHIPPED) else {
                return Err("the shipped shard presets are not a table of tables");
            };
            let mut presets = Vec::new();
            for (name, preset) in table {
                let Value::Object(mut params) = preset else {
                    return Err("a shipped shard preset is not a table");
                };
                let Some(Value::String(description)) = params.shift_remove("description") else {
                    return Err("a shipped shard preset has no description");
                };
                presets.push((name, Preset { description, params }));
            }
            Ok(presets)
        })
        .as_ref()
        .map(Vec::as_slice)
        .map_err(|said| *said)
}

/// The shipped presets of the region kind `kind` (`"shards"` is the only one that has any), in the order the shipped
/// file lists them: each a name and a [`Preset`], the sentence saying what it looks like and the fields of the region
/// that it sets, as a region declaration writes them. A shards region with `preset` set to the name is read with
/// these fields beneath the ones it gives.
///
/// # Errors
/// [`Error::Recipe`] naming the kinds that have presets if `kind` is not one of them.
pub fn region_presets(kind: &str) -> Result<Vec<(String, Preset)>, Error> {
    if kind != "shards" {
        return Err(Error::Recipe(format!(
            "region kind '{kind}' has no presets; the kinds with presets are: {}",
            KINDS.join(", ")
        )));
    }
    Ok(shipped().map_err(|said| Error::Recipe(said.to_owned()))?.to_vec())
}

/// Fills the table `region` of a `shards` region with the fields of the preset it names (`preset`), beneath the fields it
/// gives itself: a field given wins, and a given `edge` replaces the preset's whole `edge`. A region with no `preset`, or
/// one that is not text, is left for the typed read to judge.
///
/// The `preset` is kept, and the fields it sets become explicit, so reading the region again as it is written changes
/// nothing.
///
/// # Errors
/// A message listing the presets if the name is not one of them.
pub(super) fn lay_under(region: &mut Map<String, Value>) -> Result<(), String> {
    let Some(Value::String(name)) = region.get("preset") else {
        return Ok(());
    };
    let presets = shipped().map_err(str::to_owned)?;
    let Some((_, Preset { params, .. })) = presets.iter().find(|(known, _)| known == name) else {
        return Err(format!(
            "no preset '{name}'; the presets are: {}",
            presets
                .iter()
                .map(|(known, _)| known.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    };
    let mut filled = params.clone();
    filled.extend(std::mem::take(region));
    *region = filled;
    Ok(())
}
