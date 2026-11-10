//! `Effect::footprint` tells the truth. An effect that says `Pixel` or `Radius(r)` gives, for the rows of a band, the
//! same pixels whether it is run on the whole picture or only on the band with `ceil(r * ratio)` rows of margin. The
//! whole render is the oracle; the band is cropped from the input (and from the maps it is given) and run with the
//! same size ratio, so a px value scales by the whole picture, not by the band.
//!
//! The check runs on every effect in the registry that says it is less than `Whole`, with its defaults and with each
//! parameter set alone to its limits, to each choice and to each switch position (the defaults at two size ratios, the rest at the first); the
//! bands are one at the top of the picture, one at the bottom and one inside. A margin of half the picture or more is
//! clamped to the picture, so that band is the whole and proves little. Known-bad doubles prove that it fails.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "limits and ratios are small whole numbers or decimals, and a margin of rows is a small positive one"
)]

use std::error::Error;

use crate::common::Outcome;
use crate::common::compare::first_difference_within;
use bullterrier_image::params::{Field, Kind, Scalar, Table, Unit};
use bullterrier_image::{Category, Effect, Footprint, Image, Input, Inputs, Labels, Layer, Picture, effect, effects};
use serde_json::{Map, Value, json};

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
/// A distance in design px that is neither nothing nor the picture.
const TYPICAL_PX: f64 = 12.0;
/// The size ratios the picture is rendered at.
const RATIOS: [f64; 3] = [0.5, 1.0, 2.0];
/// At least this many effects of the core are less than `Whole` with their defaults, so declaring all of them `Whole`
/// does not pass.
const NOT_WHOLE: usize = 20;

type Setting = Map<String, Value>;

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

fn number(bound: Option<Scalar>) -> Option<f64> {
    match bound? {
        Scalar::Int(whole) => i32::try_from(whole).ok().map(f64::from),
        Scalar::Float(decimal) => Some(decimal),
        _ => None,
    }
}

/// The parameters to try: none given (the defaults), then each parameter alone at each end of its range (a direction
/// also at the quarter turns, a distance also at 12 px), at each choice and at the other position of a switch. Some may be refused by the table (a pair that crosses): those are
/// left out when they are checked.
fn variants(table: &Table) -> Vec<Setting> {
    let mut cases = vec![Setting::new()];
    for field in &table.fields {
        let values: Vec<Value> = match field.kind {
            Kind::Float | Kind::Int => {
                let (low, high) = (number(field.min), number(field.max));
                let mut bounds: Vec<f64> = [low, high].into_iter().flatten().collect();
                if let (Some(low), Some(high)) = (low, high) {
                    if field.cyclic {
                        // a direction is tried at the quarter turns too: at its limits it points along the rows or across them
                        bounds.extend([0.25, 0.5, 0.75].map(|share| low + (high - low) * share));
                    }
                    if field.unit == Some(Unit::Px) {
                        // a distance is tried at 12 px too, a size an effect works at: its limits are beyond the little picture
                        bounds.extend(
                            [-TYPICAL_PX, TYPICAL_PX]
                                .into_iter()
                                .filter(|px| (low..=high).contains(px)),
                        );
                    }
                }
                bounds.into_iter()
            }
            .map(|bound| {
                if field.kind == Kind::Int {
                    json!(bound as i64)
                } else {
                    json!(bound)
                }
            })
            .collect(),
            Kind::Choice => field.choices.iter().map(|choice| json!(choice)).collect(),
            Kind::Bool => vec![json!(true), json!(false)],
            _ => Vec::new(),
        };
        cases.extend(
            values
                .into_iter()
                .map(|value| Setting::from_iter([(field.name.to_owned(), value)])),
        );
    }
    cases
}

