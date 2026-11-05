//! `gaussian_blur`: the picture compared with a Gaussian worked out tap by tap from its definition, with a different
//! way of finding what lies beyond the edge than the core's, and the properties a blur keeps.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "small picture sizes and kernel reaches, and the f64 reference narrowed to the f32 picture"
)]

use bullterrier_image::effects::gaussian_blur::Params;
use bullterrier_image::params::Choice;
use bullterrier_image::{Edge, EdgeOrConstant, Error};
use proptest::prelude::*;

use crate::common::compare::first_difference_within;
use crate::common::edge::source;
use crate::common::pictures::{impulse, varied};
use crate::common::pixels::run_picture;

/// The pictures are compared at this distance: the sums are made in f64 and f32 is 6e-8 at 1.
const TOLERANCE: f32 = 1e-5;

/// The edge rules that need a value beyond the edge, and those that do not.
const EDGES: [EdgeOrConstant; 5] = [
    EdgeOrConstant::Reflect,
    EdgeOrConstant::Constant,
    EdgeOrConstant::Nearest,
    EdgeOrConstant::Mirror,
    EdgeOrConstant::Wrap,
];

/// scipy's weights at distances -reach..=reach: exp(-d^2 / 2 sigma^2), cut off at `int(truncate x sigma + 0.5)`, summing
/// to 1.
fn kernel(sigma: f64, truncate: f64) -> Vec<f64> {
    let reach = (truncate * sigma + 0.5).floor() as i64;
    let weights: Vec<f64> = (-reach..=reach)
        .map(|distance| (-((distance * distance) as f64) / (2.0 * sigma * sigma)).exp())
        .collect();
    let total: f64 = weights.iter().sum();
    weights.iter().map(|weight| weight / total).collect()
}

/// The picture blurred by the definition: every output sample the double sum of the weights and the samples they fall on,
/// `beyond` where the edge rule says the picture has none.
fn reference(params: &Params, height: usize, width: usize, values: &[f32]) -> Vec<f32> {
    let weights = kernel(f64::from(params.radius), f64::from(params.truncate));
    let reach = (weights.len() / 2) as i64;
    let (h, w) = (height as i64, width as i64);
    let beyond = f64::from(params.constant_value);
    let mut out = vec![0.0_f32; values.len()];
    for y in 0..h {
        for x in 0..w {
            for channel in 0..3 {
                let mut sum = 0.0;
                for (dy, weight_y) in (-reach..=reach).zip(&weights) {
                    for (dx, weight_x) in (-reach..=reach).zip(&weights) {
                        let at = source(params.edge, y + dy, h).zip(source(params.edge, x + dx, w));
                        let sample = at.map_or(beyond, |(row, column)| {
                            f64::from(values[(row * width + column) * 3 + channel])
                        });
                        sum += weight_y * weight_x * sample;
                    }
                }
                out[((y * w + x) * 3) as usize + channel] = sum as f32;
            }
        }
    }
    out
}

