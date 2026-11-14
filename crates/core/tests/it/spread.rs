//! `Effect::spread` tells the truth. An effect that says it paints at most `s` px beyond the box of its content paints no
//! pixel farther than that: the effect is run on a grey canvas with a bright block of content (and, for an effect that
//! works from a map or a partition, a map and pieces of the block), and every pixel that differs from what the effect
//! makes of an all-grey canvas is within the block's box grown by the effect's spread. The effect's own answer on grey
//! is the baseline, so an effect that only recolours (a grade that lifts black) spreads none.
//!
//! The check runs on every effect of the core, with its defaults and with each parameter set alone to its limits, to
//! each choice and to each switch position (as `footprint.rs`). An effect that says nothing is held to none, so an
//! effect that paints outward and does not say so fails here. A known-bad double proves that it does.

use std::error::Error as StdError;

use crate::common::Outcome;
use crate::common::pictures::asset;
use crate::common::sweep::{Setting, variants};
use bullterrier_image::params::Kind;
use bullterrier_image::region::{Grid, Shares};
use bullterrier_image::{
    Category, Each, Effect, Error, Image, Input, Inputs, Labels, Layer, Options, Photo, Picture, Place, Presets,
    Recipe, Spread, Step, effect,
};
use serde_json::Value;

/// The canvas is square, the block of content in its middle, so every side has the same room to paint in.
const SIDE: usize = 160;
const FROM: usize = 74;
const TO: usize = 86;
/// A pixel counts as painted when a channel differs from the baseline by more than this.
const PAINTED: f32 = 1e-3;

/// A plain mid grey, in which a dark halo (a shadow, a dark outline) shows as much as a light one.
fn background() -> Vec<f32> {
    vec![0.3; SIDE * SIDE * 3]
}

fn block() -> Vec<f32> {
    let mut canvas = background();
    for row in FROM..TO {
        for column in FROM..TO {
            let at = (row * SIDE + column) * 3;
            canvas[at..at + 3].copy_from_slice(&[1.0, 0.8, 0.6]);
        }
    }
    canvas
}

fn inside(at: usize) -> bool {
    (FROM..TO).contains(&(at / SIDE)) && (FROM..TO).contains(&(at % SIDE))
}

