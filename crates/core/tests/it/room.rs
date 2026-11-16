//! A render with room (`Options::with_room`) makes the box of the picture it is handed what a render of the box alone makes: the
//! oracle is the render of the box, and the picture handed is the box grown by room whose pixels continue its edge.
//!
//! A step that places itself by its picture or reads all of it is run on the box and must match the whole box, bit for bit;
//! one that does not reads the room beside the box's edge, so only the box inside its margin is compared.

use std::error::Error;
use std::sync::Arc;

use bullterrier_image::region::Bands;
use bullterrier_image::{
    Each, Entry, Footprint, Image, Input, Layer, Options, Photo, Picture, Presets, Recipe, Step, effect,
};
use serde_json::Value;

use crate::banded::{FFT_BLURS, TOLERANCE};
use crate::common::compare::first_difference_within;
use crate::common::pictures::{asset, varied};
use crate::common::sweep::{Setting, variants};
use crate::common::{Outcome, core_effects};

/// The box: a tall, narrow picture of odd sides, as in `banded.rs`.
const HEIGHT: usize = 61;
const WIDTH: usize = 41;
/// The room: top, right, bottom, left, all different.
const ROOM: [usize; 4] = [5, 3, 7, 2];
const GROWN: (usize, usize) = (HEIGHT + ROOM[0] + ROOM[2], WIDTH + ROOM[1] + ROOM[3]);

/// `values`, a `HEIGHT` x `WIDTH` picture, grown by `ROOM` with its edge pixels repeated.
fn grown(values: &[f32]) -> Vec<f32> {
    let (height, width) = GROWN;
    (0..height)
        .flat_map(|row| {
            let from = row.saturating_sub(ROOM[0]).min(HEIGHT - 1);
            (0..width).flat_map(move |column| {
                let at = (from * WIDTH + column.saturating_sub(ROOM[3]).min(WIDTH - 1)) * 3;
                values[at..at + 3].to_vec()
            })
        })
        .collect()
}

fn with_room(options: Options) -> Result<Options, Box<dyn Error>> {
    let [top, right, bottom, left] = ROOM.map(u32::try_from);
    Ok(options.with_room(top?, right?, bottom?, left?))
}

fn photo(values: &[f32], height: usize, width: usize) -> Result<Photo<'_>, Box<dyn Error>> {
    Ok(Photo::new(Picture::new(height, width, values)?).with_image("asset", Arc::new(asset())))
}

fn step_of(name: &str, setting: &Setting) -> Step {
    let mut step = Step::new(name);
    step.params.extend(setting.clone());
    step
}

/// The first value of the box of `made` (the grown render) that is not in `plain`, to `within`, ignoring the `inset` rows and
/// columns along the box's edge.
fn first_difference(made: &Image, plain: &Image, inset: usize, within: f32) -> Option<String> {
    if 2 * inset >= WIDTH.min(HEIGHT) {
        return None; // the margin reaches across the box: no pixel of it is clear of the room
    }
    let (made, plain) = (made.clone().into_values(), plain.clone().into_values());
    for row in inset..HEIGHT.saturating_sub(inset) {
        let want = &plain[(row * WIDTH + inset) * 3..(row * WIDTH + WIDTH - inset) * 3];
        let start = ((row + ROOM[0]) * GROWN.1 + ROOM[3] + inset) * 3;
        let got = &made[start..start + want.len()];
        if let Some(at) = first_difference_within(got, want, within) {
            return Some(format!("row {row}, value {at} differs"));
        }
    }
    None
}

