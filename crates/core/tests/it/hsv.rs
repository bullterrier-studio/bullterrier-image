//! The HSV conversions `selective_color` rests on, seen through the effect: its range of hues selects by the converted
//! hue and the converted pixel comes back through the inverse. Expected colours are worked out by the standard library's
//! `colorsys` (`hsv_to_rgb` of the shifted hue, offline) and by hand.

use bullterrier_image::Error;
use bullterrier_image::effects::selective_color;
use proptest::prelude::*;

use crate::common::pixels::run;

const TOLERANCE: f32 = 5e-6;

/// The parameters of a range that takes in every colour at full weight, with nothing changed.
fn every_hue() -> selective_color::Params {
    selective_color::Params {
        width: 360.0,
        feather: 0.0,
        ..selective_color::Params::default()
    }
}

fn assert_pixel(actual: [f32; 3], expected: [f32; 3], context: &str) {
    assert!(
        actual.iter().zip(expected).all(|(a, e)| (a - e).abs() <= TOLERANCE),
        "{context}: {actual:?}, not {expected:?}"
    );
}

/// (colour of full saturation, hue shift in degrees, the colour with its hue turned by that much): `colorsys`
/// `hsv_to_rgb(h, 1, v)` before and after; the first six rows turn each primary and secondary on to the next.
const TURNED: [([f32; 3], f32, [f32; 3]); 11] = [
    ([1.0, 0.0, 0.0], 60.0, [1.0, 1.0, 0.0]),
    ([1.0, 1.0, 0.0], 60.0, [0.0, 1.0, 0.0]),
    ([0.0, 1.0, 0.0], 60.0, [0.0, 1.0, 1.0]),
    ([0.0, 1.0, 1.0], 60.0, [0.0, 0.0, 1.0]),
    ([0.0, 0.0, 1.0], 60.0, [1.0, 0.0, 1.0]),
    ([1.0, 0.0, 1.0], 60.0, [1.0, 0.0, 0.0]),
    ([0.266_667, 0.8, 0.0], 70.0, [0.0, 0.8, 0.666_667]),
    ([0.1, 0.0, 0.6], -130.0, [0.0, 0.6, 0.0]),
    ([0.9, 0.0, 0.45], 95.0, [0.825, 0.9, 0.0]),
    ([0.5, 0.083_333, 0.0], -25.0, [0.5, 0.0, 0.125]),
    ([0.0, 0.666_667, 1.0], 170.0, [1.0, 0.166_667, 0.0]),
];

#[test]
fn a_colour_of_full_saturation_turns_round_the_wheel_as_the_standard_library_turns_it() -> Result<(), Error> {
    for (colour, shift, turned) in TURNED {
        let out = run(
            &selective_color::Params {
                hue_shift: shift,
                ..every_hue()
            },
            &[colour],
        )?;
        assert_pixel(out[0], turned, &format!("{colour:?} turned by {shift}"));
    }
    Ok(())
}

#[test]
fn a_turn_forwards_and_the_same_turn_backwards_give_the_same_colour() -> Result<(), Error> {
    let turn = |hue_shift| {
        run(
            &selective_color::Params {
                hue_shift,
                ..every_hue()
            },
            &[[0.9, 0.0, 0.3], [0.2, 0.7, 0.0]],
        )
    };
    let (forwards, backwards) = (turn(180.0)?, turn(-180.0)?);
    for (a, b) in forwards.into_iter().zip(backwards) {
        assert_pixel(a, b, "half a turn either way");
    }
    Ok(())
}

#[test]
fn the_pixels_outside_the_range_and_the_greys_come_back_bit_for_bit() -> Result<(), Error> {
    // red (hue 0) is the centre; green (120) and blue (240) are far outside width 40 and feather 20
    let pixels = [
        [0.3, 0.3, 0.3],
        [0.0, 1.0, 0.0],
        [0.1, 0.2, 0.9],
        [0.0, 0.0, 0.0],
        [1.0, 1.0, 1.0],
    ];
    let changed = selective_color::Params {
        hue: 0.0,
        width: 40.0,
        feather: 20.0,
        hue_shift: 90.0,
        saturation: 0.2,
        lightness: -0.5,
    };
    assert_eq!(run(&changed, &pixels)?, pixels);
    // a grey is not selected by a range of every hue, either
    let greys = [[0.0; 3], [0.25; 3], [0.5; 3], [1.0; 3]];
    let every = selective_color::Params {
        hue_shift: 90.0,
        saturation: 3.0,
        lightness: -1.0,
        ..every_hue()
    };
    assert_eq!(run(&every, &greys)?, greys);
    Ok(())
}

#[test]
fn a_hue_centre_past_the_full_turn_is_the_centre_inside_it() -> Result<(), Error> {
    let at = |hue| {
        run(
            &selective_color::Params {
                hue,
                hue_shift: 40.0,
                ..selective_color::Params::default()
            },
            &[[0.9, 0.2, 0.15], [0.9, 0.5, 0.0], [0.6, 0.0, 0.3]],
        )
    };
    assert_eq!(at(361.0)?, at(1.0)?);
    assert_ne!(at(1.0)?, at(181.0)?);
    Ok(())
}

proptest! {
    #[test]
    fn a_pixel_goes_round_the_hsv_conversions_and_comes_back_unchanged(pixel in prop::array::uniform3(0.0_f32..=1.0)) {
        let out = run(&every_hue(), &[pixel])?;
        prop_assert!(
            out[0].iter().zip(pixel).all(|(a, e)| (a - e).abs() <= 2e-6),
            "{:?} came back as {:?}", pixel, out[0]
        );
    }

    #[test]
    fn a_hue_shift_keeps_the_value_and_the_saturation_of_a_vivid_colour_and_two_shifts_add(
        value in 0.1_f32..=1.0, share in 0.0_f32..=1.0, order in 0_usize..6, first in -90.0_f32..=90.0, second in -90.0_f32..=90.0,
    ) {
        // a colour of full saturation has a channel at 0 and one at `value`: in any order, so any of the six sectors
        let [zero, partial, full] = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]][order];
        let mut pixel = [0.0; 3];
        (pixel[partial], pixel[full]) = (share * value, value);
        prop_assert!(pixel[zero] == 0.0);
        let shifted = |hue_shift| selective_color::Params { hue_shift, ..every_hue() };
        let once = run(&shifted(first), &[pixel])?[0];
        let (most, least) = (once[0].max(once[1]).max(once[2]), once[0].min(once[1]).min(once[2]));
        prop_assert!((most - value).abs() <= 1e-5 && least.abs() <= 1e-5, "{:?} turned by {}: {:?}", pixel, first, once);
        let twice = run(&shifted(second), &[once])?[0];
        let together = run(&shifted(first + second), &[pixel])?[0];
        prop_assert!(
            twice.iter().zip(together).all(|(x, y)| (x - y).abs() <= 1e-4),
            "{:?} turned by {} and {}: {:?}, not {:?}", pixel, first, second, twice, together
        );
    }
}
