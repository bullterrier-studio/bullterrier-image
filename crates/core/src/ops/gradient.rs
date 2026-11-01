//! Gradients: a value that rises evenly across a picture, taken at each pixel's centre.
//!
//! Computed in `f64` and narrowed once, so a pixel's value is the nearest `f32` to the true one and the same gradient
//! at twice the size has the same values at the shared positions.

use super::cast::single;
use super::length;

/// The pixel centres of a `height` x `width` picture, row after row, as (x, y) in px: pixel `(row, column)` is centred
/// at `(column + 0.5, row + 0.5)`.
fn centres(height: usize, width: usize) -> impl Iterator<Item = (f64, f64)> {
    (0..height)
        .flat_map(move |row| (0..width).map(move |column| (length::float(column) + 0.5, length::float(row) + 0.5)))
}

/// The linear gradient of a `height` x `width` picture rising in the direction `degrees` (clockwise from the right, so
/// 0 rises to the right and 90 downward), row after row: each pixel's position along that direction as a share of the
/// picture's extent that way, `width |cos| + height |sin|`, measured from the centre of the picture, so the two corners
/// furthest along the direction are 0 and 1 at any aspect ratio. Any finite angle is the direction a whole number of
/// turns away.
pub(crate) fn linear(height: usize, width: usize, degrees: f64) -> Vec<f32> {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let (high, wide) = (length::float(height), length::float(width));
    let extent = wide * cos.abs() + high * sin.abs();
    centres(height, width)
        .map(|(x, y)| {
            let along = ((x - wide / 2.0) * cos + (y - high / 2.0) * sin) / extent + 0.5;
            single(along.clamp(0.0, 1.0))
        })
        .collect()
}

/// The radial gradient of a `height` x `width` picture, row after row: a pixel's distance from the point (`x` of the
/// width, `y` of the height) as a share of `radius` times the shorter side, from 0 at the point to 1 at that distance
/// and beyond.
pub(crate) fn radial(height: usize, width: usize, x: f64, y: f64, radius: f64) -> Vec<f32> {
    let (high, wide) = (length::float(height), length::float(width));
    let reach = radius * high.min(wide);
    centres(height, width)
        .map(|(across, down)| single(((across - x * wide).hypot(down - y * high) / reach).clamp(0.0, 1.0)))
        .collect()
}
