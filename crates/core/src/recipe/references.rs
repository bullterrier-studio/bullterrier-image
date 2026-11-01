//! What the names in a recipe refer to: that a `mask`, a field, a `partition`, an `each` or a region's `of`, `per` or
//! `fit` names a region that exists and has the form the place needs, and that regions are not made from each other in
//! a circle.

use std::collections::HashMap;

use super::Regions;
use super::built_in::BuiltIn;
use crate::Error;

/// What a region is, which decides where it can be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Form {
    /// Pixels chosen to a degree from 0 to 1: a `mask`, a field, what a band is cut from.
    Selection,
    /// Numbered pieces: what an `each` step gives looks to, a `Pick` picks from, a `Subject` is found in.
    Partition,
}

impl Form {
    /// The word for it in a message.
    fn name(self) -> &'static str {
        match self {
            Self::Selection => "selection",
            Self::Partition => "partition",
        }
    }
}

/// What the region `name` is: a built-in one is a selection, a declared one is by its kind. None if there is none.
fn form_of(regions: &Regions, name: &str) -> Option<Form> {
    if BuiltIn::named(name).is_some() {
        return Some(Form::Selection);
    }
    regions.get(name).map(super::Region::form)
}

/// `Ok` if `name` (what `key` of `place`, as `step 2` or `region 'x'`, refers to) is a region of the `wanted` form, or
/// any region when `wanted` is none.
///
/// # Errors
/// [`Error::Recipe`] naming the place, the key and the name, saying whether there is no such region (with the closest
/// name, if one is close) or it has the other form, and listing the names that would do.
pub(super) fn refer(regions: &Regions, place: &str, key: &str, name: &str, wanted: Option<Form>) -> Result<(), Error> {
    let have = form_of(regions, name);
    if have.is_some() && (wanted.is_none() || wanted == have) {
        return Ok(());
    }
    let valid: Vec<&str> = BuiltIn::names()
        .into_iter()
        .chain(regions.iter().map(|(declared, _)| declared))
        .filter(|candidate| wanted.is_none() || form_of(regions, candidate) == wanted)
        .collect();
    let problem = match (have, wanted) {
        (Some(have), Some(wanted)) => format!("a {}, but it needs a {}", have.name(), wanted.name()),
        _ => match closest(name, &valid) {
            Some(close) => format!("which is no region (did you mean '{close}'?)"),
            None => "which is no region".to_owned(),
        },
    };
    let kind = wanted.map_or("region", Form::name);
    let hint = if regions.is_empty() {
        " (this recipe declares no regions of its own: add them with `Recipe::with_region` or a [region.NAME] table)"
    } else {
        ""
    };
    Err(Error::Recipe(format!(
        "{place}: `{key}` names '{name}', {problem}; valid {kind}s: {}{hint}",
        if valid.is_empty() {
            "none".to_owned()
        } else {
            valid.join(", ")
        }
    )))
}

/// `Ok` if the declared regions are consistent: none is called like a built-in one, each refers only to regions of the
/// form its field needs, and no region is made from itself, however indirectly.
///
/// # Errors
/// [`Error::Recipe`] naming the region (and the field and name it refers to by, as [`refer`] does), or the regions of the
/// cycle.
pub(super) fn check_regions(regions: &Regions) -> Result<(), Error> {
    for (name, region) in regions.iter() {
        if BuiltIn::named(name).is_some() {
            return Err(Error::Recipe(format!(
                "region '{name}' is a built-in region ({}) and cannot be declared",
                BuiltIn::names().join(", ")
            )));
        }
        for (key, target, form) in region.refers_to() {
            refer(regions, &format!("region '{name}'"), key, target, Some(form))?;
        }
    }
    no_cycle(regions)
}

/// `Ok` unless declared regions refer to each other in a cycle. One walk that visits every region once and keeps no
/// call stack, so a chain of thousands of regions, or of paths that all lead to one region, is no trouble.
fn no_cycle(regions: &Regions) -> Result<(), Error> {
    let names: Vec<&str> = regions.iter().map(|(name, _)| name).collect();
    let made_from: Vec<Vec<&str>> = names.iter().map(|name| regions.made_from(name)).collect();
    let index: HashMap<&str, usize> = names.iter().enumerate().map(|(at, name)| (*name, at)).collect();
    let mut done = vec![false; names.len()];
    // where each region on the current path stands in it, so that a cycle is the path from there
    let mut on_path: Vec<Option<usize>> = vec![None; names.len()];
    for start in 0..names.len() {
        if done[start] {
            continue;
        }
        let mut path = vec![start];
        let mut walks = vec![made_from[start].iter()];
        on_path[start] = Some(0);
        while let Some(walk) = walks.last_mut() {
            let Some(target) = walk.next() else {
                if let Some(finished) = path.pop() {
                    (done[finished], on_path[finished]) = (true, None);
                }
                walks.pop();
                continue;
            };
            // a built-in or an unknown name is no region of ours: typing says what is wrong with an unknown one
            let Some(&next) = index.get(target) else { continue };
            if let Some(from) = on_path[next] {
                let cycle: Vec<String> = path[from..]
                    .iter()
                    .chain([&next])
                    .map(|region| format!("'{}'", names[*region]))
                    .collect();
                return Err(Error::Recipe(format!(
                    "regions refer to each other in a cycle: {}",
                    cycle.join(" -> ")
                )));
            }
            if !done[next] {
                on_path[next] = Some(path.len());
                path.push(next);
                walks.push(made_from[next].iter());
            }
        }
    }
    Ok(())
}

