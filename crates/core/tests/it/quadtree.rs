//! `quadtree`: flat cells that split where the picture varies. The oracles are a recursive reference in the test that
//! cuts, splits and averages by the stated rule with direct sums, rounded-box pixels worked by hand, and properties: the
//! cells tile the picture (the sum is kept), a lower threshold only refines the cells (the squared error never grows).

use bullterrier_image::Error;
use bullterrier_image::effects::quadtree::Params;
use bullterrier_image::params::Color;

use crate::common::compare::assert_close;
use crate::common::pictures::{step, varied};
use crate::common::pixels::run_picture;

/// Cells with no line and square corners.
fn plain(threshold: f32, max_cell: f32, min_cell: f32) -> Params {
    Params {
        threshold,
        max_cell,
        min_cell,
        gap: 0.0,
        rounding: 0.0,
        ..Params::default()
    }
}

/// The rows `top..bottom` and columns `left..right` of `picture` (`width` wide) averaged and spread by direct sums.
fn stats(picture: &[f32], width: usize, (top, bottom, left, right): (usize, usize, usize, usize)) -> ([f64; 3], f64) {
    let mut sums = [0.0_f64; 3];
    let mut squares = [0.0_f64; 3];
    for row in top..bottom {
        for column in left..right {
            for channel in 0..3 {
                let value = f64::from(picture[(row * width + column) * 3 + channel]);
                sums[channel] += value;
                squares[channel] += value * value;
            }
        }
    }
    #[expect(clippy::cast_precision_loss, reason = "a small area")]
    let area = ((bottom - top) * (right - left)) as f64;
    let mean = sums.map(|sum| sum / area);
    let spread = (0..3)
        .map(|channel| (squares[channel] / area - mean[channel] * mean[channel]).max(0.0))
        .fold(0.0, f64::max)
        .sqrt();
    (mean, spread)
}

/// Where `count` blocks cut `length` px: the stated rule `index x length / count`, rounded down.
fn cut(length: usize, count: usize, index: usize) -> usize {
    index * length / count
}

/// The cells of the stated rule, painted with their means into `out`: block `index` of `count` rows by block `across` of
/// `columns`, split when its spread is above `threshold` and both halves, in each direction, are `min_cell` px or more.
fn paint(
    picture: &[f32],
    (height, width): (usize, usize),
    (count, columns): (usize, usize),
    (index, across): (usize, usize),
    (threshold, min_cell): (f64, usize),
    out: &mut [f32],
) {
    let rows = (cut(height, count, index), cut(height, count, index + 1));
    let cols = (cut(width, columns, across), cut(width, columns, across + 1));
    let (mean, spread) = stats(picture, width, (rows.0, rows.1, cols.0, cols.1));
    let halves = |length: usize, n: usize, i: usize| {
        let (a, b, c) = (
            cut(length, 2 * n, 2 * i),
            cut(length, 2 * n, 2 * i + 1),
            cut(length, 2 * n, 2 * i + 2),
        );
        b - a >= min_cell && c - b >= min_cell
    };
    if spread > threshold && halves(height, count, index) && halves(width, columns, across) {
        for (down, right) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
            paint(
                picture,
                (height, width),
                (2 * count, 2 * columns),
                (2 * index + down, 2 * across + right),
                (threshold, min_cell),
                out,
            );
        }
    } else {
        for row in rows.0..rows.1 {
            for column in cols.0..cols.1 {
                for channel in 0..3 {
                    #[expect(clippy::cast_possible_truncation, reason = "a mean of f32 values")]
                    let value = mean[channel] as f32;
                    out[(row * width + column) * 3 + channel] = value;
                }
            }
        }
    }
}

/// `picture` cut into cells by the reference.
fn reference(
    picture: &[f32],
    (height, width): (usize, usize),
    threshold: f64,
    (max_cell, min_cell): (usize, usize),
) -> Vec<f32> {
    let (down, across) = (height.div_ceil(max_cell), width.div_ceil(max_cell));
    let mut out = vec![0.0; picture.len()];
    for row in 0..down {
        for column in 0..across {
            paint(
                picture,
                (height, width),
                (down, across),
                (row, column),
                (threshold, min_cell),
                &mut out,
            );
        }
    }
    out
}

#[test]
fn cells_split_by_the_stated_rule_and_are_filled_with_their_mean() -> Result<(), Error> {
    // sizes that do not halve evenly, a first grid of one or several cells, thresholds from "split everything uneven" up
    for ((height, width), (max_cell, min_cell), threshold) in [
        ((32, 32), (32, 8), 0.0),
        ((30, 50), (60, 5), 0.0),
        ((30, 50), (20, 3), 0.05),
        ((37, 23), (16, 2), 0.1),
        ((17, 19), (4, 1), 0.02),
        ((9, 40), (40, 4), 0.2),
    ] {
        let picture = varied(height, width);
        #[expect(clippy::cast_precision_loss, reason = "small whole numbers")]
        let params = plain(threshold, max_cell as f32, min_cell as f32);
        assert_close(
            &run_picture(&params, height, width, &picture)?,
            &reference(&picture, (height, width), f64::from(threshold), (max_cell, min_cell)),
            1e-5,
            &format!("{height}x{width}, cells {min_cell}..{max_cell}, threshold {threshold}"),
        );
    }
    Ok(())
}

