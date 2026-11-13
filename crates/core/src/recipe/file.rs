//! The recipe file: a small TOML text, one serialisation of a [`Recipe`].

use serde_json::{Map, Value};

use super::{Each, Entry, Frame, Recipe, Region, Regions, STEP_KEYS, Step, read};
use crate::Error;

/// The format versions this release reads; a file without `format` is the first. Format 2 added regions and the step
/// keys `each`, `looks` and `partition`; format 3 added a parameter driven by a field (a table with `by`), the step key
/// `name` and the region kinds in [`KINDS_NEW_IN_3`]. A file of an earlier format may not use what a later one added,
/// so its meaning never changes.
const FORMATS: [i64; 3] = [1, 2, 3];

/// The keys of a file, each as it is written (a table or a list of tables in brackets), in the order the error for an
/// unknown one lists them.
const KEYS: [(&str, &str); 7] = [
    ("format", "format"),
    ("name", "name"),
    ("description", "description"),
    ("size", "size"),
    ("frame", "[frame]"),
    ("region", "[region.NAME]"),
    ("step", "[[step]]"),
];

/// The region kinds that need format 3.
const KINDS_NEW_IN_3: [&str; 8] = [
    "given",
    "band",
    "bands",
    "components",
    "combine",
    "colour",
    "linear_gradient",
    "radial_gradient",
];

/// Region kinds format 3 replaced, with what to write instead.
const KINDS_REPLACED_IN_3: [(&str, &str); 2] = [
    (
        "depth",
        "write `kind = \"band\"` with `of = \"depth\"`, `low` where `far` was and `high` where `near` was",
    ),
    (
        "depth_bands",
        "write `kind = \"bands\"` with `of = \"depth\"` and the same `count`",
    ),
];

impl Recipe {
    /// The recipe the TOML text `text` writes. Only the file is read: its shape and the rules of its format version
    /// (see below), not the rules of a look (see `Recipe::checked`).
    ///
    /// ```text
    /// format = 3                  # optional: 1 (the default), 2 or 3
    /// name = "green terminal"     # optional
    /// description = "Green text." # optional, a sentence
    /// size = 1024                 # optional: the longest side in px
    ///
    /// [frame]                     # optional: a table, read by the runner
    /// aspect = 1
    ///
    /// [region.panes]              # format 2: a named region, read by the runner
    /// kind = "grid"
    ///
    /// [[step]]
    /// effect = "ascii"            # the keys of a step (STEP_KEYS) are its own; every other key is a parameter
    /// cell = 9
    /// mask = "background"
    ///
    /// [[step]]                    # format 2: a look for each piece of a region
    /// each = "panes"
    /// looks = [{ effect = "halftone", cell = 9 }, "none"]   # "none": the piece stays as it was
    /// ```
    ///
    /// Format 2 adds regions and the step keys `each`, `looks` and `partition`; format 3 a parameter that is a table
    /// with `by` (a drive), the step key `name` and the region kinds `given`, `band`, `bands`, `components`, `combine`,
    /// `colour`, `linear_gradient` and `radial_gradient`. A file may not use what a later format added, and the kinds `depth`
    /// and `depth_bands` are gone in all. A whole number stays whole and a decimal stays decimal.
    ///
    /// # Errors
    /// [`Error::Recipe`] for text that is not TOML (the message keeps its line and column), a key that is not a
    /// recipe key, a format this release does not read, a feature used in a format that lacks it, a table of the
    /// wrong shape, or a value a recipe has no use for (`nan`, `inf`, a date or time), naming the step and the key.
    pub fn from_toml(text: &str) -> Result<Self, Error> {
        let parsed: toml::Table =
            toml::from_str(text).map_err(|error| Error::Recipe(error.to_string().trim_end().to_owned()))?;
        let file = model_of(parsed)?;
        let format = format_of(&file)?;
        if let Some(key) = file.keys().find(|key| KEYS.iter().all(|(known, _)| known != key)) {
            let keys: Vec<&str> = KEYS.iter().map(|(_, written)| *written).collect();
            return Err(Error::Recipe(format!(
                "`{key}` is not a recipe key; the keys are: {}",
                keys.join(", ")
            )));
        }
        let text_of = |key: &str| match file.get(key) {
            None => Ok(None),
            Some(Value::String(text)) => Ok(Some(text.clone())),
            Some(other) => Err(Error::Recipe(format!("`{key}` must be text, got {other}"))),
        };
        let description = text_of("description")?;
        if description.as_deref().is_some_and(|text| text.trim().is_empty()) {
            return Err(Error::Recipe(
                "`description` is empty; write a sentence in quotes".to_owned(),
            ));
        }
        let size = match file.get("size") {
            None => None,
            Some(value) => Some(
                value
                    .as_u64()
                    .and_then(|size| u32::try_from(size).ok())
                    .ok_or_else(|| Error::Recipe(format!("`size` must be a whole number from 1 up, got {value}")))?,
            ),
        };
        let frame = match file.get("frame") {
            None => Frame::default(),
            Some(Value::Object(frame)) => {
                read(Value::Object(frame.clone())).map_err(|why| Error::Recipe(format!("frame.{why}")))?
            }
            Some(_) => return Err(Error::Recipe("write the framing as a [frame] table".to_owned())),
        };
        let regions = regions_of(&file, format)?;
        let entries = match file.get("step") {
            None => Vec::new(),
            Some(Value::Array(entries)) if entries.iter().all(Value::is_object) => entries.clone(),
            Some(_) => {
                return Err(Error::Recipe(
                    "write each step as a [[step]] table (two brackets: there may be several)".to_owned(),
                ));
            }
        };
        if format == 1 && !regions.is_empty()
            || entries.iter().any(|entry| {
                format == 1
                    && ["each", "looks", "partition"]
                        .iter()
                        .any(|key| entry.get(key).is_some())
            })
        {
            return Err(Error::Recipe(
                "regions, `each`, `looks` and `partition` need `format = 2` at the top of the file".to_owned(),
            ));
        }
        let steps = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| step_of(entry, format, &format!("step {}", index + 1)))
            .collect::<Result<_, _>>()?;
        Ok(Self {
            steps,
            name: text_of("name")?,
            description,
            size,
            frame,
            regions,
        })
    }
}

