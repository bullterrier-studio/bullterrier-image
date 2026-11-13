//! A recipe file (TOML), read through the public API alone: the keys and tables it has, the rules of its format
//! versions, and what it refuses naming the key.

use serde_json::{Value, json};

use bullterrier_image::{Error, RECIPE_FORMATS, Recipe};

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
effect = "bordered"
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
        "frame": {"flip": "none", "angle": 0.0, "fit": "crop", "resample": "bicubic", "aspect": 1.0,
                  "anchor": "subject", "margin": 0.0, "pad_top": 0.0, "pad_right": 0.0, "pad_bottom": 0.0,
                  "pad_left": 0.0, "fill": [0.0, 0.0, 0.0], "transparent": "none", "corner_radius": 0.2},
        // a count of shares is written as the list, and every field of the kind is written, none ones included
        "regions": {"panes": {"kind": "grid", "fit": null, "edge": {"style": "straight", "amount": 4.8, "scale": 24.0, "seed": 0, "octaves": 6, "persistence": 0.55},
                              "columns": [1.0, 1.0, 1.0], "rows": [1.0, 1.0]}},
        "steps": [
            {"effect": "probe", "params": {"cell": 8, "angle": 90.0, "tint": [0.0, 0.5, 1.0]},
             "preset": "wide", "mask": "subject", "opacity": 0.4, "blend_mode": "screen", "name": "base"},
            {"effect": "bordered", "partition": "panes"},
            {"effect": "probe", "params": {"cell": {"by": "depth", "start": 4, "end": 100.0}}},
            {"each": "panes", "looks": [{"effect": "probe", "params": {"cells": 7}}, null],
             "assign": "random", "seed": 7, "feather": 3.0, "mask": "background"},
        ],
    });
    assert_eq!(written(FULL).ok(), Some(expected));
}

