//! `sharpen`: the picture plus `amount` times what a Gaussian blur takes away. The blur of the comparisons is the
//! `gaussian_blur` effect, itself held to its definition in `gaussian_blur.rs`; the rest is worked out by hand from the
//! Gaussian's weights.

use bullterrier_image::effects::{gaussian_blur, sharpen};
use bullterrier_image::{EdgeOrConstant, Error};

use crate::common::gaussian::{run_picture, varied, weights};
use proptest::prelude::*;

const EDGES: [EdgeOrConstant; 5] = [
    EdgeOrConstant::Reflect,
    EdgeOrConstant::Constant,
    EdgeOrConstant::Nearest,
    EdgeOrConstant::Mirror,
    EdgeOrConstant::Wrap,
];

/// A `size` x `size` grey picture with one pixel `height` above it in the middle (all channels).
fn bump(size: usize, base: f32, height: f32) -> Vec<f32> {
    let mut grey = vec![base; size * size * 3];
    let centre = (size / 2 * size + size / 2) * 3;
    grey[centre..centre + 3].iter_mut().for_each(|value| *value += height);
    grey
}

/// A vertical step edge, `low` on the left half of 24 x 48 and `high` on the right.
fn step(low: f32, high: f32) -> Vec<f32> {
    (0..24 * 48 * 3)
        .map(|index| if index / 3 % 48 < 24 { low } else { high })
        .collect()
}

#[test]
fn amount_zero_and_radius_zero_leave_the_picture_as_it_is() -> Result<(), Error> {
    let picture = varied(9, 13);
    for (radius, amount) in [(3.0, 0.0), (0.0, 5.0)] {
        for edge in EDGES {
            let params = sharpen::Params {
                radius,
                amount,
                edge,
                constant_value: 0.9,
                ..Default::default()
            };
            assert_eq!(
                run_picture(&params, 9, 13, &picture)?,
                picture,
                "{edge:?}, {radius}, {amount}"
            );
        }
    }
    Ok(())
}

#[test]
fn a_constant_picture_stays_that_constant_for_any_amount() -> Result<(), Error> {
    for value in [0.0, 0.35, 1.0] {
        for edge in EDGES {
            let params = sharpen::Params {
                radius: 3.0,
                amount: 5.0,
                edge,
                constant_value: value,
                ..Default::default()
            };
            let out = run_picture(&params, 20, 30, &vec![value; 20 * 30 * 3])?;
            let at = out.iter().position(|&out| (out - value).abs() > 1e-6);
            assert_eq!(at, None, "{value}, {edge:?}");
        }
    }
    Ok(())
}

