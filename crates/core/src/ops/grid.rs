//! A grid: rectangular pieces in columns and rows of given shares.

use super::length;
use super::pieces::{Bounds, renumber};

/// The positions in px, from `low` to `high`, of the borders between pieces of the relative widths `shares`.
fn borders(shares: &[f64], low: usize, high: usize) -> Vec<f64> {
    let (low, high) = (length::float(low), length::float(high));
    let total: f64 = shares.iter().sum();
    let mut so_far = 0.0; // constant: no share has been taken before the first
    shares
        .iter()
        .take(shares.len().saturating_sub(1))
        .map(|share| {
            so_far += share;
            low + (high - low) * so_far / total
        })
        .collect()
}

/// How many of the sorted `borders` lie at or before each pixel centre of `count` pixels (centre `i + 0.5`): the
/// column, or row, of the pixel. A centre exactly on a border is past it, in the later piece.
fn places(borders: &[f64], count: usize) -> Vec<usize> {
    (0..count)
        .map(|at| {
            let centre = length::float(at) + 0.5;
            borders.partition_point(|border| *border <= centre)
        })
        .collect()
}

/// The pieces of a `height` x `width` picture divided into columns of the relative widths `columns` across the
/// rectangle `frame` and rows of the relative heights `rows` down it, and the number of pieces: a pixel belongs to the
/// column and row whose range holds its centre, and the outer pieces run on to the edges of the picture. Numbered by
/// first pixel, so row after row, among those that have a pixel.
///
/// With `shift` (the shifts of the pixel centres along x and along y, row after row), the centres are moved before they
/// are classified, so the borders between pieces wander.
pub(crate) fn grid(
    height: usize,
    width: usize,
    frame: Bounds,
    columns: &[f64],
    rows: &[f64],
    shift: Option<[&[f32]; 2]>,
) -> (Vec<i32>, usize) {
    let (across, down) = (
        borders(columns, frame.left, frame.right),
        borders(rows, frame.top, frame.bottom),
    );
    let Some([along_x, along_y]) = shift else {
        let (across, down) = (places(&across, width), places(&down, height));
        return renumber(
            columns.len() * rows.len(),
            down.iter()
                .flat_map(|&row| across.iter().map(move |&column| row * columns.len() + column)),
        );
    };
    renumber(
        columns.len() * rows.len(),
        (0..height * width).map(|at| {
            let (row, column) = (at / width, at % width);
            let x = length::float(column) + 0.5 + f64::from(along_x[at]); // constant: a pixel's centre is half a pixel in
            let y = length::float(row) + 0.5 + f64::from(along_y[at]); // constant: a pixel's centre is half a pixel in
            down.partition_point(|border| *border <= y) * columns.len() + across.partition_point(|border| *border <= x)
        }),
    )
}
