//! Tests of the evaluator, through the regions a recipe declares.

use std::sync::Arc;

use proptest::prelude::*;
use serde_json::{Value, json};

use super::*;
use crate::frame::Flip;
use crate::registry::Registry;
use crate::testing::{Finds, Paint, Recorder, regions, registry};
use crate::{Each, ErrorKind, Frame, Image, Options, Photo, Presets, Recipe, Step};

/// A registry with `paint` and a `recorder` whose field parameter `near` logs the selection it is given.
struct Rig {
    registry: Registry,
    recorder: Arc<Recorder>,
}

fn rig() -> Result<Rig, Error> {
    let registry = registry()?;
    registry.register(Arc::new(Paint { name: "paint" }))?;
    let recorder = Arc::new(Recorder::new("recorder", &[], &["near"]));
    registry.register(recorder.clone())?;
    Ok(Rig { registry, recorder })
}

impl Rig {
    fn render(&self, photo: &Photo<'_>, recipe: &Recipe, options: Options) -> Result<Image, Error> {
        photo.render_in(&self.registry, recipe, &Presets::new(), options)
    }

    /// The selection each step's `near` received, in order.
    fn seen(&self) -> Vec<Vec<f32>> {
        self.recorder
            .received()
            .into_iter()
            .map(|received| received.fields["near"].clone())
            .collect()
    }
}

/// A recipe of one recorder step for each of `names`, each working from that region, declaring `declared`.
fn asking(names: &[&str], declared: &Value) -> Recipe {
    let mut recipe = Recipe::new(names.iter().map(|name| Step::new("recorder").param("near", *name)));
    recipe.regions = regions(declared.clone());
    recipe
}

/// The photo of `height` x `width` grey pixels with the maps `given` (name, values).
fn grey<'a>(height: usize, width: usize, pixels: &'a [f32], given: &[(&str, Vec<f32>)]) -> Result<Photo<'a>, Error> {
    let mut photo = Photo::new(Picture::new(height, width, pixels)?);
    for (name, values) in given {
        photo = photo.with_map(*name, Layer::new(height, width, values.clone())?)?;
    }
    Ok(photo)
}

/// The values the region `name` has on a `height` x `width` photo with the maps `given`, as a step receives them.
fn made(
    height: usize,
    width: usize,
    declared: &Value,
    name: &str,
    given: &[(&str, Vec<f32>)],
) -> Result<Vec<f32>, Error> {
    let rig = rig()?;
    let pixels = vec![0.5; height * width * 3];
    let photo = grey(height, width, &pixels, given)?;
    rig.render(&photo, &asking(&[name], declared), Options::default())?;
    let mut seen = rig.seen();
    Ok(seen.remove(0))
}

/// `actual` is `expected` to within `by`.
fn assert_close(actual: &[f32], expected: &[f32], by: f32, row: &str) {
    assert_eq!(actual.len(), expected.len(), "{row}");
    for (index, (a, e)) in actual.iter().zip(expected).enumerate() {
        assert!((a - e).abs() <= by, "{row}: value {index} is {a}, not {e}");
    }
}

