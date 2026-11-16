//! `stipple`: dots whose number follows the tone, spread by relaxation. The oracles are the tone a flat picture must keep
//! (ink = darkness while dots do not crowd), a dot count worked from the area of one anti-aliased dot, `pi (r^2 + 1/12)`,
//! the spacing of a relaxed pattern against the hexagonal lattice and the random scatter, where ink may lie, and the seed.

#![expect(
    clippy::cast_precision_loss,
    clippy::float_cmp,
    reason = "small pixel counts; paper is an exact value"
)]

use bullterrier_image::Error;
use bullterrier_image::effects::stipple::Params;

use crate::common::pixels::run_picture;

fn flat(height: usize, width: usize, level: f32) -> Vec<f32> {
    vec![level; height * width * 3]
}

/// The red channel of the render of `values`.
fn red(params: &Params, height: usize, width: usize, values: &[f32]) -> Result<Vec<f32>, Error> {
    let out = run_picture(params, height, width, values)?;
    Ok(out.as_chunks::<3>().0.iter().map(|pixel| pixel[0]).collect())
}

fn ink(out: &[f32]) -> f32 {
    1.0 - out.iter().sum::<f32>() / out.len() as f32
}

/// The centres of the dots, as the centroids of the 4-connected groups of pixels with more than half ink.
fn dots(out: &[f32], height: usize, width: usize) -> Vec<[f64; 2]> {
    let mut seen = vec![false; out.len()];
    let mut centres = Vec::new();
    for start in 0..out.len() {
        if seen[start] || out[start] >= 0.5 {
            continue;
        }
        let (mut stack, mut sum, mut count) = (vec![start], [0.0; 2], 0.0);
        seen[start] = true;
        while let Some(at) = stack.pop() {
            let (row, column) = (at / width, at % width);
            sum[0] += column as f64 + 0.5;
            sum[1] += row as f64 + 0.5;
            count += 1.0;
            let near = [
                (column > 0).then(|| at - 1),
                (column + 1 < width).then(|| at + 1),
                (row > 0).then(|| at - width),
                (row + 1 < height).then(|| at + width),
            ];
            for next in near.into_iter().flatten() {
                if !seen[next] && out[next] < 0.5 {
                    seen[next] = true;
                    stack.push(next);
                }
            }
        }
        centres.push([sum[0] / count, sum[1] / count]);
    }
    centres
}

/// The mean distance from a dot to the dot nearest it.
fn mean_nearest(centres: &[[f64; 2]]) -> f64 {
    let total: f64 = centres
        .iter()
        .enumerate()
        .map(|(at, a)| {
            centres
                .iter()
                .enumerate()
                .filter(|&(other, _)| other != at)
                .map(|(_, b)| ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt())
                .fold(f64::INFINITY, f64::min)
        })
        .sum();
    total / centres.len() as f64
}

/// A 120 x 120 picture of darkness 0.1, dots of radius 1.5 that are 6 px or so apart.
const SIDE: usize = 120;
const DARKNESS: f32 = 0.1;

/// The dots of the flat picture: `pi (1.5^2 + 1/12)` = 7.33 px of ink each, so 0.1 x 14400 / 7.33 = 196.4 of them.
const COUNT: f64 = 196.4;

fn sparse(iterations: i32) -> Params {
    Params {
        radius: 1.5,
        iterations,
        ..Params::default()
    }
}

#[test]
fn a_white_picture_has_no_dots() -> Result<(), Error> {
    let out = red(&sparse(10), 16, 16, &flat(16, 16, 1.0))?;
    assert!(out.iter().all(|&v| v == 1.0));
    Ok(())
}

