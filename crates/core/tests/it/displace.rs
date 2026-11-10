//! `displace`: every pixel moved by two fields of fractal noise. The moves are read back from the picture through a warp
//! of the `coordinates` picture (each pixel then holds the place it was read from), so the properties are those of the
//! moves: of deviation `amount`, bounded by the noise's greatest value, proportional to it, as smooth as `scale` says, different for another seed, independent
//! between the two axes.

#![expect(
    clippy::cast_precision_loss,
    reason = "small picture sizes: every count and index is far below 2^52"
)]

use bullterrier_image::effects::displace::Params;
use bullterrier_image::{Edge, Error, Interpolation};

use crate::common::compare::first_bit_difference;
use crate::common::pictures::{coordinates, places, varied};
use crate::common::pixels::run_picture;

const SIDE: usize = 96;

/// The moves (down, right) of every pixel of a `SIDE` x `SIDE` picture displaced by `params`, in px: where it reads the
/// picture minus where it lies.
fn moves(params: &Params) -> Result<Vec<(f64, f64)>, Error> {
    let out = run_picture(params, SIDE, SIDE, &coordinates(SIDE, SIDE))?;
    Ok(places(&out, SIDE, SIDE)
        .into_iter()
        .enumerate()
        .map(|(at, (row, column))| (row - (at / SIDE) as f64, column - (at % SIDE) as f64))
        .collect())
}

/// Whether a pixel lies at least `margin` px from every border, so that a move of at most `margin` reads inside the picture.
fn inside(at: usize, margin: usize) -> bool {
    let (row, column) = (at / SIDE, at % SIDE);
    (margin..SIDE - margin).contains(&row) && (margin..SIDE - margin).contains(&column)
}

fn smooth() -> Params {
    Params {
        amount: 2.0,
        scale: 40.0,
        octaves: 1,
        ..Params::default()
    }
}

/// The Pearson correlation of `left` and `right`.
fn correlation(left: &[f64], right: &[f64]) -> f64 {
    let count = left.len() as f64;
    let (mean_left, mean_right) = (left.iter().sum::<f64>() / count, right.iter().sum::<f64>() / count);
    let covariance: f64 = left
        .iter()
        .zip(right)
        .map(|(l, r)| (l - mean_left) * (r - mean_right))
        .sum();
    let spread = |values: &[f64], mean: f64| values.iter().map(|v| (v - mean).powi(2)).sum::<f64>();
    covariance / (spread(left, mean_left) * spread(right, mean_right)).sqrt()
}

/// The correlation between the move across at each pixel and at the pixel to its right.
fn neighbour_correlation(moves: &[(f64, f64)]) -> f64 {
    let pairs: Vec<(f64, f64)> = (0..SIDE * SIDE)
        .filter(|at| at % SIDE != SIDE - 1)
        .map(|at| (moves[at].1, moves[at + 1].1))
        .collect();
    let (here, next): (Vec<f64>, Vec<f64>) = pairs.into_iter().unzip();
    correlation(&here, &next)
}

#[test]
fn no_amount_leaves_the_picture_exactly_as_it_is() -> Result<(), Error> {
    let picture = varied(7, 9);
    for seed in [0, 5] {
        let params = Params {
            amount: 0.0,
            seed,
            ..Params::default()
        };
        assert_eq!(run_picture(&params, 7, 9, &picture)?, picture, "seed {seed}");
    }
    Ok(())
}

#[test]
fn a_single_pixel_has_nothing_to_move_by_and_thin_pictures_are_displaced() -> Result<(), Error> {
    let params = Params {
        amount: 500.0,
        ..Params::default()
    };
    let one = vec![0.2, 0.5, 0.9];
    assert_eq!(run_picture(&params, 1, 1, &one)?, one);
    for (height, width) in [(1, 12), (12, 1)] {
        let picture = varied(height, width);
        let out = run_picture(&params, height, width, &picture)?;
        assert_eq!(out.len(), picture.len());
        assert!(out.iter().all(|level| (0.0..=1.0).contains(level)));
    }
    Ok(())
}