/// `one` run on `picture` (SIDE x SIDE) at size ratio 1, given the block as its map and as a piece if `content`, else an
/// empty map and one piece.
fn render(one: &dyn Effect, params: &Setting, picture: &[f32], content: bool) -> Result<Vec<f32>, Box<dyn StdError>> {
    let table = one.params();
    let map = Layer::new(
        SIDE,
        SIDE,
        (0..SIDE * SIDE)
            .map(|at| f32::from(u8::from(content && inside(at))))
            .collect(),
    )?;
    let pieces = Labels::new(
        SIDE,
        SIDE,
        (0..SIDE * SIDE).map(|at| i32::from(content && inside(at))).collect(),
        2,
    )?;
    let mut inputs = Inputs::default()
        .with_size_ratio(1.0)
        .with_place(Place::new(SIDE, SIDE, 0));
    for field in table.fields.iter().filter(|field| field.kind == Kind::Field) {
        inputs = inputs.with_field(field.name, &map);
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
    let picture = Picture::new(SIDE, SIDE, picture)?;
    let image: Image = one.apply(&picture, &Value::Object(params.clone()), &inputs)?;
    Ok(image.into_values())
}

/// How many pixels beyond the block's box, on each side `[top, right, bottom, left]`, the effect makes something else of
/// the block than of the plain background.
fn painted_beyond(one: &dyn Effect, params: &Setting) -> Result<[usize; 4], Box<dyn StdError>> {
    Ok(painted(
        &render(one, params, &block(), true)?,
        &render(one, params, &background(), false)?,
    ))
}

/// How many pixels beyond the block's box, on each side `[top, right, bottom, left]`, `with` differs from `without`.
fn painted(with: &[f32], without: &[f32]) -> [usize; 4] {
    let mut beyond = [0; 4];
    for at in 0..SIDE * SIDE {
        if (0..3).all(|channel| (with[at * 3 + channel] - without[at * 3 + channel]).abs() <= PAINTED) {
            continue;
        }
        let (row, column) = (at / SIDE, at % SIDE);
        beyond[0] = beyond[0].max(FROM.saturating_sub(row));
        beyond[1] = beyond[1].max(column.saturating_sub(TO - 1));
        beyond[2] = beyond[2].max(row.saturating_sub(TO - 1));
        beyond[3] = beyond[3].max(FROM.saturating_sub(column));
    }
    beyond
}

/// Every way `one` paints farther than it says, over the variants of its parameters.
#[expect(clippy::cast_precision_loss, reason = "pixel counts of a canvas of 160")]
fn problems(one: &dyn Effect) -> Result<Vec<String>, Box<dyn StdError>> {
    let table = one.params();
    let mut found = Vec::new();
    for given in variants(&table) {
        let Ok(completed) = table.check(one.name(), &given) else {
            continue; // a combination the effect refuses
        };
        let Spread::Beyond {
            top,
            right,
            bottom,
            left,
        } = one.spread(&Value::Object(completed.clone()))?
        else {
            continue; // unbounded: true of anything
        };
        let says = [top, right, bottom, left];
        if says.iter().all(|side| *side >= (FROM - 1) as f64) {
            continue; // the canvas is too small to show a painter that far
        }
        let beyond = painted_beyond(one, &completed)?;
        if beyond.iter().zip(says).any(|(painted, says)| *painted as f64 > says) {
            found.push(format!(
                "{} says {says:?} (top, right, bottom, left) but paints {beyond:?} with {given:?} (footprint {:?})",
                one.name(),
                one.footprint(&Value::Object(completed.clone()))?
            ));
        }
    }
    Ok(found)
}

/// The effects of the core, one for each file of `src/effects/` (the registry also holds the doubles that other tests of
/// this binary register for a while, which would come and go during the run).
fn core_effects() -> Result<Vec<String>, Box<dyn StdError>> {
    let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("effects");
    let mut names = Vec::new();
    for entry in std::fs::read_dir(folder)? {
        let path = entry?.path();
        if path.extension().is_some_and(|extension| extension == "rs")
            && let Some(name) = path.file_stem().and_then(|stem| stem.to_str())
        {
            names.push(name.to_owned());
        }
    }
    assert!(names.len() > 80, "only {} effect files", names.len());
    Ok(names)
}

#[test]
fn every_effect_paints_no_farther_beyond_its_content_than_it_says() -> Outcome {
    let mut found = Vec::new();
    for name in core_effects()? {
        found.extend(problems(&*effect(&name)?)?);
    }
    assert!(found.is_empty(), "{} problems:\n{}", found.len(), found.join("\n"));
    Ok(())
}

/// The spread of a recipe of `steps`, for a picture whose longest side is `longest_side` px.
fn spread_of(steps: impl IntoIterator<Item = Step>, longest_side: u32) -> Result<Spread, Error> {
    Recipe::new(steps).spread(&Presets::new(), longest_side)
}

fn beyond(top: f64, right: f64, bottom: f64, left: f64) -> Spread {
    Spread::Beyond {
        top,
        right,
        bottom,
        left,
    }
}

/// The room a single step asks for, from its parameters, by hand. At 480 px (the design size) a `px` is a pixel.
#[test]
fn a_step_spreads_as_far_as_its_parameters_reach() -> Outcome {
    let sin_45 = std::f64::consts::FRAC_1_SQRT_2;
    let cases = [
        // a Gaussian reaches radius x truncate (4), and two pixels more are a bicubic window's
        (
            "bloom",
            Step::new("bloom").param("radius", 10.0),
            480,
            Spread::all(10.0 * 4.0 + 2.0),
        ),
        (
            "gaussian_blur",
            Step::new("gaussian_blur").param("radius", 5.0),
            480,
            Spread::all(5.0 * 4.0 + 2.0),
        ),
        // the look is for 480 px: on a picture twice as long a radius of 10 is 20 px
        (
            "bloom_scaled",
            Step::new("bloom").param("radius", 10.0),
            960,
            Spread::all(20.0 * 4.0 + 2.0),
        ),
        // the stroke and its glow, the antialiasing ramp's pixel and two more
        (
            "outline",
            Step::new("outline").param("width", 3.0).param("glow", 2.0),
            480,
            Spread::all(3.0 + 2.0 + 1.0 + 2.0),
        ),
        // a copy 20 px along 0 degrees (right): 21 px past the right edge (a pixel for a fraction), the blur's 5 x 4 + 0.5 all round
        (
            "drop_shadow_right",
            Step::new("drop_shadow")
                .param("offset", 20.0)
                .param("angle", 0.0)
                .param("blur", 5.0)
                .param("copies", 1),
            480,
            beyond(20.5, 21.0 + 20.5, 20.5, 20.5),
        ),
        // 45 degrees: down and right by 20 x sin 45, each a pixel more
        (
            "drop_shadow_diagonal",
            Step::new("drop_shadow")
                .param("offset", 20.0)
                .param("angle", 45.0)
                .param("blur", 5.0)
                .param("copies", 1),
            480,
            beyond(20.5, 20.0 * sin_45 + 1.0 + 20.5, 20.0 * sin_45 + 1.0 + 20.5, 20.5),
        ),
        // the farthest of 4 copies is 4 x 10 px to the right
        (
            "echo",
            Step::new("echo").param("copies", 4).param("dx", 10.0).param("dy", 0.0),
            480,
            beyond(0.0, 41.0, 0.0, 0.0),
        ),
        // a colour grade moves nothing
        ("grade", Step::new("grade").param("exposure", 1.0), 480, Spread::NONE),
        // a tiling is made from the whole picture
        ("tile", Step::new("tile"), 480, Spread::Unbounded),
    ];
    for (id, step, longest_side, wanted) in cases {
        let got = spread_of([step], longest_side)?;
        match (got, wanted) {
            (
                Spread::Beyond {
                    top,
                    right,
                    bottom,
                    left,
                },
                Spread::Beyond {
                    top: t,
                    right: r,
                    bottom: b,
                    left: l,
                },
            ) => {
                for (side, got, wanted) in [
                    ("top", top, t),
                    ("right", right, r),
                    ("bottom", bottom, b),
                    ("left", left, l),
                ] {
                    assert!((got - wanted).abs() < 1e-3, "{id}: {side} is {got}, by hand {wanted}");
                }
            }
            _ => assert_eq!(got, wanted, "{id}"),
        }
    }
    Ok(())
}

#[test]
fn the_spreads_of_the_steps_add_along_the_chain() -> Outcome {
    let bloom = Step::new("bloom").param("radius", 10.0); // 42
    let blur = Step::new("gaussian_blur").param("radius", 5.0); // 22
    let both = spread_of([bloom.clone(), blur.clone()], 480)?;
    assert_eq!(both, Spread::all(42.0 + 22.0));
    assert_eq!(both.pixels(), Some([64; 4]));
    // a step that cannot be bounded makes the chain unbounded, whatever is beside it
    let unbounded = spread_of([bloom, Step::new("tile"), blur], 480)?;
    assert_eq!(unbounded, Spread::Unbounded);
    assert_eq!(unbounded.pixels(), None);
    Ok(())
}

#[test]
fn a_step_shown_at_no_opacity_adds_no_room_and_one_shown_a_little_does() -> Outcome {
    let bloom = Step::new("bloom").param("radius", 10.0);
    assert_eq!(spread_of([bloom.clone().opacity(0.0)], 480)?, Spread::NONE);
    assert_eq!(spread_of([bloom.opacity(0.1)], 480)?, Spread::all(42.0));
    Ok(())
}

#[test]
fn an_each_step_spreads_as_far_as_its_widest_look() -> Outcome {
    let looks = [
        Some(Step::new("bloom").param("radius", 10.0)),
        Some(Step::new("echo").param("copies", 1).param("dx", 100.0).param("dy", 0.0)),
        None,
    ];
    let recipe = Recipe::new([Each::new("cells", looks)]).with_region(
        "cells",
        Grid {
            columns: Shares::equal(3),
            ..Grid::default()
        },
    );
    // the bloom's 42 on every side, the echo's 101 on the right alone
    assert_eq!(recipe.spread(&Presets::new(), 480)?, beyond(42.0, 101.0, 42.0, 42.0));
    Ok(())
}

#[test]
fn the_room_is_worked_out_from_the_parameters_without_the_pictures_a_render_needs() -> Outcome {
    // a look-up table that is not there: a render refuses it, the question does not need it
    let step = Step::new("lut").param("file", "no_such_table.cube");
    assert_eq!(spread_of([step], 480)?, Spread::NONE);
    Ok(())
}

#[test]
fn a_picture_of_no_length_has_no_scale_to_ask_at() {
    let refused = spread_of([Step::new("bloom")], 0);
    assert!(matches!(refused, Err(Error::Recipe(why)) if why.contains("longest_side")));
}

#[test]
fn a_step_with_values_it_refuses_is_refused_by_the_question() {
    let refused = spread_of([Step::new("bloom").param("radius", -1.0)], 480);
    assert!(
        matches!(refused, Err(Error::Invalid { .. } | Error::Recipe(_))),
        "{refused:?}"
    );
}

/// A photo of `values` (SIDE x SIDE) with the block as its subject if `content`, else an empty one.
fn photo(values: &[f32], content: bool) -> Result<Photo<'_>, Box<dyn StdError>> {
    let map = Layer::new(
        SIDE,
        SIDE,
        (0..SIDE * SIDE)
            .map(|at| f32::from(u8::from(content && inside(at))))
            .collect(),
    )?;
    Ok(Photo::new(Picture::new(SIDE, SIDE, values)?).with_map("subject", map)?)
}

