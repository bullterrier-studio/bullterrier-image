//! `swirl`: the picture compared with a point-by-point computation from the definition (every reading, every edge rule),
//! hand-worked turns of an impulse, and the properties a twist about a centre keeps.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "small picture sizes; the f64 reference narrowed to the f32 picture"
)]

use bullterrier_image::effects::swirl::Params;
use bullterrier_image::{Edge, Error, Interpolation};
use proptest::prelude::*;

use crate::common::compare::first_difference_within;
use crate::common::pictures::{impulse, ramp, varied};
use crate::common::pixels::run_picture;
use crate::common::sampling::warped;

/// The pictures are compared at this distance: the reference reads in f64 and the core stores f32.
const TOLERANCE: f32 = 1e-5;

const INTERPOLATIONS: [Interpolation; 3] = [Interpolation::Nearest, Interpolation::Bilinear, Interpolation::Bicubic];
const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];
/// The shapes the comparison runs on: a pixel, a row, a column, a few pixels, odd and even sides.
const SHAPES: [(usize, usize); 6] = [(1, 1), (1, 9), (9, 1), (5, 7), (23, 31), (16, 10)];

/// `picture` twisted as `params` says, worked out from the definition: the centre is at fraction (`x`, `y`) of the way from
/// the first to the last pixel; a pixel `d` px from it, `d` inside `radius` x (shorter side - 1), shows the picture at the
/// point that is its offset from the centre turned by `angle x (1 - d / reach)^2` degrees (clockwise as the picture
/// appears, for a positive angle), read by `interpolation` with `edge` beyond the picture; the rest keeps its place.
fn by_definition(params: &Params, height: usize, width: usize, picture: &[f32]) -> Vec<f32> {
    let reach = f64::from(params.radius) * (height.min(width) - 1) as f64;
    if params.angle == 0.0 || reach == 0.0 {
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
            let turn = f64::from(params.angle).to_radians() * inside * inside;
            (
                centre.0 + down * turn.cos() - across * turn.sin(),
                centre.1 + down * turn.sin() + across * turn.cos(),
            )
        },
    )
}

