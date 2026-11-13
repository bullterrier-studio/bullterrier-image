//! `colour` regions: how near each pixel of the photo is to a target colour, seen as the selection an effect is handed.
//! Pictures are synthesised from known colours. The oracles are hand computations: the published Oklab of the sRGB
//! primaries (Ottosson, to four places) and the lightness of a grey (the cube root of its linear level, its a and b being 0).

use bullterrier_image::{Error, Options, Photo, Picture, Presets, Recipe, Step};
use proptest::prelude::*;
use serde_json::{Value, json};

use crate::common::Outcome;
use crate::common::run::{Paint, Recorder, Registered, regions};

const WHITE: [f32; 3] = [1.0, 1.0, 1.0];
const BLACK: [f32; 3] = [0.0, 0.0, 0.0];
const RED: [f32; 3] = [1.0, 0.0, 0.0];
const GREEN: [f32; 3] = [0.0, 1.0, 0.0];
const BLUE: [f32; 3] = [0.0, 0.0, 1.0];

/// The region `colour` with the target and the two distances.
fn colour(target: [f32; 3], tolerance: f64, feather: f64) -> Value {
    json!({"kind": "colour", "target": target, "tolerance": tolerance, "feather": feather})
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
        let got = selection(&[pixel], 1, &colour(target, tolerance, feather), &[])?;
        assert!((got[0] - expected).abs() < 2e-3, "{why}: {} not {expected}", got[0]);
    }
    Ok(())
}

#[test]
fn a_grey_ramp_against_white_follows_the_lightness_and_never_rises_as_the_grey_darkens() -> Outcome {
    let levels: Vec<f32> = (0..=255_u8).map(|byte| f32::from(byte) / 255.0).collect();
    let pixels: Vec<[f32; 3]> = levels.iter().map(|&level| [level; 3]).collect();
    let (tolerance, feather) = (0.1, 0.3);
    let got = selection(&pixels, 256, &colour(WHITE, tolerance, feather), &[])?;
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
    let got = selection(&pixels, 5, &colour(target, 0.0, 0.0), &[])?;
    assert_eq!(got, [1.0, 0.0, 0.0, 0.0, 1.0]);
    Ok(())
}

#[test]
fn the_region_reads_the_photo_as_framed_and_sized_not_what_the_steps_before_it_painted() -> Outcome {
    let paint = Registered::new(Paint::new())?;
    let pixels = [RED, GREEN, WHITE, RED];
    let declared = colour(RED, 0.1, 0.05);
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
        let narrow = selection(&pixels, 8, &colour(target, tolerance, feather), &[])?;
        let wide = selection(&pixels, 8, &colour(target, tolerance + more, feather), &[])?;
        prop_assert!(narrow.iter().all(|value| (0.0..=1.0).contains(value)), "{:?}", narrow);
        prop_assert_eq!(narrow[0].to_bits(), narrow[6].to_bits(), "the same pixel, two values");
        prop_assert_eq!(narrow[7].to_bits(), 1.0_f32.to_bits(), "the target is not chosen fully");
        prop_assert!(narrow.iter().zip(&wide).all(|(a, b)| a <= b), "{:?} then {:?}", narrow, wide);
    }
}
