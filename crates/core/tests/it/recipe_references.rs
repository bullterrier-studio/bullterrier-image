//! The names a recipe refers to, through the public API alone: `Recipe::checked` holds each name in each place (a
//! mask, a drive's `by`, a field parameter, a partition, an `each`, a region's `fit`, `of`, `per`) to be a region of the
//! form the place needs, suggests the closest name, and refuses a circle of regions. The effects the recipes name are
//! the core's own and the doubles of `common::recipe` (`probe`, `bordered`, `fielded`).

use crate::common::Outcome;
use crate::common::recipe::{regions, setup};
use bullterrier_image::region::{Band, Bands, Combine, Grid, Path, Pick, Subject};
use bullterrier_image::{Each, Error, Presets, Recipe, Region, Regions, Step};
use serde_json::json;

/// The message the refusal of `recipe` says (`fielded` is an effect with one field parameter, `near`), or None if it
/// is accepted.
fn refusal(recipe: &Recipe) -> Result<Option<String>, Error> {
    setup()?;
    Ok(recipe
        .clone()
        .checked(&Presets::new())
        .err()
        .map(|error| error.to_string()))
}

/// The regions every recipe of the matrix declares: a partition and a selection.
fn declared() -> Regions {
    regions(json!({"panes": {"kind": "grid"}, "cutout": {"kind": "path"}}))
}

/// The recipe whose `place` refers to `name`, with the regions of [`declared`]; the places of a region are those of
/// `r`, a region of the kind that has them.
fn naming(place: &str, name: &str) -> Recipe {
    let mut recipe = match place {
        "mask" => Recipe::new([Step::new("probe").mask(name)]),
        "each mask" => {
            let mut each = Each::new("panes", [None]);
            each.mask = Some(name.to_owned());
            Recipe::new([each])
        }
        "look mask" => Recipe::new([Each::new("panes", [Some(Step::new("probe").mask(name))])]),
        "by" => Recipe::new([Step::new("probe").param("cell", json!({"by": name, "start": 4, "end": 9}))]),
        "field" => Recipe::new([Step::new("fielded").param("near", name)]),
        "partition" => {
            let mut bordered = Step::new("bordered");
            bordered.partition = Some(name.to_owned());
            Recipe::new([bordered])
        }
        "each" => Recipe::new([Each::new(name, [None])]),
        _ => Recipe::new([Step::new("probe")]),
    };
    recipe.regions = declared();
    let made: Option<Region> = match place {
        "fit" => Some(
            Grid {
                fit: Some(name.to_owned()),
                ..Grid::default()
            }
            .into(),
        ),
        "band of" => Some(
            Band {
                of: name.to_owned(),
                ..Band::default()
            }
            .into(),
        ),
        "bands of" => Some(
            Bands {
                of: name.to_owned(),
                ..Bands::default()
            }
            .into(),
        ),
        "combine of" => Some(
            Combine {
                of: vec!["subject".to_owned(), name.to_owned()],
                ..Combine::default()
            }
            .into(),
        ),
        "pick of" => Some(
            Pick {
                of: name.to_owned(),
                ..Pick::default()
            }
            .into(),
        ),
        "subject per" => Some(
            Subject {
                per: Some(name.to_owned()),
                ..Subject::default()
            }
            .into(),
        ),
        _ => None,
    };
    if let Some(region) = made {
        recipe.regions.insert("r", region);
    }
    recipe
}