#[test]
fn an_edge_on_a_cell_boundary_gives_cells_that_are_flat_and_the_picture_back() -> Result<(), Error> {
    // black left half, white right half: the grid's one cell splits once into quarters, each flat
    let picture = step(32, 32, 0.0, 1.0);
    let out = run_picture(&plain(0.06, 32.0, 4.0), 32, 32, &picture)?;
    assert_close(&out, &picture, 1e-6, "split once at the edge");
    Ok(())
}

#[test]
fn a_flat_picture_is_one_cell_per_grid_square_and_stays_as_it_is() -> Result<(), Error> {
    let picture = vec![0.7_f32; 20 * 30 * 3];
    assert_close(
        &run_picture(&plain(0.0, 8.0, 1.0), 20, 30, &picture)?,
        &picture,
        1e-6,
        "flat",
    );
    Ok(())
}

#[test]
fn cells_tile_the_picture_so_the_total_of_the_values_is_kept() -> Result<(), Error> {
    for threshold in [0.0, 0.03, 0.2, 0.5] {
        let picture = varied(29, 41);
        let out = run_picture(&plain(threshold, 16.0, 2.0), 29, 41, &picture)?;
        let total = |values: &[f32]| values.iter().map(|&value| f64::from(value)).sum::<f64>();
        assert!((total(&out) - total(&picture)).abs() < 1e-2, "threshold {threshold}");
    }
    Ok(())
}

#[test]
fn a_lower_threshold_only_refines_the_cells_so_the_squared_error_never_grows() -> Result<(), Error> {
    let picture = varied(40, 40);
    let mut last = f64::INFINITY;
    for threshold in [0.5, 0.3, 0.2, 0.1, 0.05, 0.0] {
        let out = run_picture(&plain(threshold, 40.0, 2.0), 40, 40, &picture)?;
        let error: f64 = out.iter().zip(&picture).map(|(a, b)| f64::from(a - b).powi(2)).sum();
        assert!(error <= last + 1e-3, "threshold {threshold}: {error} after {last}");
        last = error;
    }
    Ok(())
}

#[test]
fn the_gap_is_a_line_of_that_width_between_cells_and_none_at_the_picture_edge() -> Result<(), Error> {
    // a white 8 x 8 picture in four cells of 4: a gap of 2 takes the pixel column on each side of the boundary
    let white = vec![1.0_f32; 8 * 8 * 3];
    let params = Params {
        gap: 2.0,
        gap_color: Color([0.0; 3]),
        ..plain(0.5, 4.0, 4.0)
    };
    let out = run_picture(&params, 8, 8, &white)?;
    for row in 0..8 {
        for column in 0..8 {
            let line = matches!(row, 3 | 4) || matches!(column, 3 | 4);
            assert_eq!(
                out[(row * 8 + column) * 3],
                if line { 0.0 } else { 1.0 },
                "({row}, {column})"
            );
        }
    }
    // a gap of 1 covers half of the pixel on each side
    let out = run_picture(&Params { gap: 1.0, ..params }, 8, 8, &white)?;
    assert_close(
        &[out[(8 + 1) * 3], out[(8 + 3) * 3], out[(8 + 4) * 3]],
        &[1.0, 0.5, 0.5],
        1e-6,
        "half pixels",
    );
    Ok(())
}

#[test]
fn rounding_one_draws_a_circle_in_a_square_cell_and_gives_the_corners_to_the_gap_colour() -> Result<(), Error> {
    let white = vec![1.0_f32; 8 * 8 * 3];
    let params = Params {
        rounding: 1.0,
        gap_color: Color([0.0; 3]),
        ..plain(0.5, 8.0, 8.0)
    };
    let out = run_picture(&params, 8, 8, &white)?;
    let at = |row: usize, column: usize| out[(row * 8 + column) * 3];
    // the corner pixel's centre is 4.95 from the middle, beyond the radius of 4: all gap colour
    assert_close(&[at(0, 0), at(0, 7), at(7, 0), at(7, 7)], &[0.0; 4], 1e-6, "corners");
    // the middle is the cell's; so is a pixel next to the edge on an axis (0.965 covered)
    assert_close(
        &[at(3, 3), at(4, 4), at(3, 0)],
        &[1.0, 1.0, 0.965_f32],
        1e-3,
        "middle and axis",
    );
    // the circle is symmetric
    assert_eq!(at(1, 0), at(0, 1));
    assert_eq!(at(1, 6), at(6, 1));
    Ok(())
}

#[test]
fn a_cell_splits_only_when_its_spread_is_above_the_threshold() -> Result<(), Error> {
    // half 0 and half 0.1: the standard deviation of the whole cell is 0.05; its halves are flat
    let picture = step(32, 32, 0.0, 0.1);
    assert_close(
        &run_picture(&plain(0.045, 32.0, 4.0), 32, 32, &picture)?,
        &picture,
        1e-6,
        "below the spread: split",
    );
    let flat = run_picture(&plain(0.055, 32.0, 4.0), 32, 32, &picture)?;
    assert_close(&flat, &vec![0.05; flat.len()], 1e-6, "above the spread: one cell");
    Ok(())
}