/// The file's table as the model's values. TOML has what a recipe has no use for and `serde_json::Value` cannot hold:
/// a number that is not finite would turn into `null` and a date into a table of its own, both unseen, so they are
/// refused here, naming the key.
pub(crate) fn model_of(table: toml::Table) -> Result<Map<String, Value>, Error> {
    let mut path = String::new();
    table
        .into_iter()
        .map(|(key, value)| Ok((key.clone(), value_of(value, &mut path, &key)?)))
        .collect()
}

/// The model's value for `value`, which sits at `path` + `key` in the file.
fn value_of(value: toml::Value, path: &mut String, key: &str) -> Result<Value, Error> {
    let at = path.len();
    if !path.is_empty() && !key.starts_with('[') {
        path.push('.');
    }
    path.push_str(key);
    let converted = match value {
        toml::Value::String(text) => Ok(Value::String(text)),
        toml::Value::Integer(number) => Ok(Value::from(number)),
        toml::Value::Float(number) => serde_json::Number::from_f64(number)
            .map(Value::Number)
            .ok_or_else(|| Error::Recipe(format!("`{path}` must be a finite number, got {number}"))),
        toml::Value::Boolean(switch) => Ok(Value::Bool(switch)),
        toml::Value::Datetime(date) => Err(Error::Recipe(format!(
            "`{path}` is a date or time ({date}), which a recipe has no use for: write text in quotes"
        ))),
        toml::Value::Array(items) => items
            .into_iter()
            .enumerate()
            .map(|(index, item)| value_of(item, path, &format!("[{index}]")))
            .collect::<Result<_, _>>()
            .map(Value::Array),
        toml::Value::Table(table) => table
            .into_iter()
            .map(|(key, item)| Ok((key.clone(), value_of(item, path, &key)?)))
            .collect::<Result<_, _>>()
            .map(Value::Object),
    };
    path.truncate(at);
    converted
}

/// The format version a file declares: the first when it declares none.
fn format_of(file: &Map<String, Value>) -> Result<i64, Error> {
    let Some(declared) = file.get("format") else {
        return Ok(FORMATS[0]);
    };
    // an integer only: `3.0` and `true` are not versions
    declared
        .as_i64()
        .filter(|version| FORMATS.contains(version))
        .ok_or_else(|| {
            let read: Vec<String> = FORMATS.iter().map(ToString::to_string).collect();
            Error::Recipe(format!(
                "`format` {declared} is not one this release reads; it reads format {}",
                read.join(", ")
            ))
        })
}

