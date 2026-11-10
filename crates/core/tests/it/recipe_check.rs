//! The rules of a look, through the public API alone: `Recipe::checked` accepts the recipes that follow them and
//! refuses each that breaks one, naming the step and the trouble. The effects the recipes name are the core's own and
//! the doubles of `common::recipe` (`probe`, `bordered`, `fielded`) registered in the global registry.

use crate::common::Outcome;
use crate::common::recipe::{drive, regions, setup};
use bullterrier_image::{BlendMode, Each, Entry, Error, Presets, Recipe, Step};
use serde_json::{Value, json};

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

/// A `bordered` step (the effect that takes a partition and describes no parameters) drawing along `partition`.
fn bordered(partition: Option<&str>) -> Step {
    let mut step = Step::new("bordered");
    step.partition = partition.map(str::to_owned);
    step
}

/// `recipe` declaring the regions `panes` (a partition) and `cutout` (a selection).
fn with_regions(mut recipe: Recipe) -> Recipe {
    recipe.regions = regions(json!({"panes": {"kind": "grid"}, "cutout": {"kind": "path"}}));
    recipe
}

/// Presets of the probe: `wide` sets a cell and a count, `plain` only the count.
fn presets() -> Result<Presets, Error> {
    setup()?;
    let mut presets = Presets::new();
    let table = json!({
        "wide": {"description": "Wide cells.", "cell": 8, "cells": 7},
        "plain": {"description": "Few cells.", "cells": 2},
    });
    presets.add("probe", &table)?;
    Ok(presets)
}

