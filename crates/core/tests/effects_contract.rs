//! Own binary: it walks every effect in the process's registry, so effects other tests register would be walked too.
//!
//! What every effect the core runs keeps, whatever it does (its Python twin is `tests/effects/test_contract.py`, which
//! now covers only effects registered from Python): a picture of the input's shape, finite and in 0..1, the same for the
//! same parameters; every number working at its limits and anywhere inside its range, and refused beyond them; every
//! choice working and any other refused; an `amount` of 0 leaving the picture alone; a seed deciding the picture; a
//! cyclic angle giving the same picture a turn further; the same picture at any thread count; the same look at twice the
//! size.
//!
//! Each rule is a function `fn(&dyn Effect, &Table) -> Vec<String>` of problems, run on every effect by one test and on
//! a probe effect made to break it by another, so a checker that passes everything fails here.
//! An effect cannot change its input (`Picture` borrows it) and cannot return a NaN (`Image::new` refuses it), so those
//! rules of the Python contract have no test here.

#![expect(
    clippy::cast_possible_truncation,
    reason = "whole-number limits and the f32 pictures are narrowed from small f64 values"
)]

#[path = "it/common/mod.rs"]
mod common;

use std::error::Error;
use std::sync::Arc;

use bullterrier_image::params::{Field, Kind, Scalar, Table};
use bullterrier_image::{
    Category, Constraint, Effect, Image, Inputs, Layer, Options, Photo, Picture, Presets, Recipe, Step, effect, effects,
};
use common::compare::first_bit_difference;
use serde_json::{Map, Value, json};

type Outcome = Result<(), Box<dyn Error>>;
type Problems = Result<Vec<String>, Box<dyn Error>>;
/// A rule: the problems an effect with these parameters has with it.
type Check = fn(&dyn Effect, &Table) -> Problems;
/// An effect of the core and its parameters.
type Core = (Arc<dyn Effect>, Table);

/// A value outside 0..1 by more than this is a range violation: the effect contract clamps, so rounding stays inside.
const RANGE_SLACK: f32 = 1e-6;
/// Two pictures that must be the same are the same within this.
const SAME: f32 = 1e-6;
/// Pictures that must differ differ by more than this somewhere.
const DIFFERENT: f32 = 1e-3;

type Setting = Map<String, Value>;

fn setting(pairs: &[(&str, Value)]) -> Setting {
    pairs
        .iter()
        .map(|(key, value)| ((*key).to_owned(), value.clone()))
        .collect()
}

/// The effects the core runs, each with its parameters: every effect of this binary's registry that describes some.
fn core_effects() -> Result<Vec<Core>, Box<dyn Error>> {
    let mut found = Vec::new();
    for name in effects() {
        let one = effect(&name)?;
        if let Some(table) = one.params() {
            found.push((one, table));
        }
    }
    assert!(found.len() >= 62, "only {} core effects are registered", found.len());
    Ok(found)
}