/// The `[region.NAME]` tables as regions, in the order the file declares them, once each is a table of a kind this
/// format has.
fn regions_of(file: &Map<String, Value>, format: i64) -> Result<Regions, Error> {
    let tables = match file.get("region") {
        None => return Ok(Regions::new()),
        Some(Value::Object(tables)) if tables.values().all(Value::is_object) => tables,
        Some(_) => {
            return Err(Error::Recipe(
                "write each region as a [region.NAME] table (one table for each name)".to_owned(),
            ));
        }
    };
    let mut regions = Regions::new();
    for (name, table) in tables {
        // the gating reads the kind as written, to say which format has it or what replaced it
        if let Some(kind) = table.get("kind").and_then(Value::as_str) {
            if let Some((_, instead)) = KINDS_REPLACED_IN_3.iter().find(|(old, _)| *old == kind) {
                return Err(Error::Recipe(format!(
                    "region '{name}': `kind` '{kind}' is gone; {instead} (format = 3)"
                )));
            }
            if format < 3 && KINDS_NEW_IN_3.contains(&kind) {
                return Err(Error::Recipe(format!(
                    "region '{name}': `kind` '{kind}' needs `format = 3` at the top of the file"
                )));
            }
        }
        let region: Region = read(table.clone()).map_err(|why| Error::Recipe(format!("region '{name}': {why}")))?;
        regions.insert(name.as_str(), region);
    }
    Ok(regions)
}

/// The entry a `[[step]]` table writes: an each step when it has `each` or `looks`, else an effect step.
fn step_of(table: &Value, format: i64, place: &str) -> Result<Entry, Error> {
    let Value::Object(written) = table else {
        return Err(Error::Recipe(format!("{place}: write a step as a table")));
    };
    if format < 3 && written.contains_key("name") {
        return Err(Error::Recipe(format!(
            "{place}: `name` needs `format = 3` at the top of the file"
        )));
    }
    if format < 3
        && let Some((where_, key)) = drive_in(written)
    {
        return Err(Error::Recipe(format!(
            "{place}{where_}: `{key}` is a table with `by`, a drive, which needs `format = 3`"
        )));
    }
    let located = |error: Error| Error::Recipe(format!("{place}: {error}"));
    if written.contains_key("each") || written.contains_key("looks") {
        each_of(written).map(Entry::Each).map_err(located)
    } else {
        read::<Step>(step_table(written))
            .map(Entry::Step)
            .map_err(|message| located(Error::Recipe(message)))
    }
}

/// The each step a table writes.
fn each_of(written: &Map<String, Value>) -> Result<Each, Error> {
    let mut each = written.clone();
    if !each.contains_key("each") {
        return Err(Error::Recipe(
            "`looks` needs `each`, the name of the partition whose pieces get them".to_owned(),
        ));
    }
    // `shift_remove` keeps the file's order of the other keys, so the first unknown one is the one named
    let Some(Value::Array(looks)) = each.shift_remove("looks") else {
        return Err(Error::Recipe(
            "an each step needs `looks`, a list of tables (one effect each) or \"none\"".to_owned(),
        ));
    };
    let looks = looks
        .iter()
        .enumerate()
        .map(|(index, look)| match look {
            // a file's word for a piece left as it is
            Value::String(word) if word == "none" => Ok(Value::Null),
            Value::Object(table) => Ok(step_table(table)),
            other => Err(Error::Recipe(format!(
                "look {} is {other}: write a table with an `effect`, or \"none\"",
                index + 1
            ))),
        })
        .collect::<Result<Vec<_>, _>>()?;
    each.insert("looks".to_owned(), Value::Array(looks));
    read(Value::Object(each)).map_err(Error::Recipe)
}

/// The table of a step in the model's form: its keys as they are and every other key under `params`.
fn step_table(written: &Map<String, Value>) -> Value {
    let mut step = Map::new();
    let mut params = Map::new();
    for (key, value) in written {
        if STEP_KEYS.contains(&key.as_str()) {
            step.insert(key.clone(), value.clone());
        } else {
            params.insert(key.clone(), value.clone());
        }
    }
    if !params.is_empty() {
        step.insert("params".to_owned(), Value::Object(params));
    }
    Value::Object(step)
}

/// The first parameter of the step table `written` (or of a look in its `looks`) that is a drive, a table with `by`, as
/// (where in the step, the parameter's name).
fn drive_in(written: &Map<String, Value>) -> Option<(String, String)> {
    let own = written
        .iter()
        .find(|(_, value)| value.as_object().is_some_and(|table| table.contains_key("by")));
    if let Some((key, _)) = own {
        return Some((String::new(), key.clone()));
    }
    let Some(Value::Array(looks)) = written.get("looks") else {
        return None;
    };
    looks.iter().enumerate().find_map(|(index, look)| {
        let (_, key) = drive_in(look.as_object()?)?;
        Some((format!(", look {}", index + 1), key))
    })
}
