//! `pinch_bulge`: the picture compared with a point-by-point computation from the definition (every reading, every edge
//! rule), hand-worked ramps, and the properties a push about a centre keeps.

#![expect(
    clippy::cast_precision_loss,
    reason = "small picture sizes; the f64 reference narrowed to the f32 picture"
)]

use bullterrier_image::effects::pinch_bulge::Params;
use bullterrier_image::{Edge, Error, Interpolation};
use proptest::prelude::*;

use crate::common::compare::first_difference_within;
use crate::common::pictures::{ramp, varied};
use crate::common::pixels::run_picture;
use crate::common::sampling::warped;

/// The pictures are compared at this distance: the reference reads in f64 and the core stores f32.
const TOLERANCE: f32 = 1e-5;

const INTERPOLATIONS: [Interpolation; 3] = [Interpolation::Nearest, Interpolation::Bilinear, Interpolation::Bicubic];
const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];
/// The shapes the comparison runs on: a pixel, a row, a column, a few pixels, odd and even sides.
const SHAPES: [(usize, usize); 6] = [(1, 1), (1, 9), (9, 1), (5, 7), (23, 31), (16, 10)];

/// `picture` pushed as `params` says, worked out from the definition: the centre is at fraction (`x`, `y`) of the way from
/// the first to the last pixel; a pixel `d` px from it, `d` inside `radius` x (shorter side - 1), shows the picture on the
/// same ray at `d x (1 - amount x (1 - d / reach)^2)`, read by `interpolation` with `edge` beyond the picture; the rest
/// keeps its place.
fn by_definition(params: &Params, height: usize, width: usize, picture: &[f32]) -> Vec<f32> {
    let reach = f64::from(params.radius) * (height.min(width) - 1) as f64;
    if params.amount == 0.0 || reach == 0.0 {
        return picture.to_vec();
    }
    let centre = (
        f64::from(params.y) * (height - 1) as f64,
        f64::from(params.x) * (width - 1) as f64,
    );
    warped(
        picture,
        height,
        width,
        params.interpolation,
        (params.edge, params.edge),
        |row, column| {
            let (down, across) = (row as f64 - centre.0, column as f64 - centre.1);
            let inside = (1.0 - down.hypot(across) / reach).clamp(0.0, 1.0);
            let scale = 1.0 - f64::from(params.amount) * inside * inside;
            (centre.0 + down * scale, centre.1 + across * scale)
        },
    )
}

#[test]
fn the_picture_is_what_a_point_by_point_computation_from_the_definition_gives() -> Result<(), Error> {
    // (amount, radius, x, y): a bulge and a pinch, a small disc and one over the whole picture, the centre in the middle,
    // at a corner and off the pixel grid
    let sets = [
        (0.5, 0.5, 0.5, 0.5),
        (-0.8, 0.9, 0.0, 0.0),
        (1.0, 2.0, 0.37, 0.81),
        (-1.0, 0.3, 1.0, 0.4),
        (0.2, 1.3, 0.5, 1.0),
    ];
    for interpolation in INTERPOLATIONS {
        for edge in EDGES {
            for (amount, radius, x, y) in sets {
                let params = Params {
                    amount,
                    radius,
                    x,
                    y,
                    interpolation,
                    edge,
                };
                for (height, width) in SHAPES {
                    let picture = varied(height, width);
                    let at = first_difference_within(
                        &run_picture(&params, height, width, &picture)?,
                        &by_definition(&params, height, width, &picture),
                        TOLERANCE,
                    );
                    assert_eq!(at, None, "{params:?}, {height} x {width}: first differs at {at:?}");
                }
            }
        }
    }
    Ok(())
}

#[test]
fn the_comparison_tells_pushes_that_differ_apart() -> Result<(), Error> {
    // the known-bad rows: another amount, radius, centre, reading or edge rule is not this push (a pinch, for it reads
    // beyond the edge); and the push is not nothing, so the differences show
    let picture = varied(23, 31);
    let params = Params {
        amount: -0.7,
        radius: 1.2,
        x: 0.3,
        y: 0.6,
        interpolation: Interpolation::Bilinear,
        edge: Edge::Wrap,
    };
    let ours = run_picture(&params, 23, 31, &picture)?;
    assert!(
        first_difference_within(&ours, &picture, 0.05).is_some(),
        "the push changes the picture"
    );
    for other in [
        Params {
            amount: -0.6,
            ..params.clone()
        },
        Params {
            amount: 0.7,
            ..params.clone()
        },
        Params {
            radius: 1.1,
            ..params.clone()
        },
        Params {
            x: 0.4,
            ..params.clone()
        },
        Params {
            y: 0.5,
            ..params.clone()
        },
        Params {
            interpolation: Interpolation::Nearest,
            ..params.clone()
        },
        Params {
            edge: Edge::Mirror,
            ..params.clone()
        },
    ] {
        assert!(
            first_difference_within(&ours, &by_definition(&other, 23, 31, &picture), TOLERANCE).is_some(),
            "{other:?}"
        );
    }
    Ok(())
}