/// The claim of the question, made where scene makes it: a recipe rendered at the size the question was asked for paints
/// no farther beyond the box of its content than the room it names, summed over the chain and scaled to the size.
#[test]
fn a_rendered_recipe_paints_within_the_room_the_question_names() -> Outcome {
    let cases = [
        ("bloom", vec![Step::new("bloom").param("radius", 30.0)]),
        ("blur", vec![Step::new("gaussian_blur").param("radius", 12.0)]),
        (
            "bloom_then_blur",
            vec![
                Step::new("bloom").param("radius", 18.0),
                Step::new("gaussian_blur").param("radius", 9.0),
            ],
        ),
        (
            "shadow_then_blur",
            vec![
                Step::new("drop_shadow")
                    .param("offset", 30.0)
                    .param("angle", 30.0)
                    .param("blur", 6.0),
                Step::new("motion_blur").param("length", 12.0).param("angle", 0.0),
            ],
        ),
        (
            "outline",
            vec![Step::new("outline").param("width", 12.0).param("glow", 6.0)],
        ),
        (
            "echo",
            vec![
                Step::new("echo")
                    .param("copies", 3)
                    .param("dx", -20.0)
                    .param("dy", 12.0),
            ],
        ),
    ];
    for (id, steps) in cases {
        let recipe = Recipe::new(steps);
        let room = recipe
            .spread(&Presets::new(), 160)?
            .pixels()
            .ok_or("a bounded recipe")?;
        let render = |values: &[f32], content: bool| -> Result<Vec<f32>, Box<dyn StdError>> {
            Ok(photo(values, content)?
                .render(&recipe, &Presets::new(), Options::default())?
                .into_values())
        };
        let painted = painted(&render(&block(), true)?, &render(&background(), false)?);
        assert!(
            painted.iter().any(|side| *side > 0),
            "{id}: nothing is painted beyond the block"
        );
        assert!(
            painted.iter().zip(room).all(|(painted, room)| *painted <= room),
            "{id}: paints {painted:?} beyond the block, the room is {room:?} (top, right, bottom, left)"
        );
    }
    Ok(())
}

