//! `color_range` regions: how near each pixel of the photo is to a target colour, seen as the selection an effect is handed.
//! Pictures are synthesised from known colours. The oracles are hand computations: the published Oklab of the sRGB
//! primaries (Ottosson, to four places) and the lightness of a grey (the cube root of its linear level, its a and b being 0).

use bullterrier_image::params::Color;
use bullterrier_image::region::ColorRange;
use bullterrier_image::{Error, Input, Options, Photo, Picture, Presets, Recipe, Region, Step};
use proptest::prelude::*;
use serde_json::{Value, json};

use crate::common::Outcome;
use crate::common::run::{Paint, Recorder, Registered, regions};

const WHITE: [f32; 3] = [1.0, 1.0, 1.0];
const BLACK: [f32; 3] = [0.0, 0.0, 0.0];
const RED: [f32; 3] = [1.0, 0.0, 0.0];
const GREEN: [f32; 3] = [0.0, 1.0, 0.0];
const BLUE: [f32; 3] = [0.0, 0.0, 1.0];

/// The region `color_range` with the target and the two distances.
fn color_range(target: [f32; 3], tolerance: f64, feather: f64) -> Value {
    json!({"kind": "color_range", "target": target, "tolerance": tolerance, "feather": feather})
}

/// The selection the region `declared` makes of the photo of `pixels`, `width` wide, after the steps `before`.
fn selection(pixels: &[[f32; 3]], width: usize, declared: &Value, before: &[Step]) -> Result<Vec<f32>, Error> {
    let recorder = Registered::new(Recorder::new(&[], &["near"]))?;
    let flat: Vec<f32> = pixels.iter().flatten().copied().collect();
    let photo = Photo::new(Picture::new(pixels.len() / width, width, &flat)?);
    let mut steps = before.to_vec();
    steps.push(Step::new(recorder.name()).param("near", "c"));
    let mut recipe = Recipe::new(steps);
    recipe.regions = regions(json!({ "c": declared }));
    photo.render(&recipe, &Presets::new(), Options::default())?;
    recorder
        .seen("near")
        .pop()
        .ok_or_else(|| Error::Recipe("the recorder was handed no selection".to_owned()))
}

/// The lightness of the grey `level` (Oklab L): a grey has the same cone response in all three, the linear level.
fn grey_lightness(level: f64) -> f64 {
    let linear = if level <= 0.04045 {
        level / 12.92
    } else {
        ((level + 0.055) / 1.055).powf(2.4)
    };
    linear.cbrt()
}

#[test]
fn the_selection_is_1_at_the_target_and_falls_to_0_over_the_feather_by_oklab_distance() -> Outcome {
    // (target, pixel, tolerance, feather, the value worked out from the published Oklab of the two, and why)
    let rows = [
        (
            WHITE,
            RED,
            0.4,
            0.1,
            0.4746,
            "L, a and b all count: 0.4525 from white, 0.0525 past the tolerance",
        ),
        (
            GREEN,
            BLUE,
            0.6,
            0.2,
            0.6334,
            "the blue-yellow axis counts most here: 0.6733 apart",
        ),
        (BLACK, RED, 0.5, 0.5, 0.6424, "a wide feather: 0.6788 from black"),
        (RED, GREEN, 0.5, 0.1, 0.8018, "0.5198 apart, 0.0198 past the tolerance"),
        (WHITE, RED, 0.46, 0.1, 1.0, "within the tolerance"),
        (WHITE, RED, 0.3, 0.1, 0.0, "beyond tolerance and feather"),
    ];
    for (target, pixel, tolerance, feather, expected, why) in rows {
        let got = selection(&[pixel], 1, &color_range(target, tolerance, feather), &[])?;
        assert!((got[0] - expected).abs() < 2e-3, "{why}: {} not {expected}", got[0]);
    }
    Ok(())
}