/// Run `check` on every core effect; the test fails listing every problem.
fn every_effect(check: impl Fn(&dyn Effect, &Table) -> Problems) -> Outcome {
    let mut problems = Vec::new();
    for (one, table) in core_effects()? {
        problems.extend(check(&*one, &table)?);
    }
    assert!(
        problems.is_empty(),
        "{} problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
    Ok(())
}

/// `count` values of 0..1 from a hash of their place and `seed`: no two channels move together.
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

/// A picture of `height` x `width`: colourful noise, a different value in every channel.
fn noise(height: usize, width: usize) -> Vec<f32> {
    hashed(height * width * 3, 3)
}

/// The map every field parameter is given: a cone, 1 at the centre falling to 0 at the corners.
fn cone(height: usize, width: usize) -> Result<Layer, Box<dyn Error>> {
    let (cy, cx) = (
        f32::from(u16::try_from(height)?) / 2.0,
        f32::from(u16::try_from(width)?) / 2.0,
    );
    let far = cy.hypot(cx);
    let values = (0..height)
        .flat_map(|row| (0..width).map(move |column| (row, column)))
        .map(|(row, column)| {
            let (y, x) = (
                f32::from(u16::try_from(row).unwrap_or(0)),
                f32::from(u16::try_from(column).unwrap_or(0)),
            );
            1.0 - ((y - cy).hypot(x - cx) / far).min(1.0)
        })
        .collect();
    Ok(Layer::new(height, width, values)?)
}

/// `effect` applied with `given` (completed and checked by the table) to the `height` x `width` picture `values`.
fn run(
    one: &dyn Effect,
    table: &Table,
    given: &Setting,
    height: usize,
    width: usize,
    values: &[f32],
) -> Result<Vec<f32>, Box<dyn Error>> {
    let params = table.check(one.name(), given)?;
    let layer = cone(height, width)?;
    let mut inputs = Inputs::default();
    for field in table.fields.iter().filter(|field| field.kind == Kind::Field) {
        inputs = inputs.with_field(field.name, &layer);
    }
    let image = one.apply(&Picture::new(height, width, values)?, Value::Object(params), &inputs)?;
    if (image.height(), image.width()) != (height, width) {
        return Err(format!("{} changed the shape of a {height} x {width} picture", one.name()).into());
    }
    Ok(image.into_values())
}

/// `run` on the 24 x 32 noise picture: quick, even with every parameter at its maximum.
fn small(one: &dyn Effect, table: &Table, given: &Setting) -> Result<Vec<f32>, Box<dyn Error>> {
    run(one, table, given, 24, 32, &noise(24, 32))
}

fn real(bound: Option<Scalar>) -> Option<f64> {
    match bound? {
        Scalar::Int(number) => i32::try_from(number).ok().map(f64::from),
        Scalar::Float(number) => Some(number),
        _ => None,
    }
}

/// The numeric fields with both limits: (field, low, high).
fn numbers(table: &Table) -> Vec<(&Field, f64, f64)> {
    table
        .fields
        .iter()
        .filter(|field| matches!(field.kind, Kind::Float | Kind::Int))
        .filter_map(|field| Some((field, real(field.min)?, real(field.max)?)))
        .collect()
}

fn number(field: &Field, value: f64) -> Value {
    if field.kind == Kind::Int {
        json!(value.round() as i64)
    } else {
        json!(value)
    }
}

/// `field` set to `value`. When it is one side of a min/max pair (`band_min_height` and `band_max_height`), the other
/// side moves to the same value if its default would leave the pair the wrong way round, so each side reaches its range.
fn paired(table: &Table, field: &Field, value: Value) -> Setting {
    let words: Vec<&str> = field.name.split('_').collect();
    for (side, other_side) in [("min", "max"), ("max", "min")] {
        let other = words
            .iter()
            .map(|word| if *word == side { other_side } else { word })
            .collect::<Vec<_>>()
            .join("_");
        let partner = table.fields.iter().find(|candidate| candidate.name == other);
        if let (true, Some(partner)) = (words.contains(&side), partner) {
            let (now, theirs) = (
                value.as_f64().unwrap_or(0.0),
                real(Some(partner.default)).unwrap_or(0.0),
            );
            if (side == "min" && now > theirs) || (side == "max" && now < theirs) {
                return setting(&[(field.name, value.clone()), (partner.name, value)]);
            }
        }
    }
    setting(&[(field.name, value)])
}

fn choice_fields(table: &Table) -> impl Iterator<Item = &Field> {
    table.fields.iter().filter(|field| field.kind == Kind::Choice)
}

/// Why `values` is no picture of `length` values in 0..1, if it is none.
fn picture_fault(values: &[f32], length: usize) -> Option<String> {
    if values.len() != length {
        return Some(format!("{} values, not {length}", values.len()));
    }
    let at = values
        .iter()
        .position(|value| !value.is_finite() || *value < -RANGE_SLACK || *value > 1.0 + RANGE_SLACK)?;
    Some(format!("value {at} is {}", values[at]))
}

fn differs(left: &[f32], right: &[f32], by: f32) -> bool {
    left.iter().zip(right).any(|(one, other)| (one - other).abs() > by)
}

// --- a picture, the same for the same parameters, at any size ---------------------------------------------------------

const SIZES: [(usize, usize); 6] = [(1, 1), (1, 7), (7, 1), (3, 5), (13, 29), (61, 37)];

fn picture_problems(one: &dyn Effect, table: &Table) -> Problems {
    let mut problems = Vec::new();
    for (height, width) in SIZES {
        let values = noise(height, width);
        let first = run(one, table, &Setting::new(), height, width, &values)?;
        let second = run(one, table, &Setting::new(), height, width, &values)?;
        if let Some(fault) = picture_fault(&first, values.len()) {
            problems.push(format!("{} at {height}x{width}: {fault}", one.name()));
        }
        if let Some(at) = first_bit_difference(&first, &second) {
            problems.push(format!(
                "{} at {height}x{width}: two runs differ at value {at}",
                one.name()
            ));
        }
    }
    Ok(problems)
}

#[test]
fn every_effect_makes_a_finite_picture_of_the_input_shape_in_range_the_same_twice_at_any_size() -> Outcome {
    every_effect(picture_problems)
}

// --- the numbers ------------------------------------------------------------------------------------------------------

fn range_problems(one: &dyn Effect, table: &Table) -> Problems {
    let mut problems = Vec::new();
    for (field, low, high) in numbers(table) {
        let mut values = vec![low, high];
        values.extend((1..6).map(|step| low + (high - low) * f64::from(step) / 6.0));
        if field.kind == Kind::Int {
            values = values.iter().map(|value| value.round()).collect();
            values.dedup_by(|a, b| a.to_bits() == b.to_bits());
        }
        for value in values {
            let given = paired(table, field, number(field, value));
            let out = small(one, table, &given)?;
            if let Some(fault) = picture_fault(&out, 24 * 32 * 3) {
                problems.push(format!("{}.{} = {value}: {fault}", one.name(), field.name));
            }
        }
    }
    Ok(problems)
}

#[test]
fn every_effect_works_with_each_number_at_its_limits_and_inside_its_range() -> Outcome {
    every_effect(range_problems)
}

/// The refusal of `field` set to `value` by `table`: whether it names the field.
fn refused(table: &Table, one: &str, field: &str, value: Value) -> bool {
    matches!(
        table.check(one, &setting(&[(field, value)])),
        Err(bullterrier_image::Error::Invalid { field: named, .. }) if named == field
    )
}

#[test]
fn every_number_one_step_beyond_its_limit_is_refused_naming_the_field() -> Outcome {
    every_effect(|one, table| {
        let mut problems = Vec::new();
        for (field, low, high) in numbers(table).into_iter().filter(|(field, ..)| !field.cyclic) {
            let (below, above) = if field.kind == Kind::Int {
                (json!(low as i64 - 1), json!(high as i64 + 1))
            } else {
                (json!(low.next_down()), json!(high.next_up()))
            };
            for (side, value) in [("below", below), ("above", above)] {
                if !refused(table, one.name(), field.name, value.clone()) {
                    problems.push(format!(
                        "{}.{} {side} its limit ({value}) is not refused",
                        one.name(),
                        field.name
                    ));
                }
            }
        }
        Ok(problems)
    })
}

// --- the choices and the names ----------------------------------------------------------------------------------------

#[test]
fn every_choice_works_and_any_other_name_or_parameter_is_refused() -> Outcome {
    every_effect(|one, table| {
        let mut problems = Vec::new();
        for field in choice_fields(table) {
            for choice in field.choices {
                let out = small(one, table, &setting(&[(field.name, json!(choice))]))?;
                if let Some(fault) = picture_fault(&out, 24 * 32 * 3) {
                    problems.push(format!("{}.{} = {choice}: {fault}", one.name(), field.name));
                }
            }
            if !refused(table, one.name(), field.name, json!("not_a_choice")) {
                problems.push(format!("{}.{} takes a name it does not offer", one.name(), field.name));
            }
        }
        let unknown = table.check(one.name(), &setting(&[("not_a_parameter", json!(1))]));
        if !matches!(
            unknown,
            Err(bullterrier_image::Error::Invalid {
                constraint: Constraint::Unknown(_),
                ..
            })
        ) {
            problems.push(format!("{} takes a parameter it does not declare", one.name()));
        }
        Ok(problems)
    })
}

// --- amount -----------------------------------------------------------------------------------------------------------

/// What an effect needs besides `amount` 0 for `amount` to be the identity: `tone_map`'s `amount` is the detail on top of
/// its contrast curve.
fn amount_zero_needs(effect: &str) -> Setting {
    if effect == "tone_map" {
        setting(&[("contrast", json!(1.0))])
    } else {
        Setting::new()
    }
}

fn amount_problems(one: &dyn Effect, table: &Table) -> Problems {
    let can_be_zero = numbers(table)
        .iter()
        .any(|(field, low, _)| field.name == "amount" && *low == 0.0);
    if !can_be_zero {
        return Ok(Vec::new());
    }
    let values = noise(24, 32);
    // the defaults, then each choice of each choice field: a sampling or edge rule must not change what nothing moves in
    let mut cases = vec![Setting::new()];
    cases.extend(choice_fields(table).flat_map(|field| {
        field
            .choices
            .iter()
            .map(|choice| setting(&[(field.name, json!(choice))]))
    }));
    let mut problems = Vec::new();
    for mut case in cases {
        case.insert("amount".to_owned(), json!(0.0));
        case.extend(amount_zero_needs(one.name()));
        let out = run(one, table, &case, 24, 32, &values)?;
        if differs(&out, &values, SAME) {
            problems.push(format!("{} with amount 0 and {case:?} changes the picture", one.name()));
        }
    }
    Ok(problems)
}

#[test]
fn an_amount_of_zero_leaves_the_picture_as_it_is() -> Outcome {
    every_effect(amount_problems)
}

// --- reading between pixels -------------------------------------------------------------------------------------------

/// Every combination of the choices of `fields`.
fn combinations(fields: &[&Field]) -> Vec<Setting> {
    fields.iter().fold(vec![Setting::new()], |so_far, field| {
        so_far
            .iter()
            .flat_map(|chosen| {
                field.choices.iter().map(move |choice| {
                    let mut next = chosen.clone();
                    next.insert(field.name.to_owned(), json!(choice));
                    next
                })
            })
            .collect()
    })
}

fn sampling_problems(one: &dyn Effect, table: &Table) -> Problems {
    let Some(interpolation) = choice_fields(table).find(|field| field.name == "interpolation") else {
        return Ok(Vec::new());
    };
    let choices: Vec<&Field> = choice_fields(table).collect();
    let mut problems = Vec::new();
    let flat = vec![0.4_f32; 31 * 47 * 3];
    for case in combinations(&choices) {
        let out = run(one, table, &case, 31, 47, &flat)?;
        if differs(&out, &flat, SAME) {
            problems.push(format!(
                "{} with {case:?} turns a constant picture into another",
                one.name()
            ));
        }
    }
    let values = noise(41, 41);
    let mut outs = Vec::new();
    for choice in interpolation.choices {
        outs.push((
            choice,
            run(
                one,
                table,
                &setting(&[("interpolation", json!(choice))]),
                41,
                41,
                &values,
            )?,
        ));
    }
    for (index, (first, one_out)) in outs.iter().enumerate() {
        for (second, other_out) in &outs[index + 1..] {
            if !differs(one_out, other_out, 0.0) {
                problems.push(format!("{}: {first} and {second} give the same picture", one.name()));
            }
        }
    }
    Ok(problems)
}

#[test]
fn a_constant_picture_stays_constant_and_each_interpolation_reads_its_own_way() -> Outcome {
    every_effect(sampling_problems)
}

// --- the seed ---------------------------------------------------------------------------------------------------------

/// What an effect needs for its seed to show: at its defaults nothing is random yet.
fn seed_needs(effect: &str) -> Setting {
    if effect == "starburst" {
        setting(&[("spacing", json!(3.0))])
    } else {
        Setting::new()
    }
}

fn seed_problems(one: &dyn Effect, table: &Table) -> Problems {
    if !table.fields.iter().any(|field| field.name == "seed") {
        return Ok(Vec::new());
    }
    let values = noise(24, 32);
    let with = |seed: i64| {
        let mut given = seed_needs(one.name());
        given.insert("seed".to_owned(), json!(seed));
        run(one, table, &given, 24, 32, &values)
    };
    let (first, again, other) = (with(1)?, with(1)?, with(2)?);
    let mut problems = Vec::new();
    if first_bit_difference(&first, &again).is_some() {
        problems.push(format!("{}: the same seed gives two pictures", one.name()));
    }
    if !differs(&first, &other, 0.0) {
        problems.push(format!("{}: seeds 1 and 2 give the same picture", one.name()));
    }
    Ok(problems)
}

#[test]
fn the_same_seed_gives_the_same_picture_and_another_seed_another() -> Outcome {
    every_effect(seed_problems)
}

// --- cyclic angles ----------------------------------------------------------------------------------------------------

/// What an effect needs for its angle to show: at its defaults it changes nothing, so any angle would look cyclic.
fn cyclic_needs(effect: &str, field: &str) -> Setting {
    if (effect, field) == ("selective_color", "hue") {
        setting(&[("hue_shift", json!(90.0)), ("width", json!(120.0))])
    } else {
        Setting::new()
    }
}

fn cyclic_problems(one: &dyn Effect, table: &Table) -> Problems {
    let mut problems = Vec::new();
    let values = noise(24, 32);
    for (field, low, high) in numbers(table).into_iter().filter(|(field, ..)| field.cyclic) {
        let at = |angle: f64| {
            let mut given = cyclic_needs(one.name(), field.name);
            given.insert(field.name.to_owned(), json!(angle));
            run(one, table, &given, 24, 32, &values)
        };
        let (at_low, at_high, at_45) = (at(low)?, at(high)?, at(low + 45.0)?);
        let name = format!("{}.{}", one.name(), field.name);
        if !differs(&at_45, &at_low, DIFFERENT) {
            problems.push(format!("{name} shows nothing: the check would pass for any field"));
        }
        if differs(&at_low, &at_high, DIFFERENT) {
            problems.push(format!("{name}: min and max are a turn apart but the pictures differ"));
        }
        // four turns and a degree past the bottom of the turn is the same direction as one degree past it
        if first_bit_difference(&at(low + 4.0 * (high - low) + 1.0)?, &at(low + 1.0)?).is_some() {
            problems.push(format!(
                "{name}: a value past the turn is not the direction it wraps to"
            ));
        }
    }
    Ok(problems)
}

#[test]
fn a_cyclic_angle_gives_the_same_picture_a_turn_further_and_takes_any_real() -> Outcome {
    every_effect(cyclic_problems)
}

// --- threads and size, through a render -------------------------------------------------------------------------------

fn photo(picture: Picture<'_>, height: usize, width: usize) -> Result<Photo<'_>, Box<dyn Error>> {
    Ok(Photo::new(picture)
        .with_map("depth", cone(height, width)?)?
        .with_map("subject", cone(height, width)?)?)
}

fn render(photo: &Photo<'_>, name: &str, options: Options) -> Result<Image, Box<dyn Error>> {
    Ok(photo.render(&Recipe::new([Step::new(name)]), &Presets::new(), options)?)
}

#[test]
fn every_effect_gives_the_same_bytes_at_one_and_at_three_threads() -> Outcome {
    let values = noise(61, 37);
    let photo = photo(Picture::new(61, 37, &values)?, 61, 37)?;
    let mut problems = Vec::new();
    for (one, _) in core_effects()? {
        let single = render(&photo, one.name(), Options::default())?.into_values();
        let three = render(&photo, one.name(), Options::default().with_threads(3))?.into_values();
        if let Some(at) = first_bit_difference(&single, &three) {
            problems.push(format!("{}: threads 1 and 3 differ at value {at}", one.name()));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    Ok(())
}

/// A `rows` x `columns` grid of noise stretched to `height` x `width` by bilinear interpolation: detail that stays
/// smooth at any size.
fn stretched(rows: u16, columns: u16, height: u16, width: u16) -> Vec<f32> {
    let grid = hashed(usize::from(rows) * usize::from(columns) * 3, 7);
    let at = |row: u16, column: u16, channel: usize| {
        grid[(usize::from(row) * usize::from(columns) + usize::from(column)) * 3 + channel]
    };
    let mut values = Vec::new();
    for y in 0..height {
        let fy = f32::from(y) * f32::from(rows - 1) / f32::from(height - 1);
        let (row, ty) = (fy.floor(), fy.fract());
        let row = u16::try_from(row as i64).unwrap_or(0).min(rows - 2);
        for x in 0..width {
            let fx = f32::from(x) * f32::from(columns - 1) / f32::from(width - 1);
            let column = u16::try_from(fx.floor() as i64).unwrap_or(0).min(columns - 2);
            let tx = fx - f32::from(column);
            for channel in 0..3 {
                let top = at(row, column, channel) * (1.0 - tx) + at(row, column + 1, channel) * tx;
                let bottom = at(row + 1, column, channel) * (1.0 - tx) + at(row + 1, column + 1, channel) * tx;
                values.push(top * (1.0 - ty) + bottom * ty);
            }
        }
    }
    values
}

/// The design picture: smooth colour with some fine detail and a few bright highlights, 720 x 960 (rendered at 480 it
/// is shrunk to half).
fn design_values() -> Vec<f32> {
    let (height, width) = (720_u16, 960_u16);
    let fine = stretched(72, 96, height, width);
    let blobs = stretched(9, 12, height, width);
    let places = hashed(24 * 2, 11);
    let mut values = Vec::with_capacity(fine.len());
    for row in 0..height {
        for column in 0..width {
            let (y, x) = (
                f32::from(row) / f32::from(height - 1),
                f32::from(column) / f32::from(width - 1),
            );
            let smooth = [x, y, f32::midpoint(x, y)];
            let light: f32 = places
                .chunks(2)
                .map(|place| {
                    let (dy, dx) = (
                        f32::from(row) - place[0] * f32::from(height),
                        f32::from(column) - place[1] * f32::from(width),
                    );
                    2.0 * (-(dy * dy + dx * dx) / (2.0 * 14.0 * 14.0)).exp()
                })
                .sum();
            let at = (usize::from(row) * usize::from(width) + usize::from(column)) * 3;
            for channel in 0..3 {
                values.push(
                    (smooth[channel] * 0.5 + blobs[at + channel] * 0.35 + fine[at + channel] * 0.15 + light)
                        .clamp(0.0, 1.0),
                );
            }
        }
    }
    values
}

/// The mean of each channel of `values` (row after row, three channels a pixel).
fn channel_means(values: &[f32]) -> [f32; 3] {
    let mut sums = [0.0_f64; 3];
    for (index, value) in values.iter().enumerate() {
        sums[index % 3] += f64::from(*value);
    }
    let count = f64::from(u32::try_from(values.len() / 3).unwrap_or(1));
    sums.map(|sum| (sum / count) as f32)
}

/// The mean of |output - input| of each channel.
fn change_means(out: &[f32], input: &[f32]) -> [f32; 3] {
    let change: Vec<f32> = out.iter().zip(input).map(|(a, b)| (a - b).abs()).collect();
    channel_means(&change)
}

#[test]
fn every_effect_at_its_defaults_looks_the_same_at_its_design_size_and_at_twice_it() -> Outcome {
    let values = design_values();
    let photo = photo(Picture::new(720, 960, &values)?, 720, 960)?;
    let untouched = Recipe::new([Step::new("grade").opacity(0.0)]);
    let at = |recipe: &Recipe, size: u32| -> Result<Vec<f32>, Box<dyn Error>> {
        Ok(photo
            .render(
                recipe,
                &Presets::new(),
                Options::default().with_threads(0).with_size(size),
            )?
            .into_values())
    };
    let (plain_small, plain_large) = (at(&untouched, 480)?, at(&untouched, 960)?);
    let mut problems = Vec::new();
    for (one, _) in core_effects()? {
        let look = Recipe::new([Step::new(one.name())]);
        let (small, large) = (at(&look, 480)?, at(&look, 960)?);
        // the 960 render shrunk back to 480 by averaging each 2 x 2 block: its mean colour against the 480 render's
        let (h, w) = (360_usize, 480_usize);
        let shrunk: Vec<f32> = (0..h * w * 3)
            .map(|index| {
                let (pixel, channel) = (index / 3, index % 3);
                let (row, column) = (pixel / w, pixel % w);
                let at = |dy: usize, dx: usize| large[((row * 2 + dy) * 2 * w + column * 2 + dx) * 3 + channel];
                (at(0, 0) + at(0, 1) + at(1, 0) + at(1, 1)) / 4.0
            })
            .collect();
        let drift = channel_means(&shrunk)
            .iter()
            .zip(channel_means(&small))
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        if drift > 0.02 {
            problems.push(format!(
                "{}: the mean colour drifts by {drift} between the two sizes",
                one.name()
            ));
        }
        // the mean change must agree within 20% of its value at the design size (or 0.001)
        let (design, twice) = (change_means(&small, &plain_small), change_means(&large, &plain_large));
        if design
            .iter()
            .zip(twice)
            .any(|(d, t)| (t - d).abs() > 0.001_f32.max(0.2 * d))
        {
            problems.push(format!(
                "{}: it changes {twice:?} at twice the size, {design:?} at the design size",
                one.name()
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    Ok(())
}

// --- the checkers catch what they should ------------------------------------------------------------------------------

/// How a probe effect breaks a rule.
#[derive(Clone, Copy, PartialEq)]
enum Fault {
    None,
    OutOfRange,
    /// A different picture each call.
    Unseeded,
    IgnoresSeed,
    /// The angle is not wrapped, so a turn further is another picture.
    RawAngle,
    /// `amount` 0 still brightens.
    AlwaysBrightens,
    WrongShape,
}

struct Probe {
    fault: Fault,
    calls: std::sync::atomic::AtomicUsize,
}

impl Probe {
    fn new(fault: Fault) -> Self {
        Self {
            fault,
            calls: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

fn field(name: &'static str, kind: Kind, default: Scalar, min: Scalar, max: Scalar, cyclic: bool) -> Field {
    Field {
        name,
        kind,
        meaning: "A probe's parameter.",
        default,
        unit: None,
        min: Some(min),
        max: Some(max),
        cyclic,
        choices: &[],
        min_length: None,
        max_length: None,
        at_most: None,
        nullable: false,
        drivable: false,
        advanced: false,
    }
}

fn probe_table() -> Table {
    Table {
        doc: "A probe.",
        fields: vec![
            field(
                "seed",
                Kind::Int,
                Scalar::Int(0),
                Scalar::Int(0),
                Scalar::Int(1000),
                false,
            ),
            field(
                "angle",
                Kind::Float,
                Scalar::Float(0.0),
                Scalar::Float(0.0),
                Scalar::Float(360.0),
                true,
            ),
            field(
                "amount",
                Kind::Float,
                Scalar::Float(1.0),
                Scalar::Float(0.0),
                Scalar::Float(1.0),
                false,
            ),
        ],
    }
}

impl Effect for Probe {
    fn name(&self) -> &'static str {
        "probe"
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn params(&self) -> Option<Table> {
        Some(probe_table())
    }

    fn apply(
        &self,
        picture: &Picture<'_>,
        params: Value,
        _inputs: &Inputs<'_>,
    ) -> Result<Image, bullterrier_image::Error> {
        let seed = if self.fault == Fault::IgnoresSeed {
            0.0
        } else {
            params["seed"].as_f64().unwrap_or(0.0)
        };
        let angle = params["angle"].as_f64().unwrap_or(0.0);
        let angle = if self.fault == Fault::RawAngle {
            angle
        } else {
            angle.rem_euclid(360.0)
        };
        let amount = if self.fault == Fault::AlwaysBrightens {
            1.0
        } else {
            params["amount"].as_f64().unwrap_or(0.0)
        };
        let calls = f64::from(u32::try_from(self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst)).unwrap_or(0));
        let turned = if self.fault == Fault::RawAngle {
            angle / 3600.0
        } else {
            angle.to_radians().sin() * 0.05
        };
        let shift = (amount * (seed * 0.001 + turned + 0.1)) as f32
            + if self.fault == Fault::Unseeded {
                calls as f32 * 0.01
            } else {
                0.0
            };
        let (height, width) = match self.fault {
            Fault::WrongShape => (picture.height() + 1, picture.width()),
            _ => (picture.height(), picture.width()),
        };
        let values: Vec<f32> = (0..height * width * 3)
            .map(|index| {
                let base = picture.values().get(index).copied().unwrap_or(0.0);
                match self.fault {
                    Fault::OutOfRange => base + 2.0,
                    _ => (base + shift).clamp(0.0, 1.0),
                }
            })
            .collect();
        Image::new(height, width, &values)
    }
}

fn probed(check: Check, fault: Fault) -> Result<usize, Box<dyn Error>> {
    Ok(check(&Probe::new(fault), &probe_table())?.len())
}

#[test]
fn each_checker_passes_a_sound_effect_and_flags_one_that_breaks_its_rule() -> Outcome {
    let checks: [(&str, Check, Fault); 5] = [
        ("range", picture_problems, Fault::OutOfRange),
        ("deterministic", picture_problems, Fault::Unseeded),
        ("seed", seed_problems, Fault::IgnoresSeed),
        ("cyclic", cyclic_problems, Fault::RawAngle),
        ("amount", amount_problems, Fault::AlwaysBrightens),
    ];
    for (rule, check, fault) in checks {
        assert_eq!(probed(check, Fault::None)?, 0, "{rule}: a sound effect is flagged");
        assert!(probed(check, fault)? > 0, "{rule}: the broken effect passes");
    }
    assert!(
        probed(picture_problems, Fault::WrongShape).is_err(),
        "an effect that changes the shape passes"
    );
    Ok(())
}
