//! `Effect::footprint` tells the truth. An effect that says `Pixel` or `Radius(r)` gives, for the rows of a band, the
//! same pixels whether it is run on the whole picture or only on the band with `ceil(r)` rows of margin. The
//! whole render is the oracle; the band is cropped from the input (and from the maps it is given) and run with the
//! same size ratio, so a px value scales by the whole picture, not by the band.
//!
//! The check runs on every effect in the registry that says it is less than `Whole`, with its defaults and with each
//! parameter set alone to its limits, to each choice and to each switch position ; the
//! bands are one at the top of the picture, one at the bottom and one inside. A margin of half the picture or more is
//! clamped to the picture, so that band is the whole and proves little. Known-bad doubles prove that it fails.

use std::error::Error;

use crate::common::Outcome;
use crate::common::compare::first_difference_within;
use crate::common::pictures::asset;
use crate::common::sweep::{Setting, variants};
use bullterrier_image::params::{Kind, Table};
use bullterrier_image::{
    Category, Effect, Footprint, Image, Input, Inputs, Labels, Layer, Picture, Place, effect, effects,
};
use serde_json::{Value, json};

/// The picture is tall and narrow: only the rows matter to a band.
const HEIGHT: usize = 96;
const WIDTH: usize = 24;
/// The rows of each piece of the partition an effect that takes one is given.
const PIECE_ROWS: usize = 20;
/// The rows of a band, and where each band starts: at the top, in the middle and at the bottom of the picture.
const BAND: usize = 8;
const BANDS: [usize; 3] = [0, (HEIGHT - BAND) / 2, HEIGHT - BAND];
/// Rendering a band alone must give the whole's pixels to within this: a different order of summing is not a fault.
const SAME: f32 = 1e-4;
/// The size ratio the bands are run at: 1, where the parameters a render gets are the completed ones. Other ratios scale
/// (and floor) them in the runner, so they are tried through `Photo::render` (`banded.rs`).
const RATIO: f64 = 1.0;
/// At least this many effects of the core are less than `Whole` with their defaults, so declaring all of them `Whole`
/// does not pass.
const NOT_WHOLE: usize = 20;

/// `count` values of 0..1 from a hash of their place and `seed`.
fn hashed(count: usize, seed: u32) -> Vec<f32> {
    (0..count)
        .map(|index| {
            let mut hash = u32::try_from(index % (1 << 30))
                .unwrap_or(0)
                .wrapping_add(seed.wrapping_mul(0x9e37_79b9));
            hash = (hash ^ (hash >> 16)).wrapping_mul(0x85eb_ca6b);
            hash = (hash ^ (hash >> 13)).wrapping_mul(0xc2b2_ae35);
            f32::from(u16::try_from((hash ^ (hash >> 16)) >> 16).unwrap_or(0)) / 65535.0
        })
        .collect()
}

/// The rows `from..to` of the picture `values` (`WIDTH` wide, `channels` values a pixel).
fn rows(values: &[f32], channels: usize, from: usize, to: usize) -> &[f32] {
    &values[from * WIDTH * channels..to * WIDTH * channels]
}

/// `effect` run on rows `from..to` of the picture and of its map, with the parameters `params`, at `ratio`.
fn run(
    one: &dyn Effect,
    table: &Table,
    params: &Setting,
    ratio: f64,
    (from, to): (usize, usize),
    (picture, map): (&[f32], &Layer),
) -> Result<Image, Box<dyn Error>> {
    let cropped = Layer::new(to - from, WIDTH, rows(map.values(), 1, from, to).to_vec())?;
    // an effect that takes a partition gets bands of `PIECE_ROWS` rows, two pieces taking turns: borders far enough
    // apart that a band can lie beyond a short reach of every one
    let pieces = Labels::new(
        to - from,
        WIDTH,
        (from..to)
            .flat_map(|row| std::iter::repeat_n(i32::from(row / PIECE_ROWS % 2 == 1), WIDTH))
            .collect(),
        2,
    )?;
    // the band lies where it was cut from, on the canvas that is the whole picture
    let mut inputs = Inputs::default()
        .with_size_ratio(ratio)
        .with_place(Place::new(HEIGHT, WIDTH, from));
    for field in table.fields.iter().filter(|field| field.kind == Kind::Field) {
        inputs = inputs.with_field(field.name, &cropped);
    }
    if one.inputs().contains(&Input::Partition) {
        inputs = inputs.with_partition(&pieces);
    }
    let asset = asset();
    for field in table.fields.iter().filter(|field| field.kind == Kind::Picture) {
        if params[field.name].is_string() {
            inputs = inputs.with_image(field.name, &asset);
        }
    }
    let picture = Picture::new(to - from, WIDTH, rows(picture, 3, from, to))?;
    Ok(one.apply(&picture, &Value::Object(params.clone()), &inputs)?)
}

