//! `each` steps seen through the public API alone: a look for each piece of a partition, blended across the borders.
//!
//! The effects are the test's own (written against the public `Effect` trait): `Paint` paints one grey level and counts
//! its calls. The picture is a grey ramp, so a pixel that kept its value is told from one that was painted, and the
//! oracles are hand arithmetic on `out = before + sum_k weight_k * (look_k - before)`.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use bullterrier_image::region::{Bands, Grid, Shares};
use bullterrier_image::{
    Assign, BlendMode, Category, Each, Effect, Error, Image, Input, Inputs, Layer, Options, Photo, Picture, Presets,
    Recipe, Step, apply, register,
};
use serde_json::json;

use crate::common::compare::assert_close;

type Outcome = Result<(), Box<dyn std::error::Error>>;

/// An effect that paints the whole picture with one grey `level` and counts its calls.
struct Paint {
    name: String,
    level: f32,
    calls: AtomicUsize,
    /// The sizes of the pictures it was run on.
    seen: Mutex<Vec<(usize, usize)>>,
    /// The size ratios it was offered.
    ratios: Mutex<Vec<f64>>,
}

impl Paint {
    /// A painter called `name`, registered.
    fn register(name: &str, level: f32) -> Result<Arc<Self>, Error> {
        let paint = Arc::new(Self {
            name: name.to_owned(),
            level,
            calls: AtomicUsize::new(0),
            seen: Mutex::new(Vec::new()),
            ratios: Mutex::new(Vec::new()),
        });
        register(paint.clone())?;
        Ok(paint)
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }

    /// The step that runs it, at full opacity.
    fn step(&self) -> Step {
        Step::new(self.name.as_str()).opacity(1.0)
    }
}

impl Effect for Paint {
    fn name(&self) -> &str {
        &self.name
    }

    fn category(&self) -> Category {
        Category::Utilities
    }

    fn inputs(&self) -> &[Input] {
        &[Input::SizeRatio]
    }

    fn apply(&self, picture: &Picture<'_>, _params: serde_json::Value, inputs: &Inputs<'_>) -> Result<Image, Error> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.ratios
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(inputs.size_ratio());
        self.seen
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((picture.height(), picture.width()));
        Ok(picture.map(|_| [self.level; 3]))
    }
}

/// `count` as a float: the counts here are far below 2^16.
fn exact(count: usize) -> f32 {
    f32::from(u16::try_from(count).unwrap_or(u16::MAX))
}

/// A grey photo `height` x `width`, rising from 0.1 to 0.7 across: a different value in every column.
fn ramp(height: usize, width: usize) -> Vec<f32> {
    let mut values = Vec::new();
    for _ in 0..height {
        for column in 0..width {
            values.extend([0.1 + 0.6 * exact(column) / exact(width); 3]);
        }
    }
    values
}

/// A constant grey photo.
fn flat(height: usize, width: usize, level: f32) -> Vec<f32> {
    vec![level; height * width * 3]
}

/// The first channel of the pixel at `row`, `column` of a picture `width` wide.
fn at(values: &[f32], width: usize, row: usize, column: usize) -> f32 {
    values[(row * width + column) * 3]
}

/// An each step over `columns` equal columns, declared as the grid `cells`, with `looks`, hard-edged (feather 0) unless
/// the step is changed.
fn recipe_of(columns: usize, each: Each) -> Recipe {
    Recipe::new([each]).with_region(
        "cells",
        Grid {
            columns: Shares::equal(columns),
            ..Grid::default()
        },
    )
}

fn each_of(looks: Vec<Option<Step>>, feather: f64) -> Each {
    let mut each = Each::new("cells", looks);
    each.feather = feather;
    each
}

fn render(values: &[f32], height: usize, width: usize, recipe: &Recipe, options: Options) -> Result<Vec<f32>, Error> {
    Ok(Photo::new(Picture::new(height, width, values)?)
        .render(recipe, &Presets::new(), options)?
        .into_values())
}

