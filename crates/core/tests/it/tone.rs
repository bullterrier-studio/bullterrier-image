//! The tone remaps `levels` and `auto_levels` rest on, seen through the two effects: the Levels remap and the percentile
//! points of the automatic stretch.

use bullterrier_image::effects::{auto_levels, levels};
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
