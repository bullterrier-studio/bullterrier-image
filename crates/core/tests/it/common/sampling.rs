//! A picture read at a point between its pixels, worked out from the definition of each reading and of each edge rule
//! (`common::edge`). A pixel's centre is at whole numbers (row 0, column 0 is the middle of the first pixel). The
//! reference of every test of an effect that reads a picture again about a centre.

use bullterrier_image::{Edge, Interpolation};

use crate::common::edge::folded;

/// The pixels along one axis a reading at `at` uses and the weight of each, from the definitions: the nearest takes the
/// pixel at `floor(at + 1/2)`; the linear takes the two around it by their nearness; the cubic is Catmull-Rom through the
/// four around it, `t` being how far past the pixel before `at` lies.
#[expect(clippy::cast_possible_truncation, reason = "a test point lies near a small picture")]
fn along(interpolation: Interpolation, at: f64) -> Vec<(i64, f64)> {
    let before = at.floor();
    let t = at - before;
    let before = before as i64;
    match interpolation {
        Interpolation::Nearest => vec![((at + 0.5).floor() as i64, 1.0)],
        Interpolation::Bilinear => vec![(before, 1.0 - t), (before + 1, t)],
        Interpolation::Bicubic => vec![
            (before - 1, 0.5 * (-t * t * t + 2.0 * t * t - t)),
            (before, 1.0 - 2.5 * t * t + 1.5 * t * t * t),
            (before + 1, 0.5 * (-3.0 * t * t * t + 4.0 * t * t + t)),
            (before + 2, 0.5 * (t * t * t - t * t)),
        ],
    }
}

/// The three channels of the `height` x `width` picture `pixels` (row after row, three values a pixel) read at (`row`,
/// `column`) as `interpolation` says, a point beyond the picture read as `rule` says.
pub(crate) fn read_at(
    pixels: &[f32],
    height: usize,
    width: usize,
    interpolation: Interpolation,
    rule: Edge,
    row: f64,
    column: f64,
) -> [f64; 3] {
    read_at_ruled(pixels, height, width, interpolation, (rule, rule), row, column)
}

/// [`read_at`], a point beyond the rows read as `rules.0` says and one beyond the columns as `rules.1` says.
#[expect(clippy::cast_possible_wrap, reason = "a test picture is small")]
pub(crate) fn read_at_ruled(
    pixels: &[f32],
    height: usize,
    width: usize,
    interpolation: Interpolation,
    rules: (Edge, Edge),
    row: f64,
    column: f64,
) -> [f64; 3] {
    let mut sum = [0.0; 3];
    for (down, row_weight) in along(interpolation, row) {
        for (across, column_weight) in along(interpolation, column) {
            let at = (folded(rules.0, down, height as i64) * width + folded(rules.1, across, width as i64)) * 3;
            for (total, value) in sum.iter_mut().zip(&pixels[at..at + 3]) {
                *total += row_weight * column_weight * f64::from(*value);
            }
        }
    }
    sum
}

/// The picture a warp makes, from the definition: pixel (row, column) is the picture read at the place `source` gives for
/// it as (row, column), held to 0..1. `rules` as for [`read_at_ruled`].
#[expect(
    clippy::cast_possible_truncation,
    reason = "the f64 reading narrowed to the f32 picture"
)]
pub(crate) fn warped(
    pixels: &[f32],
    height: usize,
    width: usize,
    interpolation: Interpolation,
    rules: (Edge, Edge),
    source: impl Fn(usize, usize) -> (f64, f64),
) -> Vec<f32> {
    let mut out = Vec::with_capacity(pixels.len());
    for at in 0..height * width {
        let (row, column) = source(at / width, at % width);
        let read = read_at_ruled(pixels, height, width, interpolation, rules, row, column);
        out.extend(read.map(|level| level.clamp(0.0, 1.0) as f32));
    }
    out
}

/// The picture `pixels` moved down by `dy` and right by `dx`, from the definition: pixel (row, column) is the picture read
/// linearly at (row - `dy`, column - `dx`), a place beyond the picture read as `rule` says.
#[expect(clippy::cast_precision_loss, reason = "a test picture is small")]
pub(crate) fn moved(pixels: &[f32], height: usize, width: usize, rule: Edge, dy: f64, dx: f64) -> Vec<f32> {
    warped(
        pixels,
        height,
        width,
        Interpolation::Bilinear,
        (rule, rule),
        |row, column| (row as f64 - dy, column as f64 - dx),
    )
}