/// The candidate that `name` is most likely a slip of the finger for: at most two letters away, ignoring case, and
/// fewer than `name` is long (a one-letter name is never close to another). The first of equally close ones.
fn closest<'a>(name: &str, candidates: &[&'a str]) -> Option<&'a str> {
    const MOST_SLIPS: usize = 2; // constant: two edits are a slip of the finger, three are another word
    let typed = lowered(name);
    let mut best: Option<(usize, &'a str)> = None;
    for candidate in candidates {
        let apart = distance(&typed, &lowered(candidate));
        if apart <= MOST_SLIPS && apart < typed.len() && best.is_none_or(|(nearest, _)| apart < nearest) {
            best = Some((apart, candidate));
        }
    }
    best.map(|(_, candidate)| candidate)
}

/// `text` in lower case, as letters.
fn lowered(text: &str) -> Vec<char> {
    text.chars().flat_map(char::to_lowercase).collect()
}

/// The Levenshtein distance between two texts: the fewest letters to insert, delete or change to turn one into the
/// other.
fn distance(one: &[char], other: &[char]) -> usize {
    let mut row: Vec<usize> = (0..=other.len()).collect();
    for (at, letter) in one.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = at + 1;
        for (to, wanted) in other.iter().enumerate() {
            let above = row[to + 1];
            row[to + 1] = (diagonal + usize::from(letter != wanted))
                .min(above + 1)
                .min(row[to] + 1);
            diagonal = above;
        }
    }
    row[other.len()]
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::region::{Band, Bands, Combine, Grid, Path, Pick, Subject};
    use crate::testing::{Recorder, regions, registry};
    use crate::{Each, Presets, Recipe, Region, Step};

    /// The message the refusal of `recipe` says (checked against a registry that has `fielded`, an effect with one field
    /// parameter, `near`, beside the test ones), or None if it is accepted.
    fn refusal(recipe: &Recipe) -> Result<Option<String>, Error> {
        let registry = registry()?;
        registry.register(Arc::new(Recorder::new("fielded", &[], &["near"])))?;
        Ok(recipe
            .check_in(&registry, &Presets::new())
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
                let mut seams = Step::new("seams");
                seams.partition = Some(name.to_owned());
                Recipe::new([seams])
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
    fn a_name_must_be_a_region_of_the_form_the_place_needs() -> Result<(), Error> {
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
    fn a_name_that_exists_nowhere_gets_the_closest_name_that_would_do_when_one_is_close() -> Result<(), Error> {
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
    fn a_recipe_with_no_regions_says_so_in_the_refusal() -> Result<(), Error> {
        let message = refusal(&Recipe::new([Step::new("probe").mask("star")]))?.unwrap_or_default();
        assert!(message.contains("declares no regions of its own"), "{message}");
        let message = refusal(&naming("mask", "star"))?.unwrap_or_default();
        assert!(!message.contains("declares no regions"), "{message}");
        Ok(())
    }

    #[test]
    fn a_region_cannot_be_called_like_a_built_in_one() -> Result<(), Error> {
        for name in BuiltIn::names() {
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
    fn a_region_that_refers_to_a_missing_or_wrong_region_is_refused_even_if_no_step_uses_it() -> Result<(), Error> {
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
    fn regions_made_from_each_other_in_a_circle_are_refused_naming_the_circle() -> Result<(), Error> {
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
    fn a_very_long_chain_and_a_very_wide_diamond_are_walked_without_recursion_and_once_each() -> Result<(), Error> {
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
    fn a_field_a_preset_sets_is_a_use_of_that_region() -> Result<(), Error> {
        let registry = registry()?;
        registry.register(Arc::new(Recorder::new("fielded", &[], &["near"])))?;
        let mut presets = Presets::new();
        presets.add_in(
            &registry,
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
        assert!(preset("onto_cutout").check_in(&registry, &presets).is_ok());
        let message = preset("onto_nowhere")
            .check_in(&registry, &presets)
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
}