#[test]
fn a_linear_gradient_rises_across_the_extent_of_the_picture_in_its_direction() -> Result<(), Error> {
    let sixth = 1.0 / 6.0;
    // 45 degrees on 4 x 3 (width x height): extent (4 + 3) cos 45, so a pixel at column c, row r (centre c + 0.5,
    // r + 0.5) is at ((c - 1.5) + (r - 1)) / 7 + 0.5, worked by hand
    let oblique: Vec<f32> = (0_u8..3)
        .flat_map(|row| (0_u8..4).map(move |column| ((f32::from(column) - 1.5) + (f32::from(row) - 1.0)) / 7.0 + 0.5))
        .collect();
    // (height, width, degrees, the values row after row)
    let rows: [(usize, usize, f64, Vec<f32>); 8] = [
        (3, 4, 0.0, [0.125, 0.375, 0.625, 0.875].repeat(3)),
        (3, 4, 180.0, [0.875, 0.625, 0.375, 0.125].repeat(3)),
        (
            3,
            4,
            90.0,
            [sixth; 4].into_iter().chain([0.5; 4]).chain([1.0 - sixth; 4]).collect(),
        ),
        (
            3,
            4,
            270.0,
            [1.0 - sixth; 4].into_iter().chain([0.5; 4]).chain([sixth; 4]).collect(),
        ),
        (3, 4, 45.0, oblique.clone()),
        // any whole number of turns away is the same direction
        (3, 4, 405.0, oblique.clone()),
        (3, 4, -315.0, oblique),
        // one row: across it the value is x / width; down it there is nothing to rise through
        (1, 5, 0.0, vec![0.1, 0.3, 0.5, 0.7, 0.9]),
    ];
    for (height, width, degrees, expected) in rows {
        let declared = json!({"g": {"kind": "linear_gradient", "angle": degrees}});
        let values = made(height, width, &declared, "g", &[])?;
        assert_close(&values, &expected, 1e-6, &format!("{height} x {width} at {degrees}"));
    }
    // a single row has the middle value down the picture, wherever it points
    let down = made(
        1,
        5,
        &json!({"g": {"kind": "linear_gradient", "angle": 90.0}}),
        "g",
        &[],
    )?;
    assert_close(&down, &[0.5; 5], 1e-6, "1 x 5 at 90");
    Ok(())
}

#[test]
fn a_radial_gradient_is_the_distance_from_its_centre_over_a_share_of_the_shorter_side_capped_at_1() -> Result<(), Error>
{
    // 4 x 3 picture, centre in the middle (2, 1.5), radius 0.5 of the shorter side 3: 1.5 px. A pixel at column c,
    // row r is hypot(c - 1.5, r - 1) / 1.5 away, worked by hand
    let hypot = |across: f32, down: f32| across.hypot(down) / 1.5;
    let centre = made(3, 4, &json!({"r": {"kind": "radial_gradient"}}), "r", &[])?;
    let expected = [
        1.0,
        hypot(0.5, 1.0),
        hypot(0.5, 1.0),
        1.0, // the corners are 1.8 px away: capped
        1.0,
        1.0 / 3.0,
        1.0 / 3.0,
        1.0,
        1.0,
        hypot(0.5, 1.0),
        hypot(0.5, 1.0),
        1.0,
    ];
    assert_close(&centre, &expected, 1e-6, "centred");
    // centre a quarter across and at the top edge, radius a whole shorter side: (1, 0) and 3 px; pixel (0, 0) is
    // hypot(0.5, 0.5) / 3 away
    let moved = made(
        3,
        4,
        &json!({"r": {"kind": "radial_gradient", "x": 0.25, "y": 0.0, "radius": 1.0}}),
        "r",
        &[],
    )?;
    assert_close(
        &moved[..2],
        &[0.5_f32.hypot(0.5) / 3.0, 0.5_f32.hypot(0.5) / 3.0],
        1e-6,
        "moved",
    );
    // 1 x 5: the shorter side is the height, 1; radius 3 reaches 3 px from the centre (2.5, 0.5)
    let wide = made(
        1,
        5,
        &json!({"r": {"kind": "radial_gradient", "radius": 3.0}}),
        "r",
        &[],
    )?;
    assert_close(&wide, &[2.0 / 3.0, 1.0 / 3.0, 0.0, 1.0 / 3.0, 2.0 / 3.0], 1e-6, "1 x 5");
    Ok(())
}

#[test]
fn a_band_is_chosen_fully_inside_and_fades_linearly_beyond_its_bounds() -> Result<(), Error> {
    // dyadic numbers, so every value is exact in f32
    let depth = vec![0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0];
    let band =
        |feather: f64| json!({"b": {"kind": "band", "of": "depth", "low": 0.5, "high": 0.75, "feather": feather}});
    let given = [("depth", depth)];
    let soft = made(1, 9, &band(0.25), "b", &given)?;
    assert_eq!(soft, [0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0, 0.5, 0.0]);
    let hard = made(1, 9, &band(0.0), "b", &given)?;
    assert_eq!(hard, [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.0]);
    // a feather of a whole scale fades over all of it
    let wide = made(1, 9, &band(1.0), "b", &given)?;
    assert_close(
        &wide,
        &[0.5, 0.625, 0.75, 0.875, 1.0, 1.0, 1.0, 0.875, 0.75],
        0.0,
        "feather 1",
    );
    // a feather too small for a single-precision number is a hard edge, not a division by nothing
    let tiny = made(1, 9, &band(1e-50), "b", &given)?;
    assert_eq!(tiny, hard);
    Ok(())
}