#[test]
fn a_grey_ramp_against_white_follows_the_lightness_and_never_rises_as_the_grey_darkens() -> Outcome {
    let levels: Vec<f32> = (0..=255_u8).map(|byte| f32::from(byte) / 255.0).collect();
    let pixels: Vec<[f32; 3]> = levels.iter().map(|&level| [level; 3]).collect();
    let (tolerance, feather) = (0.1, 0.3);
    let got = selection(&pixels, 256, &color_range(WHITE, tolerance, feather), &[])?;
    for (&level, &value) in levels.iter().zip(&got) {
        let distance = 1.0 - grey_lightness(f64::from(level));
        let expected = (1.0 - (distance - tolerance) / feather).clamp(0.0, 1.0);
        assert!(
            (f64::from(value) - expected).abs() < 1e-4,
            "grey {level}: {value} not {expected}"
        );
    }
    assert!(
        got.windows(2).all(|pair| pair[0] <= pair[1]),
        "a darker grey was chosen more"
    );
    // the ramp holds all three regimes, so the checks above are not vacuous
    assert!(got.contains(&1.0) && got.contains(&0.0) && got.iter().any(|&value| 0.0 < value && value < 1.0));
    Ok(())
}

#[test]
fn a_tolerance_of_0_with_no_feather_chooses_only_the_exact_colour() -> Outcome {
    // exactly representable channels, so the pixel is the target to the bit
    let target = [0.5, 0.25, 0.75];
    let one_step = 1.0 / 256.0;
    let pixels = [
        target,
        [0.5 + one_step, 0.25, 0.75],
        [0.5, 0.25 - one_step, 0.75],
        [0.5, 0.25, 0.75 + one_step],
        target,
    ];
    let got = selection(&pixels, 5, &color_range(target, 0.0, 0.0), &[])?;
    assert_eq!(got, [1.0, 0.0, 0.0, 0.0, 1.0]);
    Ok(())
}

#[test]
fn a_target_built_in_code_with_a_channel_outside_0_to_1_or_not_a_number_is_refused_naming_region_field_and_value() {
    let refusal = |target: [f32; 3]| {
        let region = Region::from(ColorRange {
            target: Color(target),
            ..ColorRange::default()
        });
        region.check_shape("brick").err().map(|error| error.to_string())
    };
    for (target, field, value) in [
        ([1.5, 0.0, 0.0], "`target[0]` must be <= 1", "got 1.5"),
        ([0.0, -0.25, 0.0], "`target[1]` must be >= 0", "got -0.25"),
        ([0.0, 0.0, f32::NAN], "`target[2]` must be a finite number", "got NaN"),
    ] {
        let said = refusal(target).unwrap_or_default();
        assert!(
            ["region 'brick'", field, value].iter().all(|part| said.contains(part)),
            "{target:?}: {said:?}"
        );
    }
    assert_eq!(refusal([0.0, 0.5, 1.0]), None, "the edges of the range are colours");
}

#[test]
fn the_region_reads_the_photo_as_framed_and_sized_not_what_the_steps_before_it_painted() -> Outcome {
    let paint = Registered::new(Paint::new())?;
    let pixels = [RED, GREEN, WHITE, RED];
    let declared = color_range(RED, 0.1, 0.05);
    let plain = selection(&pixels, 2, &declared, &[])?;
    assert_eq!(plain, [1.0, 0.0, 0.0, 1.0]);
    // a step that paints the whole picture grey leaves what the region chooses where it was
    let painted = selection(&pixels, 2, &declared, &[Step::new(paint.name()).param("level", 0.5)])?;
    assert_eq!(painted, plain);
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn the_selection_lies_within_0_and_1_is_the_same_for_the_same_pixel_and_only_grows_with_the_tolerance(
        pixels in prop::collection::vec([0.0_f32..=1.0, 0.0..=1.0, 0.0..=1.0], 6),
        target in [0.0_f32..=1.0, 0.0..=1.0, 0.0..=1.0],
        tolerance in 0.0_f64..=0.9,
        more in 0.0_f64..=0.1,
        feather in prop_oneof![Just(0.0), 0.0_f64..=1.0],
    ) {
        let mut pixels = pixels;
        pixels.push(pixels[0]);
        pixels.push(target);
        let narrow = selection(&pixels, 8, &color_range(target, tolerance, feather), &[])?;
        let wide = selection(&pixels, 8, &color_range(target, tolerance + more, feather), &[])?;
        prop_assert!(narrow.iter().all(|value| (0.0..=1.0).contains(value)), "{:?}", narrow);
        prop_assert_eq!(narrow[0].to_bits(), narrow[6].to_bits(), "the same pixel, two values");
        prop_assert_eq!(narrow[7].to_bits(), 1.0_f32.to_bits(), "the target is not chosen fully");
        prop_assert!(narrow.iter().zip(&wide).all(|(a, b)| a <= b), "{:?} then {:?}", narrow, wide);
    }
}

