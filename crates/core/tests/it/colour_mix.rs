//! `channel_mixer`: values worked out by hand from its documented formula.

use bullterrier_image::Error;
use bullterrier_image::effects::channel_mixer;
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