#[test]
fn a_value_written_on_a_bound_is_on_it_whichever_precision_the_map_is_held_in() -> Result<(), Error> {
    // the maps hold the single-precision numbers nearest to 0.3 and 0.7; a recipe writes the bounds as 0.3 and 0.7.
    // Those are on the bounds although in double precision the first is just above and the second just below them
    let field = vec![0.3_f32, 0.7_f32];
    let on_high = json!({"b": {"kind": "band", "of": "depth", "low": 0.1, "high": 0.3, "feather": 0.0}});
    let on_low = json!({"b": {"kind": "band", "of": "depth", "low": 0.7, "high": 0.9, "feather": 0.0}});
    let given = [("depth", field)];
    assert_eq!(made(1, 2, &on_high, "b", &given)?, [1.0, 0.0]);
    assert_eq!(made(1, 2, &on_low, "b", &given)?, [0.0, 1.0]);
    // and the number just beyond the bound is not
    let above = f32::from_bits(0.3_f32.to_bits() + 1);
    assert_eq!(made(1, 1, &on_high, "b", &[("depth", vec![above])])?, [0.0]);
    Ok(())
}

#[test]
fn selections_combine_by_product_least_or_greatest_of_two_or_three() -> Result<(), Error> {
    let given = [
        ("depth", vec![0.0, 0.25, 0.5, 1.0]),
        ("subject", vec![1.0, 0.5, 0.5, 0.25]),
        ("star", vec![0.5, 0.5, 1.0, 0.5]),
    ];
    let declared =
        |op: &str, of: &[&str]| json!({"star": {"kind": "given"}, "c": {"kind": "combine", "op": op, "of": of}});
    // (op, the selections, the values worked by hand)
    let rows = [
        ("multiply", vec!["depth", "subject"], [0.0, 0.125, 0.25, 0.25]),
        ("min", vec!["depth", "subject"], [0.0, 0.25, 0.5, 0.25]),
        ("max", vec!["depth", "subject"], [1.0, 0.5, 0.5, 1.0]),
        ("multiply", vec!["depth", "subject", "star"], [0.0, 0.0625, 0.25, 0.125]),
        ("min", vec!["depth", "subject", "star"], [0.0, 0.25, 0.5, 0.25]),
        ("max", vec!["depth", "subject", "star"], [1.0, 0.5, 1.0, 1.0]),
    ];
    for (op, of, expected) in rows {
        let values = made(1, 4, &declared(op, &of), "c", &given)?;
        assert_eq!(values, expected, "{op} of {of:?}");
    }
    Ok(())
}

#[test]
fn a_region_made_from_a_region_is_made_from_what_that_one_is() -> Result<(), Error> {
    let declared = json!({
        "near": {"kind": "band", "of": "depth", "low": 0.5, "high": 1.0, "feather": 0.5},
        "nearer": {"kind": "band", "of": "near", "low": 0.5, "high": 1.0, "feather": 0.0},
    });
    let given = [("depth", vec![0.0, 0.25, 0.5, 0.75, 1.0])];
    assert_eq!(made(1, 5, &declared, "near", &given)?, [0.0, 0.5, 1.0, 1.0, 1.0]);
    assert_eq!(made(1, 5, &declared, "nearer", &given)?, [0.0, 1.0, 1.0, 1.0, 1.0]);
    Ok(())
}

#[test]
fn a_subject_region_without_per_is_the_subject_of_the_photo() -> Result<(), Error> {
    let declared = json!({"me": {"kind": "subject", "context": 0.5, "threshold": 0.2}});
    let subject = vec![0.0, 0.25, 0.5, 1.0];
    assert_eq!(made(1, 4, &declared, "me", &[("subject", subject.clone())])?, subject);
    Ok(())
}