#[test]
fn the_picture_is_what_a_point_by_point_computation_from_the_definition_gives() -> Result<(), Error> {
    // (angle, radius, x, y): both senses and several turns, a small disc and one over the whole picture, the centre in the
    // middle, at a corner and off the pixel grid
    let sets = [
        (90.0, 0.5, 0.5, 0.5),
        (-300.0, 0.9, 0.0, 0.0),
        (720.0, 2.0, 0.37, 0.81),
        (-720.0, 0.3, 1.0, 0.4),
        (33.0, 1.3, 0.5, 1.0),
    ];
    for interpolation in INTERPOLATIONS {
        for edge in EDGES {
            for (angle, radius, x, y) in sets {
                let params = Params {
                    angle,
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
fn the_comparison_tells_twists_that_differ_apart() -> Result<(), Error> {
    // the known-bad rows: another angle, sense, radius, centre, reading or edge rule is not this twist; and the twist is
    // not nothing, so the differences show
    let picture = varied(23, 31);
    let params = Params {
        angle: 200.0,
        radius: 1.2,
        x: 0.3,
        y: 0.6,
        interpolation: Interpolation::Bilinear,
        edge: Edge::Wrap,
    };
    let ours = run_picture(&params, 23, 31, &picture)?;
    assert!(
        first_difference_within(&ours, &picture, 0.05).is_some(),
        "the twist changes the picture"
    );
    for other in [
        Params {
            angle: 190.0,
            ..params.clone()
        },
        Params {
            angle: -200.0,
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
fn no_angle_no_radius_and_a_picture_with_no_distance_leave_the_picture_exactly_as_it_is() -> Result<(), Error> {
    // (height, width, angle, radius): a picture one pixel thick has no reach
    let rows = [
        (6, 5, 0.0, 0.5),
        (6, 5, 80.0, 0.0),
        (1, 9, 80.0, 1.0),
        (9, 1, -80.0, 2.0),
        (1, 1, 720.0, 2.0),
    ];
    for (height, width, angle, radius) in rows {
        let picture = varied(height, width);
        let params = Params {
            angle,
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
                angle: -500.0,
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
    // as they were, for a twist either way, whatever the reading and the edge rule
    let size = 21;
    let picture = varied(size, size);
    for angle in [300.0, -300.0] {
        for interpolation in INTERPOLATIONS {
            for edge in EDGES {
                let params = Params {
                    angle,
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
                        if down.hypot(across) >= 8.0 || (row, column) == (10, 10) {
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
fn a_positive_angle_carries_what_lies_above_the_centre_to_the_right_of_it_and_a_negative_one_to_the_left()
-> Result<(), Error> {
    // by hand: 21 x 21, centre (10, 10), reach 0.5 x 20 = 10 px, angle 360. At 5 px from the centre (1 - 5 / 10)^2 = 1/4 of
    // the angle is a quarter turn, so what lies 5 px above the centre, a lit pixel at row 5, column 10, is seen a quarter
    // turn clockwise round the centre: at row 10, column 15. (A pixel there reads the point a quarter turn the other way
    // from it, 5 px above the centre.) Turned the other way it lands at row 10, column 5, and it is seen nowhere else.
    let size = 21;
    let picture = impulse(size, size, 5, 10);
    for (angle, lit) in [(360.0, (10, 15)), (-360.0, (10, 5))] {
        let params = Params {
            angle,
            ..Params::default()
        };
        let out = run_picture(&params, size, size, &picture)?;
        let bright: Vec<(usize, usize)> = (0..size * size)
            .filter(|&at| out[at * 3] > 0.5)
            .map(|at| (at / size, at % size))
            .collect();
        assert_eq!(bright, vec![lit], "angle {angle}");
        assert!(out[(lit.0 * size + lit.1) * 3] > 0.99, "angle {angle}: lit at full");
    }
    Ok(())
}

#[test]
fn a_picture_that_depends_only_on_the_distance_from_the_centre_is_left_as_it_is() -> Result<(), Error> {
    // a twist only moves a point along its circle round the centre, so a picture that is the same all round every circle
    // does not change, beyond what reading between pixels costs (a cone, a smooth bump), unlike a picture that is not
    let size = 41;
    let rings: Vec<f32> = (0..size * size)
        .flat_map(|at| {
            let d = ((at / size) as f64 - 20.0).hypot((at % size) as f64 - 20.0);
            [(d / 30.0) as f32; 3]
        })
        .collect();
    let params = Params {
        angle: 400.0,
        radius: 0.5, // reach 20 px: every circle it turns stays inside the picture
        interpolation: Interpolation::Bicubic,
        ..Params::default()
    };
    let out = run_picture(&params, size, size, &rings)?;
    let at = first_difference_within(&out, &rings, 0.02);
    assert_eq!(at, None, "rings: first differs at {at:?}");
    let slope = ramp(size, size);
    assert!(
        first_difference_within(&run_picture(&params, size, size, &slope)?, &slope, 0.1).is_some(),
        "a ramp is twisted"
    );
    Ok(())
}

#[test]
fn turning_the_picture_half_a_turn_turns_the_result_half_a_turn() -> Result<(), Error> {
    // about the middle of an odd square the twist is symmetric under a half turn, and so is every edge rule
    let size = 21;
    let picture = varied(size, size);
    let half_turn = |pixels: &[f32]| -> Vec<f32> { pixels.chunks(3).rev().flat_map(<[f32]>::to_vec).collect() };
    for interpolation in INTERPOLATIONS {
        for edge in EDGES {
            for angle in [150.0, -150.0] {
                let params = Params {
                    angle,
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
        prop_oneof![Just(-720.0_f32), Just(720.0_f32), Just(0.0_f32), -720.0_f32..=720.0],
        prop_oneof![Just(0.0_f32), Just(2.0_f32), 0.0_f32..=2.0],
        fraction.clone(),
        fraction,
        0..INTERPOLATIONS.len(),
        0..EDGES.len(),
    )
        .prop_map(|(angle, radius, x, y, interpolation, edge)| Params {
            angle,
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
