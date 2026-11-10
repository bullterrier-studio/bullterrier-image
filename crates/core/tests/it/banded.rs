//! The byte-exact footprint honesty test (`footprint.rs`) owns small shortfalls per step; this matrix, at 1e-4, owns
//! composition (a footprint bug of 3+ rows shows far above the bound); do not tighten it.
//!
//! A banded render (`Options::with_band`) gives the pictures of the whole-picture render (within `TOLERANCE`, most
//! effects byte for byte), whatever the recipe: the whole render is the oracle, and the recipe is rendered both ways through `Photo::render`.
//!
//! Every effect the core runs is rendered as a one-step recipe with its defaults and with each parameter alone at its
//! limits, at band sizes 1, 7 and 64 rows (a band after the first starts at a row above 0, so every seeded and
//! position-dependent effect is checked away from the top). Recipes of several steps check what the runner adds:
//! margins that add up, a step that reads the whole picture between banded runs, masks, opacity, partitions, a working
//! picture that is not the photo, and margins that reach the picture. Doubles that lie about their footprint show that the
//! runner trusts it and that these comparisons can fail.

use std::error::Error;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use bullterrier_image::frame::{Fit, Resample};
use bullterrier_image::region::Bands;
use bullterrier_image::{
    Canvas, Category, Each, Effect, Entry, Footprint, Frame, Image, Input, Inputs, Layer, Options, Photo, Picture,
    Presets, Recipe, Step, effect, effects,
};
use serde_json::Value;

use crate::common::Outcome;
use crate::common::compare::first_difference_within;
use crate::common::pictures::{asset, varied};
use crate::common::run::{Registered, unique};
use crate::common::sweep::{Setting, variants};

/// A tall, narrow picture of odd sides: 64 rows is less than half of it, and a one-row band is a hundred and thirty bands.
const HEIGHT: usize = 131;
const WIDTH: usize = 19;
/// The rows of a band the matrix renders at.
const BANDS: [usize; 3] = [1, 7, 64];
/// At least this many effects are rendered banded (the others are `Whole` and so run as ever).
const BANDED_AT_LEAST: usize = 20;

/// How far the banded render of any effect may be from the whole render, in value. Nothing for most effects; a band is a
/// picture of its own height and a blur reaching 16 working px or more is summed by a Fourier transform of a length that
/// follows the height (`ops/fft`).
// constant: an FFT blur's sum depends on the window length; the largest difference measured was 5.1e-5 (relight); a
// footprint bug shows as at least a Gaussian tail, much larger than 1e-4
const TOLERANCE: f32 = 1e-4;

/// The photo of `values`, handed the maps every recipe may name (`subject`: blocks, `depth`: a hash) and the picture
/// `asset`.
fn photo(values: &[f32]) -> Result<Photo<'_>, Box<dyn Error>> {
    let (height, width) = (values.len() / 3 / WIDTH, WIDTH);
    let blocks = (0..height * width)
        .map(|at| f32::from(u8::from((at / width / 9 + at % width / 5) % 2 == 0)))
        .collect();
    let depth = (0..height * width)
        .map(|at| f32::from(u16::try_from(at * 7919 % 1000).unwrap_or(0)) / 1000.0)
        .collect();
    Ok(Photo::new(Picture::new(height, width, values)?)
        .with_map("subject", Layer::new(height, width, blocks)?)?
        .with_map("depth", Layer::new(height, width, depth)?)?
        .with_image("asset", Arc::new(asset())))
}

/// `recipe` on the photo of `values` with `options`.
fn render(values: &[f32], recipe: &Recipe, options: Options) -> Result<Image, Box<dyn Error>> {
    Ok(photo(values)?.render(recipe, &Presets::new(), options)?)
}

/// What differs between `whole` and `banded` by more than `within` (the first value at which they do, with the lengths), or none; shown by
/// a hash of the picture, never printed whole.
fn difference(whole: &Image, banded: &Image, within: f32) -> Option<String> {
    let (left, right) = (whole.clone().into_values(), banded.clone().into_values());
    first_difference_within(&left, &right, within).map(|at| {
        let hash = |values: &[f32]| {
            let mut hasher = DefaultHasher::new();
            for value in values {
                value.to_bits().hash(&mut hasher);
            }
            hasher.finish()
        };
        let largest = left
            .iter()
            .zip(&right)
            .map(|(one, other)| (one - other).abs())
            .fold(0.0_f32, f32::max);
        format!(
            "first difference at value {at}, largest {largest:e}, of {} / {} (hashes {:x} / {:x})",
            left.len(),
            right.len(),
            hash(&left),
            hash(&right)
        )
    })
}