#[test]
fn a_long_chain_of_regions_is_made_without_a_deep_call_stack() -> Result<(), Error> {
    // 3000 bands, each of the one before: the last is the depth passed through every one of them
    let mut declared = serde_json::Map::new();
    for link in 0..3000 {
        let of = if link == 0 {
            "depth".to_owned()
        } else {
            format!("b{}", link - 1)
        };
        declared.insert(
            format!("b{link}"),
            json!({"kind": "band", "of": of, "low": 0.0, "high": 1.0, "feather": 0.0}),
        );
    }
    let values = made(1, 2, &Value::Object(declared), "b2999", &[("depth", vec![0.25, 1.0])])?;
    assert_eq!(values, [1.0, 1.0]);
    Ok(())
}

#[test]
fn the_light_is_available_to_a_region_that_is_the_only_one_to_use_it() -> Result<(), Error> {
    let rig = rig()?;
    // grey pixels of brightness 0.1, 0.4, 0.6 and 0.9
    let pixels: Vec<f32> = [0.1, 0.4, 0.6, 0.9].iter().flat_map(|grey| [*grey; 3]).collect();
    let declared = json!({"bright": {"kind": "band", "of": "light", "low": 0.5, "high": 1.0, "feather": 0.0}});
    rig.render(
        &grey(1, 4, &pixels, &[])?,
        &asking(&["bright"], &declared),
        Options::default(),
    )?;
    assert_eq!(rig.seen(), [[0.0, 0.0, 1.0, 1.0]]);
    Ok(())
}

#[test]
fn a_region_is_made_for_the_working_picture_framed_and_sized_and_a_given_map_follows_the_frame() -> Result<(), Error> {
    let rig = rig()?;
    let pixels = vec![0.5; 4 * 6 * 3];
    // a map that is 1 in the left column of the 6-wide photo only
    let left: Vec<f32> = (0..24).map(|index| f32::from(u8::from(index % 6 == 0))).collect();
    let photo = grey(4, 6, &pixels, &[("star", left)])?;
    let mut flipped = asking(
        &["g", "star"],
        &json!({"star": {"kind": "given"}, "g": {"kind": "linear_gradient", "angle": 0.0}}),
    );
    flipped.frame = Frame {
        flip: Flip::X,
        ..Frame::default()
    };
    rig.render(&photo, &flipped, Options::default())?;
    let (gradient, star) = (&rig.seen()[0], &rig.seen()[1]);
    // the gradient is laid on the flipped picture, which is as wide as the photo; the given map has moved with it
    // to the right column
    assert_close(
        &gradient[..6],
        &[1.0 / 12.0, 0.25, 5.0 / 12.0, 7.0 / 12.0, 0.75, 11.0 / 12.0],
        1e-6,
        "gradient row",
    );
    assert_eq!((star[0], star[5], star.iter().sum::<f32>()), (0.0, 1.0, 4.0));
    // a square crop makes a picture of 4 x 4: the region has the pixels of that picture, not of the photo
    rig.recorder.clear();
    let mut cropped = asking(&["g"], &json!({"g": {"kind": "linear_gradient", "angle": 0.0}}));
    cropped.frame = Frame {
        aspect: Some(1.0),
        ..Frame::default()
    };
    let plain = grey(4, 6, &pixels, &[])?;
    rig.render(&plain, &cropped, Options::default())?;
    assert_close(&rig.seen()[0][..4], &[0.125, 0.375, 0.625, 0.875], 1e-6, "cropped");
    assert_eq!(rig.seen()[0].len(), 16);
    // and at a size of 12 wide the gradient has 8 x 12 values
    rig.recorder.clear();
    let sized = asking(&["g"], &json!({"g": {"kind": "linear_gradient", "angle": 0.0}}));
    rig.render(&plain, &sized, Options::default().with_size(12))?;
    assert_eq!(rig.seen()[0].len(), 8 * 12);
    assert!((rig.seen()[0][0] - 1.0 / 24.0).abs() < 1e-6);
    Ok(())
}