/// A Gaussian weight row by brute force: the indicator of `chosen` columns of a row of `width`, blurred by `sigma` with
/// the picture's edge repeated outward, each output the weighted sum over `-radius..=radius` (radius `int(4 sigma +
/// 0.5)`) of normalised `exp(-x^2 / 2 sigma^2)`. The oracle for the weights of an each step.
fn blurred_row(chosen: &[bool], sigma: f64) -> Vec<f64> {
    #[expect(clippy::cast_possible_truncation, reason = "a small radius")]
    let radius = (4.0 * sigma + 0.5).floor() as i32;
    let weights: Vec<f64> = (-radius..=radius)
        .map(|offset| (-0.5 * (f64::from(offset) / sigma).powi(2)).exp())
        .collect();
    let total: f64 = weights.iter().sum();
    let last = i32::try_from(chosen.len()).unwrap_or(i32::MAX) - 1;
    (0..=last)
        .map(|column| {
            (-radius..=radius)
                .zip(&weights)
                .map(|(offset, weight)| {
                    let source = usize::try_from((column + offset).clamp(0, last)).unwrap_or(0);
                    weight / total * f64::from(u8::from(chosen[source]))
                })
                .sum()
        })
        .collect()
}

#[test]
fn with_hard_borders_each_piece_is_exactly_its_looks_effect_and_a_none_look_leaves_it() -> Outcome {
    // 4 x 12 photo with a different value in every channel; grade at exposure 1 on the left column half, nothing right
    let values: Vec<f32> = (0_u8..144).map(|index| f32::from(index) / 160.0).collect();
    let grade = json!({"temperature": 0.0, "tint": 0.0, "exposure": 1.0, "contrast": 1.0, "pivot": 0.5, "saturation": 1.0,
                       "vibrance": 0.0});
    let graded = apply("grade", &Picture::new(4, 12, &values)?, grade, &Inputs::default())?.into_values();
    let look = Step::new("grade").param("exposure", 1.0).opacity(1.0);
    let recipe = recipe_of(2, each_of(vec![Some(look), None], 0.0));
    let out = render(&values, 4, 12, &recipe, Options::default())?;
    for row in 0..4 {
        for column in 0..12 {
            let source = if column < 6 { &graded } else { &values };
            let index = (row * 12 + column) * 3;
            assert_eq!(
                out[index..index + 3],
                source[index..index + 3],
                "row {row}, column {column}"
            );
        }
    }
    Ok(())
}

#[test]
fn the_pieces_take_the_looks_in_turn_and_a_look_no_piece_has_never_runs() -> Outcome {
    let (low, mid, high) = (
        Paint::register("each_turn_low", 0.2)?,
        Paint::register("each_turn_mid", 0.5)?,
        Paint::register("each_turn_high", 0.8)?,
    );
    let values = ramp(2, 5);
    // five columns, two looks of the three: pieces 0..4 take looks 0, 1, 0, 1, 0
    let recipe = recipe_of(
        5,
        each_of(vec![Some(low.step()), Some(mid.step()), None, Some(high.step())], 0.0),
    );
    // four looks cycle: pieces 0, 1, 2, 3, 4 take looks 0, 1, 2, 3, 0 (look 2 is None: the photo)
    let out = render(&values, 2, 5, &recipe, Options::default())?;
    let expected_columns = [0.2, 0.5, at(&values, 5, 0, 2), 0.8, 0.2];
    for row in 0..2 {
        for (column, expected) in expected_columns.iter().enumerate() {
            assert!(
                (at(&out, 5, row, column) - expected).abs() <= 1e-6,
                "row {row}, column {column}"
            );
        }
    }
    // every look with pieces ran once, over the whole picture, however many pieces it has (look 0 has two)
    assert_eq!((low.calls(), mid.calls(), high.calls()), (1, 1, 1));
    // two pieces and three looks: the third has no piece
    let (first, second, third) = (
        Paint::register("each_few_first", 0.3)?,
        Paint::register("each_few_second", 0.6)?,
        Paint::register("each_few_third", 0.9)?,
    );
    let recipe = recipe_of(
        2,
        each_of(vec![Some(first.step()), Some(second.step()), Some(third.step())], 0.0),
    );
    render(&values, 2, 5, &recipe, Options::default())?;
    assert_eq!((first.calls(), second.calls(), third.calls()), (1, 1, 0));
    Ok(())
}

