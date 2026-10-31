//! The recipe file: a small TOML text, one serialisation of a [`Recipe`].

use serde_json::{Map, Value};

use super::{Each, Entry, Recipe, STEP_KEYS, Step, read};
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
const KINDS_NEW_IN_3: [&str; 6] = [
    "given",
    "band",
    "bands",
    "combine",
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
    /// with `by` (a drive), the step key `name` and the region kinds `given`, `band`, `bands`, `combine`,
    /// `linear_gradient` and `radial_gradient`. A file may not use what a later format added, and the kinds `depth`
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
            None => Map::new(),
            Some(Value::Object(frame)) => frame.clone(),
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
fn model_of(table: toml::Table) -> Result<Map<String, Value>, Error> {
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

/// The `[region.NAME]` tables, once each is a table of a kind this format has.
fn regions_of(file: &Map<String, Value>, format: i64) -> Result<Map<String, Value>, Error> {
    let regions = match file.get("region") {
        None => return Ok(Map::new()),
        Some(Value::Object(regions)) if regions.values().all(Value::is_object) => regions,
        Some(_) => {
            return Err(Error::Recipe(
                "write each region as a [region.NAME] table (one table for each name)".to_owned(),
            ));
        }
    };
    for (name, region) in regions {
        let Some(kind) = region.get("kind").and_then(Value::as_str) else {
            continue;
        };
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
    Ok(regions.clone())
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
    let Some(Value::Array(looks)) = each.remove("looks") else {
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

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::{Error, Recipe};

    fn read(text: &str) -> Result<Recipe, Error> {
        Recipe::from_toml(text)
    }

    /// The recipe read from `text`, as the JSON it is written in.
    fn written(text: &str) -> Result<Value, Error> {
        read(text).and_then(|recipe| serde_json::to_value(recipe).map_err(|error| Error::Recipe(error.to_string())))
    }

    const FULL: &str = r#"
# a look with every kind of key
format = 3
name = "night market"
description = "Dark streets, bright signs."
size = 1024

[frame]
aspect = 1
anchor = "subject"

[region.panes]
kind = "grid"
columns = 3
rows = 2

[[step]]
effect = "probe"
name = "base"
mask = "subject"
opacity = 0.4
blend_mode = "screen"
preset = "wide"
cell = 8             # a whole number stays whole
angle = 90.0         # a decimal stays decimal
tint = [0.0, 0.5, 1.0]

[[step]]
effect = "seams"
partition = "panes"

[[step]]
effect = "probe"
cell = { by = "depth", start = 4, end = 100.0 }

[[step]]
each = "panes"
assign = "random"
seed = 7
feather = 3
mask = "background"
looks = [
  { effect = "probe", cells = 7 },
  "none",
]
"#;

    #[test]
    fn a_recipe_file_reads_into_the_steps_it_writes() {
        let expected = json!({
            "name": "night market", "description": "Dark streets, bright signs.", "size": 1024,
            "frame": {"aspect": 1, "anchor": "subject"},
            "regions": {"panes": {"kind": "grid", "columns": 3, "rows": 2}},
            "steps": [
                {"effect": "probe", "params": {"cell": 8, "angle": 90.0, "tint": [0.0, 0.5, 1.0]},
                 "preset": "wide", "mask": "subject", "opacity": 0.4, "blend_mode": "screen", "name": "base"},
                {"effect": "seams", "partition": "panes"},
                {"effect": "probe", "params": {"cell": {"by": "depth", "start": 4, "end": 100.0}}},
                {"each": "panes", "looks": [{"effect": "probe", "params": {"cells": 7}}, null],
                 "assign": "random", "seed": 7, "feather": 3.0, "mask": "background"},
            ],
        });
        assert_eq!(written(FULL).ok(), Some(expected));
    }

    #[test]
    fn a_key_that_is_no_step_key_is_a_parameter_even_when_it_is_called_params() {
        let plain = written("[[step]]\neffect = \"a\"\nparams = 1\nmask = \"subject\"\n").ok();
        assert_eq!(
            plain,
            Some(json!({"steps": [{"effect": "a", "params": {"params": 1}, "mask": "subject"}]}))
        );
    }

    #[test]
    fn a_file_with_only_steps_is_the_first_format() {
        assert!(read("[[step]]\neffect = \"a\"\n").is_ok());
        assert!(read("format = 1\n[[step]]\neffect = \"a\"\n").is_ok());
        // reading checks the shape, not the rules: a size of 0 is checked later
        assert_eq!(read("size = 0\n").ok().and_then(|recipe| recipe.size), Some(0));
    }

    /// (the rule, a file that breaks it, a word of the message).
    #[expect(clippy::too_many_lines, reason = "a table of rows, one rule each")]
    fn bad() -> Vec<(&'static str, &'static str, &'static str)> {
        vec![
            (
                "format 1 has no regions",
                "format = 1\n[region.p]\nkind = \"grid\"\n[[step]]\neffect = \"a\"\n",
                "format = 2",
            ),
            (
                "a file without format has no regions",
                "[region.p]\nkind = \"grid\"\n[[step]]\neffect = \"a\"\n",
                "format = 2",
            ),
            (
                "format 1 has no each step",
                "[[step]]\neach = \"p\"\nlooks = []\n",
                "format = 2",
            ),
            (
                "format 1 has no partition",
                "[[step]]\neffect = \"a\"\npartition = \"p\"\n",
                "format = 2",
            ),
            (
                "format 2 has no step name",
                "format = 2\n[[step]]\neffect = \"a\"\nname = \"x\"\n",
                "`name` needs `format = 3`",
            ),
            (
                "format 2 has no drive",
                "format = 2\n[[step]]\neffect = \"a\"\ncell = { by = \"depth\", start = 1, end = 2 }\n",
                "`cell` is a table with `by`",
            ),
            (
                "format 2 has no drive in a look",
                "format = 2\n[region.p]\nkind = \"grid\"\n[[step]]\neach = \"p\"\nlooks = [{ effect = \"a\" }, { effect = \"a\", cell = { by = \"depth\", start = 1, end = 2 } }]\n",
                "look 2",
            ),
            (
                "format 2 has no band region",
                "format = 2\n[region.p]\nkind = \"band\"\n[[step]]\neffect = \"a\"\n",
                "`kind` 'band' needs `format = 3`",
            ),
            (
                "format 2 has no given region",
                "format = 2\n[region.p]\nkind = \"given\"\n[[step]]\neffect = \"a\"\n",
                "needs `format = 3`",
            ),
            (
                "the kind depth is gone",
                "format = 3\n[region.p]\nkind = \"depth\"\n[[step]]\neffect = \"a\"\n",
                "band",
            ),
            (
                "the kind depth_bands is gone",
                "format = 3\n[region.p]\nkind = \"depth_bands\"\n[[step]]\neffect = \"a\"\n",
                "bands",
            ),
            (
                "format 4 is not read",
                "format = 4\n[[step]]\neffect = \"a\"\n",
                "`format` 4",
            ),
            ("format 0 is not read", "format = 0\n", "`format` 0"),
            ("a format is a whole number", "format = 3.0\n", "`format` 3.0"),
            ("a format is not text", "format = \"3\"\n", "`format`"),
            ("a format is not a switch", "format = true\n", "`format` true"),
            ("only the recipe's keys", "steps = []\n", "`steps` is not a recipe key"),
            ("steps are step tables", "step = 3\n", "[[step]]"),
            ("each step is a table", "step = [1]\n", "[[step]]"),
            ("the frame is a table", "frame = \"x\"\n", "[frame]"),
            ("regions are tables", "format = 2\nregion = 3\n", "[region.NAME]"),
            (
                "each region is a table",
                "format = 2\n[region]\np = 3\n",
                "[region.NAME]",
            ),
            ("a name is text", "name = 3\n", "`name`"),
            ("a description is a sentence", "description = \"\"\n", "`description`"),
            ("a description is text", "description = 5\n", "`description`"),
            ("a size is whole", "size = 1.5\n", "`size`"),
            ("a size is not negative", "size = -1\n", "`size`"),
            ("a size is a number", "size = \"big\"\n", "`size`"),
            ("a syntax error keeps its line", "name = \"a\"\nstep = [\n", "line 2"),
            (
                "a step has an effect or each",
                "[[step]]\nmask = \"subject\"\n",
                "`effect`",
            ),
            (
                "an each step has looks",
                "format = 2\n[[step]]\neach = \"p\"\n",
                "looks",
            ),
            ("looks come with each", "format = 2\n[[step]]\nlooks = []\n", "`each`"),
            (
                "looks are a list",
                "format = 2\n[[step]]\neach = \"p\"\nlooks = \"none\"\n",
                "looks",
            ),
            (
                "a look is a table or none",
                "format = 2\n[[step]]\neach = \"p\"\nlooks = [3]\n",
                "look 1",
            ),
            (
                "the only word for a skipped look is none",
                "format = 2\n[[step]]\neach = \"p\"\nlooks = [\"nope\"]\n",
                "look 1",
            ),
            (
                "a look is no each step",
                "format = 2\n[[step]]\neach = \"p\"\nlooks = [{ effect = \"a\", each = \"q\" }]\n",
                "each",
            ),
            (
                "an each step has its own keys",
                "format = 2\n[[step]]\neach = \"p\"\nlooks = []\ncolour = 1\n",
                "colour",
            ),
            (
                "a step key has its type",
                "[[step]]\neffect = \"a\"\nopacity = \"half\"\n",
                "opacity",
            ),
            (
                "a refusal names the step",
                "[[step]]\neffect = \"a\"\n[[step]]\neffect = 3\n",
                "step 2",
            ),
        ]
    }

    #[test]
    fn a_file_that_breaks_a_rule_of_the_format_is_refused_naming_the_trouble() {
        for (rule, text, word) in bad() {
            let refused = read(text).err().map(|error| error.to_string());
            assert!(
                refused.as_deref().is_some_and(|message| message.contains(word)),
                "{rule}: wanted {word:?}, got {refused:?}"
            );
        }
    }

    #[test]
    fn a_number_that_is_not_finite_and_a_date_are_refused_naming_the_key() {
        let step = "[[step]]\neffect = \"a\"\n";
        let region = "format = 2\n[region.p]\nkind = \"grid\"\n";
        let rows = [
            (
                "nan",
                format!("{step}{step}opacity = nan\n"),
                "step[1].opacity",
                "finite",
            ),
            ("inf", format!("{step}opacity = inf\n"), "step[0].opacity", "finite"),
            ("-inf", format!("{step}radius = -inf\n"), "step[0].radius", "finite"),
            ("+inf", format!("{step}radius = +inf\n"), "step[0].radius", "finite"),
            ("-nan", format!("{step}radius = -nan\n"), "step[0].radius", "finite"),
            (
                "in a drive",
                format!("{step}cell = {{ by = \"depth\", start = nan, end = 1 }}\n"),
                "step[0].cell.start",
                "finite",
            ),
            (
                "in a list",
                format!("{step}tint = [0.0, inf, 1.0]\n"),
                "step[0].tint[1]",
                "finite",
            ),
            (
                "in a look",
                format!("{region}[[step]]\neach = \"p\"\nlooks = [{{ effect = \"a\", radius = inf }}]\n"),
                "step[0].looks[0].radius",
                "finite",
            ),
            (
                "in the framing",
                format!("[frame]\naspect = nan\n{step}"),
                "frame.aspect",
                "finite",
            ),
            (
                "a date and time",
                format!("{step}when = 1979-05-27T07:32:00Z\n"),
                "step[0].when",
                "date",
            ),
            ("a date", format!("{step}when = 1979-05-27\n"), "step[0].when", "date"),
            ("a time", format!("{step}when = 07:32:00\n"), "step[0].when", "date"),
            (
                "a date in a list",
                format!("{step}days = [1, 1979-05-27]\n"),
                "step[0].days[1]",
                "date",
            ),
        ];
        for (what, text, key, word) in rows {
            let refused = read(&text).err().map(|error| error.to_string());
            assert!(
                refused
                    .as_deref()
                    .is_some_and(|message| message.contains(key) && message.contains(word)),
                "{what}: wanted {key:?} and {word:?}, got {refused:?}"
            );
        }
        // the finite numbers either side of the limits are read as they are written
        let read = written(&format!(
            "{step}a = 1.7976931348623157e308\nb = -0.0\nc = 5e-324\nd = [1, 2.5]\n"
        ));
        let expected = json!({"steps": [{"effect": "a", "params": {
            "a": 1.797_693_134_862_315_7e308, "b": -0.0, "c": 5e-324, "d": [1, 2.5]}}]});
        assert_eq!(read.ok(), Some(expected));
    }

    #[test]
    fn the_features_of_each_format_read_in_the_format_that_has_them() {
        let steps = "[[step]]\neffect = \"a\"\n";
        let region = |kind: &str| format!("[region.p]\nkind = \"{kind}\"\n{steps}");
        let each = "[region.p]\nkind = \"grid\"\n[[step]]\neach = \"p\"\nlooks = [{ effect = \"a\" }]\n";
        let drive = "[[step]]\neffect = \"a\"\ncell = { by = \"depth\", start = 1, end = 2 }\n";
        let named = "[[step]]\neffect = \"a\"\nname = \"x\"\n";
        for (what, text) in [
            (
                "a grid region and an each step in format 2",
                format!("format = 2\n{each}"),
            ),
            (
                "a partition key in format 2",
                "format = 2\n[[step]]\neffect = \"a\"\npartition = \"p\"\n".to_owned(),
            ),
            ("a drive in format 3", format!("format = 3\n{drive}")),
            ("a step name in format 3", format!("format = 3\n{named}")),
            ("a band region in format 3", format!("format = 3\n{}", region("band"))),
            ("a given region in format 3", format!("format = 3\n{}", region("given"))),
        ] {
            let result = read(&text);
            assert!(result.is_ok(), "{what}: {result:?}");
        }
    }
}
