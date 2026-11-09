//! Quadtree cells: a picture cut into rectangles that stay large where it is even and halve where it varies.
//!
//! Coordinates are pixel indices; a leaf `(top, bottom, left, right)` holds the rows `top..bottom` and the columns
//! `left..right`.

use crate::ops::cluster::Point;
use crate::ops::par;

/// One cell the picture was cut into.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Leaf {
    pub top: usize,
    pub bottom: usize,
    pub left: usize,
    pub right: usize,
    /// The average colour of the picture inside the cell.
    pub mean: Point,
}

/// Where `count` blocks of nearly equal size cut `length` px: the whole px at or below `index` x `length` / `count`.
/// Exact integer arithmetic, so every edge of a coarse level is also an edge of the level that halves it.
const fn edge(length: usize, count: usize, index: usize) -> usize {
    index * length / count
}

/// Whether both halves of block `index` of the `count` blocks cutting `length` px (the blocks of the level that halves
/// them) are at least `smallest` px.
const fn halves_fit(length: usize, count: usize, index: usize, smallest: usize) -> bool {
    let (from, middle, to) = (
        edge(length, 2 * count, 2 * index),
        edge(length, 2 * count, 2 * index + 1),
        edge(length, 2 * count, 2 * index + 2),
    );
    middle - from >= smallest && to - middle >= smallest
}

/// The picture's average colour per channel, in f64.
#[expect(clippy::cast_precision_loss, reason = "a pixel count is far below 2^53")]
fn average(pixels: &[Point]) -> [f64; 3] {
    let mut sum = [0.0_f64; 3];
    for pixel in pixels {
        for (total, &value) in sum.iter_mut().zip(pixel) {
            *total += f64::from(value);
        }
    }
    sum.map(|total| total / pixels.len() as f64)
}

/// The cells under one row of the first grid (`row` of `down`), split as far as `threshold` and `min_cell` say.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    reason = "a pixel count is far below 2^53, and a mean is kept in the f32 of the picture"
)]
fn row_leaves(
    pixels: &[Point],
    (height, width): (usize, usize),
    centre: &[f64; 3],
    (down, across): (usize, usize),
    row: usize,
    (threshold, min_cell): (f64, usize),
) -> Vec<Leaf> {
    let mut leaves = Vec::new();
    // cells still to look at: (level, row and column in that level's grid), where level L cuts 2^L times as finely
    let mut waiting: Vec<(u32, usize, usize)> = (0..across).rev().map(|column| (0, row, column)).collect();
    while let Some((level, i, j)) = waiting.pop() {
        let (n, m) = (down << level, across << level);
        let (top, bottom) = (edge(height, n, i), edge(height, n, i + 1));
        let (left, right) = (edge(width, m, j), edge(width, m, j + 1));
        // The spread comes from sums of squares of the values minus the picture's average, which keeps them near zero:
        // a flat 0.7 would otherwise read as uneven.
        let (mut sum, mut squares) = ([0.0_f64; 3], [0.0_f64; 3]);
        for line in top..bottom {
            for pixel in &pixels[line * width + left..line * width + right] {
                for channel in 0..3 {
                    let difference = f64::from(pixel[channel]) - centre[channel];
                    sum[channel] += difference;
                    squares[channel] += difference * difference;
                }
            }
        }
        let area = ((bottom - top) * (right - left)) as f64;
        let mean = sum.map(|total| total / area);
        let spread = (0..3)
            .map(|channel| squares[channel] / area - mean[channel] * mean[channel])
            .fold(0.0_f64, f64::max)
            .sqrt();
        if spread > threshold && halves_fit(height, n, i, min_cell) && halves_fit(width, m, j, min_cell) {
            for (a, b) in [(1, 1), (1, 0), (0, 1), (0, 0)] {
                waiting.push((level + 1, 2 * i + a, 2 * j + b));
            }
        } else {
            leaves.push(Leaf {
                top,
                bottom,
                left,
                right,
                mean: [0, 1, 2].map(|channel| (mean[channel] + centre[channel]) as f32),
            });
        }
    }
    leaves
}

/// Cut `pixels` (a `height` x `width` picture, row after row, 0..1) into leaves that follow its detail, each row of the
/// first grid after the one above.
///
/// The first cut is a grid of cells at most `max_cell` px on a side (the picture split evenly, so sides differ by a pixel
/// at most). A cell splits into four halves when the spread of colour inside it, the largest of the three channels'
/// standard deviations, is above `threshold`, and its halves are still at least `min_cell` px on a side; the halves are
/// tested the same way. `max_cell` and `min_cell` are whole px, 1 or more. A picture smaller than a cell is one leaf. The
/// leaves tile the picture.
pub(crate) fn leaves(
    pixels: &[Point],
    (height, width): (usize, usize),
    threshold: f32,
    (max_cell, min_cell): (usize, usize),
) -> Vec<Leaf> {
    let centre = average(pixels);
    let grid = (height.div_ceil(max_cell), width.div_ceil(max_cell));
    let mut rows = vec![Vec::new(); grid.0];
    par::rows(&mut rows, 1, |first, band| {
        for (offset, slot) in band.iter_mut().enumerate() {
            *slot = row_leaves(
                pixels,
                (height, width),
                &centre,
                grid,
                first + offset,
                (f64::from(threshold), min_cell),
            );
        }
    });
    rows.concat()
}