/// The ways `one` says less than `Whole` and is wrong, with `params` as written; none if it is `Whole`.
fn problems_of(one: &dyn Effect, given: &Setting, picture: &[f32], map: &Layer) -> Result<Vec<String>, Box<dyn Error>> {
    let table = one.params();
    let Ok(completed) = table.check(one.name(), given) else {
        return Ok(Vec::new()); // a combination the effect refuses
    };
    // the footprint is asked about what the render is given
    let params = completed;
    let footprint = one.footprint(&Value::Object(params.clone()))?;
    let Some(margin) = footprint.margin() else {
        return Ok(Vec::new());
    };
    let whole = run(one, &table, &params, RATIO, (0, HEIGHT), (picture, map))?.into_values();
    let mut problems = Vec::new();
    for first in BANDS {
        let (from, to) = (first.saturating_sub(margin), (first + BAND + margin).min(HEIGHT));
        let band = run(one, &table, &params, RATIO, (from, to), (picture, map))?.into_values();
        let own = rows(&band, 3, first - from, first - from + BAND);
        if let Some(at) = first_difference_within(own, rows(&whole, 3, first, first + BAND), SAME) {
            problems.push(format!(
                "{} says {footprint:?} (margin {margin} rows) with {given:?}, but the band of rows {first}..{} \
                 differs from the whole's at value {at} ({} against {})",
                one.name(),
                first + BAND,
                own[at],
                whole[first * WIDTH * 3 + at]
            ));
            break;
        }
    }
    Ok(problems)
}

/// Every wrong answer `one` gives, over the variants of its parameters.
fn problems(one: &dyn Effect) -> Result<Vec<String>, Box<dyn Error>> {
    let picture = hashed(HEIGHT * WIDTH * 3, 3);
    // noise has an edge at every pixel, so it cannot show a reach beyond a few rows; a rectangle's edges are far apart:
    // `inner` lies round the middle band, `outer` round the top and the bottom one
    let rectangle = |rows: std::ops::Range<usize>| {
        Layer::new(
            HEIGHT,
            WIDTH,
            (0..HEIGHT * WIDTH)
                .map(|at| {
                    f32::from(u8::from(
                        rows.contains(&(at / WIDTH)) && (4..20).contains(&(at % WIDTH)),
                    ))
                })
                .collect(),
        )
    };
    let maps = [
        Layer::new(HEIGHT, WIDTH, hashed(HEIGHT * WIDTH, 5))?,
        rectangle(36..60)?,
        rectangle(4..HEIGHT - 4)?,
    ];
    let table = one.params();
    let cases = variants(&table);
    let mut found = Vec::new();
    for map in &maps {
        for case in &cases {
            found.extend(problems_of(one, case, &picture, map)?);
        }
    }
    Ok(found)
}

#[test]
fn every_effect_that_says_it_reads_less_than_the_whole_picture_gives_the_same_rows_from_a_band_with_its_margin()
-> Outcome {
    let mut found = Vec::new();
    let mut not_whole = 0;
    for name in effects() {
        let one = effect(&name)?;
        found.extend(problems(&*one)?);
        let defaults = one.params().check(&name, &Setting::new())?;
        if one.footprint(&Value::Object(defaults))? != Footprint::Whole {
            not_whole += 1;
        }
    }
    assert!(found.is_empty(), "{} problems:\n{}", found.len(), found.join("\n"));
    assert!(
        not_whole >= NOT_WHOLE,
        "only {not_whole} effects read less than the whole picture"
    );
    Ok(())
}

/// A vertical box blur of `reach` rows (working px) each side, claiming `says`; nothing is read from the
/// picture's place.
struct Blur {
    reach: usize,
    says: Footprint,
}

impl Effect for Blur {
    fn name(&self) -> &'static str {
        "footprint_double_blur"
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn footprint(&self, _params: &Value) -> Result<Footprint, bullterrier_image::Error> {
        Ok(self.says)
    }

    fn apply(
        &self,
        picture: &Picture<'_>,
        _params: &Value,
        _inputs: &Inputs<'_>,
    ) -> Result<Image, bullterrier_image::Error> {
        let (height, width) = (picture.height(), picture.width());
        let reach = self.reach;
        let pixels = picture.values();
        let mut out = Vec::with_capacity(pixels.len());
        for row in 0..height {
            let (low, high) = (row.saturating_sub(reach), (row + reach).min(height - 1));
            for at in row * width..(row + 1) * width {
                for channel in 0..3 {
                    let sum: f32 = (low..=high)
                        .map(|r| pixels[(r * width + at % width) * 3 + channel])
                        .sum();
                    out.push(sum / f32::from(u16::try_from(high - low + 1).unwrap_or(1)));
                }
            }
        }
        Image::new(height, width, &out)
    }
}

