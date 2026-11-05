//! `motion_blur`: the picture compared with a convolution by the line worked out from its definition, over every way the
//! core applies a kernel (tap by tap, row by row, by the Fourier transform), and the properties a mean keeps.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "small picture sizes, and a line sampled finely"
)]

use std::collections::BTreeMap;

use bullterrier_image::effects::motion_blur::Params;
use bullterrier_image::{Edge, Error};
use proptest::prelude::*;

use crate::common::compare::first_difference_within;
use crate::common::convolution::{Beyond, Tap, convolved};
use crate::common::pictures::{impulse, varied};
use crate::common::pixels::run_picture;

/// The pictures are compared at this distance: sums in f32 or by the transform, and a line sampled finely.
const TOLERANCE: f32 = 1e-4;

const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];

/// What a point of light becomes: (rows below, columns right of the point, weight).
type Weights = Vec<(isize, isize, f32)>;

/// The weights of a line `length` px long at `angle` degrees through the middle of a pixel, found by walking along it in
/// tiny steps: each pixel weighs the share of the steps that fall in it. (The core works out the length inside each pixel
/// exactly.) Rows count downward, columns to the right, the angle is clockwise from the right.
fn line(angle: f64, length: f64) -> Vec<Tap> {
    const STEPS: usize = 400_000; // constant: a pixel's weight is right to about 1 / STEPS of the line
    let (right, down) = (angle.to_radians().cos(), angle.to_radians().sin());
    let mut weights = BTreeMap::new();
    for step in 0..STEPS {
        let along = -length / 2.0 + (step as f64 + 0.5) * length / STEPS as f64;
        let pixel = ((along * down).round() as i64, (along * right).round() as i64);
        *weights.entry(pixel).or_insert(0.0) += 1.0 / STEPS as f64;
    }
    weights
        .into_iter()
        .map(|((down, right), weight)| (down, right, weight))
        .collect()
}

fn blur(angle: f32, length: f32, edge: Edge, height: usize, width: usize, picture: &[f32]) -> Result<Vec<f32>, Error> {
    run_picture(&Params { angle, length, edge }, height, width, picture)
}

#[test]
fn the_blur_is_the_picture_averaged_along_the_line_whatever_lies_beyond_the_edge() -> Result<(), Error> {
    // Pictures from one pixel to a few, with lines shorter and longer than they are. A line crosses about as many pixels as
    // it is long (up to 1.4 times, slanting): those of 3, 6.5, 30 and 61 px are summed tap by tap, along a row (the angles
    // 0 and 180) row by row; one of 150 px, a row apart, is filtered line by line by the transform, at any other angle the
    // whole picture is.
    let shapes = [(1, 1), (1, 9), (9, 1), (5, 7), (23, 31)];
    for angle in [0.0, 33.0, 90.0, 135.0, 200.0, 347.0] {
        for length in [3.0, 6.5, 30.0, 61.0, 150.0] {
            let taps = line(f64::from(angle), f64::from(length));
            for edge in EDGES {
                for (height, width) in shapes {
                    let picture = varied(height, width);
                    let expected = convolved(&taps, Beyond::Rule(edge), height, width, &picture);
                    let at = first_difference_within(
                        &blur(angle, length, edge, height, width, &picture)?,
                        &expected,
                        TOLERANCE,
                    );
                    assert_eq!(
                        at, None,
                        "angle {angle}, length {length}, {edge:?}, {height} x {width}: first differs at {at:?}"
                    );
                }
            }
        }
    }
    Ok(())
}

#[test]
fn the_comparison_tells_a_blur_that_differs_apart() -> Result<(), Error> {
    // the known-bad rows: the line at another angle, of another length, and the other edge rule are not the blur
    let picture = varied(23, 31);
    let ours = blur(33.0, 30.0, Edge::Reflect, 23, 31, &picture)?;
    for (angle, length, edge) in [
        (53.0, 30.0, Edge::Reflect),
        (33.0, 25.0, Edge::Reflect),
        (33.0, 30.0, Edge::Wrap),
    ] {
        let other = convolved(&line(angle, length), Beyond::Rule(edge), 23, 31, &picture);
        assert!(
            first_difference_within(&ours, &other, TOLERANCE).is_some(),
            "{angle} {length} {edge:?}"
        );
    }
    Ok(())
}

