//! A render of rows of a canvas (`Options::with_window`) is the same rows of the render of the whole canvas: the oracle is
//! the whole render, and `Recipe::band_margin` says which recipes it holds for and how many rows each side a band needs.
//!
//! Every effect the query clears, with its defaults and each parameter alone at its limits, is rendered whole, then as
//! windows of the picture rows from the band's first row less the margin to its last plus the margin, and the rows of the
//! band compared with the whole (byte for byte; to `TOLERANCE` for an effect of `FFT_BLURS`, as in `banded.rs`).

use std::error::Error;
use std::sync::Arc;

use bullterrier_image::region::Bands;
use bullterrier_image::{
    Each, Entry, Frame, Image, Input, Options, Photo, Picture, Presets, Recipe, Step, effect, effects,
};
use serde_json::Value;

use crate::banded::{FFT_BLURS, TOLERANCE};
use crate::common::Outcome;
use crate::common::compare::first_difference_within;
use crate::common::pictures::{asset, varied};
use crate::common::sweep::{Setting, variants};

/// A tall, narrow picture of odd sides, as in `banded.rs`.
const HEIGHT: usize = 131;
const WIDTH: usize = 19;
/// The rows of a band: one, a prime, one that does not divide the height, half of it, and all of it.
const BANDS: [usize; 5] = [1, 13, 50, 66, HEIGHT];

fn photo(values: &[f32], rows: std::ops::Range<usize>) -> Result<Photo<'_>, Box<dyn Error>> {
    let picture = Picture::new(rows.len(), WIDTH, &values[rows.start * WIDTH * 3..rows.end * WIDTH * 3])?;
    Ok(Photo::new(picture).with_image("asset", Arc::new(asset())))
}

fn step_of(name: &str, setting: &Setting) -> Step {
    let mut step = Step::new(name);
    step.params.extend(setting.clone());
    step
}

/// The rows of the render of `recipe` on `rows` of `values` as the window `top..` of the whole canvas `HEIGHT` x `WIDTH`.
fn windowed(
    values: &[f32],
    recipe: &Recipe,
    rows: std::ops::Range<usize>,
    options: Options,
) -> Result<Image, Box<dyn Error>> {
    let options = options.with_window(
        u32::try_from(rows.start)?,
        u32::try_from(WIDTH)?,
        u32::try_from(HEIGHT)?,
    );
    Ok(photo(values, rows)?.render(recipe, &Presets::new(), options)?)
}

/// The first band of `rows` rows whose window render is not the whole render's rows, with how, or none.
fn first_problem(
    values: &[f32],
    recipe: &Recipe,
    margin: usize,
    within: f32,
    options: Options,
) -> Result<Option<String>, Box<dyn Error>> {
    let whole = photo(values, 0..HEIGHT)?
        .render(recipe, &Presets::new(), options)?
        .into_values();
    for rows in BANDS {
        for top in (0..HEIGHT).step_by(rows) {
            let (bottom, from) = ((top + rows).min(HEIGHT), top.saturating_sub(margin));
            let to = (bottom + margin).min(HEIGHT);
            let band = windowed(values, recipe, from..to, options)?.into_values();
            let (made, want) = (
                &band[(top - from) * WIDTH * 3..(bottom - from) * WIDTH * 3],
                &whole[top * WIDTH * 3..bottom * WIDTH * 3],
            );
            if let Some(at) = first_difference_within(made, want, within) {
                return Ok(Some(format!(
                    "band of {rows} rows from row {top} (window {from}..{to}): value {at} differs"
                )));
            }
        }
    }
    Ok(None)
}