/// The first band size in `bands` at which `recipe` banded (with `options`) is not the whole render, with how.
fn first_problem(
    values: &[f32],
    recipe: &Recipe,
    options: Options,
    bands: &[usize],
    within: f32,
) -> Result<Option<String>, Box<dyn Error>> {
    let whole = render(values, recipe, options)?;
    for &rows in bands {
        let banded = render(values, recipe, options.with_band(rows))?;
        if let Some(found) = difference(&whole, &banded, within) {
            return Ok(Some(format!("band {rows}: {found}")));
        }
    }
    Ok(None)
}

/// Fails unless `recipe` banded at each of `bands` rows (and `options`) is the whole render.
fn assert_same(values: &[f32], recipe: &Recipe, options: Options, bands: &[usize]) -> Outcome {
    assert_within(values, recipe, options, bands, 0.0)
}

/// As [`assert_same`], to within `within`.
fn assert_within(values: &[f32], recipe: &Recipe, options: Options, bands: &[usize], within: f32) -> Outcome {
    let found = first_problem(values, recipe, options, bands, within)?;
    assert!(found.is_none(), "{found:?}");
    Ok(())
}

/// `step` drawing along the pieces of the subject.
fn partitioned(mut step: Step) -> Step {
    step.partition = Some("subject".to_owned());
    step
}

fn step_of(name: &str, setting: &Setting, pieces: bool) -> Step {
    let mut step = Step::new(name);
    step.params.extend(setting.clone());
    if pieces { partitioned(step) } else { step }
}