#[test]
fn a_point_of_light_becomes_the_length_of_line_inside_each_pixel() -> Result<(), Error> {
    // worked by hand: the line is 1 x 1 squares' length inside each pixel, scaled to sum to 1
    let diagonal = 2.0 * std::f32::consts::SQRT_2;
    let cases: [(f32, f32, Weights); 6] = [
        (0.0, 5.0, (-2..=2).map(|right| (0, right, 0.2)).collect()),
        (
            0.0,
            4.0,
            vec![(0, -2, 0.125), (0, -1, 0.25), (0, 0, 0.25), (0, 1, 0.25), (0, 2, 0.125)],
        ),
        (90.0, 3.0, (-1..=1).map(|down| (down, 0, 1.0 / 3.0)).collect()),
        (180.0, 3.0, (-1..=1).map(|right| (0, right, 1.0 / 3.0)).collect()),
        // clockwise from the right: down and to the right
        (45.0, diagonal, vec![(-1, -1, 0.25), (0, 0, 0.5), (1, 1, 0.25)]),
        (135.0, diagonal, vec![(1, -1, 0.25), (0, 0, 0.5), (-1, 1, 0.25)]),
    ];
    for (angle, length, weights) in cases {
        let (size, middle) = (15, 7);
        let out = blur(
            angle,
            length,
            Edge::Nearest,
            size,
            size,
            &impulse(size, size, middle, middle),
        )?;
        let mut expected = vec![0.0_f32; out.len()];
        for (down, right, weight) in &weights {
            let at = (middle.wrapping_add_signed(*down) * size + middle.wrapping_add_signed(*right)) * 3;
            expected[at..at + 3].fill(*weight);
        }
        assert_eq!(
            first_difference_within(&out, &expected, 1e-6),
            None,
            "angle {angle}, length {length}"
        );
    }
    Ok(())
}

#[test]
fn opposite_directions_give_the_same_blur() -> Result<(), Error> {
    let picture = varied(11, 13);
    for length in [7.0, 40.0] {
        let (there, back) = (
            blur(30.0, length, Edge::Mirror, 11, 13, &picture)?,
            blur(210.0, length, Edge::Mirror, 11, 13, &picture)?,
        );
        assert_eq!(first_difference_within(&there, &back, 1e-5), None, "length {length}");
    }
    Ok(())
}

#[test]
fn a_line_under_a_pixel_and_no_line_leave_the_picture_as_it_is() -> Result<(), Error> {
    let picture = varied(6, 5);
    for (angle, length) in [(0.0, 0.0), (30.0, 0.0), (30.0, 0.5), (200.0, 0.9)] {
        let out = blur(angle, length, Edge::Reflect, 6, 5, &picture)?;
        assert_eq!(
            first_difference_within(&out, &picture, 0.0),
            None,
            "angle {angle}, length {length}"
        );
    }
    Ok(())
}

#[test]
fn a_flat_picture_stays_exactly_as_flat_for_every_edge_rule_and_every_way_of_filtering() -> Result<(), Error> {
    // 0.3 is not a sum of weights in f32: the result is held to the picture's range, which is the one value
    for (height, width) in [(1, 1), (1, 40), (40, 1), (9, 13), (30, 41)] {
        let flat = vec![0.3_f32; height * width * 3];
        for edge in EDGES {
            for (angle, length) in [(0.0, 5.0), (0.0, 61.0), (90.0, 61.0), (33.0, 6.5), (33.0, 61.0)] {
                let out = blur(angle, length, edge, height, width, &flat)?;
                assert_eq!(
                    first_difference_within(&out, &flat, 0.0),
                    None,
                    "{height} x {width}, {edge:?}, {angle}, {length}"
                );
            }
        }
    }
    Ok(())
}

#[test]
fn a_wrapped_blur_keeps_the_mean_of_every_channel() -> Result<(), Error> {
    // the weights sum to 1 and every tap is a shift of the whole picture, so what leaves one side comes in at the other
    let (height, width) = (23, 31);
    let picture = varied(height, width);
    let mean = |values: &[f32], channel: usize| {
        values
            .iter()
            .skip(channel)
            .step_by(3)
            .map(|value| f64::from(*value))
            .sum::<f64>()
            / (height * width) as f64
    };
    for (angle, length) in [(0.0, 9.0), (0.0, 61.0), (33.0, 9.0), (33.0, 61.0), (90.0, 61.0)] {
        let out = blur(angle, length, Edge::Wrap, height, width, &picture)?;
        for channel in 0..3 {
            let (before, after) = (mean(&picture, channel), mean(&out, channel));
            assert!(
                (before - after).abs() < 1e-5,
                "{angle}, {length}, channel {channel}: {before}, {after}"
            );
        }
        assert!(
            first_difference_within(&out, &picture, 1e-3).is_some(),
            "{angle}, {length} blurs"
        );
    }
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn a_blur_never_leaves_the_range_of_the_picture(
        height in 1_usize..9,
        width in 1_usize..9,
        picture in prop::collection::vec(0.2_f32..0.8, 81 * 3),
        angle in 0.0_f32..360.0,
        length in prop_oneof![0.0_f32..8.0, 40.0_f32..70.0],
        edge in 0_usize..4,
    ) {
        let picture = &picture[..height * width * 3];
        let (low, high) = picture.iter().fold((f32::MAX, f32::MIN), |(l, h), &v| (l.min(v), h.max(v)));
        let out = blur(angle, length, EDGES[edge], height, width, picture)?;
        prop_assert_eq!(out.len(), picture.len());
        prop_assert!(out.iter().all(|&value| (low..=high).contains(&value)), "outside {low}..{high}");
    }
}
