//! `channel_mixer` and `color_balance`: values worked out by hand from their documented formulas.

use bullterrier_image::Error;
use bullterrier_image::effects::{channel_mixer, color_balance};
use bullterrier_image::params::Color;
use proptest::prelude::*;

use crate::common::pixels::run;

const TOLERANCE: f32 = 1e-6;

fn assert_pixel(actual: [f32; 3], expected: [f32; 3], context: &str) {
    assert!(
        actual.iter().zip(expected).all(|(a, e)| (a - e).abs() <= TOLERANCE),
        "{context}: {actual:?}, not {expected:?}"
    );
}

// --- channel_mixer -----------------------------------------------------------------------------------------------

#[test]
fn each_output_channel_is_the_row_of_weights_applied_to_the_input_channels() -> Result<(), Error> {
    // nine different weights, so a transposed matrix or a swapped pair gives another number
    let mixer = channel_mixer::Params {
        red_from_red: 0.5,
        red_from_green: 0.25,
        red_from_blue: 0.1,
        green_from_red: -0.5,
        green_from_green: 1.0,
        green_from_blue: 0.75,
        blue_from_red: 2.0,
        blue_from_green: -1.0,
        blue_from_blue: 0.5,
    };
    // red: 0.5 * 0.2 + 0.25 * 0.4 + 0.1 * 0.6 = 0.26; green: -0.1 + 0.4 + 0.45 = 0.75; blue: 0.4 - 0.4 + 0.3 = 0.3
    let out = run(&mixer, &[[0.2, 0.4, 0.6]])?;
    assert_pixel(out[0], [0.26, 0.75, 0.3], "the matrix applied to (0.2, 0.4, 0.6)");
    Ok(())
}

#[test]
fn the_defaults_are_the_identity_and_a_sum_outside_0_to_1_is_held_to_it() -> Result<(), Error> {
    let pixels = [[0.0, 0.5, 1.0], [0.3, 0.3, 0.9]];
    assert_eq!(run(&channel_mixer::Params::default(), &pixels)?, pixels);
    let strong = channel_mixer::Params {
        red_from_red: 3.0,
        green_from_green: -1.0,
        ..channel_mixer::Params::default()
    };
    // red 3 * 0.5 = 1.5 is held to 1, green -1 * 0.5 = -0.5 to 0
    assert_eq!(run(&strong, &[[0.5, 0.5, 0.25]])?, [[1.0, 0.0, 0.25]]);
    Ok(())
}

proptest! {
    #[test]
    fn rows_that_sum_to_one_keep_every_grey(
        grey in 0.0_f32..=1.0, first in 0.0_f32..=1.0, second in 0.0_f32..=1.0, third in 0.0_f32..=1.0,
    ) {
        // each row is (a, b, 1 - a - b) in some order of sharing, so a grey stays itself
        let row = |a: f32, b: f32| [a * 0.5, b * 0.5, 1.0 - a * 0.5 - b * 0.5];
        let [red, green, blue] = [row(first, second), row(second, third), row(third, first)];
        let mixer = channel_mixer::Params {
            red_from_red: red[0], red_from_green: red[1], red_from_blue: red[2],
            green_from_red: green[0], green_from_green: green[1], green_from_blue: green[2],
            blue_from_red: blue[0], blue_from_green: blue[1], blue_from_blue: blue[2],
        };
        let out = run(&mixer, &[[grey; 3]])?;
        prop_assert!(out[0].iter().all(|value| (value - grey).abs() <= 1e-6), "{:?}", out);
    }
}

// --- color_balance -----------------------------------------------------------------------------------------------

/// A balance that pushes only the `ranges` given (shadows, midtones, highlights; mid grey leaves a range alone) at
/// amount 1, with the brightness left free.
fn pushing(ranges: [[f32; 3]; 3]) -> color_balance::Params {
    color_balance::Params {
        shadows: Color(ranges[0]),
        midtones: Color(ranges[1]),
        highlights: Color(ranges[2]),
        amount: 1.0,
        preserve_luminosity: false,
        ..color_balance::Params::default()
    }
}

const GREY: [f32; 3] = [0.5; 3];

#[test]
fn black_is_all_shadows_mid_grey_all_midtones_and_white_all_highlights() -> Result<(), Error> {
    let params = pushing([[0.6, 0.5, 0.5], [0.5, 0.7, 0.5], [0.5, 0.5, 0.2]]);
    let out = run(&params, &[[0.0; 3], GREY, [1.0; 3]])?;
    assert_pixel(out[0], [0.1, 0.0, 0.0], "black takes the shadows' push of +0.1 red");
    assert_pixel(
        out[1],
        [0.5, 0.7, 0.5],
        "mid grey takes the midtones' push of +0.2 green",
    );
    assert_pixel(out[2], [1.0, 1.0, 0.7], "white takes the highlights' push of -0.3 blue");
    Ok(())
}

