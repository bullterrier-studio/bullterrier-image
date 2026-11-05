//! `tone_map`: the picture compared with the effect worked out from its definition (a guided filter found window by
//! window), the hand-worked gain of an even picture, and the limits the doc promises.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "small picture sizes, and the f64 reference narrowed to the f32 picture"
)]

use bullterrier_image::Error;
use bullterrier_image::effects::tone_map::Params;

use crate::common::compare::first_difference_within;
use crate::common::guided::guided_filter;
use crate::common::pictures::varied;
use crate::common::pixels::run_picture;

/// The pictures are compared at this distance: the reference is made in f64, the effect in f32, and a gain of 2^15 at most
/// is held to 1.
const TOLERANCE: f32 = 1e-4;

/// A different value in nearly every channel of every pixel, 0..1, some of them black.
fn values(height: usize, width: usize) -> Vec<f32> {
    let mut picture = varied(height, width);
    picture.iter_mut().step_by(29).for_each(|value| *value = 0.0);
    picture
}

/// The effect by its definition: the base is the guided filter of the base 2 logarithm of the Rec. 709 luma (floored at
/// 1e-4) guided by itself, and each pixel is multiplied by 2^((contrast - 1) (base + pivot) + amount (log luma - base)).
fn reference(params: &Params, height: usize, width: usize, values: &[f32]) -> Vec<f32> {
    let log_luma: Vec<f64> = values
        .as_chunks::<3>()
        .0
        .iter()
        .map(|pixel| {
            let luma = 0.2126 * f64::from(pixel[0]) + 0.7152 * f64::from(pixel[1]) + 0.0722 * f64::from(pixel[2]);
            luma.max(1e-4).log2()
        })
        .collect();
    let radius = f64::from(params.radius).round_ties_even() as usize;
    let epsilon = f64::from(params.edge_threshold).powi(2);
    let base = guided_filter(&log_luma, &log_luma, height, width, radius, epsilon);
    let (contrast, amount, pivot) = (
        f64::from(params.contrast),
        f64::from(params.amount),
        f64::from(params.pivot),
    );
    values
        .as_chunks::<3>()
        .0
        .iter()
        .enumerate()
        .flat_map(|(at, pixel)| {
            let gain = ((contrast - 1.0) * (base[at] + pivot) + amount * (log_luma[at] - base[at])).exp2();
            pixel
                .iter()
                .map(move |&channel| (f64::from(channel) * gain).clamp(0.0, 1.0) as f32)
        })
        .collect()
}

fn with(contrast: f32, amount: f32, pivot: f32, radius: f32, edge_threshold: f32) -> Params {
    Params {
        contrast,
        amount,
        pivot,
        radius,
        edge_threshold,
    }
}

#[test]
fn the_picture_is_the_tone_map_of_the_definition() -> Result<(), Error> {
    // all five parameters at values of their own; a radius of a half rounds to the even whole number (2.5 to 2, 3.5 to 4);
    // pictures from one pixel to a few, with windows smaller and larger than they are
    let shapes = [(1, 1), (1, 9), (9, 1), (6, 7), (13, 11)];
    let looks = [
        with(0.3, 1.5, 2.0, 2.0, 0.3),
        with(0.7, 0.2, 0.5, 40.0, 8.0),
        with(0.0, 5.0, 8.0, 1.0, 0.01),
        with(0.5, 0.5, 0.0, 0.0, 0.5),
        with(0.45, 2.5, 1.25, 2.5, 0.12),
        with(0.45, 2.5, 1.25, 3.5, 0.12),
    ];
    for params in looks {
        for (height, width) in shapes {
            let picture = values(height, width);
            let at = first_difference_within(
                &run_picture(&params, height, width, &picture)?,
                &reference(&params, height, width, &picture),
                TOLERANCE,
            );
            assert_eq!(at, None, "{params:?}, {height} x {width}: first differs at {at:?}");
        }
    }
    Ok(())
}

#[test]
fn the_comparison_tells_another_look_apart() -> Result<(), Error> {
    // the known-bad pair: the reference of another radius and of another threshold is not the tone map
    let picture = values(9, 11);
    let params = with(0.4, 1.0, 1.0, 2.0, 0.2);
    let found = run_picture(&params, 9, 11, &picture)?;
    assert_eq!(
        first_difference_within(&found, &reference(&params, 9, 11, &picture), TOLERANCE),
        None
    );
    for other in [
        Params {
            radius: 3.0,
            ..params.clone()
        },
        Params {
            edge_threshold: 2.0,
            ..params.clone()
        },
    ] {
        assert!(first_difference_within(&found, &reference(&other, 9, 11, &picture), TOLERANCE).is_some());
    }
    Ok(())
}

#[test]
fn an_even_picture_comes_out_with_the_gain_worked_out_by_hand() -> Result<(), Error> {
    // a grey v is multiplied by (v 2^pivot)^(contrast - 1); a colour keeps its channel ratios (luma 0.33582 for the
    // second row, so the gain is 0.33582^-0.5); a very dark grey is not floored (0.001 > 1e-4); black stays black
    let rows: [([f32; 3], f32, f32, [f32; 3]); 7] = [
        ([0.25; 3], 0.5, 0.0, [0.5; 3]),
        ([0.2, 0.4, 0.1], 0.5, 0.0, [0.345_125_3, 0.690_250_5, 0.172_562_6]),
        ([0.001; 3], 0.5, 0.0, [0.031_622_8; 3]),
        ([0.0; 3], 0.5, 0.0, [0.0; 3]),
        ([1.0; 3], 0.5, 2.0, [0.5; 3]),
        ([0.25; 3], 0.5, 1.0, [0.353_553_4; 3]),
        ([0.25; 3], 0.0, 0.0, [1.0; 3]),
    ];
    for (colour, contrast, pivot, expected) in rows {
        let picture: Vec<f32> = colour.repeat(5 * 6);
        let found = run_picture(&with(contrast, 0.7, pivot, 3.0, 0.5), 5, 6, &picture)?;
        for pixel in found.as_chunks::<3>().0 {
            let near = pixel.iter().zip(expected).all(|(a, b)| (a - b).abs() < 1e-5);
            assert!(
                near,
                "{colour:?}, contrast {contrast}, pivot {pivot}: {pixel:?} is not {expected:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn radius_0_is_a_global_curve_and_contrast_1_amount_0_leaves_the_picture_alone() -> Result<(), Error> {
    // radius 0: every pixel is its own base, so a grey v becomes v^contrast whatever its neighbours (and `amount` has no
    // detail to raise); contrast 1 and amount 0 multiply by 2^0
    let greys: Vec<f32> = (0..7 * 5).flat_map(|at| [0.05 + 0.025 * at as f32; 3]).collect();
    let curved = run_picture(&with(0.6, 3.0, 0.0, 0.0, 0.5), 7, 5, &greys)?;
    for (found, grey) in curved.iter().zip(&greys) {
        assert!((found - grey.powf(0.6)).abs() < 1e-5, "{grey} became {found}");
    }
    let picture = values(7, 5);
    let same = run_picture(&with(1.0, 0.0, 3.0, 4.0, 0.5), 7, 5, &picture)?;
    assert_eq!(first_difference_within(&same, &picture, TOLERANCE), None);
    Ok(())
}
