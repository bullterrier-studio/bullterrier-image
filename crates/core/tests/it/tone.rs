//! The tone remaps `levels`, `auto_levels` and `curves` rest on, seen through the three effects: the Levels remap, the
//! percentile points of the automatic stretch and the monotone cubic (PCHIP, Fritsch-Carlson) of a tone curve.

use bullterrier_image::effects::{auto_levels, curves, levels};
use bullterrier_image::params::EffectParams;
use bullterrier_image::{Error, Inputs, Picture, apply};
use proptest::prelude::*;
use serde_json::Value;

const TOLERANCE: f32 = 1e-6;

/// `params` applied to `pixels` (a picture one pixel high).
fn run<P: EffectParams>(params: &P, pixels: &[[f32; 3]]) -> Result<Vec<[f32; 3]>, Error> {
    let values: Vec<f32> = pixels.iter().flatten().copied().collect();
    let image = apply(
        P::EFFECT,
        &Picture::new(1, pixels.len(), &values)?,
        Value::Object(params.values()),
        &Inputs::default(),
    )?;
    Ok(image
        .into_values()
        .as_chunks::<3>()
        .0
        .iter()
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect())
}

/// The grey pixels of `values`.
fn greys(values: &[f32]) -> Vec<[f32; 3]> {
    values.iter().map(|&value| [value; 3]).collect()
}

/// The red channel of `pixels`.
fn reds(pixels: &[[f32; 3]]) -> Vec<f32> {
    pixels.iter().map(|pixel| pixel[0]).collect()
}

fn assert_close(actual: &[f32], expected: &[f32], context: &str) {
    assert_eq!(actual.len(), expected.len(), "{context}");
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert!(
            (actual - expected).abs() <= TOLERANCE,
            "{context}: value {index} is {actual}, not {expected}"
        );
    }
}

/// The master curve through the heights `heights` at the inputs 0, 0.25, 0.5, 0.75 and 1, read at `values`; the colour
/// curves stay straight lines.
fn master(heights: [f32; 5], values: &[f32]) -> Result<Vec<f32>, Error> {
    let params = curves::Params {
        black: heights[0],
        shadows: heights[1],
        midtones: heights[2],
        highlights: heights[3],
        white: heights[4],
        ..curves::Params::default()
    };
    Ok(reds(&run(&params, &greys(values))?))
}

// --- levels ------------------------------------------------------------------------------------------------------

/// `levels` with the given parameters read at `values`.
fn levelled(params: &levels::Params, values: &[f32]) -> Result<Vec<f32>, Error> {
    Ok(reds(&run(params, &greys(values))?))
}

#[test]
fn levels_land_on_the_output_points_at_and_beyond_the_input_points_and_are_linear_between() -> Result<(), Error> {
    let params = levels::Params {
        black: 0.2,
        white: 0.8,
        output_black: 0.1,
        output_white: 0.9,
        ..levels::Params::default()
    };
    // 0.5 is half way between the input points, so half way between the output points; out = 0.1 + 0.8 * (v - 0.2) / 0.6
    let out = levelled(&params, &[0.0, 0.2, 0.5, 0.8, 1.0])?;
    assert_close(
        &out,
        &[0.1, 0.1, 0.5, 0.9, 0.9],
        "input points 0.2/0.8, output points 0.1/0.9",
    );
    Ok(())
}

#[test]
fn a_midtone_gamma_is_a_power_of_one_over_gamma_that_keeps_black_and_white() -> Result<(), Error> {
    let brighter = levels::Params {
        gamma: 2.0,
        ..levels::Params::default()
    };
    // 0.25 ** (1 / 2) and 0.81 ** (1 / 2)
    assert_close(
        &levelled(&brighter, &[0.0, 0.25, 0.81, 1.0])?,
        &[0.0, 0.5, 0.9, 1.0],
        "gamma 2",
    );
    let darker = levels::Params {
        gamma: 0.5,
        ..levels::Params::default()
    };
    assert_close(
        &levelled(&darker, &[0.0, 0.5, 0.9, 1.0])?,
        &[0.0, 0.25, 0.81, 1.0],
        "gamma 0.5",
    );
    Ok(())
}

#[test]
fn output_points_in_reverse_make_the_negative() -> Result<(), Error> {
    let negative = levels::Params {
        output_black: 1.0,
        output_white: 0.0,
        ..levels::Params::default()
    };
    let values = [0.0, 0.1, 0.35, 0.6, 1.0];
    let expected: Vec<f32> = values.iter().map(|value| 1.0 - value).collect();
    assert_close(&levelled(&negative, &values)?, &expected, "the negative");
    Ok(())
}

#[test]
fn input_points_that_are_equal_or_crossed_cut_the_picture_at_the_black_point() -> Result<(), Error> {
    for (black, white) in [(0.5, 0.5), (0.6, 0.3)] {
        let cut = levels::Params {
            black,
            white,
            ..levels::Params::default()
        };
        let out = levelled(&cut, &[0.0, 0.4, 0.7, 1.0])?;
        assert_close(&out, &[0.0, 0.0, 1.0, 1.0], &format!("black {black}, white {white}"));
    }
    Ok(())
}