#[test]
fn a_none_look_leaves_its_piece_and_fades_into_its_neighbours_only_near_the_border() -> Outcome {
    let paint = Paint::register("each_none_fade", 0.9)?;
    // 6 x 480, the design size: feather 4 is a sigma of 4 px and a kernel of 16 px each side
    let values = ramp(6, 480);
    let recipe = recipe_of(2, each_of(vec![Some(paint.step()), None], 4.0));
    let out = render(&values, 6, 480, &recipe, Options::default())?;
    for row in 0..6 {
        // far from the border at 240 each piece is what its look makes: the painted level, the photo itself
        for column in (0..240 - 17).chain(240 + 17..480) {
            let expected = if column < 240 {
                0.9
            } else {
                at(&values, 480, row, column)
            };
            assert!(
                (at(&out, 480, row, column) - expected).abs() <= 1e-6,
                "row {row}, column {column}"
            );
        }
        // near it the pixel is between the photo and the painted level, and not the photo or the paint right at the border
        for column in 240 - 17..240 + 17 {
            let (photo, got) = (at(&values, 480, row, column), at(&out, 480, row, column));
            assert!(
                got >= photo.min(0.9) - 1e-6 && got <= photo.max(0.9) + 1e-6,
                "column {column}: {got}"
            );
        }
        assert!(at(&out, 480, row, 239) < 0.9 - 0.01 && at(&out, 480, row, 240) > at(&values, 480, row, 240) + 0.01);
    }
    Ok(())
}

#[test]
fn the_weights_are_the_pieces_blurred_by_a_gaussian_of_feather_design_pixels_within_a_few_hundredths() -> Outcome {
    let paint = Paint::register("each_gauss", 0.8)?;
    // 3 pieces (columns 0..160, 160..320, 320..480) of a 480-wide photo, the middle one with the painted look: the weight
    // row is the indicator of the middle third blurred, down every column the same, so the vertical pass changes nothing.
    // The blur is three boxes that stand for the Gaussian: its weights are within 0.035 of the Gaussian's (measured 0.03),
    // so a pixel is within 0.035 of the 0.8 it is mixed with
    let values = ramp(3, 480);
    let recipe = recipe_of(3, each_of(vec![None, Some(paint.step()), None], 2.0));
    let out = render(&values, 3, 480, &recipe, Options::default())?;
    let chosen: Vec<bool> = (0..480).map(|column| (160..320).contains(&column)).collect();
    let weights = blurred_row(&chosen, 2.0);
    for row in 0..3 {
        for (column, weight) in weights.iter().enumerate() {
            let before = f64::from(at(&values, 480, row, column));
            let expected = before + weight * (0.8 - before);
            assert!(
                (f64::from(at(&out, 480, row, column)) - expected).abs() <= 0.035,
                "row {row}, column {column}: {} is not {expected}",
                at(&out, 480, row, column)
            );
        }
    }
    Ok(())
}

#[test]
fn a_piece_at_the_pictures_edge_is_not_darkened_by_it() -> Outcome {
    // the picture's border is not a border: with the edge repeated, the weight of the painted piece at column 0 stays 1
    let paint = Paint::register("each_edge", 0.9)?;
    let recipe = recipe_of(2, each_of(vec![Some(paint.step()), None], 8.0));
    let out = render(&flat(6, 480, 0.1), 6, 480, &recipe, Options::default())?;
    for row in 0..6 {
        for column in [0, 1, 2] {
            assert!((at(&out, 480, row, column) - 0.9).abs() <= 1e-6, "column {column}");
        }
    }
    Ok(())
}

#[test]
fn looks_that_paint_one_level_give_that_level_everywhere_whatever_the_feather_and_assignment() -> Outcome {
    // the weights of all the looks add up to 1 at every pixel
    let (one, other) = (
        Paint::register("each_same_one", 0.6)?,
        Paint::register("each_same_other", 0.6)?,
    );
    let values = ramp(4, 48);
    for feather in [0.0, 0.5, 5.0, 24.0] {
        for (assign, seed) in [(Assign::Cycle, 0), (Assign::Random, 3), (Assign::Random, 99)] {
            let mut each = each_of(vec![Some(one.step()), Some(other.step())], feather);
            each.assign = assign;
            each.seed = seed;
            let out = render(&values, 4, 48, &recipe_of(7, each), Options::default())?;
            assert_close(
                &out,
                &vec![0.6; 4 * 48 * 3],
                1e-6,
                &format!("feather {feather}, {assign:?}, seed {seed}"),
            );
        }
    }
    Ok(())
}