/// Every place a name can stand, with (the key the message names, where the message says it is, what the name must
/// be, the names that would do in the order they are listed) and whether the name is accepted when it is, in this
/// order, `cutout` (a selection), `panes` (a partition), `subject` (a built-in selection) and `nowhere`. The regions
/// are `panes` and `cutout` and, for a place of a region, `r`, which is a selection unless it is a grid or bands.
type Place = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    [bool; 4],
);
const PLACES: [Place; 13] = [
    (
        "mask",
        "mask",
        "step 1",
        "selection",
        "subject, background, depth, light, cutout",
        [true, false, true, false],
    ),
    (
        "each mask",
        "mask",
        "step 1",
        "selection",
        "subject, background, depth, light, cutout",
        [true, false, true, false],
    ),
    (
        "look mask",
        "mask",
        "step 1, look 1",
        "selection",
        "subject, background, depth, light, cutout",
        [true, false, true, false],
    ),
    (
        "by",
        "by",
        "step 1",
        "selection",
        "subject, background, depth, light, cutout",
        [true, false, true, false],
    ),
    (
        "field",
        "near",
        "step 1",
        "selection",
        "subject, background, depth, light, cutout",
        [true, false, true, false],
    ),
    (
        "partition",
        "partition",
        "step 1",
        "region",
        "subject, background, depth, light, panes, cutout",
        [true, true, true, false],
    ),
    (
        "each",
        "each",
        "step 1",
        "partition",
        "panes",
        [false, true, false, false],
    ),
    (
        "fit",
        "fit",
        "region 'r'",
        "selection",
        "subject, background, depth, light, cutout",
        [true, false, true, false],
    ),
    (
        "band of",
        "of",
        "region 'r'",
        "selection",
        "subject, background, depth, light, cutout, r",
        [true, false, true, false],
    ),
    (
        "bands of",
        "of",
        "region 'r'",
        "selection",
        "subject, background, depth, light, cutout",
        [true, false, true, false],
    ),
    (
        "combine of",
        "of",
        "region 'r'",
        "selection",
        "subject, background, depth, light, cutout, r",
        [true, false, true, false],
    ),
    (
        "pick of",
        "of",
        "region 'r'",
        "partition",
        "panes",
        [false, true, false, false],
    ),
    (
        "subject per",
        "per",
        "region 'r'",
        "partition",
        "panes",
        [false, true, false, false],
    ),
];