proptest! {
    #[test]
    fn levels_never_reorder_tones_and_stay_between_the_output_points(
        black in 0.0_f32..0.9, gap in 0.05_f32..1.0, gamma in 0.1_f32..10.0, low in 0.0_f32..1.0, high in 0.0_f32..1.0,
    ) {
        let (output_black, output_white) = (low.min(high), low.max(high));
        let params = levels::Params {
            black, white: (black + gap).min(1.0), gamma, output_black, output_white
        };
        let sweep: Vec<f32> = (0..=200_u16).map(|step| f32::from(step) / 200.0).collect();
        let out = levelled(&params, &sweep)?;
        prop_assert!(out.windows(2).all(|pair| pair[0] <= pair[1]), "brighter in, darker out: {out:?}");
        prop_assert!(
            out.iter().all(|&value| (output_black - TOLERANCE..=output_white + TOLERANCE).contains(&value)),
            "{out:?}"
        );
    }
}

// --- auto_levels -------------------------------------------------------------------------------------------------

#[test]
fn the_darkest_and_brightest_values_go_to_black_and_white_and_the_rest_between() -> Result<(), Error> {
    let stretch = auto_levels::Params {
        clip: 0.0,
        ..auto_levels::Params::default()
    };
    // the range is 0.2..0.9, so out = (v - 0.2) / 0.7
    let out = reds(&run(&stretch, &greys(&[0.2, 0.3, 0.5, 0.7, 0.9]))?);
    assert_close(&out, &[0.0, 1.0 / 7.0, 3.0 / 7.0, 5.0 / 7.0, 1.0], "clip 0");
    Ok(())
}

#[test]
fn the_clipped_share_at_each_end_is_saturated() -> Result<(), Error> {
    // 5 values and a clip of 0.2 drop one at each end, so the points are 0.3 and 0.7 and out = (v - 0.3) / 0.4
    let stretch = auto_levels::Params {
        clip: 0.2,
        ..auto_levels::Params::default()
    };
    let out = reds(&run(&stretch, &greys(&[0.9, 0.3, 0.5, 0.7, 0.2]))?);
    assert_close(&out, &[1.0, 0.0, 0.5, 1.0, 0.0], "clip 0.2");
    Ok(())
}

#[test]
fn a_clip_that_leaves_no_range_leaves_the_values_as_they_are() -> Result<(), Error> {
    // 3 values and a clip of 0.4 drop one at each end: the same middle value is both points
    let stretch = auto_levels::Params {
        clip: 0.4,
        ..auto_levels::Params::default()
    };
    let values = [0.2, 0.5, 0.9];
    assert_close(&reds(&run(&stretch, &greys(&values))?), &values, "clip 0.4 of 3 values");
    Ok(())
}

#[test]
fn per_channel_stretches_each_channel_alone_and_linked_all_alike() -> Result<(), Error> {
    // red runs 0.1..0.5, green 0.4..0.8, blue stays 0.3
    let pixels = [[0.1, 0.4, 0.3], [0.3, 0.6, 0.3], [0.5, 0.8, 0.3]];
    let alone = auto_levels::Params {
        clip: 0.0,
        mode: auto_levels::Mode::PerChannel,
    };
    let out = run(&alone, &pixels)?;
    let channel = |index: usize| out.iter().map(|pixel| pixel[index]).collect::<Vec<_>>();
    assert_close(&channel(0), &[0.0, 0.5, 1.0], "red alone");
    assert_close(&channel(1), &[0.0, 0.5, 1.0], "green alone");
    assert_close(
        &channel(2),
        &[0.3, 0.3, 0.3],
        "a channel with no range is left as it is",
    );
    // one range 0.1..0.8 for all: out = (v - 0.1) / 0.7
    let together = auto_levels::Params {
        clip: 0.0,
        mode: auto_levels::Mode::Linked,
    };
    let out = run(&together, &pixels)?;
    let expected = [
        [0.0, 3.0 / 7.0, 2.0 / 7.0],
        [2.0 / 7.0, 5.0 / 7.0, 2.0 / 7.0],
        [4.0 / 7.0, 1.0, 2.0 / 7.0],
    ];
    for (pixel, expected) in out.iter().zip(expected) {
        assert_close(pixel, &expected, "linked");
    }
    Ok(())
}

// --- curves ------------------------------------------------------------------------------------------------------

#[test]
fn the_defaults_are_the_straight_line() -> Result<(), Error> {
    let values = [0.0, 0.1, 0.25, 0.4, 0.5, 0.77, 1.0];
    assert_close(
        &reds(&run(&curves::Params::default(), &greys(&values))?),
        &values,
        "defaults",
    );
    Ok(())
}

/// The heights of a master curve, and three readings of it: (input, output).
type ByHand = ([f32; 5], [(f32, f32); 3]);

