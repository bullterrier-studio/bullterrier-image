//! `rgb_split`: red moved `amount` px along `angle`, blue (and for `red_cyan` green) the same way back. Checked on a white
//! impulse in a black picture, where each channel's place is worked out by hand, on the channels that must not move, and
//! against the picture moved from the definition.

#![expect(
    clippy::cast_precision_loss,
    reason = "small picture sizes; the f64 references narrowed to the f32 picture"
)]

use bullterrier_image::effects::rgb_split::{Pair, Params};
use bullterrier_image::{Edge, Error};

use crate::common::compare::{first_difference, first_difference_within};
use crate::common::pictures::{impulse, varied};
use crate::common::pixels::{channel, run_picture};
use crate::common::sampling::moved;

const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];
const SIDE: usize = 11;

/// Where the (brightness-weighted) middle of the channel `channel` of an `SIDE` x `SIDE` picture is, as (row, column).
fn centre(values: &[f32], which: usize) -> (f64, f64) {
    let levels = channel(values, which);
    let total: f64 = levels.iter().map(|&level| f64::from(level)).sum();
    let at = |position: fn(usize) -> usize| {
        levels
            .iter()
            .enumerate()
            .map(|(index, &level)| f64::from(level) * position(index) as f64)
            .sum::<f64>()
            / total
    };
    (at(|index| index / SIDE), at(|index| index % SIDE))
}

#[test]
fn red_moves_along_the_angle_and_blue_the_opposite_way_while_green_stays() -> Result<(), Error> {
    // a white impulse in the middle of a black picture, 2 px: angle 0 right, 90 down, 180 left, 270 up (rows count down)
    let middle = impulse(SIDE, SIDE, 5, 5);
    for (angle, red, blue) in [
        (0.0, (5, 7), (5, 3)),
        (90.0, (7, 5), (3, 5)),
        (180.0, (5, 3), (5, 7)),
        (270.0, (3, 5), (7, 5)),
    ] {
        let params = Params {
            angle,
            amount: 2.0,
            ..Params::default()
        };
        let out = run_picture(&params, SIDE, SIDE, &middle)?;
        for (which, (row, column)) in [(0, red), (1, (5, 5)), (2, blue)] {
            let expected = channel(&impulse(SIDE, SIDE, row, column), 0);
            let at = first_difference_within(&channel(&out, which), &expected, 1e-6);
            assert_eq!(at, None, "angle {angle}, channel {which}: first differs at {at:?}");
        }
    }
    Ok(())
}

#[test]
fn with_red_cyan_green_goes_with_blue() -> Result<(), Error> {
    let params = Params {
        pair: Pair::RedCyan,
        angle: 0.0,
        amount: 3.0,
        ..Params::default()
    };
    let out = run_picture(&params, SIDE, SIDE, &impulse(SIDE, SIDE, 5, 5))?;
    for (which, column) in [(0, 8), (1, 2), (2, 2)] {
        let expected = channel(&impulse(SIDE, SIDE, 5, column), 0);
        assert_eq!(
            first_difference_within(&channel(&out, which), &expected, 1e-6),
            None,
            "channel {which}"
        );
    }
    Ok(())
}

#[test]
fn the_default_angle_moves_red_up_and_to_the_right_by_the_amount() -> Result<(), Error> {
    // 330 degrees clockwise from right is up and right: red 4 px along it is 2 up (4 sin 330) and 3.46 right (4 cos 330);
    // a linear read keeps the place of the middle of the impulse
    let params = Params {
        amount: 4.0,
        ..Params::default()
    };
    let out = run_picture(&params, SIDE, SIDE, &impulse(SIDE, SIDE, 5, 5))?;
    let (red, green, blue) = (centre(&out, 0), centre(&out, 1), centre(&out, 2));
    for (found, expected) in [
        (red, (3.0, 5.0 + 3.464_101_6)),
        (green, (5.0, 5.0)),
        (blue, (7.0, 5.0 - 3.464_101_6)),
    ] {
        assert!(
            (found.0 - expected.0).abs() < 1e-4 && (found.1 - expected.1).abs() < 1e-4,
            "{found:?} not {expected:?}"
        );
    }
    Ok(())
}

#[test]
fn no_amount_leaves_the_picture_exactly_as_it_is_and_an_angle_of_360_is_the_angle_0() -> Result<(), Error> {
    let picture = varied(7, 9);
    for pair in [Pair::RedBlue, Pair::RedCyan] {
        let none = Params {
            pair,
            amount: 0.0,
            ..Params::default()
        };
        assert_eq!(
            first_difference(&run_picture(&none, 7, 9, &picture)?, &picture),
            None,
            "{pair:?}"
        );
        let (full, zero) = (
            Params {
                pair,
                angle: 360.0,
                ..Params::default()
            },
            Params {
                pair,
                angle: 0.0,
                ..Params::default()
            },
        );
        assert_eq!(
            first_difference_within(
                &run_picture(&full, 7, 9, &picture)?,
                &run_picture(&zero, 7, 9, &picture)?,
                1e-5
            ),
            None,
            "{pair:?}"
        );
    }
    Ok(())
}

#[test]
fn each_channel_is_the_picture_moved_from_the_definition_on_every_shape_and_rule() -> Result<(), Error> {
    for (height, width) in [(1, 1), (1, 7), (7, 1), (5, 9), (16, 10)] {
        let picture = varied(height, width);
        for edge in EDGES {
            for pair in [Pair::RedBlue, Pair::RedCyan] {
                for (angle, amount) in [(330.0_f32, 8.0_f32), (45.0, 2.5), (200.0, 13.7), (90.0, 0.4)] {
                    let params = Params {
                        pair,
                        angle,
                        amount,
                        edge,
                    };
                    let out = run_picture(&params, height, width, &picture)?;
                    let radians = f64::from(angle).to_radians();
                    let (dx, dy) = (f64::from(amount) * radians.cos(), f64::from(amount) * radians.sin());
                    let forward = moved(&picture, height, width, edge, dy, dx);
                    let back = moved(&picture, height, width, edge, -dy, -dx);
                    let expected: Vec<f32> = (0..height * width * 3)
                        .map(|at| match (at % 3, pair) {
                            (0, _) => forward[at],
                            (1, Pair::RedBlue) => picture[at],
                            _ => back[at],
                        })
                        .collect();
                    let at = first_difference_within(&out, &expected, 1e-5);
                    assert_eq!(at, None, "{height} x {width}, {params:?}: first differs at {at:?}");
                }
            }
        }
    }
    Ok(())
}