#[test]
fn the_blur_is_the_gaussian_of_the_definition_whatever_lies_beyond_the_edge() -> Result<(), Error> {
    // Pictures from one pixel to a few, with kernels shorter and longer than they are. A kernel reaches 12, 15, 7 (the
    // last of 6.6, rounded up) px and is summed tap by tap, or 16, 20, 36, 48, 17 (16.6, rounded up) and is filtered by
    // the Fourier transform.
    let shapes = [(1, 1), (1, 7), (7, 1), (2, 3), (3, 2), (5, 4), (11, 9)];
    for (radius, truncate) in [
        (3.0, 4.0),
        (3.8, 4.0),
        (4.0, 4.0),
        (5.0, 4.0),
        (9.0, 4.0),
        (6.0, 8.0),
        (3.3, 2.0),
        (8.3, 2.0),
    ] {
        for edge in EDGES {
            for (height, width) in shapes {
                let params = Params {
                    radius,
                    truncate,
                    edge,
                    constant_value: 0.7,
                };
                let picture = varied(height, width);
                let at = first_difference_within(
                    &run_picture(&params, height, width, &picture)?,
                    &reference(&params, height, width, &picture),
                    TOLERANCE,
                );
                assert_eq!(
                    at, None,
                    "radius {radius}, truncate {truncate}, {edge:?}, {height} x {width}: first differs at {at:?}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn the_comparison_tells_a_blur_that_differs_apart() -> Result<(), Error> {
    // the known-bad pair: the reference of another edge rule, and of a kernel cut off sooner, are not the blur
    let picture = varied(5, 4);
    let params = Params {
        radius: 3.0,
        edge: EdgeOrConstant::Reflect,
        ..Params::default()
    };
    let blur = run_picture(&params, 5, 4, &picture)?;
    for other in [
        Params {
            edge: EdgeOrConstant::Wrap,
            ..params.clone()
        },
        Params {
            truncate: 2.0,
            ..params.clone()
        },
    ] {
        assert!(first_difference_within(&blur, &reference(&other, 5, 4, &picture), TOLERANCE).is_some());
    }
    Ok(())
}

#[test]
fn an_impulse_becomes_the_outer_product_of_the_weights() -> Result<(), Error> {
    // from the formula alone: a short kernel (summed) and one longer than 100 px (the transform); beyond the kernel the
    // black is exact
    for (radius, size) in [(3.0, 61), (40.0, 401)] {
        let weights = kernel(radius, 4.0);
        let reach = weights.len() / 2;
        let centre = size / 2;
        let params = Params {
            radius: radius as f32,
            edge: EdgeOrConstant::Constant,
            ..Params::default()
        };
        let out = run_picture(&params, size, size, &impulse(size, size, size / 2, size / 2))?;
        for row in 0..size {
            for column in 0..size {
                let (dy, dx) = (row.abs_diff(centre), column.abs_diff(centre));
                let expected = if dy <= reach && dx <= reach {
                    (weights[reach + dy] * weights[reach + dx]) as f32
                } else {
                    0.0
                };
                for channel in 0..3 {
                    let actual = out[(row * size + column) * 3 + channel];
                    // (the transform's rounding noise far from the impulse is held to the picture's own range)
                    assert!(
                        (0.0..=1.0).contains(&actual),
                        "radius {radius}, ({row}, {column}): {actual}"
                    );
                    assert!(
                        (actual - expected).abs() <= 2e-6,
                        "radius {radius}, ({row}, {column}) channel {channel}: {actual}, not {expected}"
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn the_kernel_is_cut_off_where_truncate_says() -> Result<(), Error> {
    // radius 2: truncate 4 reaches 8 px, truncate 2 reaches 4 px, so a pixel 6 px from the impulse is lit by one only
    let at_six_px = (40 * 81 + 46) * 3;
    for (truncate, lit) in [(4.0, true), (2.0, false)] {
        let params = Params {
            radius: 2.0,
            truncate,
            ..Params::default()
        };
        let out = run_picture(&params, 81, 81, &impulse(81, 81, 81 / 2, 81 / 2))?;
        assert_eq!(out[at_six_px] > 0.0, lit, "truncate {truncate}");
    }
    Ok(())
}

#[test]
fn radius_zero_and_a_kernel_of_one_tap_leave_the_picture_as_it_is() -> Result<(), Error> {
    let picture = varied(9, 13);
    // 0.1 x 4 + 0.5 rounds down to a reach of 0, as in scipy
    for radius in [0.0, 0.1] {
        let params = Params {
            radius,
            ..Params::default()
        };
        assert_eq!(run_picture(&params, 9, 13, &picture)?, picture, "radius {radius}");
    }
    Ok(())
}

#[test]
fn each_colour_channel_is_blurred_on_its_own() -> Result<(), Error> {
    let mut red_dot = impulse(41, 41, 41 / 2, 41 / 2);
    red_dot.chunks_mut(3).for_each(|pixel| pixel[1..].fill(0.0));
    let out = run_picture(&Params::default(), 41, 41, &red_dot)?;
    assert!(out[(20 * 41 + 24) * 3] > 0.0);
    assert!(out.chunks(3).all(|pixel| pixel[1] == 0.0 && pixel[2] == 0.0));
    Ok(())
}

#[test]
fn a_constant_picture_stays_as_it_is_whatever_lies_beyond_its_edge() -> Result<(), Error> {
    let grey = vec![0.4; 21 * 34 * 3];
    for radius in [0.3, 6.0, 9.0] {
        for edge in EDGES {
            let params = Params {
                radius,
                edge,
                constant_value: 0.4,
                ..Params::default()
            };
            let out = run_picture(&params, 21, 34, &grey)?;
            assert_eq!(
                first_difference_within(&out, &grey, TOLERANCE),
                None,
                "radius {radius}, {edge:?}"
            );
        }
    }
    Ok(())
}

#[test]
fn with_wrap_the_blur_keeps_each_channels_mean() -> Result<(), Error> {
    // nothing leaves or enters the picture, and the weights sum to 1
    let picture = varied(23, 31);
    let mean = |values: &[f32], channel: usize| {
        values
            .iter()
            .skip(channel)
            .step_by(3)
            .map(|&value| f64::from(value))
            .sum::<f64>()
            / (23.0 * 31.0)
    };
    for radius in [2.0, 9.0] {
        let params = Params {
            radius,
            edge: EdgeOrConstant::Wrap,
            ..Params::default()
        };
        let out = run_picture(&params, 23, 31, &picture)?;
        for channel in 0..3 {
            let (before, after) = (mean(&picture, channel), mean(&out, channel));
            assert!(
                (before - after).abs() < 1e-5,
                "radius {radius}, channel {channel}: {before}, {after}"
            );
        }
        assert_ne!(out, picture);
    }
    Ok(())
}

#[test]
fn a_constant_edge_fades_the_border_toward_the_value_and_a_wide_blur_ends_at_it() -> Result<(), Error> {
    // a blur far wider than the picture sees almost only what lies beyond it
    let grey = vec![0.5; 7 * 9 * 3];
    for beyond in [0.0, 1.0] {
        let params = Params {
            radius: 60.0,
            edge: EdgeOrConstant::Constant,
            constant_value: beyond,
            ..Params::default()
        };
        let out = run_picture(&params, 7, 9, &grey)?;
        assert!(out.iter().all(|value| (value - beyond).abs() < 0.1), "beyond {beyond}");
        let inner = run_picture(&Params { radius: 1.0, ..params }, 7, 9, &grey)?;
        let corner = inner[0];
        assert!(
            (corner - beyond).abs() < (0.5 - beyond).abs(),
            "beyond {beyond}: the corner is {corner}"
        );
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn a_blur_never_leaves_the_range_of_the_picture_and_what_lies_beyond_it(
        height in 1_usize..9,
        width in 1_usize..9,
        picture in prop::collection::vec(0.2_f32..0.8, 81 * 3),
        radius in prop_oneof![0.0_f32..3.0, 8.0_f32..10.0],
        edge in 0_usize..5,
        constant_value in 0.0_f32..1.0,
    ) {
        let picture = &picture[..height * width * 3];
        let params = Params { radius, edge: EDGES[edge], constant_value, ..Params::default() };
        let (mut low, mut high) = picture.iter().fold((f32::MAX, f32::MIN), |(l, h), &v| (l.min(v), h.max(v)));
        if EDGES[edge] == EdgeOrConstant::Constant {
            (low, high) = (low.min(constant_value), high.max(constant_value));
        }
        let out = run_picture(&params, height, width, picture).map_err(|error| TestCaseError::fail(error.to_string()))?;
        prop_assert!(out.iter().all(|&value| (low..=high).contains(&value)), "outside {low}..{high}");
    }
}

#[test]
fn the_four_edge_rules_are_the_same_choices_as_the_five_without_constant() {
    let names: Vec<&str> = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap]
        .into_iter()
        .map(|edge| EdgeOrConstant::from(edge).name())
        .collect();
    assert_eq!(names, ["reflect", "nearest", "mirror", "wrap"]);
    assert_eq!(Edge::CHOICES, names);
    assert_eq!(
        EdgeOrConstant::CHOICES,
        ["reflect", "constant", "nearest", "mirror", "wrap"]
    );
}