/// An effect that grows what is brighter by `reach` pixels in every direction, saying `says` about its spread.
struct Grow {
    reach: usize,
    says: Spread,
}

impl Effect for Grow {
    fn name(&self) -> &'static str {
        "spread_double_grow"
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn spread(&self, _params: &Value) -> Result<Spread, Error> {
        Ok(self.says)
    }

    fn apply(&self, picture: &Picture<'_>, _params: &Value, _inputs: &Inputs<'_>) -> Result<Image, Error> {
        let (height, width) = (picture.height(), picture.width());
        Ok(picture.map_at(|row, column, _| {
            let mut brightest = [0.0_f32; 3];
            for near_row in row.saturating_sub(self.reach)..(row + self.reach + 1).min(height) {
                for near_column in column.saturating_sub(self.reach)..(column + self.reach + 1).min(width) {
                    let at = (near_row * width + near_column) * 3;
                    brightest = std::array::from_fn(|channel| brightest[channel].max(picture.values()[at + channel]));
                }
            }
            brightest
        }))
    }
}

/// The check is not vacuous: a known-bad effect, which paints 5 px beyond its content and says less, fails it, and a
/// known-good one, which says what it paints or more, passes.
#[test]
fn an_effect_that_paints_farther_than_it_says_is_caught() -> Outcome {
    let says = |says: Spread| problems(&Grow { reach: 5, says });
    assert_eq!(says(Spread::NONE)?.len(), 1, "an effect that says nothing");
    assert_eq!(
        says(Spread::all(4.0))?.len(),
        1,
        "an effect that says a pixel too little"
    );
    assert!(
        says(beyond(5.0, 5.0, 5.0, 5.0))?.is_empty(),
        "an effect that says it all"
    );
    assert!(says(Spread::all(9.0))?.is_empty(), "an effect that says more");
    assert!(says(Spread::Unbounded)?.is_empty(), "an effect that says anything goes");
    Ok(())
}