#[test]
fn the_feather_is_in_design_pixels_so_the_blend_is_twice_as_wide_at_twice_the_size() -> Outcome {
    let paint = Paint::register("each_scale", 0.8)?;
    let recipe = recipe_of(2, each_of(vec![Some(paint.step()), None], 8.0));
    let values = flat(6, 480, 0.2);
    // the pixels of the first row that are neither the photo (0.2) nor the paint (0.8) by more than 1e-3
    let blend_width = |options: Options, width: usize| -> Result<usize, Error> {
        let out = render(&values, 6, 480, &recipe, options)?;
        Ok((0..width)
            .filter(|&column| {
                let value = at(&out, width, 0, column);
                value > 0.2 + 1e-3 && value < 0.8 - 1e-3
            })
            .count())
    };
    let design = blend_width(Options::default(), 480)?;
    let double = blend_width(Options::default().with_size(960), 960)?;
    assert!(design >= 40, "{design} pixels blend at the design size");
    assert!(
        double.abs_diff(2 * design) <= 2,
        "{double} pixels at twice the size, {design} at 480"
    );
    Ok(())
}

#[test]
fn the_each_steps_own_opacity_mask_and_blend_mode_lay_the_blended_picture_over_the_input() -> Outcome {
    let paint = Paint::register("each_lay", 0.8)?;
    let values = ramp(2, 8);
    let looks = || vec![Some(paint.step()), None];
    let at_half = |opacity: f64| {
        let mut each = each_of(looks(), 0.0);
        each.opacity = Some(opacity);
        render(&values, 2, 8, &recipe_of(2, each), Options::default())
    };
    // opacity 0: the photo; 1 (and unset): the pieces; 0.5: half way, hand arithmetic on column 1 (painted) and 6 (not)
    assert_eq!(at_half(0.0)?, values);
    let half = at_half(0.5)?;
    assert!((at(&half, 8, 0, 1) - (at(&values, 8, 0, 1) + 0.5 * (0.8 - at(&values, 8, 0, 1)))).abs() <= 1e-6);
    assert_eq!(at(&half, 8, 0, 6), at(&values, 8, 0, 6));
    let unset = render(&values, 2, 8, &recipe_of(2, each_of(looks(), 0.0)), Options::default())?;
    assert!((at(&unset, 8, 0, 1) - 0.8).abs() <= 1e-6);
    // multiply lays the blended picture (painted piece 0.8, the other the photo itself) over the photo: photo x 0.8 on
    // the painted piece, photo x photo on the other
    let mut multiply = each_of(looks(), 0.0);
    multiply.blend_mode = BlendMode::Multiply;
    let out = render(&values, 2, 8, &recipe_of(2, multiply), Options::default())?;
    assert!((at(&out, 8, 0, 1) - at(&values, 8, 0, 1) * 0.8).abs() <= 1e-6);
    assert!((at(&out, 8, 0, 6) - at(&values, 8, 0, 6).powi(2)).abs() <= 1e-6);
    // a mask: the picture where it is 1 (left four columns), the photo where it is 0
    let mut masked = each_of(vec![Some(paint.step()), Some(paint.step())], 0.0);
    masked.mask = Some("subject".to_owned());
    let left: Vec<f32> = (0..16).map(|index| f32::from(u8::from(index % 8 < 4))).collect();
    let photo = Photo::new(Picture::new(2, 8, &values)?).with_map("subject", Layer::new(2, 8, left)?)?;
    let out = photo
        .render(&recipe_of(2, masked), &Presets::new(), Options::default())?
        .into_values();
    for column in 0..8 {
        let expected = if column < 4 { 0.8 } else { at(&values, 8, 0, column) };
        assert!((at(&out, 8, 1, column) - expected).abs() <= 1e-6, "column {column}");
    }
    Ok(())
}

#[test]
fn a_looks_own_opacity_applies_to_its_result_before_the_weights() -> Outcome {
    let paint = Paint::register("each_look_opacity", 0.8)?;
    let values = ramp(2, 8);
    let recipe = recipe_of(2, each_of(vec![Some(paint.step().opacity(0.5)), None], 0.0));
    let out = render(&values, 2, 8, &recipe, Options::default())?;
    for column in 0..4 {
        let before = at(&values, 8, 0, column);
        assert!(
            (at(&out, 8, 0, column) - (before + 0.5 * (0.8 - before))).abs() <= 1e-6,
            "column {column}"
        );
    }
    assert_eq!(at(&out, 8, 0, 6), at(&values, 8, 0, 6));
    Ok(())
}

