//! `anamorphic_streaks`: light smeared along the rows, compared with a sum worked out from its definition (summed row by
//! row and by the Fourier transform), and the properties a screen keeps.

#![expect(
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    reason = "small picture sizes"
)]

use bullterrier_image::effects::anamorphic_streaks::Params;
use bullterrier_image::params::Color;
use proptest::prelude::*;

use crate::common::compare::first_difference_within;
use crate::common::convolution::{Beyond, Tap, convolved};
use crate::common::pictures::{impulse, varied};
use crate::common::pixels::run_picture;

/// The pictures are compared at this distance: sums in f32 or by the transform, and the reference in f64.
const TOLERANCE: f32 = 1e-4;

fn streaks(params: &Params, height: usize, width: usize, picture: &[f32]) -> Vec<f32> {
    run_picture(params, height, width, picture).unwrap_or_default()
}

/// The weights of a streak along a row of `width` pixels: exp(-|distance| / length) to the farthest two columns of a row
/// can be apart, scaled to sum to 1.
fn row_taps(length: f32, width: usize) -> Vec<Tap> {
    let reach = width as i64 - 1;
    let total: f64 = (-reach..=reach)
        .map(|d| (-(d.abs() as f64) / f64::from(length)).exp())
        .sum();
    (-reach..=reach)
        .map(|d| (0, d, (-(d.abs() as f64) / f64::from(length)).exp() / total))
        .collect()
}

/// `picture` with the streaks of `params` worked out from the definition: each channel's highlight sends light along its
/// row to the pixel `d` columns away with the weight of `row_taps`, nothing enters from beyond the picture, and the light
/// times `amount` and `color` is screened over the picture.
fn by_definition(params: &Params, height: usize, width: usize, picture: &[f32]) -> Vec<f32> {
    let lit: Vec<f32> = picture
        .iter()
        .map(|value| ((value - params.threshold) / (1.0 - params.threshold)).clamp(0.0, 1.0))
        .collect();
    let light = convolved(&row_taps(params.length, width), Beyond::Black, height, width, &lit);
    picture
        .iter()
        .zip(&light)
        .enumerate()
        .map(|(index, (base, light))| {
            let strength = params.amount * params.color.0[index % 3] * light;
            base + strength.clamp(0.0, 1.0) * (1.0 - base)
        })
        .collect()
}

#[test]
fn the_picture_is_what_a_pixel_by_pixel_computation_from_the_definition_gives() {
    // widths up to 16 are summed row by row, wider ones filtered by the Fourier transform; lengths from a pixel to far
    // more than the width (the tail then runs on to the edge)
    let shapes = [(1, 1), (1, 9), (9, 1), (5, 7), (6, 37), (3, 100)];
    for (length, threshold, amount, color) in [
        (1.0, 0.8, 1.5, Color([1.0, 0.5, 0.25])),
        (9.0, 0.85, 0.8, Color([1.0, 1.0, 1.0])),
        (37.0, 0.9, 2.0, Color([0.2, 0.4, 1.0])),
        (400.0, 0.7, 6.0, Color([0.56, 0.72, 1.0])),
    ] {
        let params = Params {
            length,
            threshold,
            amount,
            color,
        };
        for (height, width) in shapes {
            let picture = varied(height, width);
            let at = first_difference_within(
                &streaks(&params, height, width, &picture),
                &by_definition(&params, height, width, &picture),
                TOLERANCE,
            );
            assert_eq!(at, None, "length {length}, {height} x {width}: first differs at {at:?}");
        }
    }
}

#[test]
fn the_comparison_tells_streaks_that_differ_apart() {
    // the known-bad rows: another length, another threshold and another colour are not these streaks; and the light does
    // not saturate, so the differences show
    let picture = varied(6, 37);
    let params = Params {
        length: 9.0,
        threshold: 0.85,
        amount: 0.8,
        color: Color([1.0, 0.5, 0.25]),
    };
    let ours = streaks(&params, 6, 37, &picture);
    let unsaturated = ours
        .iter()
        .zip(&picture)
        .filter(|(out, base)| out > base && **out < 0.99)
        .count();
    assert!(unsaturated > 50, "{unsaturated}");
    for other in [
        Params {
            length: 7.0,
            ..params.clone()
        },
        Params {
            threshold: 0.8,
            ..params.clone()
        },
        Params {
            color: Color([0.25, 0.5, 1.0]),
            ..params.clone()
        },
    ] {
        assert!(first_difference_within(&ours, &by_definition(&other, 6, 37, &picture), TOLERANCE).is_some());
    }
}