#[test]
fn a_grey_between_two_ranges_takes_the_smoothstep_share_of_each() -> Result<(), Error> {
    // grey 0.25 is half way through the shadow/midtone hand-over (centred on 0.25): half of each push
    let params = pushing([[0.7, 0.5, 0.5], [0.9, 0.5, 0.5], GREY]);
    assert_pixel(
        run(&params, &[[0.25; 3]])?[0],
        [0.25 + 0.5 * 0.2 + 0.5 * 0.4, 0.25, 0.25],
        "grey 0.25",
    );
    // grey 0.2: the hand-over argument is x = (0.2 - 0.25) / 0.5 + 0.5 = 0.4 and 3x^2 - 2x^3 = 0.352 of the way to the
    // midtones, so the shadows hold 0.648 and the midtones 0.352
    let share = run(&params, &[[0.2; 3]])?[0][0];
    assert!(
        (share - (0.2 + 0.648 * 0.2 + 0.352 * 0.4)).abs() <= TOLERANCE,
        "{share}"
    );
    Ok(())
}

#[test]
fn balance_softness_and_amount_set_where_the_shadows_hand_over_how_sharply_and_how_far() -> Result<(), Error> {
    let shadows = |balance, softness, amount| color_balance::Params {
        balance,
        softness,
        amount,
        ..pushing([[0.7, 0.5, 0.5], GREY, GREY])
    };
    let red = |params: &color_balance::Params, grey: f32| run(params, &[[grey; 3]]).map(|out| out[0][0]);
    // grey 0.4 is 0.104 shadow at balance 0.5 (the shadow/midtone hand-over centred on 0.25 is 0.896 done), 0.5 at
    // balance 0.8 (centred on 0.4), 0.784 at balance 1 (centred on 0.5: smoothstep(0.3) = 0.216)
    for (balance, share) in [(0.5, 0.104), (0.8, 0.5), (1.0, 0.784)] {
        let out = red(&shadows(balance, 1.0, 1.0), 0.4)?;
        assert!(
            (out - (0.4 + 0.2 * share)).abs() <= TOLERANCE,
            "balance {balance}: {out}"
        );
    }
    // greys 0.2 and 0.3 are equally far from the hand-over at 0.25, which is `softness` * 0.5 wide
    for (softness, dark, lighter) in [(1.0, 0.648, 0.352), (0.4, 0.84375, 0.15625), (0.1, 1.0, 0.0)] {
        let params = shadows(0.5, softness, 1.0);
        let (low, high) = (red(&params, 0.2)?, red(&params, 0.3)?);
        assert!(
            (low - (0.2 + 0.2 * dark)).abs() <= TOLERANCE,
            "softness {softness}: {low}"
        );
        assert!(
            (high - (0.3 + 0.2 * lighter)).abs() <= TOLERANCE,
            "softness {softness}: {high}"
        );
    }
    // amount scales the push
    let out = run(&shadows(0.5, 1.0, 0.25), &[[0.0; 3]])?;
    assert_pixel(out[0], [0.05, 0.0, 0.0], "amount 0.25 of a push of 0.2");
    Ok(())
}

#[test]
fn preserving_the_luminosity_takes_the_brightness_a_push_added_back_out_of_every_channel() -> Result<(), Error> {
    let params = |preserve_luminosity| color_balance::Params {
        preserve_luminosity,
        ..pushing([GREY, [0.6, 0.5, 0.5], GREY])
    };
    assert_pixel(run(&params(false), &[GREY])?[0], [0.6, 0.5, 0.5], "luminosity free");
    // 0.1 of red is 0.2126 * 0.1 = 0.02126 of luma (Rec. 709), taken off all three channels
    assert_pixel(
        run(&params(true), &[GREY])?[0],
        [0.578_74, 0.478_74, 0.478_74],
        "luminosity kept",
    );
    Ok(())
}

#[test]
fn the_result_is_held_to_0_and_1() -> Result<(), Error> {
    // black pushed by (-0.5, +0.5, 0) by the shadows; white pushed by (+0.5, -0.5, 0) by the highlights
    let params = pushing([[0.0, 1.0, 0.5], GREY, [1.0, 0.0, 0.5]]);
    let out = run(&params, &[[0.0; 3], [1.0; 3]])?;
    assert_pixel(out[0], [0.0, 0.5, 0.0], "black");
    assert_pixel(out[1], [1.0, 0.5, 1.0], "white");
    Ok(())
}

proptest! {
    #[test]
    fn one_push_given_to_all_three_ranges_moves_every_pixel_by_exactly_that_push(
        pixel in prop::array::uniform3(0.2_f32..=0.8), balance in 0.0_f32..=1.0, softness in 0.01_f32..=1.0,
    ) {
        let colour = [0.6, 0.4, 0.55]; // pushes of +0.1, -0.1, +0.05: too small to reach 0 or 1 from 0.2..0.8
        let params = color_balance::Params { balance, softness, ..pushing([colour; 3]) };
        let out = run(&params, &[pixel])?[0];
        for ((out, start), push) in out.into_iter().zip(pixel).zip([0.1, -0.1, 0.05]) {
            prop_assert!((out - (start + push)).abs() <= 1e-5, "{:?} -> {:?}", pixel, out);
        }
    }
}
