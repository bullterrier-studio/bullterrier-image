//! The recipe model, through the public API alone: the builders of a step, how a recipe is written and read back as
//! JSON (the form a person would write, in the order read), what a key or value that is not the model's is refused
//! with, and the blend modes.

use crate::common::recipe::regions;
use bullterrier_image::frame::Anchor;
use bullterrier_image::params::Color;
use bullterrier_image::{Assign, BlendMode, Each, Entry, Error, Frame, Recipe, Step};
use serde_json::{Value, json};

fn read(text: &Value) -> Result<Recipe, Error> {
    Recipe::from_json(&text.to_string())
}

#[test]
fn builders_set_what_they_say_and_leave_the_rest_at_its_default() {
    let step = Step::new("bloom")
        .param("radius", 12)
        .param("threshold", 0.25)
        .mask("subject")
        .opacity(0.4)
        .blend_mode(BlendMode::Screen);
    assert_eq!(step.effect, "bloom");
    assert_eq!(
        Value::Object(step.params.clone()),
        json!({"radius": 12, "threshold": 0.25})
    );
    assert_eq!(
        (step.mask.as_deref(), step.opacity, step.blend_mode),
        (Some("subject"), Some(0.4), BlendMode::Screen)
    );
    assert_eq!((step.preset, step.partition, step.name), (None, None, None));
    let plain = Step::new("grade");
    assert_eq!(
        (plain.opacity, plain.blend_mode, plain.params.len()),
        (None, BlendMode::Normal, 0)
    );
    // a later call for the same key replaces it
    assert_eq!(Step::new("a").param("k", 1).param("k", 2).params["k"], json!(2));
}

#[test]
fn a_recipe_is_written_in_the_form_a_person_would_and_reads_back_equal() {
    let mut each = Each::new("panes", [Some(Step::new("grade").param("exposure", 0.5)), None]);
    each.assign = Assign::Random;
    each.seed = 7;
    each.feather = 3.0;
    each.mask = Some("background".to_owned());
    each.opacity = Some(0.5);
    each.blend_mode = BlendMode::Multiply;
    each.name = Some("tiles".to_owned());
    let mut bordered = Step::new("bordered");
    bordered.partition = Some("panes".to_owned());
    bordered.preset = Some("thin".to_owned());
    bordered.name = Some("edges".to_owned());
    let mut recipe = Recipe::new([
        Entry::from(Step::new("bloom").param("radius", 12).opacity(0.4)),
        bordered.into(),
        each.into(),
    ]);
    recipe.name = Some("night".to_owned());
    recipe.description = Some("Dark and glowing.".to_owned());
    recipe.size = Some(1024);
    recipe.frame = Frame {
        aspect: Some(1.0),
        anchor: Anchor::Subject,
        pad_left: 0.25,
        fill: Color([0.0, 0.5, 1.0]),
        ..Frame::default()
    };
    recipe.regions = regions(json!({"panes": {"kind": "grid", "columns": 2}}));
    let written = json!({
        "steps": [
            {"effect": "bloom", "params": {"radius": 12}, "opacity": 0.4},
            {"effect": "bordered", "preset": "thin", "partition": "panes", "name": "edges"},
            {"each": "panes", "looks": [{"effect": "grade", "params": {"exposure": 0.5}}, null],
             "assign": "random", "seed": 7, "feather": 3.0, "mask": "background", "opacity": 0.5,
             "blend_mode": "multiply", "name": "tiles"},
        ],
        "name": "night", "description": "Dark and glowing.", "size": 1024,
        "frame": {"flip": "none", "angle": 0.0, "fit": "crop", "resample": "bicubic", "aspect": 1.0,
                  "anchor": "subject", "margin": 0.0, "pad_top": 0.0, "pad_right": 0.0, "pad_bottom": 0.0,
                  "pad_left": 0.25, "fill": [0.0, 0.5, 1.0], "transparent": "none", "corner_radius": 0.2},
        "regions": {"panes": {"kind": "grid", "edge": {"style": "straight", "amount": 4.8, "scale": 24.0, "seed": 0, "octaves": 6, "persistence": 0.55}, "columns": [1.0, 1.0], "rows": [1.0]}},
    });
    assert_eq!(serde_json::to_value(&recipe).ok(), Some(written.clone()));
    assert_eq!(read(&written).ok(), Some(recipe));
}