#[test]
fn a_name_must_be_a_region_of_the_form_the_place_needs() -> Outcome {
    let names = ["cutout", "panes", "subject", "nowhere"];
    let forms = ["a selection", "a partition", "a selection", ""];
    for (place, key, said_at, wanted, valid, accepted) in PLACES {
        for ((name, form), ok) in names.into_iter().zip(forms).zip(accepted) {
            let result = refusal(&naming(place, name))?;
            if ok {
                assert_eq!(result, None, "{place} naming {name}");
                continue;
            }
            let problem = if form.is_empty() {
                "which is no region".to_owned()
            } else {
                format!("{form}, but it needs a {wanted}")
            };
            let expected = [
                format!("{said_at}: `{key}` names '{name}', {problem}"),
                format!("valid {wanted}s: {valid}"),
            ];
            assert!(
                result
                    .as_deref()
                    .is_some_and(|message| expected.iter().all(|part| message.contains(part))),
                "{place} naming {name}: wanted {expected:?}, got {result:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn a_name_that_exists_nowhere_gets_the_closest_name_that_would_do_when_one_is_close() -> Outcome {
    let regions = regions(json!({
        "panes": {"kind": "grid"}, "cells": {"kind": "grid"}, "aab": {"kind": "path"}, "aac": {"kind": "path"},
    }));
    let each = |name: &str| {
        let mut recipe = Recipe::new([Each::new(name, [None])]);
        recipe.regions = regions.clone();
        recipe
    };
    let mask = |name: &str| {
        let mut recipe = Recipe::new([Step::new("probe").mask(name)]);
        recipe.regions = regions.clone();
        recipe
    };
    // (the recipe, the name it can be told to mean; none: no suggestion)
    let rows = [
        ("a letter missing", each("pane"), Some("panes")),
        ("a letter too many", each("panesz"), Some("panes")),
        ("a letter changed", each("panez"), Some("panes")),
        ("one letter missing", each("pnes"), Some("panes")),
        ("two slips", each("pnez"), Some("panes")),
        ("three slips of a short name", each("pn"), None),
        ("three slips", each("paneszzz"), None),
        ("capitals do not matter", each("PANES"), Some("panes")),
        ("nothing is close", each("zzzzzzz"), None),
        ("the first declared of two equally close", mask("aaa"), Some("aab")),
        ("a built-in", mask("subjet"), Some("subject")),
        ("a one-letter name is close to nothing", mask("a"), None),
        ("an empty name is close to nothing", mask(""), None),
        (
            "the other form is not offered: a partition for a mask",
            mask("pane"),
            None,
        ),
    ];
    for (what, recipe, close) in rows {
        let message = refusal(&recipe)?.unwrap_or_default();
        match close {
            Some(close) => assert!(
                message.contains(&format!("(did you mean '{close}'?)")),
                "{what}: {message}"
            ),
            None => assert!(!message.contains("did you mean"), "{what}: {message}"),
        }
    }
    // a region that exists but has the other form is told so, not told to be another name
    let message = refusal(&mask("panes"))?.unwrap_or_default();
    assert!(!message.contains("did you mean"), "{message}");
    Ok(())
}

#[test]
fn a_recipe_with_no_regions_says_so_in_the_refusal() -> Outcome {
    let message = refusal(&Recipe::new([Step::new("probe").mask("star")]))?.unwrap_or_default();
    assert!(message.contains("declares no regions of its own"), "{message}");
    let message = refusal(&naming("mask", "star"))?.unwrap_or_default();
    assert!(!message.contains("declares no regions"), "{message}");
    Ok(())
}

#[test]
fn a_region_cannot_be_called_like_a_built_in_one() -> Outcome {
    for name in ["subject", "depth", "background", "light"] {
        let recipe = Recipe::new([Step::new("probe")]).with_region(name, Grid::default());
        let message = refusal(&recipe)?.unwrap_or_default();
        assert!(
            message.contains(&format!("region '{name}' is a built-in region"))
                && message.contains("subject, background"),
            "{name}: {message}"
        );
    }
    let fine = Recipe::new([Step::new("probe")]).with_region("subjects", Grid::default());
    assert_eq!(refusal(&fine)?, None);
    Ok(())
}

#[test]
fn a_region_that_refers_to_a_missing_or_wrong_region_is_refused_even_if_no_step_uses_it() -> Outcome {
    let unused = |region: Region| Recipe::new([Step::new("probe")]).with_region("r", region);
    let band = |of: &str| Band {
        of: of.to_owned(),
        ..Band::default()
    };
    assert_eq!(refusal(&unused(band("depth").into()))?, None, "a good one is accepted");
    for (what, recipe, said) in [
        (
            "a band of nothing",
            unused(band("nowhere").into()),
            "region 'r': `of` names 'nowhere'",
        ),
        (
            "a band of a partition",
            unused(band("r2").into()),
            "region 'r': `of` names 'r2'",
        ),
    ] {
        let recipe = recipe.with_region("r2", Grid::default());
        let message = refusal(&recipe)?;
        assert!(
            message.as_deref().is_some_and(|message| message.contains(said)),
            "{what}: {message:?}"
        );
    }
    Ok(())
}

#[test]
fn regions_made_from_each_other_in_a_circle_are_refused_naming_the_circle() -> Outcome {
    let band = |of: &str| {
        Region::from(Band {
            of: of.to_owned(),
            ..Band::default()
        })
    };
    let path = |fit: &str| {
        Region::from(Path {
            fit: Some(fit.to_owned()),
            ..Path::default()
        })
    };
    let mut by_per = Recipe::new([Step::new("probe")]);
    by_per.regions.insert(
        "s",
        Subject {
            per: Some("g".to_owned()),
            ..Subject::default()
        },
    );
    by_per.regions.insert(
        "g",
        Grid {
            fit: Some("s".to_owned()),
            ..Grid::default()
        },
    );
    let declare = |regions: &[(&str, Region)]| {
        let mut recipe = Recipe::new([Step::new("probe")]);
        for (name, region) in regions {
            recipe.regions.insert(*name, region.clone());
        }
        recipe
    };
    for (what, recipe, circle) in [
        ("a region of itself", declare(&[("a", band("a"))]), "'a' -> 'a'"),
        (
            "two",
            declare(&[("a", band("b")), ("b", band("a"))]),
            "'a' -> 'b' -> 'a'",
        ),
        (
            "three, not starting at the first",
            declare(&[
                ("x", band("depth")),
                ("a", band("b")),
                ("b", band("c")),
                ("c", band("a")),
            ]),
            "'a' -> 'b' -> 'c' -> 'a'",
        ),
        (
            "reached through a lead-in that is not part of it",
            declare(&[("lead", band("a")), ("a", band("b")), ("b", band("a"))]),
            "'a' -> 'b' -> 'a'",
        ),
        (
            "by `fit`",
            declare(&[("p", path("q")), ("q", path("p"))]),
            "'p' -> 'q' -> 'p'",
        ),
        ("by `per` and `fit`", by_per, "'s' -> 'g' -> 's'"),
    ] {
        let message = refusal(&recipe)?;
        assert!(
            message
                .as_deref()
                .is_some_and(|message| message.ends_with(&format!("in a cycle: {circle}"))),
            "{what}: wanted {circle}, got {message:?}"
        );
    }
    Ok(())
}

#[test]
fn a_very_long_chain_and_a_very_wide_diamond_are_walked_without_recursion_and_once_each() -> Outcome {
    let mut chain = Recipe::new([Step::new("probe").mask("b0")]);
    for link in 0..5000 {
        let of = if link == 4999 {
            "depth".to_owned()
        } else {
            format!("b{}", link + 1)
        };
        chain.regions.insert(format!("b{link}"), Band { of, ..Band::default() });
    }
    assert_eq!(refusal(&chain)?, None);
    // 40 levels, each made from the next level twice: 2^40 paths, 41 regions
    let mut diamond = Recipe::new([Step::new("probe").mask("d0")]);
    for level in 0..40 {
        let next = format!("d{}", level + 1);
        diamond.regions.insert(
            format!("d{level}"),
            Combine {
                of: vec![next.clone(), next],
                ..Combine::default()
            },
        );
    }
    diamond.regions.insert(
        "d40",
        Band {
            of: "depth".to_owned(),
            ..Band::default()
        },
    );
    assert_eq!(refusal(&diamond)?, None);
    // and a circle at the end of the chain is still found
    chain.regions.insert(
        "b4999",
        Band {
            of: "b0".to_owned(),
            ..Band::default()
        },
    );
    let message = refusal(&chain)?.unwrap_or_default();
    assert!(
        message.contains("cycle") && message.contains("'b0' -> 'b1'"),
        "{}",
        &message[..message.len().min(200)]
    );
    Ok(())
}

#[test]
fn a_field_a_preset_sets_is_a_use_of_that_region() -> Outcome {
    setup()?;
    let mut presets = Presets::new();
    presets.add(
        "fielded",
        &json!({"onto_cutout": {"description": "Cutout.", "near": "cutout"},
                "onto_nowhere": {"description": "Nowhere.", "near": "nowhere"}}),
    )?;
    let preset = |name: &str| {
        let mut step = Step::new("fielded");
        step.preset = Some(name.to_owned());
        let mut recipe = Recipe::new([step]);
        recipe.regions = declared();
        recipe
    };
    assert!(preset("onto_cutout").checked(&presets).is_ok());
    let message = preset("onto_nowhere")
        .checked(&presets)
        .err()
        .map(|error| error.to_string());
    assert!(
        message
            .as_deref()
            .is_some_and(|message| message.contains("`near` names 'nowhere'")),
        "{message:?}"
    );
    Ok(())
}