/// Curves with heights, and readings of them at inputs between the points, worked out by hand from the Fritsch-Carlson
/// slopes and cubic Hermite form (h = 0.25, so a slope is the chord over 0.25): the first by the harmonic mean of the
/// chords, the second where every point turns (slope 0 inside, three-point estimates at the ends). Checked once against
/// `scipy.interpolate.PchipInterpolator` to 7 places.
const BY_HAND: [ByHand; 3] = [
    // chords 0.4, 3.2, 0.4, 0: slopes 0 at both ends (the estimates point away), 0.7111 (= 2 * 0.4 * 3.2 / 3.6) at the
    // 0.25 and 0.5 points and 0 at the 0.75 point. At 0.125: 0.5 * 0.1 - 0.125 * 0.25 * 0.7111 = 1 / 36; at 0.375 the
    // two slopes cancel: (0.1 + 0.9) / 2; at 0.625: 0.5 * 0.9 + 0.125 * 0.25 * 0.7111 + 0.5 * 1 = 35 / 36
    (
        [0.0, 0.1, 0.9, 1.0, 1.0],
        [(0.125, 1.0 / 36.0), (0.375, 0.5), (0.625, 35.0 / 36.0)],
    ),
    // chords 2.4, -1.6, 2, -1.2: slope 0 at the three turns, 4.4 at the start; at 0.375 and 0.625 the middle of a smooth
    // step between two flat points, (0.7 + 0.3) / 2 and (0.3 + 0.8) / 2; at 0.125: 0.5 * 0.1 + 0.125 * 0.25 * 4.4 +
    // 0.5 * 0.7 = 0.5375
    (
        [0.1, 0.7, 0.3, 0.8, 0.5],
        [(0.125, 0.5375), (0.375, 0.5), (0.625, 0.55)],
    ),
    // chords 0.4, -2, 2.8, -0.4: the start estimate (3 * 0.4 + 2) / 2 = 1.6 is steeper than three times the first chord,
    // so the slope is capped at 3 * 0.4 = 1.2; at 0.125: 0.5 * 0.5 + 0.125 * 0.25 * 1.2 + 0.5 * 0.6 = 0.5875
    (
        [0.5, 0.6, 0.1, 0.8, 0.7],
        [(0.125, 0.5875), (0.375, 0.35), (0.625, 0.45)],
    ),
];

#[test]
fn a_curve_gives_the_values_worked_out_by_hand_between_its_points() -> Result<(), Error> {
    for (heights, readings) in BY_HAND {
        let inputs = readings.map(|(input, _)| input);
        let expected = readings.map(|(_, output)| output);
        assert_close(&master(heights, &inputs)?, &expected, &format!("heights {heights:?}"));
    }
    Ok(())
}

#[test]
fn a_curve_passes_through_every_point_even_when_it_is_not_monotone() -> Result<(), Error> {
    let heights = [0.1, 0.7, 0.3, 0.8, 0.5];
    assert_close(
        &master(heights, &[0.0, 0.25, 0.5, 0.75, 1.0])?,
        &heights,
        "the control points",
    );
    Ok(())
}

#[test]
fn a_curve_is_held_at_its_end_points_beyond_them() -> Result<(), Error> {
    // a picture can hold values outside 0..1, where a cubic carried on would run away
    let heights = [0.1, 0.7, 0.3, 0.8, 0.5];
    assert_close(
        &master(heights, &[-0.3, 1.4, 7.0])?,
        &[0.1, 0.5, 0.5],
        "beyond the ends",
    );
    Ok(())
}

proptest! {
    #[test]
    fn a_rising_curve_never_reorders_tones_and_never_leaves_its_outputs(
        mut heights in proptest::array::uniform5(0.0_f32..=1.0),
    ) {
        heights.sort_by(f32::total_cmp);
        let sweep: Vec<f32> = (0..=400_u16).map(|step| f32::from(step) / 400.0).collect();
        let out = master(heights, &sweep)?;
        prop_assert!(out.windows(2).all(|pair| pair[0] <= pair[1] + TOLERANCE), "brighter in, darker out: {out:?}");
        prop_assert!(
            out.iter().all(|&value| (heights[0] - TOLERANCE..=heights[4] + TOLERANCE).contains(&value)),
            "{out:?}"
        );
    }

    #[test]
    fn a_curve_stays_between_the_outputs_of_the_points_around_it(heights in proptest::array::uniform5(0.0_f32..=1.0)) {
        let sweep: Vec<f32> = (0..=400_u16).map(|step| f32::from(step) / 400.0).collect();
        let out = master(heights, &sweep)?;
        // 100 steps of the sweep lie between one point and the next: no cubic through them may dip or bulge past them
        for (left, stretch) in out.windows(101).step_by(100).enumerate() {
            let (low, high) = (heights[left].min(heights[left + 1]), heights[left].max(heights[left + 1]));
            prop_assert!(
                stretch.iter().all(|output| (low - TOLERANCE..=high + TOLERANCE).contains(output)),
                "between point {left} and the next, outside {low}..{high}: {stretch:?}"
            );
        }
    }
}