// --- the reason the kind exists: windows that no band of light can tell from the wall ------------------------------

const WIDTH: usize = 112;
const HEIGHT: usize = 120;
/// The brick wall, #8a3b2e.
const BRICK: [u8; 3] = [0x8a, 0x3b, 0x2e];
/// Eight blue-grey panes, darker than the brick, as bright as it and brighter, in reading order.
const PANES: [[u8; 3]; 8] = [
    [0x2a, 0x35, 0x44],
    [0xa9, 0xbc, 0xcf],
    [0x3c, 0x4e, 0x63],
    [0x7f, 0x93, 0xa8],
    [0xc5, 0xd3, 0xe0],
    [0x1f, 0x2a, 0x38],
    [0x5a, 0x6b, 0x7d],
    [0x45, 0x57, 0x6b],
];

fn level(byte: u8) -> f32 {
    f32::from(byte) / 255.0
}

/// The rows and columns the pane `number` covers: a 2 x 4 grid of 30 x 18 panes, 26 columns and 50 rows apart.
fn pane_rows_and_columns(number: usize) -> (std::ops::Range<usize>, std::ops::Range<usize>) {
    let (top, left) = (10 + 50 * (number / 4), 8 + 26 * (number % 4));
    (top..top + 30, left..left + 18)
}

/// The facade: brick with eight panes, every pixel nudged by up to 0.015 a channel by a fixed hash, so no two pixels of a
/// surface are alike. Its brightness is the Rec. 709 weighting of the encoded channels, as `light` is.
fn facade() -> Vec<[f32; 3]> {
    let mut pixels = Vec::with_capacity(WIDTH * HEIGHT);
    for row in 0..HEIGHT {
        for column in 0..WIDTH {
            let pane = (0..PANES.len()).find(|&number| {
                let (rows, columns) = pane_rows_and_columns(number);
                rows.contains(&row) && columns.contains(&column)
            });
            let base = pane.map_or(BRICK, |number| PANES[number]);
            let at = u32::try_from((row * WIDTH + column) * 3).unwrap_or(0);
            let nudge = |channel: u32| {
                let hash = (at + channel).wrapping_mul(2_654_435_761) >> 24; // Knuth's multiplicative hash, 0..255
                (f32::from(u8::try_from(hash).unwrap_or(0)) / 255.0 - 0.5) * 0.03
            };
            pixels.push([0, 1, 2].map(|channel| (level(base[channel as usize]) + nudge(channel)).clamp(0.0, 1.0)));
        }
    }
    pixels
}

fn brightness(bytes: [u8; 3]) -> f32 {
    0.2126 * level(bytes[0]) + 0.7152 * level(bytes[1]) + 0.0722 * level(bytes[2])
}

