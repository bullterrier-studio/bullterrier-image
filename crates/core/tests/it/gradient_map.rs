//! `gradient_map`, and the colour ramp it rests on: where the stops sit, what the stretch's clip points are under each
//! quantile method, and the order of the result. Expected clip points come from `numpy.quantile`, worked out offline.

use bullterrier_image::Error;
use bullterrier_image::effects::gradient_map::{self, QuantileMethod};
use bullterrier_image::params::Color;
use proptest::prelude::*;

use crate::common::pixels::run;

const TOLERANCE: f32 = 1e-5;

const RED: [f32; 3] = [1.0, 0.0, 0.0];
const GREEN: [f32; 3] = [0.0, 1.0, 0.0];
const BLUE: [f32; 3] = [0.0, 0.0, 1.0];

fn greys(values: &[f32]) -> Vec<[f32; 3]> {
    values.iter().map(|&grey| [grey; 3]).collect()
}

/// Black to white, with `shadow_clip` and `highlight_clip` and the quantile `method`.
fn black_to_white(shadow_clip: f32, highlight_clip: f32, method: QuantileMethod) -> gradient_map::Params {
    gradient_map::Params {
        shadow_clip,
        highlight_clip,
        quantile_method: method,
        ..gradient_map::Params::default()
    }
}

fn assert_close(actual: &[[f32; 3]], expected: &[[f32; 3]], context: &str) {
    assert!(
        actual.len() == expected.len()
            && actual
                .iter()
                .zip(expected)
                .all(|(a, e)| a.iter().zip(e).all(|(a, e)| (a - e).abs() <= TOLERANCE)),
        "{context}: {actual:?}, not {expected:?}"
    );
}

#[test]
fn stops_sit_evenly_spaced_and_the_colour_between_two_is_their_linear_mix() -> Result<(), Error> {
    // five brightnesses a quarter apart over the whole range: positions 0, 1/4, 1/2, 3/4, 1 on the ramp
    let picture = greys(&[0.0, 0.25, 0.5, 0.75, 1.0]);
    let stops = |colours: &[[f32; 3]]| gradient_map::Params {
        stops: colours.iter().map(|&colour| Color(colour)).collect(),
        shadow_clip: 0.0,
        highlight_clip: 0.0,
        ..gradient_map::Params::default()
    };
    // three stops at 0, 1/2 and 1: the quarter points are halfway between neighbours
    assert_close(
        &run(&stops(&[RED, GREEN, BLUE]), &picture)?,
        &[RED, [0.5, 0.5, 0.0], GREEN, [0.0, 0.5, 0.5], BLUE],
        "three stops",
    );
    // four stops at 0, 1/3, 2/3 and 1: a quarter is three quarters of the way from the first stop to the second
    let white = [1.0; 3];
    assert_close(
        &run(&stops(&[RED, GREEN, BLUE, white]), &picture)?,
        &[RED, [0.25, 0.75, 0.0], [0.0, 0.5, 0.5], [0.25, 0.25, 1.0], white],
        "four stops",
    );
    Ok(())
}

/// A picture of `values` greys, the clip shares, and for each method the clip points `numpy.quantile` finds in the
/// values at the quantiles `shadow_clip` and `1 - highlight_clip`.
struct Quantiles {
    values: &'static [f32],
    shadow_clip: f32,
    highlight_clip: f32,
    points: [(QuantileMethod, f32, f32); 13],
}