#[test]
fn the_result_never_leaves_zero_to_one_though_the_sums_are_rounded() -> Outcome {
    // a look that paints 1 over a picture of many different values: before + (1 - before) is 1 plus a rounding error for
    // some of them; the result is held in range
    let white = Paint::register("each_range_white", 1.0)?;
    let values: Vec<f32> = (1_u16..=3000).map(|index| f32::from(index) / 3001.0).collect();
    let mut grey = Vec::new();
    for value in &values {
        grey.extend([*value; 3]);
    }
    let recipe = recipe_of(1, each_of(vec![Some(white.step())], 0.0));
    let out = render(&grey, 1, 3000, &recipe, Options::default())?;
    assert!(out.iter().all(|value| (0.0..=1.0).contains(value)));
    assert!(out.iter().all(|value| *value >= 1.0 - 1e-6));
    Ok(())
}

#[test]
fn a_random_assignment_gives_each_piece_exactly_one_look_and_the_same_seed_the_same_picture() -> Outcome {
    let looks = [
        Paint::register("each_random_a", 0.2)?,
        Paint::register("each_random_b", 0.5)?,
        Paint::register("each_random_c", 0.8)?,
    ];
    let values = flat(2, 40, 0.1);
    let which = |seed: u32| -> Result<Vec<usize>, Error> {
        let mut each = each_of(looks.iter().map(|look| Some(look.step())).collect(), 0.0);
        each.assign = Assign::Random;
        each.seed = seed;
        let out = render(&values, 2, 40, &recipe_of(40, each), Options::default())?;
        // forty pieces, one column each: the look of a piece is the level its column shows
        (0..40)
            .map(|column| {
                [0.2_f32, 0.5, 0.8]
                    .iter()
                    .position(|level| {
                        (at(&out, 40, 0, column) - level).abs() <= 1e-6
                            && (at(&out, 40, 1, column) - level).abs() <= 1e-6
                    })
                    .ok_or_else(|| Error::Recipe(format!("column {column} shows no look")))
            })
            .collect()
    };
    let first = which(1)?;
    assert_eq!(first, which(1)?, "the same seed draws the same looks");
    assert_ne!(first, which(2)?, "another seed draws other looks");
    // over many draws every look is used and none is favoured: 20 seeds x 40 pieces = 800 draws, 267 each when fair
    let mut counts = [0_usize; 3];
    for seed in 0..20 {
        for look in which(seed)? {
            counts[look] += 1;
        }
    }
    assert!(counts.iter().all(|count| (200..=335).contains(count)), "{counts:?}");
    Ok(())
}

#[test]
fn a_look_runs_on_the_whole_working_picture_once_whatever_the_pieces() -> Outcome {
    let paint = Paint::register("each_once", 0.8)?;
    let recipe = recipe_of(6, each_of(vec![Some(paint.step())], 3.0));
    render(&ramp(4, 12), 4, 12, &recipe, Options::default())?;
    assert_eq!(paint.calls(), 1);
    assert_eq!(
        *paint.seen.lock().unwrap_or_else(PoisonError::into_inner),
        vec![(4, 12)]
    );
    Ok(())
}

#[test]
fn an_each_over_the_bands_of_a_map_gives_each_band_its_look() -> Outcome {
    let (low, high) = (
        Paint::register("each_bands_low", 0.3)?,
        Paint::register("each_bands_high", 0.9)?,
    );
    // depth falls from 1 at the left to 0 at the right of 4 columns: two bands, the nearer (highest) first
    let depth = Layer::new(1, 4, vec![1.0, 0.75, 0.25, 0.0])?;
    let values = flat(1, 4, 0.5);
    let recipe = Recipe::new([each_of(vec![Some(high.step()), Some(low.step())], 0.0)]).with_region(
        "cells",
        Bands {
            of: "depth".to_owned(),
            count: 2,
        },
    );
    let out = Photo::new(Picture::new(1, 4, &values)?)
        .with_map("depth", depth)?
        .render(&recipe, &Presets::new(), Options::default())?
        .into_values();
    let levels: Vec<f32> = (0..4).map(|column| at(&out, 4, 0, column)).collect();
    assert_close(&levels, &[0.9, 0.9, 0.3, 0.3], 1e-6, "two bands");
    Ok(())
}

#[test]
fn an_each_over_a_region_that_does_not_exist_is_refused_before_anything_runs() -> Outcome {
    let paint = Paint::register("each_nowhere", 0.8)?;
    let recipe = Recipe::new([Each::new("nowhere", [Some(paint.step())])]);
    let result = render(&flat(2, 2, 0.5), 2, 2, &recipe, Options::default());
    assert!(
        matches!(&result, Err(error) if error.to_string().contains("nowhere")),
        "{result:?}"
    );
    assert_eq!(paint.calls(), 0);
    Ok(())
}

