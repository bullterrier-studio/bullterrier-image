//! A render of rows of a canvas (`Options::with_window`) is the same rows of the render of the whole canvas: the oracle is
//! the whole render, and `Recipe::band_margin` says which recipes it holds for and how many rows each side a band needs.
//!
//! Every effect the query clears, with its defaults and each parameter alone at its limits, is rendered whole, then as
//! windows of the picture rows from the band's first row less the margin to its last plus the margin, and the rows of the
//! band compared with the whole, byte for byte: a recipe with a blur summed by a Fourier transform is not cleared.

use std::error::Error;
use std::sync::Arc;

use bullterrier_image::region::Bands;
use bullterrier_image::{Each, Entry, Frame, Image, Input, Options, Photo, Picture, Presets, Recipe, Step, effect};
use serde_json::Value;

use crate::common::compare::{first_bit_difference, first_difference_within};
use crate::common::pictures::{asset, varied};
use crate::common::sweep::{Setting, awkward, fine, variants};
use crate::common::{Outcome, core_effects};

/// A canvas: its sides, and the rows of the bands it is cut into: one, a prime, one that does not divide the height, half
/// of it, and all of it.
#[derive(Clone, Copy)]
struct Canvas {
    height: usize,
    width: usize,
    bands: &'static [usize],
}

/// A tall, narrow picture of odd sides, as in `banded.rs`.
const SMALL: Canvas = Canvas {
    height: 131,
    width: 19,
    bands: &[1, 13, 50, 66, 131],
};

/// A taller one, where a row is far from the top: a window that starts low is told its rows by their place on the canvas, and
/// a fraction of a pixel worked out from the row inside the window rounds otherwise than from the row on the canvas.
const TALL: Canvas = Canvas {
    height: 481,
    width: 8,
    bands: &[7, 241, 481],
};

/// A very tall one, where a row is far enough from the top for a position that differs from it by 1e-9 to round by the row.
const FINE: Canvas = Canvas {
    height: 30_001,
    width: 2,
    bands: &[997],
};

fn photo(canvas: Canvas, values: &[f32], rows: std::ops::Range<usize>) -> Result<Photo<'_>, Box<dyn Error>> {
    let width = canvas.width;
    let picture = Picture::new(rows.len(), width, &values[rows.start * width * 3..rows.end * width * 3])?;
    Ok(Photo::new(picture).with_image("asset", Arc::new(asset())))
}

fn step_of(name: &str, setting: &Setting) -> Step {
    let mut step = Step::new(name);
    step.params.extend(setting.clone());
    step
}

/// The rows of the render of `recipe` on `rows` of `values` as the window `top..` of the whole `canvas`.
fn windowed(
    canvas: Canvas,
    values: &[f32],
    recipe: &Recipe,
    rows: std::ops::Range<usize>,
    options: Options,
) -> Result<Image, Box<dyn Error>> {
    let options = options.with_window(
        u32::try_from(rows.start)?,
        u32::try_from(canvas.width)?,
        u32::try_from(canvas.height)?,
    );
    Ok(photo(canvas, values, rows)?.render(recipe, &Presets::new(), options)?)
}

