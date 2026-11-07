//! `grade`, the stages `colour.rs` leaves out: the colour balance by temperature, exposure, contrast around a pivot,
//! vibrance, and the order they run in. Expected values are worked out by hand, or in double precision offline from the
//! sRGB transfer function (IEC 61966-2-1) and the standard library's HSV conversion (`colorsys`).

use bullterrier_image::Error;
use bullterrier_image::effects::grade;

use crate::common::pixels::run;

const TOLERANCE: f32 = 1e-5;

fn assert_pixel(actual: [f32; 3], expected: [f32; 3], context: &str) {
    assert!(
        actual.iter().zip(expected).all(|(a, e)| (a - e).abs() <= TOLERANCE),
        "{context}: {actual:?}, not {expected:?}"
    );
}

#[test]
fn an_exposure_of_one_stop_doubles_every_value() -> Result<(), Error> {
    let out = run(
        &grade::Params {
            exposure: 1.0,
            ..grade::Params::default()
        },
        &[[0.2; 3]],
    )?;
    assert_pixel(out[0], [0.4; 3], "one stop up");
    Ok(())
}

#[test]
fn contrast_pushes_values_away_from_the_pivot() -> Result<(), Error> {
    let greys = [[0.3; 3], [0.4; 3], [0.5; 3]];
    let at = |pivot| {
        run(
            &grade::Params {
                contrast: 2.0,
                pivot,
                ..grade::Params::default()
            },
            &greys,
        )
    };
    // pivot + (v - pivot) * 2
    for (pixel, expected) in at(0.3)?.into_iter().zip([0.3, 0.5, 0.7]) {
        assert_pixel(pixel, [expected; 3], "pivot 0.3");
    }
    for (pixel, expected) in at(0.5)?.into_iter().zip([0.1, 0.3, 0.5]) {
        assert_pixel(pixel, [expected; 3], "pivot 0.5");
    }
    Ok(())
}

#[test]
fn the_brightness_saturation_keeps_is_measured_after_the_contrast_is_clipped() -> Result<(), Error> {
    // contrast 2 around 0.5 takes (0.9, 0.5, 0.3) to (1.3, 0.5, 0.1), clipped to (1.0, 0.5, 0.1); its Rec. 709 grey is
    // 0.2126 * 1.0 + 0.7152 * 0.5 + 0.0722 * 0.1 = 0.57742 (unclipped it would be 0.64120)
    let out = run(
        &grade::Params {
            contrast: 2.0,
            saturation: 0.0,
            ..grade::Params::default()
        },
        &[[0.9, 0.5, 0.3]],
    )?;
    assert_pixel(out[0], [0.57742; 3], "grey after the clip");
    Ok(())
}

/// (settings, grey 0.5 after it): temperature and tint scale the linear light by gains whose largest is 1, worked
/// offline: temperature 1 stop = red x 2**0.5, blue x 2**-0.5, tint 1 = green x 2**-1, each divided by the largest.
const BALANCED: [(f32, f32, [f32; 3]); 5] = [
    (1.0, 0.0, [0.5, 0.425_372_8, 0.360_780_2]),
    (-1.0, 0.0, [0.360_780_2, 0.425_372_8, 0.5]),
    (2.0, 0.0, [0.5, 0.360_780_2, 0.256_483_2]),
    (0.0, -1.0, [0.360_780_2, 0.5, 0.360_780_2]),
    (1.0, 1.0, [0.5, 0.304_873, 0.360_780_2]),
];

#[test]
fn temperature_and_tint_scale_the_channels_in_linear_light_by_gains_whose_largest_is_one() -> Result<(), Error> {
    for (temperature, tint, expected) in BALANCED {
        let params = grade::Params {
            temperature,
            tint,
            ..grade::Params::default()
        };
        assert_pixel(
            run(&params, &[[0.5; 3]])?[0],
            expected,
            &format!("temperature {temperature}, tint {tint}"),
        );
    }
    // each channel has its own gain: red keeps its value (gain 1), green and blue lose a half and a full stop of
    // temperature of linear light
    let warm = grade::Params {
        temperature: 1.0,
        ..grade::Params::default()
    };
    assert_pixel(
        run(&warm, &[[0.8, 0.4, 0.2]])?[0],
        [0.8, 0.338_819_1, 0.136_034_2],
        "a coloured pixel",
    );
    Ok(())
}

#[test]
fn the_colour_balance_is_set_before_the_exposure() -> Result<(), Error> {
    let params = grade::Params {
        temperature: 1.0,
        exposure: -1.0,
        ..grade::Params::default()
    };
    // the balanced pixel (0.8, 0.3388191, 0.1360342) halved
    assert_pixel(
        run(&params, &[[0.8, 0.4, 0.2]])?[0],
        [0.4, 0.169_409_6, 0.068_017_1],
        "balance, then exposure",
    );
    Ok(())
}

/// (pixel, vibrance, the pixel after it): HSV saturation S = (max - min) / max rises to S + vibrance * S * (1 - S)
/// with the strongest channel kept (`colorsys`, offline).
const VIBRANT: [([f32; 3], f32, [f32; 3]); 6] = [
    ([0.6, 0.5, 0.4], 0.5, [0.6, 0.466_667, 0.333_333]), // S = 1/3 rises to 0.4444
    ([0.6, 0.5, 0.4], 1.0, [0.6, 0.433_333, 0.266_667]), // S rises to 0.5556
    ([0.6, 0.5, 0.4], 0.0, [0.6, 0.5, 0.4]),             // none
    ([1.0, 0.0, 0.0], 1.0, [1.0, 0.0, 0.0]),             // a fully saturated colour has no room to rise
    ([0.5; 3], 1.0, [0.5; 3]),                           // a grey stays grey
    ([0.0; 3], 1.0, [0.0; 3]),                           // black stays black
];

#[test]
fn vibrance_raises_a_colours_saturation_by_the_documented_amount() -> Result<(), Error> {
    for (pixel, vibrance, expected) in VIBRANT {
        let params = grade::Params {
            vibrance,
            ..grade::Params::default()
        };
        assert_pixel(
            run(&params, &[pixel])?[0],
            expected,
            &format!("{pixel:?}, vibrance {vibrance}"),
        );
    }
    Ok(())
}

#[test]
fn vibrance_works_on_the_saturated_picture() -> Result<(), Error> {
    // saturation 2 takes (0.6, 0.5, 0.4) to (0.68596, 0.48596, 0.28596) (Rec. 709 grey 0.51404); vibrance 1 then
    // raises that colour's S = 0.58315 to 0.8262 (`colorsys`, offline)
    let params = grade::Params {
        saturation: 2.0,
        vibrance: 1.0,
        ..grade::Params::default()
    };
    assert_pixel(
        run(&params, &[[0.6, 0.5, 0.4]])?[0],
        [0.68596, 0.402_585, 0.11921],
        "saturation, then vibrance",
    );
    Ok(())
}
