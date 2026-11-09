//! Edges: how fast a plane's value changes at each cell ([`sobel`]), the thin lines along its steepest places ([`crests`]:
//! Canny's non-maximum suppression and hysteresis) and the steepest a step can read ([`sobel_of_line`]).
//!
//! The maps this module makes have a border of one cell of zeros all round, so that a cell's eight neighbours always
//! exist: row `row`, column `column` of a map `width` columns wide is at [`at`].

use super::edge::{Edge, Source};
use super::par;

/// How steeply a cell rises, and along which of four lines.
#[derive(Clone, Copy, Default)]
pub(crate) struct Slope {
    /// The gradient's length times the `scale` the map was made with.
    pub(crate) strength: f32,
    /// The line the gradient lies along, its direction rounded to a multiple of 45 degrees. A gradient and its opposite
    /// are one line.
    line: Line,
}

/// A line through a cell, by the direction of the gradient along it.
#[derive(Clone, Copy, Default)]
enum Line {
    /// Left to right.
    #[default]
    Across,
    /// Down to the right.
    Falling,
    /// Top to bottom.
    Down,
    /// Up to the right.
    Rising,
}

impl Line {
    /// The step from a cell to its neighbour along the line, in a map `stride` cells wide.
    const fn step(self, stride: isize) -> isize {
        match self {
            Self::Across => 1,
            Self::Falling => stride + 1,
            Self::Down => stride,
            Self::Rising => stride - 1,
        }
    }
}

/// Where row `row`, column `column` of a map `width` columns wide is in a map with the border of one cell.
pub(crate) const fn at(width: usize, row: usize, column: usize) -> usize {
    (row + 1) * (width + 2) + column + 1
}

/// A Sobel kernel reads a slope of 1 per px as 8: 2 across the difference and 4 from the 1-2-1 weights along it.
const SOBEL_GAIN: f32 = 8.0; // constant: Sobel and Feldman's kernels

/// The 1-2-1 weights along the edge of a Sobel kernel add up to this.
const SOBEL_SMOOTHING: f32 = 4.0; // constant: 1 + 2 + 1

/// The Sobel gradient of `plane` (`height` x `width`, row after row) in one pass over its rows, as a map with a border:
/// its length times `scale` and its direction. `edge` is what lies beyond the plane. Where a plane rises 0.1 per px and
/// does not end, the strength is 0.1 x `scale`. Rows are independent, so the result is the same at any thread count.
pub(crate) fn sobel(plane: &[f32], (height, width): (usize, usize), edge: Edge, scale: f32) -> Vec<Slope> {
    let stride = width + 2;
    let extension = edge.extension();
    let beyond = |len: usize, index: isize| match extension.locate(len, index) {
        Source::Sample(at) => at,
        Source::Value(_) => 0, // an `Edge` has no value beyond the plane
    };
    let (before, after) = (beyond(width, -1), beyond(width, width.cast_signed()));
    let gain = scale / SOBEL_GAIN;
    // constant: the tangents of 22.5 and 67.5 degrees, where the direction passes from one multiple of 45 degrees to the next
    let (shallow, steep) = (std::f32::consts::SQRT_2 - 1.0, std::f32::consts::SQRT_2 + 1.0);
    let mut slopes = vec![Slope::default(); (height + 2) * stride];
    par::rows(&mut slopes[stride..stride * (height + 1)], stride, |first, band| {
        for (offset, out) in band.chunks_exact_mut(stride).enumerate() {
            let row = first + offset;
            let line = |index: usize| &plane[index * width..][..width];
            let above = line(beyond(height, row.cast_signed() - 1));
            let here = line(row);
            let below = line(beyond(height, (row + 1).cast_signed()));
            for column in 0..width {
                let left = if column == 0 { before } else { column - 1 };
                let right = if column + 1 == width { after } else { column + 1 };
                // constant: the 1-2-1 weights of the Sobel kernels
                let across =
                    (above[right] - above[left]) + 2.0 * (here[right] - here[left]) + (below[right] - below[left]);
                let down = (below[left] + 2.0 * below[column] + below[right])
                    - (above[left] + 2.0 * above[column] + above[right]); // constant: the same weights
                let (long, tall) = (across.abs(), down.abs());
                let line = if tall <= shallow * long {
                    Line::Across
                } else if tall >= steep * long {
                    Line::Down
                } else if (across > 0.0) == (down > 0.0) {
                    Line::Falling
                } else {
                    Line::Rising
                };
                out[column + 1] = Slope {
                    strength: (across * across + down * down).sqrt() * gain,
                    line,
                };
            }
        }
    });
    slopes
}

/// What the thinning and the hysteresis make of a cell.
#[derive(Clone, Copy, PartialEq)]
enum Crest {
    /// Not a crest.
    No,
    /// A crest stronger than the lower threshold, not known to join a strong one.
    Weak,
    /// A crest stronger than the upper threshold, or one that joins such a crest.
    Kept,
}

/// The thin lines of `slopes` (from [`sobel`] of a `height` x `width` plane): Canny's non-maximum suppression, then
/// hysteresis. A cell is a crest if it is stronger than `lower` and no neighbour on either side of it along its gradient
/// is stronger (of two equal neighbours one stays); a crest stronger than `upper` stays, and so does every crest that
/// touches, diagonals too, a crest that stays. The result is `true` where a cell stays, in a map with a border.
pub(crate) fn crests(slopes: &[Slope], (height, width): (usize, usize), lower: f32, upper: f32) -> Vec<bool> {
    let stride = (width + 2).cast_signed();
    let lines = [Line::Across, Line::Falling, Line::Down, Line::Rising].map(|line| line.step(stride));
    let mut states = vec![Crest::No; slopes.len()];
    par::rows(
        &mut states[stride.cast_unsigned()..][..(height * (width + 2))],
        width + 2,
        |first, band| {
            for (offset, out) in band.chunks_exact_mut(width + 2).enumerate() {
                let start = (first + offset + 1) * (width + 2);
                for (column, flag) in out.iter_mut().enumerate().skip(1).take(width) {
                    let cell = start + column;
                    let Slope { strength, line } = slopes[cell];
                    let step = line.step(stride);
                    let (next, previous) = (
                        slopes[cell.wrapping_add_signed(step)].strength,
                        slopes[cell.wrapping_add_signed(-step)].strength,
                    );
                    if strength > lower && strength >= next && strength > previous {
                        *flag = if strength > upper { Crest::Kept } else { Crest::Weak };
                    }
                }
            }
        },
    );
    // a stack of cells to spread from, over the flat map: the border is never a crest, so no neighbour is out of it
    let mut stack: Vec<usize> = (0..states.len()).filter(|&cell| states[cell] == Crest::Kept).collect();
    while let Some(cell) = stack.pop() {
        for step in lines.into_iter().flat_map(|step| [step, -step]) {
            let neighbour = cell.wrapping_add_signed(step);
            if states[neighbour] == Crest::Weak {
                states[neighbour] = Crest::Kept;
                stack.push(neighbour);
            }
        }
    }
    states.into_iter().map(|state| state == Crest::Kept).collect()
}

/// The strongest Sobel gradient (per unit of `scale` 1) in a plane whose every row is `line`, such as a blurred step: the
/// gradient of identical rows is the difference across, weighted by the sum of the 1-2-1 weights.
pub(crate) fn sobel_of_line(line: &[f32]) -> f32 {
    line.iter()
        .zip(line.iter().skip(2))
        .map(|(before, after)| (after - before) * SOBEL_SMOOTHING / SOBEL_GAIN)
        .fold(0.0, f32::max)
}