/// The first band of `rows` rows whose window render is not the whole render's rows, with how, or none.
fn first_problem(
    canvas: Canvas,
    values: &[f32],
    recipe: &Recipe,
    margin: usize,
    within: f32,
    options: Options,
) -> Result<Option<String>, Box<dyn Error>> {
    let Canvas { height, width, bands } = canvas;
    let whole = photo(canvas, values, 0..height)?
        .render(recipe, &Presets::new(), options)?
        .into_values();
    for &rows in bands {
        for top in (0..height).step_by(rows) {
            let (bottom, from) = ((top + rows).min(height), top.saturating_sub(margin));
            let to = (bottom + margin).min(height);
            let band = windowed(canvas, values, recipe, from..to, options)?.into_values();
            let (made, want) = (
                &band[(top - from) * width * 3..(bottom - from) * width * 3],
                &whole[top * width * 3..bottom * width * 3],
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
    every_cleared_effect_over(SMALL, &[variants], 200)
}

/// The same on a tall canvas, at the defaults and limits and at settings that move content by a fraction of a pixel along no
/// axis (the limits of a range are whole numbers and the quarter turns, which hide a rounding that depends on the row a
/// window starts at).
#[test]
fn every_effect_the_query_clears_gives_the_rows_of_the_whole_render_on_a_tall_canvas_at_awkward_settings() -> Outcome {
    every_cleared_effect_over(TALL, &[variants, awkward], 200)
}

/// The same on a canvas of 30,001 rows, at settings that move content by a distance too fine for a row to carry.
#[test]
fn every_effect_the_query_clears_gives_the_rows_of_the_whole_render_at_distances_too_fine_for_a_row() -> Outcome {
    every_cleared_effect_over(FINE, &[fine], 20)
}

fn every_cleared_effect_over(
    canvas: Canvas,
    settings: &[fn(&bullterrier_image::params::Table) -> Vec<Setting>],
    at_least: usize,
) -> Outcome {
    let values = varied(canvas.height, canvas.width);
    let (mut problems, mut cleared) = (Vec::new(), 0);
    let longest = u32::try_from(canvas.height)?;
    for name in core_effects()? {
        let one = effect(&name)?;
        if one.inputs().contains(&Input::Partition) || !one.fields().is_empty() {
            continue; // the recipe needs a region to name (the query answers none for it; the check refuses it without one)
        }
        let table = one.params();
        for setting in settings.iter().flat_map(|settings| settings(&table)) {
            if table.check(&name, &setting).is_err() {
                continue; // a combination the effect refuses
            }
            let recipe = Recipe::new([step_of(&name, &setting)]);
            let Some(margin) = recipe.band_margin(&Presets::new(), longest)? else {
                continue;
            };
            cleared += 1;
            if let Some(found) = first_problem(canvas, &values, &recipe, margin, 0.0, Options::default())? {
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
    assert!(cleared > at_least, "only {cleared} cases were cleared");
    Ok(())
}

fn grade() -> Step {
    Step::new("grade").param("exposure", 0.4)
}

#[test]
fn a_chain_of_steps_needs_the_margins_added_and_gives_the_rows_of_the_whole_render() -> Outcome {
    let values = varied(SMALL.height, SMALL.width);
    let recipe = Recipe::new([
        Step::new("gaussian_blur").param("radius", 3.0),
        Step::new("vignette"),
        grade(),
        Step::new("grain").param("size", 2.0),
        Step::new("sharpen").param("radius", 2.0),
        Step::new("scanlines"),
    ]);
    let margin = recipe.band_margin(&Presets::new(), u32::try_from(SMALL.height)?)?;
    let margin = margin.ok_or("a chain of banded steps is banded")?;
    let single = |name: &str| Recipe::new([Step::new(name)]).band_margin(&Presets::new(), 131);
    assert!(
        margin > single("gaussian_blur")?.ok_or("blur")?,
        "the margins add: {margin}"
    );
    assert_eq!(
        first_problem(SMALL, &values, &recipe, margin, 0.0, Options::default())?,
        None
    );
    Ok(())
}

#[test]
fn a_window_and_the_runners_own_bands_compose() -> Outcome {
    let values = varied(SMALL.height, SMALL.width);
    let recipe = Recipe::new([Step::new("vignette"), Step::new("grain"), Step::new("pixel_grid")]);
    let margin = recipe
        .band_margin(&Presets::new(), 131)?
        .ok_or("these steps are banded")?;
    let rows = std::num::NonZeroUsize::new(7).ok_or("rows")?;
    let options = Options::default().with_band(rows);
    assert_eq!(first_problem(SMALL, &values, &recipe, margin, 0.0, options)?, None);
    Ok(())
}

#[test]
fn a_canvas_fixes_the_scale_of_a_window_and_the_window_the_place() -> Outcome {
    let values = varied(SMALL.height, SMALL.width);
    // blur reaches by px, the vignette by place: the scale from a canvas of 600 on the long side, not the window's 131
    let recipe = Recipe::new([Step::new("gaussian_blur").param("radius", 3.0), Step::new("vignette")]);
    let margin = recipe.band_margin(&Presets::new(), 600)?.ok_or("banded")?;
    let scaled = Options::default().with_canvas(300, 600);
    assert_eq!(first_problem(SMALL, &values, &recipe, margin, 0.0, scaled)?, None);
    // the canvas is used: the bands without it are not that whole render
    let whole = photo(SMALL, &values, 0..SMALL.height)?
        .render(&recipe, &Presets::new(), scaled)?
        .into_values();
    let rows = 40..100;
    let band = windowed(SMALL, &values, &recipe, rows.clone(), Options::default())?.into_values();
    let want = &whole[rows.start * SMALL.width * 3..rows.end * SMALL.width * 3];
    assert_eq!(band.len(), want.len());
    assert!(
        first_bit_difference(&band, want).is_some(),
        "a window alone follows its own canvas, not the scale asked for"
    );
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
    let values = varied(SMALL.height, SMALL.width);
    let recipe = Recipe::new([grade()]);
    let photo = photo(SMALL, &values, 0..10)?;
    let (width, height) = (u32::try_from(SMALL.width)?, u32::try_from(SMALL.height)?);
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

/// The canvas `height` rows by `THIN` columns: a thin picture whose long side is its height, so a blur's reach passes the
/// switch to the Fourier transform at a height that is cheap to render.
const THIN: usize = 64;

/// The first problem in the render of `recipe` on a thin canvas of `height` rows: a band whose rows differ from the whole
/// render's, byte for byte, or an answer of the query other than `cleared`.
fn thin_problem(recipe: &Recipe, height: usize, cleared: bool) -> Result<Option<String>, Box<dyn Error>> {
    let values = varied(height, THIN);
    let (width, canvas) = (u32::try_from(THIN)?, u32::try_from(height)?);
    let render = |rows: std::ops::Range<usize>, window: bool| -> Result<Vec<f32>, Box<dyn Error>> {
        let picture = Picture::new(rows.len(), THIN, &values[rows.start * THIN * 3..rows.end * THIN * 3])?;
        let mut options = Options::default();
        if window {
            options = options.with_window(u32::try_from(rows.start)?, width, canvas);
        }
        Ok(Photo::new(picture)
            .render(recipe, &Presets::new(), options)?
            .into_values())
    };
    let margin = recipe.band_margin(&Presets::new(), canvas)?;
    if let Some(margin) = margin {
        let whole = render(0..height, false)?;
        for rows in [height / 3 + 1, 13] {
            for top in (0..height).step_by(rows) {
                let (bottom, from) = ((top + rows).min(height), top.saturating_sub(margin));
                let band = render(from..(bottom + margin).min(height), true)?;
                let made = &band[(top - from) * THIN * 3..(bottom - from) * THIN * 3];
                if let Some(at) = first_bit_difference(made, &whole[top * THIN * 3..bottom * THIN * 3]) {
                    return Ok(Some(format!("band of {rows} rows from row {top}: value {at} differs")));
                }
            }
        }
    }
    Ok((margin.is_some() != cleared).then(|| format!("cleared is {}, want {cleared}", margin.is_some())))
}

#[test]
fn a_recipe_the_query_clears_is_the_whole_render_byte_for_byte_on_both_sides_of_the_fourier_switch() {
    let one = |name: &str, key: &str, value: f64| Recipe::new([Step::new(name).param(key, value)]);
    // a blur reaching under 16 working px is summed tap by tap; from 16, or a kernel of more than 128 taps, by a transform
    // whose rounding depends on the rows it is given
    let rows = [
        ("grain, a speck summed tap by tap", one("grain", "size", 4.0), 480, true),
        (
            "grain, a speck summed by Fourier",
            one("grain", "size", 1.0),
            3900,
            false,
        ),
        (
            "gaussian_blur summed tap by tap",
            one("gaussian_blur", "radius", 3.0),
            480,
            true,
        ),
        (
            "gaussian_blur summed by Fourier",
            one("gaussian_blur", "radius", 5.0),
            480,
            false,
        ),
        ("lens_blur of 81 taps", one("lens_blur", "radius", 4.0), 480, true),
        (
            "lens_blur of more than 128 taps, by Fourier",
            one("lens_blur", "radius", 7.0),
            480,
            false,
        ),
        ("sharpen summed tap by tap", one("sharpen", "radius", 1.0), 480, true),
        ("sharpen summed by Fourier", one("sharpen", "radius", 5.0), 480, false),
        ("motion_blur of 20 taps", one("motion_blur", "length", 20.0), 480, true),
        (
            "motion_blur of more than 128 taps, by Fourier",
            one("motion_blur", "length", 200.0),
            480,
            false,
        ),
        (
            "anamorphic_streaks, a row as wide as the picture",
            Recipe::new([Step::new("anamorphic_streaks")]),
            480,
            false,
        ),
        ("vhs, noise summed tap by tap", one("vhs", "noise_size", 3.0), 480, true),
        (
            "vhs, noise summed by Fourier",
            one("vhs", "noise_size", 20.0),
            480,
            false,
        ),
        (
            "a shift of 40 px has no transform",
            Recipe::new([Step::new("rgb_split").param("amount", 40.0).param("angle", 0.0)]),
            480,
            true,
        ),
        (
            "a shift of 40 px along 330 degrees moves rows by a fraction",
            Recipe::new([Step::new("rgb_split").param("amount", 40.0)]),
            480,
            true,
        ),
        ("vignette", Recipe::new([Step::new("vignette")]), 3900, true),
        (
            "a chain with one transform step",
            Recipe::new([Step::new("vignette"), Step::new("grain").param("size", 1.0)]),
            3900,
            false,
        ),
    ];
    let problems: Vec<String> = rows
        .iter()
        .filter_map(|(what, recipe, height, cleared)| {
            thin_problem(recipe, *height, *cleared).map_or_else(
                |error| Some(format!("{what}: {error}")),
                |problem| problem.map(|found| format!("{what}: {found}")),
            )
        })
        .collect();
    assert!(
        problems.is_empty(),
        "{} problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

/// `echo` has no `Place`, so a window reads a distance down by the row inside the window: only a distance a row can carry
/// (a multiple of 2^-26) is cleared.
#[test]
fn echo_is_cleared_for_a_distance_a_row_can_carry_and_not_for_a_finer_one() {
    let down = |dy: f64| Recipe::new([Step::new("echo").param("dy", dy)]);
    let rows = [
        ("a whole number of rows", down(3.0), true),
        ("0.3 px, a multiple of 2^-26", down(0.3), true),
        ("too fine for a row to carry", down(1e-7), false),
    ];
    let problems: Vec<String> = rows
        .iter()
        .filter_map(|(what, recipe, cleared)| {
            thin_problem(recipe, 480, *cleared).map_or_else(
                |error| Some(format!("{what}: {error}")),
                |problem| problem.map(|found| format!("{what}: {found}")),
            )
        })
        .collect();
    assert!(
        problems.is_empty(),
        "{} problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn a_halftone_of_every_shape_gives_the_rows_of_the_whole_render_from_a_window() -> Outcome {
    // a sampled spot function (the cosine screen, an ellipse) and a closed one (the diamond) read the same cell tone as the dot
    let values = varied(SMALL.height, SMALL.width);
    for shape in ["euclidean", "ellipse", "diamond"] {
        let recipe = Recipe::new([Step::new("halftone")
            .param("shape", shape)
            .param("aspect", 2.5)
            .param("cell", 9.5)
            .param("sigma", 0.4)]);
        let margin = recipe
            .band_margin(&Presets::new(), u32::try_from(SMALL.height)?)?
            .ok_or("halftone is banded")?;
        assert_eq!(
            first_problem(SMALL, &values, &recipe, margin, 0.0, Options::default())?,
            None,
            "{shape}"
        );
    }
    Ok(())
}

#[test]
fn a_halftone_of_a_fractional_cell_gives_the_rows_of_the_whole_render_from_a_window() -> Outcome {
    let values = varied(SMALL.height, SMALL.width);
    for (cell, sigma) in [(13.3, 0.0), (2.7, 1.5), (7.5, 0.3)] {
        let recipe = Recipe::new([Step::new("halftone").param("cell", cell).param("sigma", sigma)]);
        let margin = recipe
            .band_margin(&Presets::new(), u32::try_from(SMALL.height)?)?
            .ok_or("halftone is banded")?;
        assert_eq!(
            first_problem(SMALL, &values, &recipe, margin, 0.0, Options::default())?,
            None,
            "cell {cell} sigma {sigma}"
        );
    }
    Ok(())
}

/// A halftone (no blur, so that the footprint is only what the geometry reads) whose screen is bent about a centre on the canvas, which lies in a later band than the first, gives the rows of
/// the whole render from a window: the centre is a fraction of the canvas, not of the band's picture.
#[test]
fn a_halftone_of_every_screen_geometry_gives_the_rows_of_the_whole_render_from_a_window() -> Outcome {
    let values = varied(SMALL.height, SMALL.width);
    // the centre, at row 104 of 131 and column 5 of 19, lies in the last band of every cut but the whole
    let at = |screen: &str, wavelength: f64| {
        Recipe::new([Step::new("halftone")
            .param("screen", screen)
            .param("cell", 100.0) // about 26 px here, so that the rows a cell reads are most of the margin
            .param("sigma", 0.0)
            .param("amplitude", 3.0)
            .param("wavelength", wavelength)
            .param("x", 0.3)
            .param("y", 0.8)])
    };
    // the shortest wave swings a cell centre furthest from its pixel
    for (screen, wavelength) in [("wave", 1.0), ("wave", 4.0), ("rings", 4.0), ("spiral", 4.0)] {
        let recipe = at(screen, wavelength);
        let margin = recipe
            .band_margin(&Presets::new(), u32::try_from(SMALL.height)?)?
            .ok_or("this halftone is banded")?;
        assert_eq!(
            first_problem(SMALL, &values, &recipe, margin, 0.0, Options::default())?,
            None,
            "{screen} {wavelength}"
        );
    }
    // rays widen with the distance from the centre, so a pixel may read from anywhere: no band is cleared ...
    let recipe = at("rays", 4.0);
    assert_eq!(recipe.band_margin(&Presets::new(), u32::try_from(SMALL.height)?)?, None);
    // ... and the rays themselves, which a flat picture shows, are the same from a band that holds no more than its rows
    let flat = vec![0.4; SMALL.height * SMALL.width * 3];
    assert_eq!(
        first_problem(SMALL, &flat, &recipe, 0, 0.0, Options::default())?,
        None,
        "rays"
    );
    Ok(())
}

/// The footprint is true: changing one row of the picture changes no output row beyond the reach the margin is made of (the
/// margin less the two rows kept for an interpolation, which a halftone does not make). The picture is a tall one with a
/// cell of about 26 px, so the reach, rows from the centre of the cell a pixel reads, is most of the margin; the wave at its
/// shortest wavelength, which swings a cell centre furthest from its pixel, reads furthest.
#[test]
fn a_halftone_of_every_screen_geometry_reads_no_further_than_its_margin_says() -> Outcome {
    let values = varied(SMALL.height, SMALL.width);
    for (screen, wavelength) in [
        ("grid", 4.0),
        ("wave", 1.0),
        ("wave", 4.0),
        ("rings", 4.0),
        ("spiral", 4.0),
    ] {
        let recipe = Recipe::new([Step::new("halftone")
            .param("screen", screen)
            .param("cell", 100.0)
            .param("sigma", 0.0)
            .param("amplitude", 3.0)
            .param("wavelength", wavelength)
            .param("x", 0.3)
            .param("y", 0.8)]);
        let margin = recipe
            .band_margin(&Presets::new(), u32::try_from(SMALL.height)?)?
            .ok_or("this halftone is banded")?;
        let render = |values: &[f32]| -> Result<Vec<f32>, Box<dyn Error>> {
            Ok(photo(SMALL, values, 0..SMALL.height)?
                .render(&recipe, &Presets::new(), Options::default())?
                .into_values())
        };
        let whole = render(&values)?;
        let row = SMALL.width * 3;
        let mut furthest = 0;
        for changed in 0..SMALL.height {
            let mut other = values.clone();
            other[changed * row..(changed + 1) * row].fill(0.5);
            let made = render(&other)?;
            for out in
                (0..SMALL.height).filter(|out| made[out * row..(out + 1) * row] != whole[out * row..(out + 1) * row])
            {
                furthest = furthest.max(out.abs_diff(changed));
            }
        }
        assert!(
            furthest + 2 <= margin,
            "{screen} {wavelength}: reads {furthest} rows away, margin {margin}"
        );
    }
    Ok(())
}