#[test]
fn what_is_left_at_its_default_is_not_written_and_is_read_as_the_default() {
    let recipe = Recipe::new([Step::new("grade"), Step::new("pattern")]);
    assert_eq!(
        serde_json::to_value(&recipe).ok(),
        Some(json!({"steps": [{"effect": "grade"}, {"effect": "pattern"}]}))
    );
    let each = read(&json!({"steps": [{"each": "p", "looks": []}]}));
    assert_eq!(
        each.ok().map(|recipe| recipe.steps),
        Some(vec![Entry::Each(Each::new("p", []))]),
    );
    let each = Each::new("p", []);
    assert_eq!(
        (each.assign, each.seed, each.blend_mode),
        (Assign::Cycle, 0, BlendMode::Normal)
    );
    assert!((each.feather - 0.5).abs() < f64::EPSILON);
}

#[test]
fn a_whole_number_stays_whole_and_a_decimal_stays_decimal() {
    let recipe = Recipe::new([Step::new("a").param("whole", 4).param("decimal", 4.0)]);
    let text = serde_json::to_string(&recipe).ok();
    assert_eq!(
        text.as_deref(),
        Some(r#"{"steps":[{"effect":"a","params":{"whole":4,"decimal":4.0}}]}"#)
    );
    assert_eq!(
        Recipe::from_json(text.as_deref().unwrap_or_default()).ok(),
        Some(recipe)
    );
}

#[test]
fn a_recipe_is_written_in_the_order_it_was_read_in() {
    // names that are not in alphabetical order, at every table a recipe has
    let text = r#"{"steps": [{"effect": "a", "params": {"zebra": 1, "apple": 2, "mango": 3}}],
                   "regions": {"zeta": {"kind": "grid"}, "alpha": {"kind": "grid"}, "mid": {"kind": "grid"}}}"#;
    let recipe = Recipe::from_json(text).ok();
    let regions = recipe.as_ref().map(|recipe| {
        recipe
            .regions
            .iter()
            .map(|(name, _)| name.to_owned())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        regions,
        Some(vec!["zeta".to_owned(), "alpha".to_owned(), "mid".to_owned()])
    );
    let written = recipe.and_then(|recipe| serde_json::to_value(&recipe).ok());
    let keys_at = |pointer: &str| {
        let table = written.as_ref()?.pointer(pointer)?.as_object()?;
        Some(table.keys().map(String::as_str).collect::<Vec<_>>())
    };
    assert_eq!(keys_at("/steps/0/params"), Some(vec!["zebra", "apple", "mango"]));
    assert_eq!(keys_at("/regions"), Some(vec!["zeta", "alpha", "mid"]));
}

#[test]
fn an_entry_is_an_each_by_its_each_key_and_a_step_by_its_effect() {
    let steps = read(&json!({"steps": [{"effect": "grade"}, {"each": "p", "looks": [null]}]})).map(|r| r.steps);
    assert!(
        matches!(steps.as_deref(), Ok([Entry::Step(_), Entry::Each(_)])),
        "{steps:?}"
    );
    let neither = read(&json!({"steps": [{"mask": "subject"}]}));
    assert!(
        matches!(&neither, Err(Error::Recipe(message)) if message.contains("`effect`") && message.contains("`each`")),
        "{neither:?}"
    );
}

#[test]
fn a_key_that_is_not_the_models_is_refused_by_name() {
    for (what, text, key) in [
        (
            "a parameter beside the step's keys",
            json!({"steps": [{"effect": "bloom", "radius": 12}]}),
            "radius",
        ),
        ("an unknown recipe key", json!({"steps": [], "format": 3}), "format"),
        (
            "an unknown key of an each step",
            json!({"steps": [{"each": "p", "looks": [], "count": 2}]}),
            "count",
        ),
        (
            "an unknown key of a look",
            json!({"steps": [{"each": "p", "looks": [{"effect": "a", "each": "x"}]}]}),
            "each",
        ),
    ] {
        let result = read(&text);
        assert!(
            matches!(&result, Err(Error::Recipe(message)) if message.contains(key)),
            "{what}: {result:?}"
        );
    }
}

#[test]
fn a_value_of_the_wrong_type_is_refused_naming_the_key() {
    for (what, text, key) in [
        (
            "an opacity as text",
            json!({"steps": [{"effect": "a", "opacity": "half"}]}),
            "opacity",
        ),
        (
            "a blend mode that is none of the seven",
            json!({"steps": [{"effect": "a", "blend_mode": "burn"}]}),
            "blend_mode",
        ),
        (
            "an assign that is none of the two",
            json!({"steps": [{"each": "p", "looks": [], "assign": "shuffle"}]}),
            "assign",
        ),
        (
            "a seed beyond 32 bits",
            json!({"steps": [{"each": "p", "looks": [], "seed": 4_294_967_296_u64}]}),
            "seed",
        ),
        ("a negative size", json!({"steps": [], "size": -1}), "size"),
        ("steps that are not a list", json!({"steps": {"effect": "a"}}), "steps"),
        (
            "a look that is not a step",
            json!({"steps": [{"each": "p", "looks": [{"effect": 3}]}]}),
            "effect",
        ),
    ] {
        let result = read(&text);
        assert!(
            matches!(&result, Err(Error::Recipe(message)) if message.contains(key)),
            "{what}: {result:?}"
        );
    }
    assert!(matches!(read(&json!("steps")), Err(Error::Recipe(_))));
    assert!(matches!(Recipe::from_json("{steps"), Err(Error::Recipe(_))));
    // the largest seed is allowed
    let seed = read(&json!({"steps": [{"each": "p", "looks": [], "seed": 4_294_967_295_u32}]}));
    assert!(seed.is_ok(), "{seed:?}");
}

#[test]
fn a_key_written_twice_is_refused_wherever_it_is() {
    for (what, text, key) in [
        ("in a recipe", r#"{"steps": [], "size": 1, "size": 2}"#, "size"),
        ("in a step", r#"{"steps": [{"effect": "a", "effect": "b"}]}"#, "effect"),
        (
            "in a step's parameters",
            r#"{"steps": [{"effect": "a", "params": {"x": 1, "x": 2}}]}"#,
            "x",
        ),
        (
            "in a drive",
            r#"{"steps": [{"effect": "a", "params": {"x": {"by": "depth", "by": "light"}}}]}"#,
            "by",
        ),
        (
            "in a look",
            r#"{"steps": [{"each": "p", "looks": [{"effect": "a", "mask": "x", "mask": "y"}]}]}"#,
            "mask",
        ),
        (
            "in a region",
            r#"{"steps": [], "regions": {"p": {"kind": "grid", "kind": "path"}}}"#,
            "kind",
        ),
        (
            "in the table of regions",
            r#"{"steps": [], "regions": {"p": {"kind": "grid"}, "p": {"kind": "path"}}}"#,
            "p",
        ),
        (
            "in a list of parameters",
            r#"{"steps": [{"effect": "a", "params": {"x": [{"y": 1, "y": 1}]}}]}"#,
            "y",
        ),
    ] {
        let result = Recipe::from_json(text);
        assert!(
            matches!(&result, Err(Error::Recipe(message)) if message.contains(&format!("`{key}`"))),
            "{what}: {result:?}"
        );
    }
    // the same key in different tables is fine
    let steps = r#"{"steps": [{"effect": "a", "params": {"x": 1}}, {"effect": "b", "params": {"x": 2}}]}"#;
    assert!(Recipe::from_json(steps).is_ok());
}

#[test]
fn a_region_that_is_not_a_table_is_refused_naming_it() {
    for (what, regions) in [
        ("null", json!({"depth": null})),
        ("text", json!({"depth": "band"})),
        ("a list", json!({"depth": []})),
        ("one of two", json!({"fine": {"kind": "grid"}, "depth": 3})),
    ] {
        let result = read(&json!({"steps": [], "regions": regions}));
        assert!(
            matches!(&result, Err(Error::Recipe(message)) if message.contains("depth") && message.contains("table")),
            "{what}: {result:?}"
        );
    }
    let tables = read(&json!({"steps": [], "regions": {"depth": {"kind": "band"}, "empty": {"kind": "given"}}}));
    assert!(tables.is_ok(), "{tables:?}");
}

#[test]
fn all_lists_every_blend_mode_once_in_order() {
    // `BlendMode` is non-exhaustive outside the crate, so a new mode cannot stop this compiling: the seven are named
    assert_eq!(
        BlendMode::ALL,
        [
            BlendMode::Normal,
            BlendMode::Multiply,
            BlendMode::Screen,
            BlendMode::Overlay,
            BlendMode::SoftLight,
            BlendMode::Difference,
            BlendMode::Color,
        ]
    );
}

#[test]
fn the_blend_modes_are_the_librarys_seven_in_recipe_order() {
    let names: Vec<&str> = BlendMode::ALL.iter().map(|mode| mode.name()).collect();
    assert_eq!(
        names,
        [
            "normal",
            "multiply",
            "screen",
            "overlay",
            "soft_light",
            "difference",
            "color"
        ]
    );
    for mode in BlendMode::ALL {
        let read: Result<BlendMode, _> = serde_json::from_value(json!(mode.name()));
        assert_eq!(read.ok(), Some(*mode));
    }
}