#[test]
fn every_effect_with_its_defaults_and_each_parameter_at_its_limits_is_the_same_banded_at_any_band_size() -> Outcome {
    let values = varied(HEIGHT, WIDTH);
    let mut problems = Vec::new();
    let (mut banded_effects, mut cases) = (0, 0);
    for name in effects() {
        let one = effect(&name)?;
        let table = one.params();
        let partitioned = one.inputs().contains(&Input::Partition);
        let mut is_banded = false;
        for setting in table.as_ref().map_or_else(|| vec![Setting::new()], variants) {
            let ratio = f64::from(u16::try_from(HEIGHT)?) / 480.0;
            let completed = match &table {
                Some(table) => match table.check(&name, &setting) {
                    Ok(completed) => completed,
                    Err(_) => continue, // a combination the effect refuses
                },
                None => setting.clone(),
            };
            let footprint = one.footprint(&Value::Object(completed), &Canvas::new(HEIGHT, WIDTH, ratio))?;
            if footprint.margin(ratio).is_none() {
                continue; // Whole: the same code as the whole render
            }
            is_banded = true;
            let recipe = Recipe::new([step_of(&name, &setting, partitioned)]);
            // the defaults are also rendered at a ratio of 1 and 2, the sizes recipes are written for
            let sizes: &[Option<u32>] = if setting.is_empty() {
                &[None, Some(480), Some(960)]
            } else {
                &[None]
            };
            for &size in sizes {
                let options = size.map_or_else(Options::default, |size| Options::default().with_size(size));
                let bands: &[usize] = if size.is_some() { &BANDS[1..] } else { &BANDS };
                cases += bands.len();
                if let Some(found) = first_problem(&values, &recipe, options, bands, TOLERANCE)? {
                    problems.push(format!("{name} {setting:?} size {size:?}: {found}"));
                }
            }
        }
        banded_effects += usize::from(is_banded);
    }
    assert!(
        problems.is_empty(),
        "{} problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
    assert!(
        banded_effects >= BANDED_AT_LEAST,
        "only {banded_effects} effects were banded"
    );
    assert!(cases > 500, "only {cases} cases ran");
    Ok(())
}

fn grade() -> Step {
    Step::new("grade").param("exposure", 0.4).param("saturation", 1.3)
}

fn blur(sigma: f64) -> Step {
    Step::new("gaussian_blur").param("radius", sigma)
}

#[test]
fn a_run_of_steps_that_each_have_a_margin_is_the_same_banded() -> Outcome {
    let recipe = Recipe::new([
        grade(),
        Step::new("vignette"),
        blur(3.0),
        Step::new("grain").param("amount", 0.3).param("seed", 5),
        Step::new("chromatic_aberration").param("amount", 6.0),
        blur(1.0),
    ]);
    assert_same(&varied(HEIGHT, WIDTH), &recipe, Options::default(), &BANDS)
}

#[test]
fn a_step_that_reads_the_whole_picture_between_two_banded_runs_is_the_same_banded() -> Outcome {
    let recipe = Recipe::new([
        grade(),
        blur(2.0),
        Step::new("kaleidoscope"),
        Step::new("vignette"),
        blur(2.0),
        Step::new("kaleidoscope").param("segments", 3),
        Step::new("grain").param("seed", 2),
    ]);
    assert_same(&varied(HEIGHT, WIDTH), &recipe, Options::default(), &BANDS)
}

#[test]
fn opacity_a_mask_a_blend_a_partition_and_an_each_step_are_the_same_banded() -> Outcome {
    let recipe = Recipe::new([
        Entry::from(blur(3.0).opacity(0.6)),
        Entry::from(grade().mask("subject")),
        Entry::from(
            blur(2.0)
                .mask("depth")
                .opacity(0.8)
                .blend_mode(bullterrier_image::BlendMode::Multiply),
        ),
        Entry::from(partitioned(Step::new("seams").param("width", 2.0)).mask("subject")),
        Entry::from(Each::new("cells", [Some(blur(2.0)), Some(grade())])),
        Entry::from(blur(1.5).mask("subject")),
    ])
    .with_region(
        "cells",
        Bands {
            of: "depth".to_owned(),
            count: 3,
        },
    );
    assert_same(&varied(HEIGHT, WIDTH), &recipe, Options::default(), &BANDS)
}

#[test]
fn a_working_picture_that_is_not_the_photo_is_the_same_banded() -> Outcome {
    let mut recipe = Recipe::new([
        grade(),
        blur(4.0).mask("subject"),
        Step::new("vignette"),
        Step::new("grain"),
    ]);
    recipe.frame = Frame {
        angle: 17.0,
        fit: Fit::Crop,
        resample: Resample::Bicubic,
        ..Frame::default()
    };
    assert_same(
        &varied(HEIGHT, WIDTH),
        &recipe,
        Options::default().with_size(300),
        &BANDS,
    )
}

#[test]
fn margins_that_reach_the_picture_give_the_whole_render() -> Outcome {
    let recipe = Recipe::new([blur(60.0), blur(40.0), Step::new("vignette")]);
    assert_within(
        &varied(HEIGHT, WIDTH),
        &recipe,
        Options::default().with_size(480),
        &BANDS,
        1e-5,
    )?;
    // a band as tall as the picture, or taller
    assert_same(
        &varied(HEIGHT, WIDTH),
        &Recipe::new([blur(2.0)]),
        Options::default(),
        &[HEIGHT, HEIGHT * 3],
    )
}

/// A relief effect multiplies a blurred height by a large factor, so a blur summed by the Fourier transform (whose sum
/// follows the window length) must not show: each case is the effect with two parameters at their limits at once, on a
/// picture 481 rows high (a design pixel is a working pixel), blurs reaching 8 to 24 px.
#[test]
fn a_relief_effect_with_a_steep_height_and_a_wide_blur_is_the_same_banded() -> Outcome {
    let values = varied(481, WIDTH);
    let mut cases = Vec::new();
    for blur in [2.0, 3.0, 4.0, 6.0, 20.0] {
        cases.push(
            Step::new("relight")
                .param("blur", blur)
                .param("elevation", 1.0)
                .param("height", 1000.0),
        );
        cases.push(
            Step::new("liquid_metal")
                .param("smoothing", blur)
                .param("relief", 200.0),
        );
    }
    let mut problems = Vec::new();
    for step in cases {
        let found = first_problem(
            &values,
            &Recipe::new([step.clone()]),
            Options::default(),
            &[17, 150],
            TOLERANCE,
        )?;
        problems.extend(found.map(|found| format!("{} {:?}: {found}", step.effect, step.params)));
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    Ok(())
}

#[test]
fn a_band_of_no_rows_is_refused_naming_it() {
    let values = varied(HEIGHT, WIDTH);
    let result = render(&values, &Recipe::new([grade()]), Options::default().with_band(0));
    assert!(
        matches!(&result, Err(error) if error.to_string().contains("band")),
        "{result:?}"
    );
}

#[test]
fn the_same_banded_picture_comes_at_any_number_of_threads() -> Outcome {
    let values = varied(HEIGHT, WIDTH);
    let recipe = Recipe::new([
        grade(),
        blur(3.0),
        Step::new("kaleidoscope"),
        Step::new("grain"),
        blur(2.0),
    ]);
    let one = render(&values, &recipe, Options::default().with_band(7))?;
    for threads in [2, 3, 10] {
        let other = render(&values, &recipe, Options::default().with_band(7).with_threads(threads))?;
        let found = difference(&one, &other, 0.0);
        assert!(found.is_none(), "threads {threads}: {found:?}");
    }
    Ok(())
}

/// A vertical box blur of `reach` working rows each side that claims `says`, dims by the size ratio and, if `uses_place`,
/// adds a stripe by the canvas row; it is registered under a name of its own.
struct Double {
    name: String,
    reach: usize,
    says: Option<Footprint>,
    uses_place: bool,
}

impl Double {
    /// The double, in the registry for as long as the guard lives.
    fn registered(reach: usize, says: Option<Footprint>, uses_place: bool) -> Result<Registered<Self>, Box<dyn Error>> {
        Ok(Registered::new(Self {
            name: unique("banded_double"),
            reach,
            says,
            uses_place,
        })?)
    }
}

impl Effect for Double {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn inputs(&self) -> &[Input] {
        if self.uses_place {
            &[Input::Place, Input::SizeRatio]
        } else {
            &[Input::SizeRatio]
        }
    }

    fn footprint(&self, _params: &Value, canvas: &Canvas) -> Result<Footprint, bullterrier_image::Error> {
        // honest: the reach in design px, which the runner scales back by the ratio
        Ok(self.says.unwrap_or(Footprint::Radius(
            f64::from(u32::try_from(self.reach).unwrap_or(u32::MAX)) / canvas.ratio,
        )))
    }

    fn apply(
        &self,
        picture: &Picture<'_>,
        _params: Value,
        inputs: &Inputs<'_>,
    ) -> Result<Image, bullterrier_image::Error> {
        let (height, width) = (picture.height(), picture.width());
        let place = inputs.place(picture);
        let pixels: Vec<[f32; 3]> = picture.values().as_chunks::<3>().0.to_vec();
        Ok(picture.map_at(|row, column, pixel| {
            let low = row.saturating_sub(self.reach);
            let high = (row + self.reach + 1).min(height);
            let mut sum = [0.0_f32; 3];
            for near in low..high {
                for (total, value) in sum.iter_mut().zip(pixels[near * width + column]) {
                    *total += value;
                }
            }
            #[expect(clippy::cast_precision_loss, reason = "a few rows")]
            let count = (high - low) as f32;
            let stripe = if self.uses_place && (place.first_row + row) % 3 == 0 {
                0.5
            } else {
                1.0
            };
            let _ = pixel;
            // the light the design scale lets through: the canvas's, never the band's
            #[expect(clippy::cast_possible_truncation, reason = "a ratio of a few")]
            let scale = 1.0 - 0.5 * inputs.size_ratio() as f32;
            sum.map(|total| total / count * stripe * scale)
        }))
    }
}

#[test]
fn the_runner_trusts_a_footprint_and_a_place_is_the_band_on_its_canvas() -> Outcome {
    let values = varied(HEIGHT, WIDTH);
    // known good: an honest reach, with and without an effect that needs its canvas row
    for uses_place in [false, true] {
        let honest = Double::registered(5, None, uses_place)?;
        assert_same(
            &values,
            &Recipe::new([Step::new(honest.name())]),
            Options::default(),
            &BANDS,
        )?;
    }
    // known bad: the same blur claiming to read no row but its own differs at the band edges, so the comparison bites
    let liar = Double::registered(5, Some(Footprint::Pixel), false)?;
    let recipe = Recipe::new([Step::new(liar.name())]);
    let whole = render(&values, &recipe, Options::default())?;
    let banded = render(&values, &recipe, Options::default().with_band(7))?;
    assert!(difference(&whole, &banded, 0.0).is_some());
    Ok(())
}