#[test]
fn no_amount_no_radius_and_a_picture_with_no_distance_leave_the_picture_exactly_as_it_is() -> Result<(), Error> {
    // (height, width, amount, radius): a picture one pixel thick has no reach
    let rows = [
        (6, 5, 0.0, 0.5),
        (6, 5, 0.8, 0.0),
        (1, 9, 0.8, 1.0),
        (9, 1, -0.8, 2.0),
        (1, 1, 1.0, 2.0),
    ];
    for (height, width, amount, radius) in rows {
        let picture = varied(height, width);
        let params = Params {
            amount,
            radius,
            ..Params::default()
        };
        assert_eq!(
            run_picture(&params, height, width, &picture)?,
            picture,
            "{height} x {width}, {params:?}"
        );
    }
    Ok(())
}

#[test]
fn a_flat_picture_stays_as_flat() -> Result<(), Error> {
    let flat = vec![0.3_f32; 5 * 7 * 3];
    for interpolation in INTERPOLATIONS {
        for edge in EDGES {
            let params = Params {
                amount: -1.0,
                radius: 2.0,
                x: 0.2,
                y: 0.9,
                interpolation,
                edge,
            };
            let at = first_difference_within(&run_picture(&params, 5, 7, &flat)?, &flat, TOLERANCE);
            assert_eq!(at, None, "{params:?}: first differs at {at:?}");
        }
    }
    Ok(())
}