#[test]
fn the_result_is_the_picture_plus_amount_times_its_difference_from_the_gaussian_blur() -> Result<(), Error> {
    let picture = varied(11, 17);
    for edge in EDGES {
        for (radius, amount) in [(1.5, 1.0), (2.0, 0.4), (3.0, 2.5)] {
            let params = sharpen::Params {
                radius,
                amount,
                edge,
                constant_value: 0.6,
                ..Default::default()
            };
            let blur = gaussian_blur::Params {
                radius,
                edge,
                constant_value: 0.6,
                ..Default::default()
            };
            let blurred = run_picture(&blur, 11, 17, &picture)?;
            let out = run_picture(&params, 11, 17, &picture)?;
            for (at, ((&out, &was), &blurred)) in out.iter().zip(&picture).zip(&blurred).enumerate() {
                let expected = (was + amount * (was - blurred)).clamp(0.0, 1.0);
                assert!(
                    (out - expected).abs() <= 2e-6,
                    "{edge:?}, {radius}, {amount}, value {at}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn a_bump_is_raised_by_amount_times_its_height_less_its_own_share_of_the_blur() -> Result<(), Error> {
    // the blur at a bump of height h is h w0^2 above the grey, one pixel beside it h w0 w1 (w the 1-D Gaussian weights)
    let (height, base) = (0.1_f32, 0.5_f32);
    for (radius, amount) in [(1.5_f32, 1.0), (2.0, 0.4), (3.0, 2.5)] {
        let (weights, middle) = weights(f64::from(radius));
        let (w0, w1) = (weights[middle], weights[middle + 1]);
        let params = sharpen::Params {
            radius,
            amount,
            ..Default::default()
        };
        let out = run_picture(&params, 41, 41, &bump(41, base, height))?;
        let (at_bump, beside) = ((20 * 41 + 20) * 3, (20 * 41 + 21) * 3);
        let raised = f64::from(base + height) + f64::from(amount * height) * (1.0 - w0 * w0);
        let dipped = f64::from(base) - f64::from(amount * height) * w0 * w1;
        assert!(
            (f64::from(out[at_bump]) - raised).abs() < 2e-6,
            "sigma {radius}, amount {amount}"
        );
        assert!(
            (f64::from(out[beside]) - dipped).abs() < 2e-6,
            "sigma {radius}, amount {amount}"
        );
    }
    Ok(())
}

#[test]
fn detail_below_the_threshold_is_left_alone_and_detail_at_or_above_it_is_run_picture() -> Result<(), Error> {
    // a bump of 0.04 differs from its blur by 0.04 (1 - w0^2), about 0.036 at sigma 2; one of 0.3 by about 0.27
    for (height, threshold, changes) in [
        (0.04, 0.05, false),
        (0.04, 0.02, true),
        (0.3, 0.05, true),
        (0.3, 0.5, false),
    ] {
        let grey = bump(41, 0.5, height);
        let params = sharpen::Params {
            radius: 2.0,
            amount: 1.0,
            threshold,
            ..Default::default()
        };
        assert_eq!(
            run_picture(&params, 41, 41, &grey)? != grey,
            changes,
            "{height}, {threshold}"
        );
    }
    Ok(())
}

#[test]
fn the_threshold_decides_pixel_by_pixel_not_for_the_picture() -> Result<(), Error> {
    // a bold edge and a faint ripple in one picture: only the edge is sharpened
    let mut both = step(0.2, 0.8);
    for row in 0..24 {
        for column in (2..24).step_by(2) {
            both[(row * 48 + column) * 3..][..3]
                .iter_mut()
                .for_each(|value| *value += 0.01);
        }
    }
    let params = sharpen::Params {
        radius: 1.5,
        amount: 2.0,
        threshold: 0.05,
        ..Default::default()
    };
    let out = run_picture(&params, 24, 48, &both)?;
    for row in 0..24 {
        assert_eq!(
            out[row * 48 * 3..][..10 * 3],
            both[row * 48 * 3..][..10 * 3],
            "row {row}"
        );
    }
    assert!(out[(12 * 48 + 24) * 3] > 0.85);
    Ok(())
}

#[test]
fn an_edge_overshoots_on_both_sides_and_beyond_the_blurs_reach_nothing_changes() -> Result<(), Error> {
    let params = sharpen::Params {
        amount: 1.0,
        ..Default::default()
    };
    let out = run_picture(&params, 24, 48, &step(0.3, 0.7))?;
    let row = |column: usize| out[(12 * 48 + column) * 3];
    assert!(row(23) < 0.29 && row(24) > 0.71);
    assert!((0..8).all(|column| (row(column) - 0.3).abs() < 1e-6));
    assert!((40..48).all(|column| (row(column) - 0.7).abs() < 1e-6));
    Ok(())
}

#[test]
fn truncate_sets_where_the_blur_is_cut_off() -> Result<(), Error> {
    // six px from a bump, sigma 2 is 3 sigma out: the blur reaches it with truncate 4 (8 px), not with truncate 2 (4 px)
    let beside = (20 * 41 + 26) * 3;
    for (truncate, reached) in [(4.0, true), (2.0, false)] {
        let params = sharpen::Params {
            radius: 2.0,
            truncate,
            amount: 1.0,
            ..Default::default()
        };
        let out = run_picture(&params, 41, 41, &bump(41, 0.5, 0.4))?;
        assert_eq!((out[beside] - 0.5).abs() > 1e-4, reached, "truncate {truncate}");
    }
    Ok(())
}

#[test]
fn a_strong_sharpen_is_cut_off_at_black_and_white() -> Result<(), Error> {
    for right in [0.0, 1.0] {
        let side: Vec<f32> = (0..24 * 48 * 3)
            .map(|index| if index / 3 % 48 < 24 { 0.5 } else { right })
            .collect();
        let params = sharpen::Params {
            amount: 5.0,
            ..Default::default()
        };
        let out = run_picture(&params, 24, 48, &side)?;
        let (low, high) = out.iter().fold((1.0_f32, 0.0_f32), |(l, h), &v| (l.min(v), h.max(v)));
        assert_eq!((low, high), (0.0, 1.0), "{right}");
    }
    Ok(())
}

#[test]
fn each_colour_channel_is_sharpened_on_its_own() -> Result<(), Error> {
    let mut red_edge = step(0.3, 0.7);
    red_edge.chunks_mut(3).for_each(|pixel| pixel[1..].fill(0.5));
    let params = sharpen::Params {
        amount: 1.0,
        ..Default::default()
    };
    let out = run_picture(&params, 24, 48, &red_edge)?;
    assert!(
        out.chunks(3)
            .all(|pixel| pixel[1..].iter().all(|&value| (value - 0.5).abs() < 1e-6))
    );
    assert!(out[(12 * 48 + 24) * 3] > 0.71);
    Ok(())
}

#[test]
fn a_constant_edge_blurs_in_the_given_brightness_beyond_the_picture() -> Result<(), Error> {
    // mid-way down the left edge of a tall grey picture only the row direction sees past the edge: the blur there is
    // grey - (grey - beyond) x (the weights at the offsets left of the pixel)
    let (weights, middle) = weights(1.5);
    let left: f64 = weights[..middle].iter().sum();
    for beyond in [0.0_f32, 0.2, 0.5] {
        let params = sharpen::Params {
            radius: 1.5,
            amount: 1.0,
            edge: EdgeOrConstant::Constant,
            constant_value: beyond,
            ..Default::default()
        };
        let out = run_picture(&params, 61, 21, &vec![0.5; 61 * 21 * 3])?;
        let blurred = 0.5 - f64::from(0.5 - beyond) * left;
        assert!(
            (f64::from(out[(30 * 21) * 3]) - (0.5 + (0.5 - blurred))).abs() < 2e-6,
            "{beyond}"
        );
        assert!((out[(30 * 21 + 10) * 3] - 0.5).abs() < 1e-6, "{beyond}");
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn the_result_is_finite_and_within_zero_to_one(
        height in 1_usize..9,
        width in 1_usize..9,
        picture in prop::collection::vec(0.0_f32..1.0, 81 * 3),
        radius in prop_oneof![0.0_f32..3.0, 8.0_f32..10.0],
        amount in 0.0_f32..5.0,
        threshold in 0.0_f32..1.0,
        edge in 0_usize..5,
    ) {
        let params = sharpen::Params { radius, amount, threshold, edge: EDGES[edge], ..Default::default() };
        let out = run_picture(&params, height, width, &picture[..height * width * 3])
            .map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert!(out.iter().all(|value| (0.0..=1.0).contains(value)));
    }
}
