//! The shipped presets of the `shards` region kind: named sets of its fields, laid under the ones a declaration gives.
//!
//! The presets are data, one file for the whole package (`presets/regions/shards.toml`), compiled in so that the core
//! needs no file at run time.

use std::sync::OnceLock;

use serde_json::{Map, Value};

/// The text of the presets file.
const SHIPPED: &str = include_str!("../../../../../src/bullterrier_image/presets/regions/shards.toml");

/// The presets by name, each the table of its fields (without its `description`).
fn shipped() -> Result<&'static Map<String, Value>, &'static str> {
    static PRESETS: OnceLock<Result<Map<String, Value>, &'static str>> = OnceLock::new();
    PRESETS
        .get_or_init(|| {
            let Ok(Value::Object(mut presets)) = toml::from_str::<Value>(SHIPPED) else {
                return Err("the shipped shard presets are not a table of tables");
            };
            for preset in presets.values_mut() {
                if let Value::Object(fields) = preset {
                    fields.shift_remove("description");
                }
            }
            Ok(presets)
        })
        .as_ref()
        .map_err(|said| *said)
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
    let Some(Value::Object(fields)) = presets.get(name) else {
        return Err(format!(
            "no preset '{name}'; the presets are: {}",
            presets.keys().cloned().collect::<Vec<_>>().join(", ")
        ));
    };
    let mut filled = fields.clone();
    filled.extend(std::mem::take(region));
    *region = filled;
    Ok(())
}