#[test]
fn every_effect_renders_the_box_of_a_grown_picture_as_it_renders_the_box_alone() -> Outcome {
    let values = varied(HEIGHT, WIDTH);
    let grown_values = grown(&values);
    let (mut problems, mut cases, mut boxed) = (Vec::new(), 0, 0);
    let mut unbounded = std::collections::BTreeSet::new();
    for name in core_effects()? {
        let one = effect(&name)?;
        if one.inputs().contains(&Input::Partition) || !one.fields().is_empty() {
            continue; // they name a region: the chain tests below
        }
        let table = one.params();
        for setting in variants(&table) {
            let Ok(completed) = table.check(&name, &setting) else {
                continue; // a combination the effect refuses
            };
            let recipe = Recipe::new([step_of(&name, &setting)]);
            let Ok(plain) = photo(&values, HEIGHT, WIDTH)?.render(&recipe, &Presets::new(), Options::default()) else {
                continue; // a double that other tests register, which this picture does not suit
            };
            let made = photo(&grown_values, GROWN.0, GROWN.1)?.render(
                &recipe,
                &Presets::new(),
                with_room(Options::default())?,
            )?;
            // a step that is run on the box is the same bit for bit; the others are read at the box's edge from the room
            let (inset, within) = match one.footprint(&Value::Object(completed.clone()))? {
                _ if one.inputs().contains(&Input::Place) => (0, 0.0),
                Footprint::Whole => (0, 0.0),
                footprint => {
                    // it reads sideways as far as it paints (its spread), the rows it reads as far as its margin; an effect
                    // that paints without a bound reads the room's edge anywhere, and only its rows are compared
                    let margin = footprint.margin().ok_or("a footprint that is not whole has a margin")?;
                    let Some(sides) = one.spread(&Value::Object(completed.clone()))?.pixels() else {
                        unbounded.insert(name.clone());
                        continue;
                    };
                    (
                        margin.max(sides.into_iter().max().unwrap_or(0)),
                        if FFT_BLURS.contains(&name.as_str()) {
                            TOLERANCE
                        } else {
                            0.0
                        },
                    )
                }
            };
            boxed += usize::from(inset == 0);
            cases += 1;
            if let Some(found) = first_difference(&made, &plain, inset, within) {
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
    // the effects that read the room at the box's edge and say nothing of how far: their spread is not worked out, so no pixel
    // of the box is clear of the room. One more is a new one to look at.
    assert_eq!(
        unbounded.iter().map(String::as_str).collect::<Vec<_>>(),
        ["anamorphic_streaks"]
    );
    assert!(cases > 500 && boxed > 200, "only {cases} cases, {boxed} on the box");
    Ok(())
}

/// A map of the box (`HEIGHT` x `WIDTH`) and the same map of the grown picture, its edge repeated.
fn maps_of() -> Result<[(Layer, Layer); 2], Box<dyn Error>> {
    let blocks: Vec<f32> = (0..HEIGHT * WIDTH)
        .map(|at| f32::from(u8::from((at / WIDTH / 9 + at % WIDTH / 5).is_multiple_of(2))))
        .collect();
    let depth: Vec<f32> = (0..HEIGHT * WIDTH)
        .map(|at| f32::from(u16::try_from(at * 7919 % 1000).unwrap_or(0)) / 1000.0)
        .collect();
    let spread = |map: &[f32]| -> Result<(Layer, Layer), Box<dyn Error>> {
        let around: Vec<f32> = (0..GROWN.0)
            .flat_map(|row| {
                let from = row.saturating_sub(ROOM[0]).min(HEIGHT - 1);
                (0..GROWN.1).map(move |column| map[from * WIDTH + column.saturating_sub(ROOM[3]).min(WIDTH - 1)])
            })
            .collect();
        Ok((
            Layer::new(HEIGHT, WIDTH, map.to_vec())?,
            Layer::new(GROWN.0, GROWN.1, around)?,
        ))
    };
    Ok([spread(&blocks)?, spread(&depth)?])
}

fn mapped(
    values: &[f32],
    height: usize,
    width: usize,
    subject: Layer,
    depth: Layer,
) -> Result<Photo<'_>, Box<dyn Error>> {
    Ok(photo(values, height, width)?
        .with_map("subject", subject)?
        .with_map("depth", depth)?)
}

fn chain() -> Recipe {
    let blur = |radius: f64| Step::new("gaussian_blur").param("radius", radius);
    Recipe::new([
        Entry::from(Step::new("grade").param("exposure", 0.4).mask("subject")),
        Entry::from(Step::new("vignette")),
        Entry::from(blur(3.0).mask("depth").opacity(0.8)),
        Entry::from(Step::new("grain").param("size", 2.0)),
        Entry::from(Each::new(
            "cells",
            [Some(blur(2.0)), Some(Step::new("grade").param("exposure", -0.3))],
        )),
        Entry::from(Step::new("auto_levels")),
        Entry::from(Step::new("sharpen").param("radius", 2.0)),
    ])
    .with_region(
        "cells",
        Bands {
            of: "depth".to_owned(),
            count: 3,
        },
    )
}

#[test]
fn a_chain_with_masks_pieces_and_whole_picture_steps_renders_the_box_as_the_box_alone() -> Outcome {
    let values = varied(HEIGHT, WIDTH);
    let grown_values = grown(&values);
    let [(subject, subject_grown), (depth, depth_grown)] = maps_of()?;
    let recipe = chain();
    let plain = mapped(&values, HEIGHT, WIDTH, subject, depth)?.render(&recipe, &Presets::new(), Options::default())?;
    let photo = mapped(&grown_values, GROWN.0, GROWN.1, subject_grown, depth_grown)?;
    let made = photo.render(&recipe, &Presets::new(), with_room(Options::default())?)?;
    // the blur and the sharpening read the room: only the box well inside is the box's
    let reach = 10;
    assert!(2 * reach < WIDTH.min(HEIGHT), "the box is wider than the reach");
    assert_eq!(first_difference(&made, &plain, reach, TOLERANCE), None);
    // a render of its bands is the render of the whole
    let rows = std::num::NonZeroUsize::new(7).ok_or("rows")?;
    let banded = photo.render(&recipe, &Presets::new(), with_room(Options::default().with_band(rows))?)?;
    assert_eq!(
        first_difference_within(&banded.into_values(), &made.into_values(), TOLERANCE),
        None
    );
    Ok(())
}

fn chain_without_maps() -> Recipe {
    Recipe::new([
        Step::new("gaussian_blur").param("radius", 3.0),
        Step::new("vignette"),
        Step::new("grade").param("exposure", 0.4),
        Step::new("grain").param("size", 2.0),
        Step::new("sharpen").param("radius", 2.0),
        Step::new("scanlines"),
    ])
}

#[test]
fn windows_of_a_picture_with_room_are_the_rows_of_the_render_with_room() -> Outcome {
    let grown_values = grown(&varied(HEIGHT, WIDTH));
    let (height, width) = GROWN;
    let recipe = chain_without_maps();
    let margin = recipe
        .band_margin(&Presets::new(), u32::try_from(HEIGHT.max(WIDTH))?)?
        .ok_or("these steps are banded")?;
    let whole = photo(&grown_values, height, width)?
        .render(&recipe, &Presets::new(), with_room(Options::default())?)?
        .into_values();
    for rows in [1, 7, 13, 30] {
        for top in (0..height).step_by(rows) {
            let (bottom, from) = ((top + rows).min(height), top.saturating_sub(margin));
            let to = (bottom + margin).min(height);
            let options = with_room(Options::default())?.with_window(
                u32::try_from(from)?,
                u32::try_from(width)?,
                u32::try_from(height)?,
            );
            let band = photo(&grown_values[from * width * 3..to * width * 3], to - from, width)?
                .render(&recipe, &Presets::new(), options)?
                .into_values();
            let made = &band[(top - from) * width * 3..(bottom - from) * width * 3];
            let want = &whole[top * width * 3..bottom * width * 3];
            assert_eq!(
                first_difference_within(made, want, TOLERANCE),
                None,
                "band of {rows} rows from row {top}"
            );
        }
    }
    Ok(())
}

#[test]
fn a_step_that_places_itself_leaves_the_room_as_it_received_it() -> Outcome {
    let grown_values = grown(&varied(HEIGHT, WIDTH));
    let recipe = Recipe::new([Step::new("vignette").param("amount", 1.0)]);
    let made = photo(&grown_values, GROWN.0, GROWN.1)?
        .render(&recipe, &Presets::new(), with_room(Options::default())?)?
        .into_values();
    let (height, width) = GROWN;
    let in_box = |row: usize, column: usize| {
        (ROOM[0]..height - ROOM[2]).contains(&row) && (ROOM[3]..width - ROOM[1]).contains(&column)
    };
    let (mut room, mut changed) = (0, 0);
    for row in 0..height {
        for column in 0..width {
            let at = (row * width + column) * 3;
            let same = made[at..at + 3] == grown_values[at..at + 3];
            if in_box(row, column) {
                changed += usize::from(!same);
            } else {
                assert!(same, "room pixel at row {row}, column {column} changed");
                room += 1;
            }
        }
    }
    assert!(
        room > 0 && changed > 0,
        "{room} room pixels, {changed} changed in the box"
    );
    Ok(())
}

#[test]
fn room_that_leaves_no_box_or_a_recipe_that_changes_the_picture_is_refused_naming_the_room() -> Outcome {
    let grown_values = grown(&varied(HEIGHT, WIDTH));
    let (height, width) = (u32::try_from(GROWN.0)?, u32::try_from(GROWN.1)?);
    let mut sized = Recipe::new([Step::new("vignette")]);
    sized.size = Some(40);
    let plain = Recipe::new([Step::new("vignette")]);
    for (what, recipe, options) in [
        (
            "rows used up",
            &plain,
            Options::default().with_room(height / 2, 0, height - height / 2, 0),
        ),
        ("columns used up", &plain, Options::default().with_room(0, width, 0, 0)),
        ("a recipe size", &sized, with_room(Options::default())?),
        ("an options size", &plain, with_room(Options::default().with_size(40))?),
    ] {
        match photo(&grown_values, GROWN.0, GROWN.1)?.render(recipe, &Presets::new(), options) {
            Err(bullterrier_image::Error::Recipe(why)) if why.contains("`room`") => {}
            other => return Err(format!("{what}: {other:?}").into()),
        }
    }
    Ok(())
}