#[test]
fn regions_and_parameters_are_read_in_the_order_the_file_has_them() {
    let text = "format = 2\n[region.zeta]\nkind = \"grid\"\n[region.alpha]\nkind = \"grid\"\n\
                [[step]]\neffect = \"a\"\nzebra = 1\napple = 2\nmango = 3\n";
    let recipe = read(text).ok();
    let regions = recipe
        .as_ref()
        .map(|recipe| recipe.regions.iter().map(|(name, _)| name.to_owned()).collect());
    assert_eq!(regions, Some(vec!["zeta".to_owned(), "alpha".to_owned()]));
    let written = recipe
        .and_then(|recipe| serde_json::to_string(&recipe).ok())
        .unwrap_or_default();
    assert!(
        written.contains(r#""params":{"zebra":1,"apple":2,"mango":3}"#),
        "the parameters: {written}"
    );
}

#[test]
fn of_several_keys_that_are_no_recipe_key_the_first_in_the_file_is_named() {
    let refused = read("zebra = 1\napple = 2\n").err().map(|error| error.to_string());
    assert!(
        refused.as_deref().is_some_and(|message| message.contains("`zebra`")),
        "{refused:?}"
    );
}

#[test]
fn of_several_keys_an_each_step_does_not_have_the_first_in_the_file_is_named() {
    let text = "format = 2\n[[step]]\neach = \"p\"\nlooks = []\nzebra = 1\napple = 2\nmango = 3\n";
    let refused = read(text).err().map(|error| error.to_string());
    assert!(
        refused.as_deref().is_some_and(|message| message.contains("`zebra`")),
        "{refused:?}"
    );
}

#[test]
fn a_frame_is_read_as_every_field_with_the_defaults_for_the_ones_left_out() {
    let frame = written("[frame]\npad_top = 0.5\nfill = \"#fff\"\nflip = \"both\"\n[[step]]\neffect = \"a\"\n")
        .ok()
        .and_then(|recipe| recipe.get("frame").cloned());
    let expected = json!({
        "flip": "both", "angle": 0.0, "fit": "crop", "resample": "bicubic", "aspect": null, "anchor": "center",
        "margin": 0.0, "pad_top": 0.5, "pad_right": 0.0, "pad_bottom": 0.0, "pad_left": 0.0,
        "fill": [1.0, 1.0, 1.0], "transparent": "none", "corner_radius": 0.2,
    });
    assert_eq!(frame, Some(expected));
    // an empty table is the default frame, which is not written
    let empty = written("[frame]\n[[step]]\neffect = \"a\"\n").ok();
    assert_eq!(empty, Some(json!({"steps": [{"effect": "a"}]})));
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
            "format 2 has no color_range region",
            "format = 2\n[region.p]\nkind = \"color_range\"\n[[step]]\neffect = \"a\"\n",
            "`kind` 'color_range' needs `format = 3`",
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
        ("a frame has only its fields", "[frame]\nzoom = 2\n", "zoom"),
        ("a frame choice is a listed name", "[frame]\nflip = \"z\"\n", "flip"),
        ("a frame number is a number", "[frame]\nangle = \"up\"\n", "angle"),
        ("a frame fill is a colour", "[frame]\nfill = \"#abcd\"\n", "'#rrggbb'"),
        ("regions are tables", "format = 2\nregion = 3\n", "[region.NAME]"),
        (
            "each region is a table",
            "format = 2\n[region]\np = 3\n",
            "[region.NAME]",
        ),
        (
            "a region has a kind",
            "format = 2\n[region.p]\ncolumns = 2\n[[step]]\neffect = \"a\"\n",
            "`kind` is missing",
        ),
        (
            "a region's kind is one of the thirteen",
            "format = 2\n[region.p]\nkind = \"gird\"\n[[step]]\neffect = \"a\"\n",
            "linear_gradient",
        ),
        (
            "a region has only the fields of its kind",
            "format = 2\n[region.p]\nkind = \"grid\"\ncount = 2\n[[step]]\neffect = \"a\"\n",
            "unknown field `count`",
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
            "a table written where text is wanted shows what was written",
            "[[step]]\neffect = [\"grade\"]\n",
            "[\"grade\"]",
        ),
        (
            "a look is named by its place, counted from 1 as everywhere else",
            "format = 2\n[[step]]\neach = \"p\"\nlooks = [{ effect = \"a\" }, { opacity = 1 }]\n",
            "look 2",
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
        (
            "a color_range region in format 3",
            format!("format = 3\n{}", region("color_range")),
        ),
    ] {
        let result = read(&text);
        assert!(result.is_ok(), "{what}: {result:?}");
    }
}

/// The file reader is the oracle for the format a recipe needs: the recipe a text reads says the lowest format at which
/// the same text is read, and the one below it refuses the text.
#[test]
fn a_recipe_says_the_lowest_format_a_file_of_it_would_declare() {
    let step = "[[step]]\neffect = \"a\"\n";
    let region = |kind: &str| format!("[region.p]\nkind = \"{kind}\"\n{step}");
    let each = |look: &str| {
        format!("[region.p]\nkind = \"grid\"\n[[step]]\neach = \"p\"\nlooks = [{{ effect = \"a\"{look} }}]\n")
    };
    let rows: Vec<(String, i64, String)> = [
        ("one step", 1, step.to_owned()),
        (
            "every key a first format has",
            1,
            "name = \"x\"\ndescription = \"y\"\nsize = 9\n[frame]\naspect = 1\n[[step]]\neffect = \"a\"\nmask = \"subject\"\n\
             opacity = 0.5\nblend_mode = \"screen\"\nk = 1\n"
                .to_owned(),
        ),
        ("a grid", 2, region("grid")),
        ("a subject region", 2, region("subject")),
        ("an each step", 2, each("")),
        (
            "an each step of a region nobody declared",
            2,
            "[[step]]\neach = \"p\"\nlooks = [{ effect = \"a\" }]\n".to_owned(),
        ),
        ("a partition key", 2, "[[step]]\neffect = \"a\"\npartition = \"p\"\n".to_owned()),
        ("a drive", 3, "[[step]]\neffect = \"a\"\ncell = { by = \"depth\", start = 1, end = 2 }\n".to_owned()),
        ("a drive in a look", 3, each(", cell = { by = \"depth\", start = 1, end = 2 }")),
        ("a name on a look, which nothing addresses", 2, each(", name = \"x\"")),
        ("a step name", 3, "[[step]]\neffect = \"a\"\nname = \"x\"\n".to_owned()),
        (
            "an each step's name",
            3,
            "[region.p]\nkind = \"grid\"\n[[step]]\neach = \"p\"\nname = \"x\"\nlooks = [{ effect = \"a\" }]\n".to_owned(),
        ),
    ]
    .into_iter()
    .map(|(what, lowest, body)| (what.to_owned(), lowest, body))
    .chain(
        [
            "given",
            "band",
            "bands",
            "components",
            "combine",
            "color_range",
            "linear_gradient",
            "radial_gradient",
        ]
        .map(|kind| (format!("a {kind} region"), 3, region(kind))),
    )
    .collect();
    for (what, lowest, body) in rows {
        let at = |format: i64| read(&format!("format = {format}\n{body}"));
        let recipe = at(3);
        assert_eq!(
            recipe.as_ref().map(Recipe::format).ok(),
            Some(lowest),
            "{what}: {recipe:?}"
        );
        assert!(at(lowest).is_ok(), "{what} is read in format {lowest}");
        if lowest > 1 {
            assert!(at(lowest - 1).is_err(), "{what} is refused in format {}", lowest - 1);
        }
    }
}

#[test]
fn the_formats_a_release_reads_are_the_ones_a_file_may_declare() {
    let read_at = |format: i64| read(&format!("format = {format}\n[[step]]\neffect = \"a\"\n")).is_ok();
    let read_formats: Vec<i64> = (-1..=6).filter(|format| read_at(*format)).collect();
    assert_eq!(read_formats, RECIPE_FORMATS);
}
