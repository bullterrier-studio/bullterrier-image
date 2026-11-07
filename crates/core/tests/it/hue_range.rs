//! The range of hues `selective_color` changes: its centre, width and feather, and the weight that scales each change
//! (the place in the range times the pixel's HSV saturation). Colours and expectations are worked out by hand.

use bullterrier_image::Error;
use bullterrier_image::effects::selective_color;

use crate::common::pixels::run;

const TOLERANCE: f32 = 1e-6;

fn assert_pixel(actual: [f32; 3], expected: [f32; 3], context: &str) {
    assert!(
        actual.iter().zip(expected).all(|(a, e)| (a - e).abs() <= TOLERANCE),
        "{context}: {actual:?}, not {expected:?}"
    );
}

/// (hue of a colour of full saturation and value, hue shift, the colour after it), for the range centre 0, width 40
/// (full weight up to 20 degrees off the centre) and feather 20 (no weight from 40 degrees on).
const SHIFTED: [(f32, f32, [f32; 3], [f32; 3]); 6] = [
    (0.0, 120.0, [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
    (0.0, -60.0, [1.0, 0.0, 0.0], [1.0, 0.0, 1.0]), // a negative shift wraps round to magenta
    (15.0, 45.0, [1.0, 0.25, 0.0], [1.0, 1.0, 0.0]), // still fully inside: 15 + 45 = 60, yellow
    (30.0, 60.0, [1.0, 0.5, 0.0], [1.0, 1.0, 0.0]), // half way down the feather: half the shift, 30 + 30
    (350.0, 30.0, [1.0, 0.0, 1.0 / 6.0], [1.0, 1.0 / 3.0, 0.0]), // the range wraps round red: 350 is 10 below
    (120.0, 90.0, [0.0, 1.0, 0.0], [0.0, 1.0, 0.0]), // outside the feather nothing moves
];

#[test]
fn the_shift_is_scaled_by_the_place_of_a_hue_in_the_range() -> Result<(), Error> {
    for (hue, hue_shift, colour, expected) in SHIFTED {
        let params = selective_color::Params {
            hue: 0.0,
            width: 40.0,
            feather: 20.0,
            hue_shift,
            ..selective_color::Params::default()
        };
        assert_pixel(
            run(&params, &[colour])?[0],
            expected,
            &format!("hue {hue} shifted by {hue_shift}"),
        );
    }
    Ok(())
}

/// (colour, saturation gain, lightness, the colour after it) for a range of every hue at full weight: a colour of full
/// saturation has weight 1, one of saturation 0.5 has weight 0.5, so its change is half as strong.
const CHANGED: [([f32; 3], f32, f32, [f32; 3]); 9] = [
    ([1.0, 0.0, 0.0], 0.0, 0.0, [1.0, 1.0, 1.0]), // saturation 0 turns red grey, keeping its value
    ([1.0, 0.0, 0.0], 0.5, 0.0, [1.0, 0.5, 0.5]), // halves the saturation
    ([1.0, 0.0, 0.0], 3.0, 0.0, [1.0, 0.0, 0.0]), // saturation cannot go past full
    ([1.0, 0.0, 0.0], 1.0, -0.5, [0.5, 0.0, 0.0]), // halves the value
    ([0.5, 0.0, 0.0], 1.0, 0.5, [0.75, 0.0, 0.0]), // goes half way to the full value
    ([1.0, 0.0, 0.0], 1.0, -1.0, [0.0, 0.0, 0.0]), // black
    ([0.5, 0.0, 0.0], 1.0, 1.0, [1.0, 0.0, 0.0]), // the full value
    ([0.5, 0.25, 0.25], 1.0, -0.5, [0.375, 0.1875, 0.1875]), // a pale colour (saturation 0.5) changes less
    ([0.5, 0.25, 0.25], 0.0, 0.0, [0.5, 0.375, 0.375]), // and loses only half its saturation
];

#[test]
fn saturation_and_lightness_change_the_selected_colours_by_their_weight() -> Result<(), Error> {
    for (colour, saturation, lightness, expected) in CHANGED {
        let params = selective_color::Params {
            width: 360.0,
            feather: 0.0,
            saturation,
            lightness,
            ..selective_color::Params::default()
        };
        assert_pixel(
            run(&params, &[colour])?[0],
            expected,
            &format!("{colour:?}, saturation {saturation}, lightness {lightness}"),
        );
    }
    Ok(())
}

#[test]
fn the_centre_of_the_range_can_lie_anywhere_on_the_wheel() -> Result<(), Error> {
    // centre 120 (green), width 0 and feather 60: green is at full weight, yellow (60 off) and cyan (60 off) at none
    let params = selective_color::Params {
        hue: 120.0,
        width: 0.0,
        feather: 60.0,
        hue_shift: 60.0,
        ..selective_color::Params::default()
    };
    let out = run(
        &params,
        &[[0.0, 1.0, 0.0], [1.0, 1.0, 0.0], [0.0, 1.0, 1.0], [1.0, 0.0, 0.0]],
    )?;
    assert_pixel(out[0], [0.0, 1.0, 1.0], "green turned to cyan");
    assert_eq!(out[1..], [[1.0, 1.0, 0.0], [0.0, 1.0, 1.0], [1.0, 0.0, 0.0]]);
    Ok(())
}

#[test]
fn a_range_without_feather_ends_where_its_width_ends() -> Result<(), Error> {
    // width 40 is 20 degrees either side of the centre 0; the hues 19.5 (inside) and 20.5 (outside) have full saturation
    // and value, so green is hue / 60 and the hue shift of 90 takes 19.5 to 109.5: red (120 - 109.5) / 60, green 1
    let params = selective_color::Params {
        hue: 0.0,
        width: 40.0,
        feather: 0.0,
        hue_shift: 90.0,
        ..selective_color::Params::default()
    };
    let out = run(&params, &[[1.0, 19.5 / 60.0, 0.0], [1.0, 20.5 / 60.0, 0.0]])?;
    assert_pixel(out[0], [0.175, 1.0, 0.0], "inside");
    assert_eq!(out[1], [1.0, 20.5 / 60.0, 0.0], "outside");
    Ok(())
}