#[test]
fn feather_at_its_most_on_a_picture_at_the_most_it_blends_without_trouble() -> Outcome {
    let paint = Paint::register("each_most", 0.8)?;
    let recipe = recipe_of(2, each_of(vec![Some(paint.step()), None], 24.0));
    let out = render(&flat(4, 480, 0.2), 4, 480, &recipe, Options::default())?;
    assert!(out.iter().all(|value| (0.2 - 1e-6..=0.8 + 1e-6).contains(value)));
    assert!((at(&out, 480, 0, 0) - 0.8).abs() <= 1e-6);
    assert!((at(&out, 480, 0, 479) - 0.2).abs() <= 1e-6);
    Ok(())
}

#[test]
fn the_shape_of_the_pieces_does_not_change_what_a_look_is() -> Outcome {
    // a grid with rows and columns: four pieces, four looks, one pixel each in a 2 x 2 photo
    let levels = [0.1_f32, 0.3, 0.5, 0.7];
    let looks = [
        Paint::register("each_quad_0", levels[0])?,
        Paint::register("each_quad_1", levels[1])?,
        Paint::register("each_quad_2", levels[2])?,
        Paint::register("each_quad_3", levels[3])?,
    ];
    let recipe = Recipe::new([each_of(looks.iter().map(|look| Some(look.step())).collect(), 0.0)]).with_region(
        "cells",
        Grid {
            columns: Shares::equal(2),
            rows: Shares::equal(2),
            ..Grid::default()
        },
    );
    let out = render(&flat(2, 2, 0.9), 2, 2, &recipe, Options::default())?;
    // pieces are numbered row after row
    assert_close(
        &[
            at(&out, 2, 0, 0),
            at(&out, 2, 0, 1),
            at(&out, 2, 1, 0),
            at(&out, 2, 1, 1),
        ],
        &levels,
        1e-6,
        "four pieces",
    );
    Ok(())
}

#[test]
fn a_look_is_run_at_the_working_pictures_size_ratio_and_its_pixel_values_scaled_by_it() -> Outcome {
    // at twice the design size (960 across) a look is offered the ratio 2 and a px value of its own is doubled
    let paint = Paint::register("each_ratio", 0.8)?;
    let values = ramp(6, 480);
    let doubled = Options::default().with_size(960);
    render(
        &values,
        6,
        480,
        &recipe_of(1, each_of(vec![Some(paint.step())], 0.0)),
        doubled,
    )?;
    assert_eq!(*paint.ratios.lock().unwrap_or_else(PoisonError::into_inner), vec![2.0]);
    // `pixel_grid`'s cell is in px: the look makes the picture the same step makes on its own at that size
    let grid = || {
        Step::new("pixel_grid")
            .param("cell", 6)
            .param("amount", 1.0)
            .opacity(1.0)
    };
    let by_each = render(
        &values,
        6,
        480,
        &recipe_of(1, each_of(vec![Some(grid())], 0.0)),
        doubled,
    )?;
    let alone = render(&values, 6, 480, &Recipe::new([grid()]), doubled)?;
    assert_close(&by_each, &alone, 1e-6, "a px look inside an each step and on its own");
    Ok(())
}

#[test]
fn looks_that_paint_the_extremes_stay_within_zero_to_one_though_the_weights_add_up_to_one_only_to_rounding() -> Outcome
{
    // two looks that paint white (and two that paint black) over pieces that blend: before + w1 (1 - before) + w2 (1 -
    // before) with w1 + w2 a hair over 1 would leave the range without the final clamp
    let (white_a, white_b) = (
        Paint::register("each_extreme_wa", 1.0)?,
        Paint::register("each_extreme_wb", 1.0)?,
    );
    let (black_a, black_b) = (
        Paint::register("each_extreme_ba", 0.0)?,
        Paint::register("each_extreme_bb", 0.0)?,
    );
    let values = ramp(3, 480);
    for (a, b) in [(&white_a, &white_b), (&black_a, &black_b)] {
        for feather in [0.5, 1.0, 2.0, 3.7, 8.0] {
            let recipe = recipe_of(9, each_of(vec![Some(a.step()), Some(b.step())], feather));
            let out = render(&values, 3, 480, &recipe, Options::default())?;
            assert!(
                out.iter().all(|value| (0.0..=1.0).contains(value)),
                "{} at feather {feather}",
                a.name
            );
        }
    }
    Ok(())
}