/// `params` as the runner hands them to an effect for a picture of `ratio` working px per design px: `px` multiplied
/// by it and `density` divided by its square (a whole number rounded; a `px` number does not fall below its minimum).
fn scaled(table: &Table, params: &Setting, ratio: f64) -> Setting {
    params
        .iter()
        .map(|(name, value)| {
            let field: Option<&Field> = table.fields.iter().find(|field| field.name == name);
            let number = value.as_f64();
            let value = match (field, number) {
                (Some(field), Some(number)) if field.unit == Some(Unit::Px) || field.unit == Some(Unit::Density) => {
                    let by = if field.unit == Some(Unit::Px) {
                        ratio
                    } else {
                        1.0 / (ratio * ratio)
                    };
                    let kept = if field.unit == Some(Unit::Px) && ratio < 1.0 {
                        (number * by).max(self::number(field.min).unwrap_or(f64::MIN))
                    } else {
                        number * by
                    };
                    if field.kind == Kind::Int {
                        json!(kept.round_ties_even() as i64)
                    } else {
                        json!(kept)
                    }
                }
                _ => value.clone(),
            };
            (name.clone(), value)
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
    table: Option<&Table>,
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
    let mut inputs = Inputs::default().with_size_ratio(ratio);
    for field in table
        .iter()
        .flat_map(|table| &table.fields)
        .filter(|field| field.kind == Kind::Field)
    {
        inputs = inputs.with_field(field.name, &cropped);
    }
    if one.inputs().contains(&Input::Partition) {
        inputs = inputs.with_partition(&pieces);
    }
    let picture = Picture::new(to - from, WIDTH, rows(picture, 3, from, to))?;
    Ok(one.apply(&picture, Value::Object(params.clone()), &inputs)?)
}

/// The ways `one` says less than `Whole` and is wrong, with `params` as written and `ratio`; none if it is `Whole`.
fn problems_of(
    one: &dyn Effect,
    given: &Setting,
    ratio: f64,
    picture: &[f32],
    map: &Layer,
) -> Result<Vec<String>, Box<dyn Error>> {
    let table = one.params();
    let completed = match &table {
        Some(table) => match table.check(one.name(), given) {
            Ok(completed) => completed,
            Err(_) => return Ok(Vec::new()), // a combination the effect refuses
        },
        None => given.clone(),
    };
    let footprint = one.footprint(&Value::Object(completed.clone()))?;
    let Some(margin) = footprint.margin(ratio) else {
        return Ok(Vec::new());
    };
    let params = table
        .as_ref()
        .map_or(completed.clone(), |table| scaled(table, &completed, ratio));
    let whole = run(one, table.as_ref(), &params, ratio, (0, HEIGHT), (picture, map))?.into_values();
    let mut problems = Vec::new();
    for first in BANDS {
        let (from, to) = (first.saturating_sub(margin), (first + BAND + margin).min(HEIGHT));
        let band = run(one, table.as_ref(), &params, ratio, (from, to), (picture, map))?.into_values();
        let own = rows(&band, 3, first - from, first - from + BAND);
        if let Some(at) = first_difference_within(own, rows(&whole, 3, first, first + BAND), SAME) {
            problems.push(format!(
                "{} says {footprint:?} (margin {margin} rows) with {given:?} at ratio {ratio}, but the band of rows {first}..{} \
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

/// Every wrong answer `one` gives, over the variants of its parameters and the ratios.
fn problems(one: &dyn Effect) -> Result<Vec<String>, Box<dyn Error>> {
    let picture = hashed(HEIGHT * WIDTH * 3, 3);
    // noise has an edge at every pixel, so it cannot show a reach beyond a few rows; a rectangle's edges are far apart:
    // `inner` lies round the middle band, `outer` round the top and the bottom one
    let rectangle = |rows: std::ops::Range<usize>| {
        Layer::new(
            HEIGHT,
            WIDTH,
            (0..HEIGHT * WIDTH)
                .map(|at| f32::from(u8::from(rows.contains(&(at / WIDTH)) && (4..20).contains(&(at % WIDTH)))))
                .collect(),
        )
    };
    let maps = [
        Layer::new(HEIGHT, WIDTH, hashed(HEIGHT * WIDTH, 5))?,
        rectangle(36..60)?,
        rectangle(4..HEIGHT - 4)?,
    ];
    let cases = one
        .params()
        .map_or_else(|| vec![Setting::new()], |table| variants(&table));
    let mut found = Vec::new();
    for map in &maps {
        for case in &cases {
            // the size ratio matters to every parameter alike: the second one is tried with the defaults alone
            let ratios = if case.is_empty() { &RATIOS[..] } else { &RATIOS[..1] };
            for &ratio in ratios {
                found.extend(problems_of(one, case, ratio, &picture, map)?);
            }
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
        let defaults = one
            .params()
            .map(|table| table.check(&name, &Setting::new()))
            .transpose()?;
        if let Some(defaults) = defaults
            && one.footprint(&Value::Object(defaults))? != Footprint::Whole
        {
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

/// A vertical box blur of `reach` design px (rounded to rows) each side, claiming `says`; nothing is read from the
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

    fn inputs(&self) -> &[Input] {
        &[Input::SizeRatio]
    }

    fn footprint(&self, _params: &Value) -> Result<Footprint, bullterrier_image::Error> {
        Ok(self.says)
    }

    fn apply(
        &self,
        picture: &Picture<'_>,
        _params: Value,
        inputs: &Inputs<'_>,
    ) -> Result<Image, bullterrier_image::Error> {
        let (height, width) = (picture.height(), picture.width());
        let reach = (self.reach as f64 * inputs.size_ratio()).round() as usize;
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
        _params: Value,
        _inputs: &Inputs<'_>,
    ) -> Result<Image, bullterrier_image::Error> {
        Ok(picture.map_at(|row, _, _| [f32::from(u16::try_from(row).unwrap_or(0)) / 255.0; 3]))
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