/// `numpy.quantile(values, [shadow_clip, 1 - highlight_clip], method=...)` for each method, offline (numpy 2.5). The
/// first picture has fractional indexes throughout; the other three have whole ones at a quantile (a jump of the
/// distribution), where the discrete methods choose. In the last, `closest_observation` meets a whole index of each
/// parity: it takes the value below at 1 and the one above at 6.
const QUANTILES: [Quantiles; 4] = [
    Quantiles {
        values: &[0.0, 0.03, 0.1, 0.17, 0.3, 0.42, 0.5, 0.61, 0.78, 0.9, 1.0],
        shadow_clip: 0.15,
        highlight_clip: 0.3,
        points: [
            (QuantileMethod::Linear, 0.065, 0.61),
            (QuantileMethod::Lower, 0.03, 0.5),
            (QuantileMethod::Higher, 0.1, 0.61),
            (QuantileMethod::Nearest, 0.1, 0.61),
            (QuantileMethod::Midpoint, 0.065, 0.555),
            (QuantileMethod::InvertedCdf, 0.03, 0.61),
            (QuantileMethod::AveragedInvertedCdf, 0.03, 0.61),
            (QuantileMethod::ClosestObservation, 0.03, 0.61),
            (QuantileMethod::InterpolatedInvertedCdf, 0.019_5, 0.577),
            (QuantileMethod::Hazen, 0.040_5, 0.644),
            (QuantileMethod::Weibull, 0.024, 0.678),
            (QuantileMethod::MedianUnbiased, 0.032_333, 0.655_333),
            (QuantileMethod::NormalUnbiased, 0.034_375, 0.652_5),
        ],
    },
    Quantiles {
        values: &[0.0, 0.1, 0.2, 0.35, 0.5, 0.65, 0.8, 1.0],
        shadow_clip: 0.25,
        highlight_clip: 0.25,
        points: [
            (QuantileMethod::Linear, 0.175, 0.687_5),
            (QuantileMethod::Lower, 0.1, 0.65),
            (QuantileMethod::Higher, 0.2, 0.8),
            (QuantileMethod::Nearest, 0.2, 0.65),
            (QuantileMethod::Midpoint, 0.15, 0.725),
            (QuantileMethod::InvertedCdf, 0.1, 0.65),
            (QuantileMethod::AveragedInvertedCdf, 0.15, 0.725),
            (QuantileMethod::ClosestObservation, 0.1, 0.65),
            (QuantileMethod::InterpolatedInvertedCdf, 0.1, 0.65),
            (QuantileMethod::Hazen, 0.15, 0.725),
            (QuantileMethod::Weibull, 0.125, 0.762_5),
            (QuantileMethod::MedianUnbiased, 0.141_667, 0.737_5),
            (QuantileMethod::NormalUnbiased, 0.143_75, 0.734_375),
        ],
    },
    Quantiles {
        values: &[0.05, 0.1, 0.15, 0.3, 0.4, 0.45, 0.6, 0.7, 0.75, 0.85, 0.95, 1.0],
        shadow_clip: 0.5,
        highlight_clip: 0.25,
        points: [
            (QuantileMethod::Linear, 0.525, 0.775),
            (QuantileMethod::Lower, 0.45, 0.75),
            (QuantileMethod::Higher, 0.6, 0.85),
            (QuantileMethod::Nearest, 0.6, 0.75),
            (QuantileMethod::Midpoint, 0.525, 0.8),
            (QuantileMethod::InvertedCdf, 0.45, 0.75),
            (QuantileMethod::AveragedInvertedCdf, 0.525, 0.8),
            (QuantileMethod::ClosestObservation, 0.45, 0.75),
            (QuantileMethod::InterpolatedInvertedCdf, 0.45, 0.75),
            (QuantileMethod::Hazen, 0.525, 0.8),
            (QuantileMethod::Weibull, 0.525, 0.825),
            (QuantileMethod::MedianUnbiased, 0.525, 0.808_333),
            (QuantileMethod::NormalUnbiased, 0.525, 0.806_25),
        ],
    },
    Quantiles {
        values: &[0.0, 0.05, 0.2, 0.3, 0.4, 0.55, 0.6, 0.7, 0.85, 1.0],
        shadow_clip: 0.25,
        highlight_clip: 0.25,
        points: [
            (QuantileMethod::Linear, 0.225, 0.675),
            (QuantileMethod::Lower, 0.2, 0.6),
            (QuantileMethod::Higher, 0.3, 0.7),
            (QuantileMethod::Nearest, 0.2, 0.7),
            (QuantileMethod::Midpoint, 0.25, 0.65),
            (QuantileMethod::InvertedCdf, 0.2, 0.7),
            (QuantileMethod::AveragedInvertedCdf, 0.2, 0.7),
            (QuantileMethod::ClosestObservation, 0.05, 0.7),
            (QuantileMethod::InterpolatedInvertedCdf, 0.125, 0.65),
            (QuantileMethod::Hazen, 0.2, 0.7),
            (QuantileMethod::Weibull, 0.1625, 0.7375),
            (QuantileMethod::MedianUnbiased, 0.1875, 0.7125),
            (QuantileMethod::NormalUnbiased, 0.190_625, 0.709_375),
        ],
    },
];

#[test]
fn every_quantile_method_cuts_the_stretch_where_numpy_finds_the_clip_points() -> Result<(), Error> {
    for Quantiles {
        values,
        shadow_clip,
        highlight_clip,
        points,
    } in QUANTILES
    {
        for (method, dark, bright) in points {
            // black to white: a pixel's result is its place between the clip points, kept within 0..1
            let expected: Vec<[f32; 3]> = values
                .iter()
                .map(|&grey| [((grey - dark) / (bright - dark)).clamp(0.0, 1.0); 3])
                .collect();
            let out = run(&black_to_white(shadow_clip, highlight_clip, method), &greys(values))?;
            assert_close(&out, &expected, &format!("{method:?} over {values:?}"));
        }
    }
    Ok(())
}

#[test]
fn a_picture_of_one_brightness_maps_to_the_first_stop_whatever_the_method() -> Result<(), Error> {
    let flat = greys(&[0.4; 5]);
    let one = greys(&[0.9]);
    for method in QUANTILES[0].points.map(|(method, ..)| method) {
        let params = gradient_map::Params {
            stops: vec![Color(RED), Color(BLUE)],
            quantile_method: method,
            ..gradient_map::Params::default()
        };
        assert_eq!(run(&params, &flat)?, vec![RED; 5], "{method:?}, flat");
        assert_eq!(run(&params, &one)?, vec![RED], "{method:?}, one pixel");
    }
    Ok(())
}

proptest! {
    #[test]
    fn with_no_clip_the_darkest_pixel_is_the_first_stop_the_brightest_the_last_and_brighter_never_maps_lower(
        values in prop::collection::vec(0.0_f32..=1.0, 2..40),
    ) {
        let out = run(&black_to_white(0.0, 0.0, QuantileMethod::Linear), &greys(&values))?;
        let (least, most) = (
            values.iter().copied().fold(f32::MAX, f32::min),
            values.iter().copied().fold(f32::MIN, f32::max),
        );
        for (&grey, pixel) in values.iter().zip(&out) {
            prop_assert!(pixel.iter().all(|&channel| (0.0..=1.0).contains(&channel)));
            if most - least > 1e-3 {
                prop_assert!(grey > least || pixel[0] <= 1e-5, "the darkest, {}, gave {:?}", grey, pixel);
                prop_assert!(grey < most || pixel[0] >= 1.0 - 1e-5, "the brightest, {}, gave {:?}", grey, pixel);
            }
        }
        for (a, b) in values.iter().zip(&out).flat_map(|a| values.iter().zip(&out).map(move |b| (a, b))) {
            prop_assert!(a.0 >= b.0 || a.1[0] <= b.1[0] + 1e-5, "{} gave {:?} but {} gave {:?}", a.0, a.1, b.0, b.1);
        }
    }
}