#[test]
fn no_move_is_larger_than_the_amount_times_the_greatest_value_the_noise_can_take() -> Result<(), Error> {
    // A noise value is a sum of lattice values (within -1..1) read with Catmull-Rom weights, which sum to at most 5/4 in size
    // along an axis, so at most (5/4)^2 x (the octave weights) = 1.5625 x W; it is divided by its deviation, at least
    // sqrt(S / 3) x 41/64 for S the sum of the squared octave weights (a lattice value has variance 1/3, the weights of an axis
    // sum to at least 41/64 in square). So |value| <= 1.5625 x W / (sqrt(S / 3) x 0.640625), worked out by hand per row:
    // one octave: W = S = 1, 1.5625 / (0.57735 x 0.640625) = 4.2247;
    // 4 octaves of weights 1, .9, .81, .729: W = 3.439, S = 2.99754, 5.3734 / (0.99959 x 0.640625) = 8.3910;
    // lacunarity 4 from 80 px: spacings 80, 20, 5, 1.25 are kept (0.3125 is under a pixel), weights 1: W = S = 4,
    // 6.25 / (1.1547 x 0.640625) = 8.4490.
    // (amount, scale, octaves, persistence, lacunarity, greatest value). With the edge rule 'nearest' a read beyond the picture
    // lands on the border, which is never farther than the unclamped read.
    let rows = [
        (10.0, 40.0, 1, 0.5, 2.0, 4.2247),
        (3.0, 5.0, 4, 0.9, 1.0, 8.3910),
        (25.0, 80.0, 8, 1.0, 4.0, 8.4490),
        (0.5, 1.0, 1, 0.5, 2.0, 4.2247),
    ];
    for (amount, scale, octaves, persistence, lacunarity, greatest) in rows {
        let params = Params {
            amount,
            scale,
            octaves,
            persistence,
            lacunarity,
            seed: 11,
            edge: Edge::Nearest,
            ..Params::default()
        };
        let largest = moves(&params)?
            .into_iter()
            .map(|(down, across)| down.abs().max(across.abs()))
            .fold(0.0, f64::max);
        assert!(
            largest <= f64::from(amount) * greatest + 1e-3,
            "{params:?}: a pixel moves {largest} px along an axis"
        );
    }
    Ok(())
}

#[test]
fn each_move_has_the_amount_as_its_deviation_and_the_moves_scale_with_it() -> Result<(), Error> {
    // every place has noise of deviation 1, so over many seeds each move has mean 0 and deviation `amount` (a place by itself
    // is one draw, so the picture is not standardised): read off the interior, away from the border, over 8 seeds
    let amount: f32 = 3.0;
    let (mut down, mut across) = (Vec::new(), Vec::new());
    for seed in 1..=8 {
        let params = Params {
            amount,
            scale: 8.0,
            octaves: 1,
            seed,
            edge: Edge::Nearest,
            ..Params::default()
        };
        for (_, (d, a)) in moves(&params)?
            .into_iter()
            .enumerate()
            .filter(|(at, _)| inside(*at, 13))
        {
            down.push(d);
            across.push(a);
        }
    }
    for (axis, values) in [("down", &down), ("across", &across)] {
        let count = values.len() as f64;
        let mean = values.iter().sum::<f64>() / count;
        let deviation = (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / count).sqrt();
        assert!(mean.abs() < 0.4, "{axis}: mean move {mean}");
        assert!(
            (deviation - f64::from(amount)).abs() < 0.1 * f64::from(amount),
            "{axis}: deviation {deviation} for an amount of {amount}"
        );
    }
    // twice the amount is twice every move
    let params = |amount| Params { amount, ..smooth() };
    let (once, twice) = (moves(&params(2.0))?, moves(&params(4.0))?);
    for (at, (one, two)) in once.iter().zip(&twice).enumerate().filter(|(at, _)| inside(*at, 18)) {
        assert!(
            (2.0 * one.0 - two.0).abs() < 1e-3 && (2.0 * one.1 - two.1).abs() < 1e-3,
            "pixel {at}: {one:?} and {two:?}"
        );
    }
    Ok(())
}

#[test]
fn the_edge_rule_only_matters_within_the_greatest_move_of_the_border() -> Result<(), Error> {
    let picture = varied(SIDE, SIDE);
    let run = |edge| run_picture(&Params { edge, ..smooth() }, SIDE, SIDE, &picture);
    let reflected = run(Edge::Reflect)?;
    for edge in [Edge::Mirror, Edge::Nearest, Edge::Wrap] {
        let other = run(edge)?;
        let mut near_border = false;
        for at in 0..SIDE * SIDE {
            let same = reflected[at * 3..at * 3 + 3] == other[at * 3..at * 3 + 3];
            if inside(at, 11) {
                assert!(same, "{edge:?}: pixel {at} differs inside");
            } else {
                near_border |= !same;
            }
        }
        assert!(near_border, "{edge:?} should read differently somewhere at the border");
    }
    Ok(())
}