/// The error `recipe` is refused with by the registry and `presets`; None if it is accepted.
fn refusal(recipe: &Recipe, presets: &Presets) -> Result<Option<Error>, Error> {
    setup()?;
    Ok(recipe.clone().checked(presets).err())
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
            with_regions(Recipe::new([bordered(Some("panes"))])),
        ),
        (
            "a built-in selection as a step's partition",
            Recipe::new([bordered(Some("subject"))]),
        ),
        (
            "an effect with no table keeps its parameters as given",
            Recipe::new([bordered(Some("subject")).param("anything", json!([1, "two"]))]),
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
fn recipes_that_follow_the_rules_are_accepted() -> Outcome {
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
            Recipe::new([bordered(None)]),
            "`partition`",
        ),
        (
            "a partition only for that effect",
            with_regions(Recipe::new([partition_on_probe])),
            "takes no partition",
        ),
        (
            "the partition is a region",
            Recipe::new([bordered(Some("nowhere"))]),
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
fn a_rule_broken_is_refused_naming_the_trouble() -> Outcome {
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
fn a_field_parameter_names_a_region_by_its_value_or_its_default_and_must_name_one() -> Outcome {
    setup()?;
    // `fielded` has one field parameter, `near`, whose default field is `NEAR`
    let recipe = |step: Step, declared: Value| {
        let mut recipe = Recipe::new([step]);
        recipe.regions = regions(declared);
        recipe
    };
    let declared = || json!({"NEAR": {"kind": "given"}, "star": {"kind": "given"}});
    // (the step, the regions declared, a word of the refusal; none: accepted)
    let rows = [
        (Step::new("fielded"), declared(), None),
        (Step::new("fielded").param("near", "star"), declared(), None),
        (Step::new("fielded").param("near", "depth"), json!({}), None),
        (Step::new("fielded"), json!({}), Some("no region")),
        (
            Step::new("fielded").param("near", "nowhere"),
            declared(),
            Some("'nowhere'"),
        ),
        (Step::new("fielded").param("near", 3), declared(), Some("names a field")),
    ];
    for (step, regions, said) in rows {
        let result = recipe(step, regions).checked(&Presets::new());
        match said {
            None => assert!(result.is_ok(), "{result:?}"),
            Some(said) => assert!(
                matches!(&result, Err(error @ Error::Recipe(_)) if error.to_string().contains(said)
                    && error.to_string().contains("step 1")),
                "{said}: {result:?}"
            ),
        }
    }
    Ok(())
}

#[test]
fn the_regions_a_refusal_lists_are_in_the_order_they_were_declared() -> Outcome {
    let mut recipe = Recipe::new([probe().mask("nowhere")]);
    recipe.regions = regions(json!({"zeta": {"kind": "given"}, "alpha": {"kind": "given"}}));
    let said = refusal(&recipe, &Presets::new())?.map(|error| error.to_string());
    assert!(
        said.as_deref()
            .is_some_and(|text| text.ends_with("valid selections: subject, background, depth, light, zeta, alpha")),
        "{said:?}"
    );
    Ok(())
}

#[test]
fn of_two_drives_that_break_a_rule_the_first_written_is_named() -> Outcome {
    let broken = |by: &str| json!({"by": by, "start": 4, "end": 9, "levels": 1});
    // `cell` is written before `angle`, against the alphabet
    let step = probe().param("cell", broken("depth")).param("angle", broken("light"));
    let said = refusal(&Recipe::new([step]), &Presets::new())?.map(|error| error.to_string());
    assert!(said.as_deref().is_some_and(|text| text.contains("`cell`")), "{said:?}");
    Ok(())
}

#[test]
fn no_parameter_is_named_like_a_key_of_the_step() -> Outcome {
    // `partition` is also an input of the effect, which the check names first
    for key in [
        "effect",
        "mask",
        "opacity",
        "blend_mode",
        "preset",
        "each",
        "looks",
        "name",
    ] {
        let refused = refusal(&Recipe::new([probe().param(key, json!("x"))]), &Presets::new())?;
        let said = refused.map(|error| error.to_string());
        assert!(
            said.as_deref()
                .is_some_and(|text| text.contains(&format!("`{key}` is a key of the step"))),
            "{key}: {said:?}"
        );
    }
    Ok(())
}

#[test]
fn the_refusal_of_a_later_step_names_its_place() -> Outcome {
    let recipe = Recipe::new([probe(), probe(), probe().mask("nowhere")]);
    let refused = refusal(&recipe, &Presets::new())?.map(|error| error.to_string());
    assert!(
        refused.as_deref().is_some_and(|text| text.contains("step 3")),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn an_unknown_effect_is_the_registrys_own_error() -> Outcome {
    let refused = refusal(&Recipe::new([Step::new("nope")]), &Presets::new())?;
    assert!(matches!(refused, Some(Error::UnknownEffect(name)) if name == "nope"));
    Ok(())
}

#[test]
fn a_parameter_out_of_range_is_the_tables_own_error() -> Outcome {
    let refused = refusal(&Recipe::new([probe().param("cell", 1)]), &Presets::new())?;
    assert!(
        matches!(&refused, Some(Error::Invalid { effect, field, value, .. }) if effect == "probe" && field == "cell" && **value == json!(1)),
        "{refused:?}"
    );
    Ok(())
}

#[test]
fn a_preset_lies_under_the_given_parameters() -> Outcome {
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
    let mut step = bordered(Some("subject"));
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
fn a_parameter_refused_in_a_look_names_the_step_and_the_look() -> Outcome {
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

#[test]
fn a_built_in_selection_is_known_by_its_exact_name_and_nothing_else_is_one() -> Outcome {
    for name in ["subject", "background", "depth", "light"] {
        let refused = refusal(&Recipe::new([probe().mask(name)]), &Presets::new())?;
        assert!(refused.is_none(), "{name}: {refused:?}");
    }
    for name in ["", "Subject", "subjects", "star"] {
        let refused = refusal(&Recipe::new([probe().mask(name)]), &Presets::new())?;
        assert!(
            matches!(&refused, Some(Error::Recipe(message)) if message.contains("valid selections: subject, background, depth, light")),
            "{name:?}: {refused:?}"
        );
    }
    Ok(())
}
