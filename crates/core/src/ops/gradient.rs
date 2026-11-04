//! Gradients: a value that rises evenly across a picture, taken at each pixel's centre.
//!
//! Computed in `f64` and narrowed once, so a pixel's value is the nearest `f32` to the true one and the same gradient
//! at twice the size has the same values at the shared positions.

use std::f64::consts::TAU;

use super::cast::single;
use super::length;
use super::par;

/// A `height` x `width` map, row after row, whose value at the pixel centre `(column + 0.5, row + 0.5)` (in px) is
/// `at(x, y)`; filled by bands of rows on the render's threads.
fn centres(height: usize, width: usize, at: impl Fn(f64, f64) -> f64 + Sync) -> Vec<f32> {
    let mut map = vec![0.0; height * width];
    par::rows(&mut map, width, |first_row, band| {
        for (index, value) in band.iter_mut().enumerate() {
            let (row, column) = (first_row + index / width, index % width);
            *value = single(at(length::float(column) + 0.5, length::float(row) + 0.5));
        }
    });
    map
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
    centres(height, width, |x, y| {
        (((x - wide / 2.0) * cos + (y - high / 2.0) * sin) / extent + 0.5).clamp(0.0, 1.0)
    })
}

/// The radial gradient of a `height` x `width` picture, row after row: a pixel's distance from the point (`x` of the
/// width, `y` of the height) as a share of `radius` times the shorter side, from 0 at the point to 1 at that distance
/// and beyond.
pub(crate) fn radial(height: usize, width: usize, x: f64, y: f64, radius: f64) -> Vec<f32> {
    let (high, wide) = (length::float(height), length::float(width));
    let reach = radius * high.min(wide);
    centres(height, width, |across, down| {
        ((across - x * wide).hypot(down - y * high) / reach).clamp(0.0, 1.0)
    })
}

/// The conic gradient of a `height` x `width` picture, row after row: the angle of a pixel round the point (`x` of the
/// width, `y` of the height), clockwise on screen from the direction `degrees` (as for [`linear`], 0 is to the right,
/// 90 downward), as a share of the full turn, from 0 up to 1 (the seam). The point itself is 0.
pub(crate) fn conic(height: usize, width: usize, x: f64, y: f64, degrees: f64) -> Vec<f32> {
    let (high, wide) = (length::float(height), length::float(width));
    let start = degrees.to_radians();
    centres(height, width, |across, down| {
        (((down - y * high).atan2(across - x * wide) - start) / TAU).rem_euclid(1.0)
    })
}