#[test]
fn a_flat_tone_is_kept_while_the_dots_do_not_crowd() -> Result<(), Error> {
    for dark in [0.05_f32, 0.1, 0.3, 0.5] {
        let out = red(&sparse(10), SIDE, SIDE, &flat(SIDE, SIDE, 1.0 - dark))?;
        // a dot's ink is its area to the rounding of the dot count (half a dot in 196, 0.3%) and the dots that overlap
        assert!((ink(&out) - dark).abs() < 0.02, "darkness {dark}: ink {}", ink(&out));
    }
    Ok(())
}

#[test]
fn the_dot_count_is_the_darkness_over_the_area_of_a_dot() -> Result<(), Error> {
    let out = red(&sparse(10), SIDE, SIDE, &flat(SIDE, SIDE, 1.0 - DARKNESS))?;
    let found = dots(&out, SIDE, SIDE).len() as f64;
    assert!((found - COUNT).abs() < 0.03 * COUNT, "{found} dots, {COUNT} expected");
    Ok(())
}

#[test]
fn relaxed_dots_lie_near_a_hexagonal_lattice_and_unrelaxed_ones_clump() -> Result<(), Error> {
    let values = flat(SIDE, SIDE, 1.0 - DARKNESS);
    // density of dots per px^2; a hexagonal lattice has nearest neighbours 1.0746 / sqrt(density) apart, random points
    // 0.5 / sqrt(density) on average (a Poisson process)
    let root = (COUNT / (SIDE * SIDE) as f64).sqrt().recip();
    let relaxed = mean_nearest(&dots(&red(&sparse(10), SIDE, SIDE, &values)?, SIDE, SIDE));
    let random = mean_nearest(&dots(&red(&sparse(0), SIDE, SIDE, &values)?, SIDE, SIDE));
    assert!(relaxed > 0.85 * root, "relaxed {relaxed}, lattice {}", 1.0746 * root);
    assert!(random < 0.7 * root, "random {random}, Poisson {}", 0.5 * root);
    Ok(())
}

#[test]
fn ink_lies_only_where_the_picture_is_dark() -> Result<(), Error> {
    let (height, width) = (40, 80);
    let mut values = flat(height, width, 1.0);
    for row in 0..height {
        for column in 0..width / 2 {
            values[(row * width + column) * 3..][..3].fill(0.6);
        }
    }
    let out = red(&sparse(10), height, width, &values)?;
    // a dot's centre is in the dark half; its edge reaches radius + half a px beyond it
    for row in 0..height {
        for column in width / 2 + 2..width {
            assert_eq!(out[row * width + column], 1.0, "row {row} column {column}");
        }
    }
    assert!(
        out[..width / 2].iter().any(|&v| v < 0.5),
        "there are dots in the dark half"
    );
    Ok(())
}

#[test]
fn the_seed_gives_the_pattern_and_relaxation_moves_it() -> Result<(), Error> {
    let values = flat(SIDE, SIDE, 1.0 - DARKNESS);
    let run = |iterations, seed| {
        red(
            &Params {
                seed,
                ..sparse(iterations)
            },
            SIDE,
            SIDE,
            &values,
        )
    };
    assert!(run(10, 3)? == run(10, 3)?, "same seed, same dots");
    assert!(run(10, 3)? != run(10, 4)?, "another seed, other dots");
    assert!(run(0, 3)? != run(10, 3)?, "relaxation moves the dots");
    Ok(())
}

#[test]
fn dots_that_overlap_print_once_and_the_darkest_tone_stops_short_of_black() -> Result<(), Error> {
    // random dots overlap, and ink is lost where they do: well under the tone asked
    let half = red(&sparse(0), SIDE, SIDE, &flat(SIDE, SIDE, 0.5))?;
    assert!(ink(&half) < 0.42, "ink {}", ink(&half));
    // relaxed, the dots pack at best hexagonally: pi / sqrt(12) = 0.907 of the area, less for dots of a few px
    let black = red(&sparse(10), SIDE, SIDE, &flat(SIDE, SIDE, 0.0))?;
    assert!((0.7..0.907).contains(&ink(&black)), "ink {}", ink(&black));
    Ok(())
}