#[test]
fn the_same_gradient_at_twice_the_size_has_the_means_of_the_small_one() -> Result<(), Error> {
    let declared = json!({"g": {"kind": "linear_gradient", "angle": 30.0}});
    let (rig, pixels) = (rig()?, vec![0.5; 4 * 6 * 3]);
    let photo = grey(4, 6, &pixels, &[])?;
    let at = |size: u32| -> Result<Vec<f32>, Error> {
        rig.recorder.clear();
        rig.render(&photo, &asking(&["g"], &declared), Options::default().with_size(size))?;
        Ok(rig.seen().remove(0))
    };
    let (small, large) = (at(12)?, at(24)?);
    assert_eq!((small.len(), large.len()), (8 * 12, 16 * 24));
    for row in 0..8 {
        for column in 0..12 {
            let block: f32 = [(0, 0), (0, 1), (1, 0), (1, 1)]
                .iter()
                .map(|(down, across)| large[(2 * row + down) * 24 + 2 * column + across])
                .sum::<f32>()
                / 4.0;
            assert!(
                (block - small[row * 12 + column]).abs() < 1e-6,
                "row {row}, column {column}"
            );
        }
    }
    Ok(())
}

#[test]
fn a_region_no_step_uses_is_never_made_and_asks_for_no_map() -> Result<(), Error> {
    let rig = rig()?;
    let pixels = vec![0.5; 4 * 6 * 3];
    let (used, unused) = (Arc::new(Finds::flat(0.25)), Arc::new(Finds::flat(0.75)));
    let photo = Photo::new(Picture::new(4, 6, &pixels)?)
        .with_finder("moon", used.clone())
        .with_finder("star", unused.clone());
    let declared = json!({
        "moon": {"kind": "given"}, "star": {"kind": "given"},
        "lit": {"kind": "band", "of": "moon"}, "dim": {"kind": "band", "of": "star"},
    });
    // two steps use `lit` and the map under it: the finder is asked once; `dim` and `star` are never touched
    rig.render(&photo, &asking(&["lit", "lit"], &declared), Options::default())?;
    assert_eq!((used.calls(), unused.calls()), (1, 0));
    assert_eq!(rig.seen(), vec![vec![0.0; 24]; 2]);
    Ok(())
}

#[test]
fn a_map_stays_while_any_later_step_uses_it_directly_or_through_a_region() -> Result<(), Error> {
    let rig = rig()?;
    let photo = grey(1, 1, &[0.5; 3], &[("depth", vec![0.75])])?;
    // step 1 uses `near` (made from the depth), step 3 uses `far` (made from the depth too) and step 2 nothing of it
    let declared = json!({
        "near": {"kind": "band", "of": "depth", "low": 0.5, "high": 1.0, "feather": 0.0},
        "far": {"kind": "band", "of": "depth", "low": 0.0, "high": 0.5, "feather": 0.0},
    });
    let mut recipe = asking(&["near", "far"], &declared);
    recipe.steps.insert(1, Step::new("paint").param("level", 0.5).into());
    rig.render(&photo, &recipe, Options::default())?;
    assert_eq!(rig.seen(), [[1.0], [0.0]]);
    // a map used directly again after other steps
    let mut again = asking(&["depth", "near", "depth"], &declared);
    again.steps.insert(1, Step::new("paint").param("level", 0.5).into());
    rig.recorder.clear();
    rig.render(&photo, &again, Options::default())?;
    assert_eq!(rig.seen(), [[0.75], [1.0], [0.75]]);
    Ok(())
}

