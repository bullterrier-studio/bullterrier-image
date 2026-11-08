//! The shift the core's effects share (`ops/shift`): a picture moved by a whole or fractional distance, a rule for the
//! edge. It is private to the core; `rgb_split` shows it plain, as its red channel is the picture moved by `amount` along
//! `angle` (0 = right, 90 = down), so the shift is checked through that channel.

#![expect(
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    reason = "small picture sizes and distances; the references narrowed to the f32 picture"
)]

use bullterrier_image::effects::rgb_split::{Pair, Params};
use bullterrier_image::{Edge, Error};

use crate::common::compare::{first_difference, first_difference_within};
use crate::common::edge::folded;
use crate::common::pictures::varied;
use crate::common::pixels::{channel, run_picture};
use crate::common::sampling::moved;

const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];
const TOLERANCE: f32 = 1e-5;

/// The red channel of the picture `values`.
fn red(values: &[f32]) -> Vec<f32> {
    channel(values, 0)
}

/// A grey picture of one row (every channel the value) and red of it moved by `amount` to the right, with `edge` beyond.
fn row_moved(row: &[f32], amount: f32, edge: Edge) -> Result<Vec<f32>, Error> {
    let grey: Vec<f32> = row.iter().flat_map(|&level| [level; 3]).collect();
    let params = Params {
        pair: Pair::RedBlue,
        angle: 0.0,
        amount,
        edge,
    };
    Ok(red(&run_picture(&params, 1, row.len(), &grey)?))
}

#[test]
fn a_whole_distance_moves_pixels_exactly_by_index_under_every_edge_rule() -> Result<(), Error> {
    let (height, width) = (5, 7);
    let picture = varied(height, width);
    for edge in EDGES {
        // up to beyond the picture's width
        for amount in [0, 1, 2, 3, 6, 7, 8, 15] {
            let params = Params {
                pair: Pair::RedBlue,
                angle: 0.0,
                amount: amount as f32,
                edge,
            };
            let out = red(&run_picture(&params, height, width, &picture)?);
            let expected: Vec<f32> = (0..height * width)
                .map(|at| {
                    let (row, column) = (at / width, at % width);
                    let from = folded(edge, column as i64 - i64::from(amount), width as i64);
                    picture[(row * width + from) * 3]
                })
                .collect();
            assert_eq!(first_difference(&out, &expected), None, "{edge:?} by {amount}");
        }
    }
    Ok(())
}

#[test]
fn a_move_down_leaves_a_column_in_its_column() -> Result<(), Error> {
    // angle 90: red moves down; rows are counted from the top, so row i shows row i - amount
    let (height, width) = (9, 4);
    let picture = varied(height, width);
    for edge in EDGES {
        let params = Params {
            pair: Pair::RedBlue,
            angle: 90.0,
            amount: 2.0,
            edge,
        };
        let out = red(&run_picture(&params, height, width, &picture)?);
        let expected: Vec<f32> = (0..height * width)
            .map(|at| {
                let from = folded(edge, (at / width) as i64 - 2, height as i64);
                picture[(from * width + at % width) * 3]
            })
            .collect();
        assert_eq!(first_difference_within(&out, &expected, TOLERANCE), None, "{edge:?}");
    }
    Ok(())
}

#[test]
fn a_fractional_distance_is_the_linear_blend_of_the_two_pixels_it_lies_between() -> Result<(), Error> {
    // an impulse at column 2, moved by 1.25: three quarters of it 1 away, a quarter 2 away; by 0.5 half and half
    for (amount, expected) in [
        (1.25, [0.0, 0.0, 0.0, 0.75, 0.25, 0.0]),
        (0.5, [0.0, 0.0, 0.5, 0.5, 0.0, 0.0]),
        (-0.5, [0.0, 0.5, 0.5, 0.0, 0.0, 0.0]),
        (-1.25, [0.25, 0.75, 0.0, 0.0, 0.0, 0.0]),
    ] {
        let out = row_moved(&[0.0, 0.0, 1.0, 0.0, 0.0, 0.0], amount, Edge::Nearest)?;
        assert_eq!(
            first_difference_within(&out, &expected, 1e-6),
            None,
            "by {amount}: {out:?}"
        );
    }
    Ok(())
}

#[test]
fn what_a_move_uncovers_is_what_the_edge_rule_says() -> Result<(), Error> {
    // the row a b c d moved right by 2
    let row = [0.1, 0.2, 0.3, 0.4];
    for (edge, expected) in [
        (Edge::Wrap, [0.3, 0.4, 0.1, 0.2]),
        (Edge::Nearest, [0.1, 0.1, 0.1, 0.2]),
        (Edge::Reflect, [0.2, 0.1, 0.1, 0.2]),
        (Edge::Mirror, [0.3, 0.2, 0.1, 0.2]),
    ] {
        let out = row_moved(&row, 2.0, edge)?;
        assert_eq!(
            first_difference_within(&out, &expected, 1e-6),
            None,
            "{edge:?}: {out:?}"
        );
    }
    Ok(())
}

#[test]
fn any_distance_agrees_with_the_definition_on_every_shape_and_rule() -> Result<(), Error> {
    for (height, width) in [(1, 1), (1, 7), (7, 1), (5, 9), (16, 10)] {
        let picture = varied(height, width);
        for edge in EDGES {
            // along the rows, along the columns, and (the angle 90 leaves a speck of 1e-16 in the other axis) beyond the size
            for (angle, amount) in [
                (0.0, 0.5),
                (0.0, 3.7),
                (0.0, 25.3),
                (90.0, 0.5),
                (90.0, 2.25),
                (90.0, 33.9),
            ] {
                let params = Params {
                    pair: Pair::RedBlue,
                    angle,
                    amount,
                    edge,
                };
                let out = red(&run_picture(&params, height, width, &picture)?);
                let (dy, dx) = if angle == 0.0 { (0.0, amount) } else { (amount, 0.0) };
                let grey: Vec<f32> = red(&picture).into_iter().flat_map(|level| [level; 3]).collect();
                let expected = red(&moved(&grey, height, width, edge, f64::from(dy), f64::from(dx)));
                let at = first_difference_within(&out, &expected, TOLERANCE);
                assert_eq!(at, None, "{height} x {width}, {edge:?}, angle {angle}, amount {amount}");
            }
        }
    }
    Ok(())
}

#[test]
fn a_wrapped_move_keeps_the_total() -> Result<(), Error> {
    let picture = varied(6, 11);
    for (angle, amount) in [(0.0, 3.4), (90.0, 2.6), (330.0, 7.7), (45.0, 60.1)] {
        let params = Params {
            pair: Pair::RedBlue,
            angle,
            amount,
            edge: Edge::Wrap,
        };
        let out = red(&run_picture(&params, 6, 11, &picture)?);
        let (before, after) = (red(&picture).iter().sum::<f32>(), out.iter().sum::<f32>());
        assert!(
            (before - after).abs() < 1e-3,
            "angle {angle}, amount {amount}: {before} became {after}"
        );
    }
    Ok(())
}
