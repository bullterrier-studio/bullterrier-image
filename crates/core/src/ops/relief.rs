//! Relief: how fast a height field rises at each pixel, handed to whatever the effect makes of it in the same pass.

use crate::ops::edge::{Extension, Source};
use crate::ops::par;

/// The index a neighbour rule leads to; `Extension` has a value of its own only for a constant, which no relief effect
/// offers (its `edge` is an `Edge`), so the pixel itself stands in for it.
const fn sample(source: Source, itself: usize) -> usize {
    match source {
        Source::Sample(at) => at,
        Source::Value(_) => itself,
    }
}

/// `out[i]` = `shade(i, rise_y, rise_x)` for every pixel `i` of the `rows` x `cols` field `height` (row after row), where
/// `rise_y` and `rise_x` are how fast the surface rises per pixel down the picture and across it: the central difference
/// of the two neighbours, `(next - previous) / 2`, times `scale`, the height of a value of 1 in px. `edge` is what lies
/// beyond the field's edge; it decides the slope in the first and last row and column only. One pass over the field, in
/// row bands on the render's threads: `shade` is called in any order, and its results are the same at any thread count.
/// `height` and `out` hold `rows * cols` values, both at least 1 wide.
pub(crate) fn shade_slopes<T: Send>(
    height: &[f32],
    (rows, cols): (usize, usize),
    scale: f32,
    edge: Extension,
    out: &mut [T],
    shade: impl Fn(usize, f32, f32) -> T + Sync,
) {
    let half = scale / 2.0; // constant: a central difference spans 2 px
    let beside: Vec<(usize, usize)> = (0..cols)
        .map(|column| {
            let (before, after) = (column.cast_signed() - 1, column.cast_signed() + 1);
            (
                sample(edge.locate(cols, before), column),
                sample(edge.locate(cols, after), column),
            )
        })
        .collect();
    par::rows(out, cols, |first_row, band| {
        for (offset, line) in band.chunks_exact_mut(cols).enumerate() {
            let row = first_row + offset;
            let above = sample(edge.locate(rows, row.cast_signed() - 1), row);
            let below = sample(edge.locate(rows, row.cast_signed() + 1), row);
            let this = &height[row * cols..][..cols];
            let (up, down) = (&height[above * cols..][..cols], &height[below * cols..][..cols]);
            for (column, (value, &(left, right))) in line.iter_mut().zip(&beside).enumerate() {
                *value = shade(
                    row * cols + column,
                    half * (down[column] - up[column]),
                    half * (this[right] - this[left]),
                );
            }
        }
    });
}
