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
#[expect(clippy::cast_possible_wrap, reason = "a test picture is small")]
pub(crate) fn read_at(
    pixels: &[f32],
    height: usize,
    width: usize,
    interpolation: Interpolation,
    rule: Edge,
    row: f64,
    column: f64,
) -> [f64; 3] {
    let mut sum = [0.0; 3];
    for (down, row_weight) in along(interpolation, row) {
        for (across, column_weight) in along(interpolation, column) {
            let at = (folded(rule, down, height as i64) * width + folded(rule, across, width as i64)) * 3;
            for (total, value) in sum.iter_mut().zip(&pixels[at..at + 3]) {
                *total += row_weight * column_weight * f64::from(*value);
            }
        }
    }
    sum
}
