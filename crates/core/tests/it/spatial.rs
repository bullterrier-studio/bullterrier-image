//! `vignette`, `scanlines` and `equalize`: values worked out by hand from their documented formulas.

use bullterrier_image::effects::vignette;
use bullterrier_image::params::{Color, EffectParams};
use bullterrier_image::{Error, Inputs, Picture, apply};
use proptest::prelude::*;
use serde_json::Value;

use crate::common::pixels::run;

const TOLERANCE: f32 = 1e-5;

/// `params` applied to the `height` x `width` grey picture of `greys` (one value per pixel); one channel's values.
fn grid_of<P: EffectParams>(params: &P, height: usize, width: usize, greys: &[f32]) -> Result<Vec<f32>, Error> {
    let values: Vec<f32> = greys.iter().flat_map(|&grey| [grey; 3]).collect();
    let image = apply(
        P::EFFECT,
        &Picture::new(height, width, &values)?,
        Value::Object(params.values()),
        &Inputs::default(),
    )?;
    Ok(image.into_values().chunks(3).map(|pixel| pixel[0]).collect())
}

/// `params` applied to a picture of `height` rows of `width` pixels, each `grey` in all channels; one channel's values.
fn grid<P: EffectParams>(params: &P, height: usize, width: usize, grey: f32) -> Result<Vec<f32>, Error> {
    grid_of(params, height, width, &vec![grey; height * width])
}

fn assert_close(actual: &[f32], expected: &[f32], context: &str) {
    assert!(
        actual.len() == expected.len() && actual.iter().zip(expected).all(|(a, e)| (a - e).abs() <= TOLERANCE),
        "{context}: {actual:?}, not {expected:?}"
    );
}

// --- vignette ----------------------------------------------------------------------------------------------------

/// Black fading to white from 0.5 out over 0.5, so a pixel's value is its smoothstep share.
fn to_white(roundness: f32) -> vignette::Params {
    vignette::Params {
        amount: 1.0,
        radius: 0.5,
        feather: 0.5,
        roundness,
        color: Color([1.0; 3]),
    }
}

#[test]
fn a_rectangle_fades_by_the_larger_offset_along_a_smoothstep() -> Result<(), Error> {
    // one row of 4 pixels: offsets 0.75, 0.25, 0.25, 0.75; 0.75 is halfway through the fade (smoothstep 0.5), 0.25 before it
    let out = grid(&to_white(0.0), 1, 4, 0.0)?;
    assert_close(&out, &[0.5, 0.0, 0.0, 0.5], "a rectangle, one row");
    Ok(())
}

#[test]
fn an_ellipse_reaches_a_corner_less_far_than_a_rectangle() -> Result<(), Error> {
    // distance 0.75 x sqrt(1/2) = 0.53033 (offset 0.75 across, 0 down), so t = 0.06066 and 3t^2 - 2t^3 = 0.0105926
    let out = grid(&to_white(1.0), 1, 4, 0.0)?;
    assert_close(&out, &[0.010_592_55, 0.0, 0.0, 0.010_592_55], "an ellipse, one row");
    Ok(())
}

#[test]
fn a_rounded_rectangle_takes_the_norm_with_exponent_two_over_roundness() -> Result<(), Error> {
    // 2 rows of 4: the corner pixel is 0.5 down and 0.75 across; exponent 4: 0.75 x ((0.5 / 0.75)^4 + 1)^(1/4) / 2^(1/4)
    // = 0.65974, so t = 0.31949 and the share is 0.240996
    let out = grid(&to_white(0.5), 2, 4, 0.0)?;
    assert_close(&out[..1], &[0.240_995_8], "the corner of a rounded rectangle");
    Ok(())
}

#[test]
fn the_middle_pixel_of_an_odd_picture_is_left_alone_even_with_a_hard_edge_at_radius_zero() -> Result<(), Error> {
    let params = vignette::Params {
        radius: 0.0,
        feather: 0.0,
        amount: 1.0,
        ..vignette::Params::default()
    };
    let out = grid(&params, 3, 3, 0.7)?;
    assert!(out.iter().all(|value| value.is_finite()), "{out:?}");
    assert_close(&out[4..5], &[0.7], "the middle of 3 x 3");
    assert_close(&out[..1], &[0.0], "a corner, fully faded to black");
    Ok(())
}

proptest! {
    #[test]
    fn a_pixel_ends_between_its_own_value_and_the_colour(
        value in 0.0_f32..=1.0, amount in 0.0_f32..=1.0, radius in 0.0_f32..=1.0, feather in 0.0_f32..=1.0,
        roundness in 0.0_f32..=1.0, tint in 0.0_f32..=1.0,
    ) {
        let params = vignette::Params { amount, radius, feather, roundness, color: Color([tint; 3]) };
        let out = run(&params, &[[value; 3]; 7])?;
        let (low, high) = (value.min(tint) - 1e-6, value.max(tint) + 1e-6);
        prop_assert!(out.iter().flatten().all(|v| (low..=high).contains(v)), "{:?}", out);
    }
}