/// The partition `panes` makes of the facade when the selection `not_wall` is `declared`, with the other regions of the
/// recipe: the labels, row after row, and how many pieces there are.
fn pieces_of(declared: Value, regions_before: Value) -> Result<(Vec<i32>, usize), Error> {
    let recorder = Registered::new(Recorder::new(&[Input::Partition], &[]))?;
    let flat: Vec<f32> = facade().iter().flatten().copied().collect();
    let photo = Photo::new(Picture::new(HEIGHT, WIDTH, &flat)?);
    let mut step = Step::new(recorder.name());
    step.partition = Some("panes".to_owned());
    let mut recipe = Recipe::new([step]);
    let mut all = regions_before;
    all["not_wall"] = declared;
    all["panes"] = json!({"kind": "components", "of": "not_wall"});
    recipe.regions = regions(all);
    photo.render(&recipe, &Presets::new(), Options::default())?;
    let labels = recorder
        .received()
        .pop()
        .and_then(|received| received.partition)
        .ok_or_else(|| Error::Recipe("the recorder was handed no partition".to_owned()))?;
    let count = labels
        .iter()
        .copied()
        .max()
        .map_or(0, |last| usize::try_from(last + 1).unwrap_or(0));
    Ok((labels, count))
}

/// How many panes are a piece of their own in `labels`: every pixel of the pane has one label, no other pane has it and
/// no pixel of the wall has it.
fn panes_found(labels: &[i32]) -> usize {
    let wall: std::collections::HashSet<i32> = (0..HEIGHT)
        .flat_map(|row| (0..WIDTH).map(move |column| (row, column)))
        .filter(|&(row, column)| {
            !(0..PANES.len()).any(|number| {
                let (rows, columns) = pane_rows_and_columns(number);
                rows.contains(&row) && columns.contains(&column)
            })
        })
        .map(|(row, column)| labels[row * WIDTH + column])
        .collect();
    let own: Vec<Option<i32>> = (0..PANES.len())
        .map(|number| {
            let (rows, columns) = pane_rows_and_columns(number);
            let mut seen = rows.flat_map(|row| columns.clone().map(move |column| labels[row * WIDTH + column]));
            let first = seen.next()?;
            (seen.all(|label| label == first) && !wall.contains(&first)).then_some(first)
        })
        .collect();
    own.iter()
        .enumerate()
        .filter(|&(number, label)| {
            label.is_some() && own.iter().enumerate().all(|(other, l)| other == number || l != label)
        })
        .count()
}

#[test]
fn the_panes_of_a_brick_facade_are_each_a_piece_by_color_range_though_light_cannot_tell_them_from_the_wall() -> Outcome
{
    // the premise: some panes are brighter than the brick, some darker and one as bright, so no band of light fits them
    let wall = brightness(BRICK);
    assert!(PANES.iter().filter(|&&pane| brightness(pane) > wall + 0.05).count() >= 3);
    assert!(PANES.iter().filter(|&&pane| brightness(pane) < wall - 0.05).count() >= 2);
    assert!(PANES.iter().any(|&pane| (brightness(pane) - wall).abs() < 0.01));

    let brick =
        json!({"brick": {"kind": "color_range", "target": BRICK.map(level), "tolerance": 0.1, "feather": 0.05}});
    // every pane is far from the brick in Oklab, so what is near it is the wall and what is not is the panes
    let by_color_range = json!({"kind": "band", "of": "brick", "low": 0.0, "high": 0.3});
    let (labels, pieces) = pieces_of(by_color_range, brick)?;
    assert_eq!(panes_found(&labels), PANES.len(), "each pane is a piece of its own");
    assert_eq!(pieces, PANES.len() + 1, "the panes and the wall");

    // the same recipe by light: whatever band of it is taken (a fine grid of bounds, hard edges), the panes are not
    // all found, since the brick's brightness lies between theirs
    let mut best = 0;
    for low in (0..=20).map(|step| f64::from(step) / 20.0) {
        for high in (0..=20).map(|step| f64::from(step) / 20.0).filter(|&high| high >= low) {
            let by_light = json!({"kind": "band", "of": "light", "low": low, "high": high, "feather": 0.0});
            let (labels, _) = pieces_of(by_light, json!({}))?;
            best = best.max(panes_found(&labels));
        }
    }
    assert!(best < PANES.len(), "a band of light found all {best} panes");
    assert!(best > 0, "the sweep found no pane at all, so it shows nothing");
    Ok(())
}
