//! The alpha of a picture cut to a shape: 1 inside, 0 outside, a soft edge one pixel wide.
//!
//! Each pixel's alpha comes from the signed distance (negative inside) from its centre to the shape's edge: a pixel whose
//! centre is half a pixel or more inside is whole, one half a pixel or more outside is clear, and between them alpha
//! falls linearly, so the edge of a cut is anti-aliased without sampling. The shapes are defined by the picture's own
//! size, so the same cut fits at any resolution.

use super::length::float;

/// A pixel's centre is half a pixel from its edge, so a distance of half a pixel across the edge is the whole ramp.
const HALF_PIXEL: f64 = 0.5; // constant: the soft edge of a cut is one pixel wide, centred on the shape's edge

/// The alpha of a `height` x `width` picture, row after row, where `signed_distance` gives the distance from the edge
/// of the shape of a pixel whose centre is `dy` rows and `dx` columns from the picture's middle.
fn alpha(height: usize, width: usize, signed_distance: impl Fn(f32, f32) -> f32) -> Vec<f32> {
    let half = single(HALF_PIXEL);
    let columns: Vec<f32> = (0..width).map(|column| from_middle(column, width)).collect();
    let mut alpha = Vec::with_capacity(height * width);
    for row in 0..height {
        let dy = from_middle(row, height);
        alpha.extend(
            columns
                .iter()
                .map(|&dx| (half - signed_distance(dy, dx)).clamp(0.0, 1.0)),
        );
    }
    alpha
}

/// A length in px as the f32 the distances are worked in: far below f32's 2^24, so nothing is lost.
#[expect(clippy::cast_possible_truncation, reason = "a length in px, far below f32's 2^24")]
const fn single(length: f64) -> f32 {
    length as f32
}

/// How far the centre of pixel `index` of `length` is from the middle of the picture, in px (negative before it).
fn from_middle(index: usize, length: usize) -> f32 {
    single(float(index) + HALF_PIXEL - float(length) / 2.0)
}

/// The alpha of the circle inscribed in a `height` x `width` picture: its diameter is the shorter side.
pub(crate) fn circle(height: usize, width: usize) -> Vec<f32> {
    let radius = single(float(height.min(width)) / 2.0);
    alpha(height, width, |dy, dx| dy.hypot(dx) - radius)
}

/// The alpha of a `height` x `width` picture with its corners rounded by `radius` px (at most half the shorter side).
pub(crate) fn rounded(height: usize, width: usize, radius: f64) -> Vec<f32> {
    // the signed distance of a rounded box (Inigo Quilez): the distance beyond the corner's centre, or how far inside the
    // straight sides, less the radius. `reach` is how far from the middle the corners' centres lie.
    let reach = |length: usize| single(float(length) / 2.0 - radius);
    let (reach_y, reach_x) = (reach(height), reach(width));
    let radius = single(radius);
    alpha(height, width, |dy, dx| {
        let (beyond_y, beyond_x) = (dy.abs() - reach_y, dx.abs() - reach_x);
        beyond_y.max(0.0).hypot(beyond_x.max(0.0)) + beyond_y.max(beyond_x).min(0.0) - radius
    })
}