#[test]
fn a_single_bright_pixel_spreads_along_its_row_only_and_nothing_enters_from_beyond_the_edge() {
    let (height, width) = (9, 40);
    let params = Params {
        length: 8.0,
        threshold: 0.5,
        amount: 3.0,
        color: Color([1.0, 0.5, 0.25]),
    };
    let out = streaks(&params, height, width, &impulse(height, width, 4, 12));
    assert!(out[(4 * width + 30) * 3] > 0.0, "far along the row");
    for row in (0..height).filter(|row| *row != 4) {
        assert!(
            out[row * width * 3..(row + 1) * width * 3]
                .iter()
                .all(|value| *value == 0.0),
            "row {row}"
        );
    }
    // a light on the left edge: the streak runs right, and nothing wraps round to the right edge
    let edge = streaks(
        &Params {
            length: 2.0,
            amount: 10.0,
            color: Color([1.0, 1.0, 1.0]),
            ..params
        },
        height,
        width,
        &impulse(height, width, 4, 0),
    );
    assert!(edge[(4 * width + 1) * 3] > 0.1);
    assert!(edge[(4 * width + 39) * 3] < 1e-6);
    assert!((edge[4 * width * 3] - 1.0).abs() < 1e-6);
}

#[test]
fn a_longer_streak_reaches_further_and_lights_less_close_by() {
    let (height, width) = (5, 81);
    let picture = impulse(height, width, 2, 40);
    let row = |length: f32| {
        let params = Params {
            length,
            threshold: 0.5,
            amount: 1.0,
            ..Params::default()
        };
        streaks(&params, height, width, &picture)[2 * width * 3..3 * width * 3]
            .iter()
            .step_by(3)
            .copied()
            .collect::<Vec<_>>()
    };
    let (short, long) = (row(4.0), row(32.0));
    assert!(long[70] > short[70], "30 px away the long streak is brighter");
    assert!(long[41] < short[41], "next to the source the short one is");
}

#[test]
fn white_keeps_the_colour_of_the_light_and_a_colour_tints_it() {
    let (height, width) = (9, 40);
    let mut red = vec![0.0_f32; height * width * 3];
    red[(4 * width + 12) * 3] = 1.0;
    let params = Params {
        length: 8.0,
        threshold: 0.5,
        amount: 3.0,
        color: Color([1.0, 1.0, 1.0]),
    };
    let out = streaks(&params, height, width, &red);
    assert!(out[(4 * width + 20) * 3] > 0.01);
    assert!(
        out.iter()
            .skip(1)
            .step_by(3)
            .chain(out.iter().skip(2).step_by(3))
            .all(|value| *value == 0.0)
    );
    let spot = impulse(height, width, 4, 12);
    let out = streaks(
        &Params {
            color: Color([0.2, 0.5, 1.0]),
            ..params
        },
        height,
        width,
        &spot,
    );
    let at = (4 * width + 20) * 3;
    let (r, g, b) = (out[at], out[at + 1], out[at + 2]);
    assert!(b > g && g > r && r > 0.0);
    assert!((g / b - 0.5).abs() < 1e-4 && (r / b - 0.2).abs() < 1e-4);
}

#[test]
fn black_no_amount_and_a_threshold_above_every_value_leave_the_picture_as_it_is() {
    let black = vec![0.0_f32; 9 * 40 * 3];
    assert_eq!(
        first_difference_within(&streaks(&Params::default(), 9, 40, &black), &black, 0.0),
        None
    );
    let dim: Vec<f32> = varied(9, 40).iter().map(|value| value * 0.5).collect();
    let high = Params {
        threshold: 0.5,
        amount: 10.0,
        ..Params::default()
    };
    assert_eq!(first_difference_within(&streaks(&high, 9, 40, &dim), &dim, 1e-6), None);
    let off = Params {
        amount: 0.0,
        threshold: 0.0,
        ..Params::default()
    };
    assert_eq!(first_difference_within(&streaks(&off, 9, 40, &dim), &dim, 0.0), None);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn the_picture_only_gets_brighter_and_stays_in_range(
        height in 1_usize..6,
        width in 1_usize..40,
        picture in prop::collection::vec(0.0_f32..1.0, 5 * 40 * 3),
        length in 0.5_f32..480.0,
        threshold in 0.0_f32..0.99,
        amount in 0.0_f32..10.0,
    ) {
        let picture = &picture[..height * width * 3];
        let params = Params { length, threshold, amount, ..Params::default() };
        let out = streaks(&params, height, width, picture);
        prop_assert_eq!(out.len(), picture.len());
        prop_assert!(out.iter().zip(picture).all(|(out, base)| out >= base && *out <= 1.0), "brighter, at most white");
    }
}