#[test]
fn pixels_from_the_radius_out_and_the_centre_keep_their_place() -> Result<(), Error> {
    // a 21 x 21 picture, centre (10, 10), reach 0.4 x 20 = 8 px: a pixel 8 or more from the centre, and the centre, are
    // as they were, for a bulge and a pinch, whatever the reading and the edge rule
    let size = 21;
    let picture = varied(size, size);
    for amount in [0.9, -0.9] {
        for interpolation in INTERPOLATIONS {
            for edge in EDGES {
                let params = Params {
                    amount,
                    radius: 0.4,
                    interpolation,
                    edge,
                    ..Params::default()
                };
                let out = run_picture(&params, size, size, &picture)?;
                for row in 0..size {
                    for column in 0..size {
                        let (down, across) = (row as f64 - 10.0, column as f64 - 10.0);
                        let at = (row * size + column) * 3;
                        let fixed = down.hypot(across) >= 8.0 || (row, column) == (10, 10);
                        if fixed {
                            assert!(
                                out[at..at + 3]
                                    .iter()
                                    .zip(&picture[at..at + 3])
                                    .all(|(out, picture)| (out - picture).abs() < TOLERANCE),
                                "{params:?} ({row}, {column}) moved"
                            );
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[test]
fn a_bulge_and_a_pinch_of_a_ramp_read_it_where_the_definition_by_hand_says() -> Result<(), Error> {
    // by hand: the ramp rises 1/20 a column over 21 columns; centre column 10, reach 0.5 x 20 = 10 px. On the centre row
    // a pixel 5 px right of the centre is d = 5, (1 - d / reach)^2 = 1/4. Amount 1 scales its offset by 1 - 1/4 = 3/4: it
    // shows column 10 + 3.75 = 13.75, value 0.6875 (a linear reading is exact on a ramp). Amount -1 scales it by 5/4: it
    // shows column 16.25, 0.8125. Five px to the left: columns 6.25 and 3.75, 0.3125 and 0.1875. Amount 0.4 scales by 0.9:
    // columns 14.5 and 5.5, 0.725 and 0.275.
    let size = 21;
    let picture = ramp(size, size);
    let value = |amount: f32, column: usize| -> Result<f32, Error> {
        let params = Params {
            amount,
            ..Params::default()
        };
        Ok(run_picture(&params, size, size, &picture)?[(10 * size + column) * 3])
    };
    for (amount, right, left) in [(1.0, 0.6875, 0.3125), (-1.0, 0.8125, 0.1875), (0.4, 0.725, 0.275)] {
        assert!((value(amount, 15)? - right).abs() < TOLERANCE, "amount {amount}, right");
        assert!((value(amount, 5)? - left).abs() < TOLERANCE, "amount {amount}, left");
    }
    Ok(())
}

#[test]
fn a_bulge_magnifies_toward_the_centre_and_a_pinch_shrinks_from_it() -> Result<(), Error> {
    // on a ramp rising to the right the pixels right of the centre show lower values for a bulge (nearer the centre) and
    // higher for a pinch, and the more so the stronger the push; pixels to its left the reverse
    let size = 21;
    let picture = ramp(size, size);
    let at = |column: usize| (10 * size + column) * 3;
    let mut previous = picture.clone();
    for amount in [0.25, 0.5, 0.75, 1.0] {
        let params = Params {
            amount,
            radius: 1.0,
            ..Params::default()
        };
        let bulge = run_picture(&params, size, size, &picture)?;
        let pinch = run_picture(
            &Params {
                amount: -amount,
                ..params
            },
            size,
            size,
            &picture,
        )?;
        for column in [12, 14, 16, 18] {
            assert!(
                bulge[at(column)] < previous[at(column)],
                "amount {amount}, column {column}"
            );
            assert!(
                pinch[at(column)] > picture[at(column)],
                "amount -{amount}, column {column}"
            );
        }
        for column in [2, 4, 6, 8] {
            assert!(
                bulge[at(column)] > previous[at(column)],
                "amount {amount}, column {column}"
            );
            assert!(
                pinch[at(column)] < picture[at(column)],
                "amount -{amount}, column {column}"
            );
        }
        previous = bulge;
    }
    Ok(())
}

#[test]
fn turning_the_picture_half_a_turn_turns_the_result_half_a_turn() -> Result<(), Error> {
    // about the middle of an odd square the push is symmetric under a half turn, and so is every edge rule
    let size = 21;
    let picture = varied(size, size);
    let half_turn = |pixels: &[f32]| -> Vec<f32> { pixels.chunks(3).rev().flat_map(<[f32]>::to_vec).collect() };
    for interpolation in INTERPOLATIONS {
        for edge in EDGES {
            for amount in [0.8, -0.8] {
                let params = Params {
                    amount,
                    radius: 1.5,
                    interpolation,
                    edge,
                    ..Params::default()
                };
                let turned_after = half_turn(&run_picture(&params, size, size, &picture)?);
                let turned_before = run_picture(&params, size, size, &half_turn(&picture))?;
                let at = first_difference_within(&turned_after, &turned_before, TOLERANCE);
                assert_eq!(at, None, "{params:?}: first differs at {at:?}");
            }
        }
    }
    Ok(())
}

/// Parameters over their whole ranges, the ends included.
fn parameters() -> impl Strategy<Value = Params> {
    let fraction = prop_oneof![Just(0.0_f32), Just(1.0_f32), 0.0_f32..=1.0];
    (
        prop_oneof![Just(-1.0_f32), Just(1.0_f32), Just(0.0_f32), -1.0_f32..=1.0],
        prop_oneof![Just(0.0_f32), Just(2.0_f32), 0.0_f32..=2.0],
        fraction.clone(),
        fraction,
        0..INTERPOLATIONS.len(),
        0..EDGES.len(),
    )
        .prop_map(|(amount, radius, x, y, interpolation, edge)| Params {
            amount,
            radius,
            x,
            y,
            interpolation: INTERPOLATIONS[interpolation],
            edge: EDGES[edge],
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn at_every_end_of_every_range_the_picture_is_the_definitions_and_stays_in_range(
        params in parameters(),
        height in 1_usize..6,
        width in 1_usize..9,
        picture in prop::collection::vec(0.0_f32..=1.0, 5 * 8 * 3),
    ) {
        let picture = &picture[..height * width * 3];
        let out = run_picture(&params, height, width, picture)
            .map_err(|error: Error| TestCaseError::fail(error.to_string()))?;
        prop_assert_eq!(out.len(), picture.len());
        prop_assert!(out.iter().all(|value| (0.0..=1.0).contains(value)), "within 0..1");
        let at = first_difference_within(&out, &by_definition(&params, height, width, picture), TOLERANCE);
        prop_assert_eq!(at, None, "{:?}: first differs at {:?}", params, at);
    }
}