#[test]
fn an_svg_region_without_a_file_refuses_the_render_naming_the_region_when_a_step_makes_it() -> Result<(), Error> {
    let rig = rig()?;
    let pixels = vec![0.5; 4 * 6 * 3];
    let photo = grey(4, 6, &pixels, &[])?;
    let mut recipe = Recipe::new([Step::new("paint").param("level", 0.5)]);
    recipe.steps.push(Each::new("p", [None]).into());
    recipe.regions = regions(json!({"p": {"kind": "svg"}}));
    let result = rig.render(&photo, &recipe, Options::default());
    assert!(
        matches!(&result, Err(error) if error.kind() == ErrorKind::Recipe
            && ["region 'p'", "`file` is not set"].iter().all(|word| error.to_string().contains(word))),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn a_release_drops_the_maps_no_later_step_needs_including_one_no_step_ever_asked_for() -> Result<(), Error> {
    // `frame` is a map only the frame needed (no step names it, so `last` has no entry), `late` is needed by step 2
    let map = || Layer::new(1, 2, vec![0.5; 2]).map(Arc::new);
    let held = BTreeMap::from([("frame".to_owned(), map()?), ("late".to_owned(), map()?)]);
    let mut evaluator = Evaluator::new(
        &Regions::default(),
        held,
        BTreeMap::from([("late".to_owned(), 2)]),
        None,
    );
    evaluator.release(0);
    assert!(evaluator.held("frame").is_none(), "a map no step needs is kept");
    assert!(
        evaluator.held("late").is_some(),
        "a map step 2 needs was dropped after step 0"
    );
    evaluator.release(1);
    assert!(
        evaluator.held("late").is_some(),
        "a map step 2 needs was dropped after step 1"
    );
    evaluator.release(2);
    assert!(evaluator.held("late").is_none(), "a map no later step needs is kept");
    Ok(())
}

proptest! {
    #[test]
    fn every_selection_a_region_makes_lies_within_0_and_1_whatever_its_numbers(
        field in prop::collection::vec(0.0_f32..=1.0, 12),
        other in prop::collection::vec(0.0_f32..=1.0, 12),
        (low, high) in (0.0_f64..=1.0, 0.0_f64..=1.0).prop_map(|(a, b)| (a.min(b), a.max(b))),
        feather in prop_oneof![Just(0.0), 0.0_f64..=1.0],
        op in prop::sample::select(vec!["multiply", "min", "max"]),
        angle in -720.0_f64..720.0,
        (x, y, radius) in (0.0_f64..=1.0, 0.0_f64..=1.0, 0.01_f64..=3.0),
    ) {
        let declared = json!({
            "b": {"kind": "band", "of": "depth", "low": low, "high": high, "feather": feather},
            "c": {"kind": "combine", "op": op, "of": ["b", "subject", "depth"]},
            "l": {"kind": "linear_gradient", "angle": angle},
            "r": {"kind": "radial_gradient", "x": x, "y": y, "radius": radius},
        });
        let given = [("depth", field), ("subject", other)];
        for name in ["b", "c", "l", "r"] {
            let values = made(3, 4, &declared, name, &given)?;
            prop_assert_eq!(values.len(), 12);
            prop_assert!(values.iter().all(|value| (0.0..=1.0).contains(value)), "{}: {:?}", name, values);
        }
    }
}

#[test]
fn a_torn_grid_shifts_the_columns_by_the_first_field_and_the_rows_by_the_second_in_px_of_the_picture()
-> Result<(), Error> {
    use crate::ops::length;
    use crate::ops::noise::{Tear, bend};
    // 60 x 40 px: the longest side is an eighth of the design size 480, so the tear of 24 px at a scale of 96 px is 3 px at
    // a spacing of a fifth of the long side; two columns and two rows split at x = 30 and y = 20
    let (height, width) = (40_usize, 60_usize);
    let declared = regions(json!({"g": {
        "kind": "grid", "columns": 2, "rows": 2,
        "edge": {"style": "torn", "amount": 24.0, "scale": 96.0, "seed": 5, "octaves": 2, "persistence": 0.5}
    }}));
    let pixels = vec![0.5; height * width * 3];
    let picture = Picture::new(height, width, &pixels)?;
    let last = BTreeMap::from([("g".to_owned(), 0)]);
    let mut evaluator = Evaluator::new(&declared, BTreeMap::new(), last, None);
    let made = evaluator.pieces("g", &picture)?;
    let [along_x, along_y] = bend(
        height,
        width,
        &Tear {
            scale_share: 0.2,
            octaves: 2,
            persistence: 0.5,
            seed: 5,
        },
    );
    let mut numbered = BTreeMap::new();
    let mut expected = Vec::new();
    for at in 0..height * width {
        let (row, column) = (length::float(at / width) + 0.5, length::float(at % width) + 0.5);
        let (x, y) = (
            column + f64::from(along_x[at]) * 3.0,
            row + f64::from(along_y[at]) * 3.0,
        );
        let kind = usize::from(y >= 20.0) * 2 + usize::from(x >= 30.0);
        let next = numbered.len();
        expected.push(*numbered.entry(kind).or_insert(next));
    }
    let got: Vec<usize> = made.values().iter().map(|&label| pieces::piece(label)).collect();
    let first_difference = got.iter().zip(&expected).position(|(got, want)| got != want);
    assert_eq!((made.pieces(), first_difference), (numbered.len(), None));
    assert!(numbered.len() > 1);
    Ok(())
}