/// The picture's row number as the pixel: it reads nothing around a pixel, but it knows where the pixel is.
struct Ruler;

impl Effect for Ruler {
    fn name(&self) -> &'static str {
        "footprint_double_ruler"
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn footprint(&self, _params: &Value) -> Result<Footprint, bullterrier_image::Error> {
        Ok(Footprint::Pixel)
    }

    fn apply(
        &self,
        picture: &Picture<'_>,
        _params: &Value,
        _inputs: &Inputs<'_>,
    ) -> Result<Image, bullterrier_image::Error> {
        Ok(picture.map_at(|row, _, _| [f32::from(u16::try_from(row).unwrap_or(0)) / 255.0; 3]))
    }
}

/// The canvas row of the picture's row as the pixel: it reads nothing around a pixel, and it is told where the pixel is.
struct PlacedRuler;

impl Effect for PlacedRuler {
    fn name(&self) -> &'static str {
        "footprint_double_placed_ruler"
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn inputs(&self) -> &[Input] {
        &[Input::Place]
    }

    fn footprint(&self, _params: &Value) -> Result<Footprint, bullterrier_image::Error> {
        Ok(Footprint::Pixel)
    }

    fn apply(
        &self,
        picture: &Picture<'_>,
        _params: &Value,
        inputs: &Inputs<'_>,
    ) -> Result<Image, bullterrier_image::Error> {
        let first = inputs.place(picture).first_row;
        Ok(picture.map_at(|row, _, _| [f32::from(u16::try_from(first + row).unwrap_or(0)) / 255.0; 3]))
    }
}

#[test]
fn a_blur_that_says_it_reads_nothing_around_a_pixel_or_too_little_fails() -> Outcome {
    for says in [Footprint::Pixel, Footprint::Radius(0.5), Footprint::Radius(2.0)] {
        let found = problems(&Blur { reach: 5, says })?;
        assert!(!found.is_empty(), "{says:?} passed for a blur of 5 rows");
    }
    Ok(())
}

#[test]
fn a_blur_that_says_what_it_reads_or_everything_passes() -> Outcome {
    for says in [Footprint::Radius(5.0), Footprint::Radius(40.0), Footprint::Whole] {
        let found = problems(&Blur { reach: 5, says })?;
        assert!(found.is_empty(), "{says:?} failed for a blur of 5 rows: {found:?}");
    }
    Ok(())
}

#[test]
fn an_effect_that_knows_the_place_of_a_pixel_cannot_say_pixel() -> Outcome {
    assert!(
        !problems(&Ruler)?.is_empty(),
        "Pixel passed for an effect that draws the row"
    );
    Ok(())
}

#[test]
fn an_effect_that_reads_its_place_from_the_input_can_say_pixel() -> Outcome {
    let found = problems(&PlacedRuler)?;
    assert!(
        found.is_empty(),
        "Pixel failed for an effect that draws the canvas row: {found:?}"
    );
    Ok(())
}