#[test]
fn every_effect_the_query_clears_gives_the_rows_of_the_whole_render_from_a_window() -> Outcome {
    let values = varied(HEIGHT, WIDTH);
    let (mut problems, mut cleared) = (Vec::new(), 0);
    let longest = u32::try_from(HEIGHT)?;
    for name in effects() {
        let one = effect(&name)?;
        if one.inputs().contains(&Input::Partition) || !one.fields().is_empty() {
            continue; // the recipe needs a region to name (the query answers none for it; the check refuses it without one)
        }
        let table = one.params();
        for setting in variants(&table) {
            if table.check(&name, &setting).is_err() {
                continue; // a combination the effect refuses
            }
            let recipe = Recipe::new([step_of(&name, &setting)]);
            let Some(margin) = recipe.band_margin(&Presets::new(), longest)? else {
                continue;
            };
            cleared += 1;
            let within = if FFT_BLURS.contains(&name.as_str()) {
                TOLERANCE
            } else {
                0.0
            };
            if let Some(found) = first_problem(&values, &recipe, margin, within, Options::default())? {
                problems.push(format!("{name} {}: {found}", Value::Object(setting)));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "{} problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
    assert!(cleared > 500, "only {cleared} cases were cleared");
    Ok(())
}

fn grade() -> Step {
    Step::new("grade").param("exposure", 0.4)
}

#[test]
fn a_chain_of_steps_needs_the_margins_added_and_gives_the_rows_of_the_whole_render() -> Outcome {
    let values = varied(HEIGHT, WIDTH);
    let recipe = Recipe::new([
        Step::new("gaussian_blur").param("radius", 3.0),
        Step::new("vignette"),
        grade(),
        Step::new("grain").param("size", 2.0),
        Step::new("sharpen").param("radius", 2.0),
        Step::new("scanlines"),
    ]);
    let margin = recipe.band_margin(&Presets::new(), u32::try_from(HEIGHT)?)?;
    let margin = margin.ok_or("a chain of banded steps is banded")?;
    let single = |name: &str| Recipe::new([Step::new(name)]).band_margin(&Presets::new(), 131);
    assert!(
        margin > single("gaussian_blur")?.ok_or("blur")?,
        "the margins add: {margin}"
    );
    assert_eq!(
        first_problem(&values, &recipe, margin, TOLERANCE, Options::default())?,
        None
    );
    Ok(())
}

#[test]
fn a_window_and_the_runners_own_bands_compose() -> Outcome {
    let values = varied(HEIGHT, WIDTH);
    let recipe = Recipe::new([Step::new("vignette"), Step::new("grain"), Step::new("pixel_grid")]);
    let margin = recipe
        .band_margin(&Presets::new(), 131)?
        .ok_or("these steps are banded")?;
    let rows = std::num::NonZeroUsize::new(7).ok_or("rows")?;
    let options = Options::default().with_band(rows);
    assert_eq!(first_problem(&values, &recipe, margin, TOLERANCE, options)?, None);
    Ok(())
}

#[test]
fn a_recipe_that_a_window_cannot_make_has_no_band_margin() -> Outcome {
    let presets = Presets::new();
    let none = |recipe: &Recipe| recipe.band_margin(&presets, 131);
    // an effect that reads the whole picture, then a banded one after it
    assert_eq!(none(&Recipe::new([Step::new("auto_levels"), grade()]))?, None);
    let mut masked = grade();
    masked.mask = Some("subject".to_owned());
    assert_eq!(none(&Recipe::new([masked]))?, None);
    let each = Recipe::new([Entry::from(Each::new("cells", [Some(grade()), None]))]).with_region(
        "cells",
        Bands {
            of: "depth".to_owned(),
            count: 3,
        },
    );
    assert_eq!(none(&each)?, None);
    let mut sized = Recipe::new([grade()]);
    sized.size = Some(200);
    assert_eq!(none(&sized)?, None);
    let mut framed = Recipe::new([grade()]);
    framed.frame = Frame {
        angle: 17.0,
        ..Frame::default()
    };
    assert_eq!(none(&framed)?, None);
    assert_eq!(none(&Recipe::new([grade()]))?, Some(0));
    assert!(matches!(
        Recipe::new([grade()]).band_margin(&presets, 0),
        Err(bullterrier_image::Error::Recipe(why)) if why.contains("longest_side")
    ));
    Ok(())
}

#[test]
fn a_window_that_is_not_rows_of_the_canvas_is_refused_naming_the_window() -> Outcome {
    let values = varied(HEIGHT, WIDTH);
    let recipe = Recipe::new([grade()]);
    let photo = photo(&values, 0..10)?;
    let (width, height) = (u32::try_from(WIDTH)?, u32::try_from(HEIGHT)?);
    let mut sized = Recipe::new([grade()]);
    sized.size = Some(40);
    for (what, options, recipe) in [
        (
            "another width",
            Options::default().with_window(0, width + 1, height),
            &recipe,
        ),
        (
            "past the canvas",
            Options::default().with_window(height - 9, width, height),
            &recipe,
        ),
        (
            "a canvas of no rows",
            Options::default().with_window(0, width, 0),
            &recipe,
        ),
        (
            "a recipe size",
            Options::default().with_window(0, width, height),
            &sized,
        ),
        (
            "an options size",
            Options::default().with_size(40).with_window(0, width, height),
            &recipe,
        ),
    ] {
        match photo.render(recipe, &Presets::new(), options) {
            Err(bullterrier_image::Error::Recipe(why)) if why.contains("`window`") => {}
            other => return Err(format!("{what}: {other:?}").into()),
        }
    }
    // the same picture with the rows it is
    photo.render(
        &recipe,
        &Presets::new(),
        Options::default().with_window(height - 10, width, height),
    )?;
    Ok(())
}