#[test]
fn the_same_seed_gives_the_same_displacement_and_another_seed_another() -> Result<(), Error> {
    let picture = varied(40, 50);
    let run = |seed| {
        run_picture(
            &Params {
                seed,
                ..Params::default()
            },
            40,
            50,
            &picture,
        )
    };
    assert_eq!(first_bit_difference(&run(3)?, &run(3)?), None);
    for other in [0, 1, 4, u32::MAX] {
        assert!(
            first_bit_difference(&run(3)?, &run(other)?).is_some(),
            "seeds 3 and {other} give one picture"
        );
    }
    assert!(first_bit_difference(&run(0)?, &run(u32::MAX)?).is_some());
    Ok(())
}

#[test]
fn a_larger_scale_is_smoother_and_a_scale_of_one_moves_every_pixel_on_its_own() -> Result<(), Error> {
    // the move of a pixel and of its right-hand neighbour: alike on broad waves, unrelated when each pixel has a value of
    // its own (a lattice 1 px apart with one octave)
    let correlation_at = |scale| {
        let params = Params {
            amount: 8.0,
            scale,
            octaves: 1,
            seed: 2,
            ..Params::default()
        };
        moves(&params).map(|moves| neighbour_correlation(&moves))
    };
    let (broad, medium, own) = (correlation_at(48.0)?, correlation_at(8.0)?, correlation_at(1.0)?);
    assert!(broad > 0.98, "scale 48: {broad}");
    assert!(medium > 0.7 && medium < broad, "scale 8: {medium}");
    assert!(own.abs() < 0.1, "scale 1: {own}");
    Ok(())
}

#[test]
fn the_two_axes_move_independently() -> Result<(), Error> {
    // some 360 independent cells in the interior: the correlation of that many pairs is within 0.2 of 0 by a wide margin
    let params = Params {
        amount: 10.0,
        scale: 4.0,
        octaves: 1,
        seed: 9,
        edge: Edge::Nearest,
        ..Params::default()
    };
    let (down, across): (Vec<f64>, Vec<f64>) = moves(&params)?
        .into_iter()
        .enumerate()
        .filter(|(at, _)| inside(*at, 10))
        .map(|(_, moves)| moves)
        .unzip();
    let together = correlation(&down, &across);
    assert!(together.abs() < 0.2, "the moves correlate by {together}");
    // and each axis is a field of its own, not a constant
    assert!(down.iter().any(|value| value.abs() > 3.0) && across.iter().any(|value| value.abs() > 3.0));
    Ok(())
}

#[test]
fn only_the_layers_that_reach_a_pixel_count_and_persistence_weighs_them() -> Result<(), Error> {
    let picture = varied(40, 50);
    let run = |octaves, persistence, scale, lacunarity| {
        run_picture(
            &Params {
                octaves,
                persistence,
                scale,
                lacunarity,
                seed: 6,
                ..Params::default()
            },
            40,
            50,
            &picture,
        )
    };
    // a persistence of 0 leaves the coarsest layer alone, however many are asked for
    assert_eq!(
        first_bit_difference(&run(5, 0.0, 30.0, 2.0)?, &run(1, 0.0, 30.0, 2.0)?),
        None
    );
    // layers finer than a pixel are dropped: a scale of 3 holds spacings 3, 1.5 (and 0.75, dropped) at lacunarity 2, and 3
    // (then 0.75, dropped) at lacunarity 4
    assert_eq!(
        first_bit_difference(&run(3, 0.5, 3.0, 2.0)?, &run(2, 0.5, 3.0, 2.0)?),
        None
    );
    assert_eq!(
        first_bit_difference(&run(3, 0.5, 3.0, 4.0)?, &run(1, 0.5, 3.0, 4.0)?),
        None
    );
    // a layer that does reach adds its detail, and a lacunarity of 1 keeps every layer a full spacing
    assert!(first_bit_difference(&run(2, 0.5, 30.0, 2.0)?, &run(1, 0.5, 30.0, 2.0)?).is_some());
    assert!(first_bit_difference(&run(2, 0.5, 30.0, 2.0)?, &run(2, 1.0, 30.0, 2.0)?).is_some());
    assert!(first_bit_difference(&run(3, 0.5, 3.0, 1.0)?, &run(1, 0.5, 3.0, 1.0)?).is_some());
    Ok(())
}

#[test]
fn every_reading_stays_in_range() -> Result<(), Error> {
    let picture = varied(31, 17);
    for interpolation in [Interpolation::Nearest, Interpolation::Bilinear, Interpolation::Bicubic] {
        let params = Params {
            amount: 40.0,
            scale: 6.0,
            interpolation,
            ..Params::default()
        };
        let out = run_picture(&params, 31, 17, &picture)?;
        assert!(out.iter().all(|level| (0.0..=1.0).contains(level)), "{interpolation:?}");
    }
    Ok(())
}