/// Parameters read together by an effect's footprint, set to values that make each term matter at once: the sweep above
/// moves one parameter at a time, so a claim that adds or multiplies two of them is never tried at both extremes. The
/// reaches are between a few rows and about half the picture (a larger margin is clamped to the whole and proves
/// little). `edge` stays at its default: a wrapping edge says `Whole`.
fn combinations() -> Vec<(&'static str, Value)> {
    let mut rows = vec![
        ("light_wrap", json!({})),
        ("light_wrap", json!({"width": 96, "blur": 1})), // a wide band, a short reach
        ("light_wrap", json!({"width": 1, "blur": 100})), // a thin band, a long reach
        ("light_wrap", json!({"width": 96, "blur": 8})),
        ("outline", json!({"width": 30, "glow": 12})),
        ("outline", json!({"width": 0, "glow": 30})),
        ("outline", json!({"width": 30, "glow": 0})),
        ("outline", json!({"width": 200, "glow": 200})), // clamped to the whole picture
        ("defringe", json!({"width": 4, "amount": 1, "field_threshold": 0.1})),
        ("defringe", json!({"width": 1, "amount": 1})),
        ("echo", json!({"copies": 32, "dy": 1})),
        ("echo", json!({"copies": 32, "dy": -1.5})),
        ("echo", json!({"copies": 1, "dy": 40})),
        ("parallax", json!({"amount": 48, "focus": 0})),
        ("parallax", json!({"amount": 48, "focus": 1})),
        ("parallax", json!({"amount": 20, "focus": 0.3})),
        ("drop_shadow", json!({"copies": 5, "offset": 4, "blur": 2})),
        (
            "drop_shadow",
            json!({"copies": 1, "offset": 0, "blur": 10, "truncate": 1}),
        ),
        ("drop_shadow", json!({"copies": 4, "offset": 12, "blur": 0})),
        (
            "drop_shadow",
            json!({"copies": 3, "offset": 6, "blur": 4, "truncate": 10}),
        ),
        ("edges", json!({"method": "sobel", "radius": 2, "truncate": 10})),
        ("edges", json!({"method": "dog", "radius": 2, "truncate": 10})),
        ("edges", json!({"method": "dog", "radius": 12, "truncate": 1})),
        ("focus_blur", json!({"radius": 2, "truncate": 10})),
        ("focus_blur", json!({"radius": 12, "truncate": 1})),
        ("relight", json!({"blur": 2, "truncate": 10})),
        ("relight", json!({"blur": 12, "truncate": 1})),
        ("halftone", json!({"cell": 3, "sigma": 5, "truncate": 1})), // a small screen, a long blur
        ("halftone", json!({"cell": 12, "sigma": 0.2, "truncate": 1})), // a large screen, a short blur
        ("halftone", json!({"cell": 4, "sigma": 1, "truncate": 3})),
        ("halftone", json!({"cell": 40, "sigma": 0, "angle": 0})), // aligned: a pixel reads the centre of a tall cell
        ("liquid_metal", json!({"smoothing": 2, "truncate": 10})),
        ("liquid_metal", json!({"smoothing": 12, "truncate": 1})),
    ];
    // the blurs that reach `radius x truncate`
    for name in ["gaussian_blur", "bloom", "sharpen", "highpass", "ambient_occlusion"] {
        rows.push((name, json!({"radius": 2, "truncate": 10})));
        rows.push((name, json!({"radius": 12, "truncate": 1})));
        rows.push((name, json!({"radius": 5, "truncate": 5})));
    }
    rows
}

/// `inner` with its footprint replaced by `says`: its pixels are the real effect's.
struct Claim {
    inner: std::sync::Arc<dyn Effect>,
    says: Footprint,
}

impl Effect for Claim {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn category(&self) -> Category {
        self.inner.category()
    }

    fn inputs(&self) -> &[Input] {
        self.inner.inputs()
    }

    fn fields(&self) -> &[bullterrier_image::FieldParam] {
        self.inner.fields()
    }

    fn params(&self) -> Table {
        self.inner.params()
    }

    fn footprint(&self, _params: &Value) -> Result<Footprint, bullterrier_image::Error> {
        Ok(self.says)
    }

    fn apply(
        &self,
        picture: &Picture<'_>,
        params: &Value,
        inputs: &Inputs<'_>,
    ) -> Result<Image, bullterrier_image::Error> {
        self.inner.apply(picture, params, inputs)
    }
}

/// `problems_of` for one setting over the same maps `problems` uses.
fn problems_with(one: &dyn Effect, setting: &Setting) -> Result<Vec<String>, Box<dyn Error>> {
    let picture = hashed(HEIGHT * WIDTH * 3, 3);
    let rectangle = |rows: std::ops::Range<usize>| {
        Layer::new(
            HEIGHT,
            WIDTH,
            (0..HEIGHT * WIDTH)
                .map(|at| {
                    f32::from(u8::from(
                        rows.contains(&(at / WIDTH)) && (4..20).contains(&(at % WIDTH)),
                    ))
                })
                .collect(),
        )
    };
    let maps = [
        Layer::new(HEIGHT, WIDTH, hashed(HEIGHT * WIDTH, 5))?,
        rectangle(36..60)?,
        rectangle(4..HEIGHT - 4)?,
    ];
    let mut found = Vec::new();
    for map in &maps {
        found.extend(problems_of(one, setting, &picture, map)?);
    }
    Ok(found)
}

#[test]
fn an_effect_whose_footprint_reads_two_parameters_tells_the_truth_with_both_at_once() -> Outcome {
    for (name, setting) in combinations() {
        let Value::Object(setting) = setting else {
            return Err("a row is an object".into());
        };
        let found = problems_with(&*effect(name)?, &setting)?;
        assert!(found.is_empty(), "{name} {setting:?}: {found:?}");
    }
    Ok(())
}

#[test]
fn every_combination_can_fail_a_claim_that_reads_nothing_around_a_pixel() -> Outcome {
    for (name, setting) in combinations() {
        let Value::Object(setting) = setting else {
            return Err("a row is an object".into());
        };
        let liar = Claim {
            inner: effect(name)?,
            says: Footprint::Pixel,
        };
        assert!(
            !problems_with(&liar, &setting)?.is_empty(),
            "{name} {setting:?} passed a claim of Pixel: the row proves nothing"
        );
    }
    Ok(())
}
