//! `echo`: the picture averaged with copies of itself moved a step further each, the copy k weighted (1 - fade)^k. Checked
//! on impulses with the weights worked out by hand, on flat and wrapped pictures where the average is known, and against
//! the weighted sum of the moved copies from the definition.

#![expect(
    clippy::cast_possible_truncation,
    reason = "small picture sizes; the f64 references narrowed to the f32 picture"
)]

use bullterrier_image::effects::echo::Params;
use bullterrier_image::{Edge, Error};

use crate::common::compare::{first_difference, first_difference_within};
use crate::common::pictures::{impulse, varied};
use crate::common::pixels::{channel, run_picture};
use crate::common::sampling::moved;

const EDGES: [Edge; 4] = [Edge::Reflect, Edge::Nearest, Edge::Mirror, Edge::Wrap];
const TOLERANCE: f32 = 1e-5;

/// The red channel of an `echo` of an impulse in one row at column `at`, the picture `width` pixels wide.
fn row_of_echo(width: usize, at: usize, params: &Params) -> Result<Vec<f32>, Error> {
    let out = run_picture(params, 1, width, &impulse(1, width, 0, at))?;
    Ok(channel(&out, 0))
}

#[test]
fn nothing_to_lay_over_leaves_the_picture_exactly_as_it_is() -> Result<(), Error> {
    let picture = varied(7, 9);
    for (name, params) in [
        (
            "no copies",
            Params {
                copies: 0,
                ..Params::default()
            },
        ),
        (
            "copies that vanish",
            Params {
                fade: 1.0,
                ..Params::default()
            },
        ),
        (
            "copies on the picture",
            Params {
                dx: 0.0,
                dy: 0.0,
                ..Params::default()
            },
        ),
    ] {
        let out = run_picture(&params, 7, 9, &picture)?;
        assert_eq!(first_difference_within(&out, &picture, 1e-6), None, "{name}");
    }
    assert_eq!(
        first_difference(
            &run_picture(
                &Params {
                    copies: 0,
                    ..Params::default()
                },
                7,
                9,
                &picture
            )?,
            &picture
        ),
        None,
        "no copies is bit for bit"
    );
    Ok(())
}

#[test]
fn the_weights_of_the_picture_and_each_copy_are_1_and_the_powers_of_one_less_than_fade() -> Result<(), Error> {
    // an impulse at column 2 of a row; each copy is the impulse moved one more to the right
    // (copies, fade, weights of the picture and its copies); the result is the weights over their total
    for (copies, fade, weights) in [
        (2, 0.5, vec![1.0, 0.5, 0.25]),
        (3, 0.0, vec![1.0, 1.0, 1.0, 1.0]),
        (1, 0.75, vec![1.0, 0.25]),
        (3, 0.2, vec![1.0, 0.8, 0.64, 0.512]),
    ] {
        let params = Params {
            copies,
            dx: 1.0,
            dy: 0.0,
            fade,
            ..Params::default()
        };
        let total: f32 = weights.iter().sum();
        let mut expected = vec![0.0; 12];
        for (copy, weight) in weights.iter().enumerate() {
            expected[2 + copy] = weight / total;
        }
        let out = row_of_echo(12, 2, &params)?;
        assert_eq!(
            first_difference_within(&out, &expected, 1e-6),
            None,
            "{params:?}: {out:?}"
        );
    }
    Ok(())
}

#[test]
fn the_copies_lie_the_step_further_along_in_the_direction_the_sign_of_the_step_says() -> Result<(), Error> {
    // an impulse at (3, 2) of a 7 x 5 picture; the copy moves 2 down and 1 left; fade 0: the picture and the copy count alike
    let params = Params {
        copies: 1,
        dx: -1.0,
        dy: 2.0,
        fade: 0.0,
        ..Params::default()
    };
    let out = run_picture(&params, 7, 5, &impulse(7, 5, 3, 2))?;
    let mut expected = vec![0.0; 7 * 5 * 3];
    for (row, column) in [(3, 2), (5, 1)] {
        expected[(row * 5 + column) * 3..][..3].fill(0.5);
    }
    assert_eq!(first_difference_within(&out, &expected, 1e-6), None);
    Ok(())
}

#[test]
fn a_flat_picture_stays_flat_and_a_wrapped_echo_keeps_the_mean() -> Result<(), Error> {
    let flat = vec![0.37_f32; 6 * 11 * 3];
    let busy = varied(6, 11);
    for edge in EDGES {
        for (copies, dx, dy, fade) in [(4, 14.0, 0.0, 0.5), (32, -3.3, 2.2, 0.1), (3, 500.0, -500.0, 0.9)] {
            let params = Params {
                copies,
                dx,
                dy,
                fade,
                edge,
            };
            let out = run_picture(&params, 6, 11, &flat)?;
            assert_eq!(first_difference_within(&out, &flat, TOLERANCE), None, "{params:?}");
            if edge == Edge::Wrap {
                let (before, after) = (
                    busy.iter().sum::<f32>(),
                    run_picture(&params, 6, 11, &busy)?.iter().sum::<f32>(),
                );
                assert!((before - after).abs() < 1e-2, "{params:?}: {before} became {after}");
            }
        }
    }
    Ok(())
}

#[test]
fn the_result_is_the_weighted_average_of_the_moved_copies_from_the_definition() -> Result<(), Error> {
    for (height, width) in [(1, 1), (1, 7), (7, 1), (5, 9), (16, 10)] {
        let picture = varied(height, width);
        for edge in EDGES {
            for (copies, dx, dy, fade) in [
                (4, 14.0, 0.0, 0.5),
                (3, -2.5, 1.5, 0.3),
                (6, 0.4, -7.3, 0.0),
                (1, 1.0, 1.0, 0.9),
            ] {
                let params = Params {
                    copies,
                    dx,
                    dy,
                    fade,
                    edge,
                };
                let mut sum: Vec<f64> = picture.iter().map(|&level| f64::from(level)).collect();
                let mut total = 1.0;
                for copy in 1..=copies {
                    let weight = f64::from(1.0 - fade).powi(copy);
                    total += weight;
                    let step = f64::from(copy);
                    let shifted = moved(
                        &picture,
                        height,
                        width,
                        edge,
                        step * f64::from(dy),
                        step * f64::from(dx),
                    );
                    for (level, other) in sum.iter_mut().zip(shifted) {
                        *level += weight * f64::from(other);
                    }
                }
                let expected: Vec<f32> = sum.into_iter().map(|level| (level / total) as f32).collect();
                let out = run_picture(&params, height, width, &picture)?;
                let at = first_difference_within(&out, &expected, TOLERANCE);
                assert_eq!(at, None, "{height} x {width}, {params:?}: first differs at {at:?}");
            }
        }
    }
    Ok(())
}
