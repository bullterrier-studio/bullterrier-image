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

#[cfg(test)]
mod tests {
    use super::{circle, rounded};

    /// `actual` is `expected` to within what an f32 distance loses.
    fn assert_alpha(actual: &[f32], expected: &[f32]) {
        assert_eq!(actual.len(), expected.len());
        for (index, (a, e)) in actual.iter().zip(expected).enumerate() {
            assert!((a - e).abs() < 1e-6, "pixel {index}: {a} is not {e}");
        }
    }

    #[test]
    fn a_circle_of_a_6_by_6_picture_is_worked_by_hand() {
        // radius 3; a pixel centre lies (dy, dx) = odd halves from the middle, and alpha = 3.5 - hypot(dy, dx) within 0..1.
        // Row 0 (dy = -2.5): dx = 2.5 -> hypot 3.5355 -> 0 (clear); dx = 1.5 -> 2.9155 -> 0.5845; dx = 0.5 -> 2.5495 -> 0.9505
        // clipped to 0.9505. Row 2 (dy = -0.5): dx = 2.5 -> 2.5495 -> 0.9505; dx = 1.5 -> 1.5811 -> 1; dx = 0.5 -> 1.
        let alpha = circle(6, 6);
        let (a, b, c) = (0.0, 3.5 - 2.915_476_f32, 3.5 - 2.549_509_8_f32);
        let row0 = [a, b, c, c, b, a];
        let row2 = [c, 1.0, 1.0, 1.0, 1.0, c];
        assert_alpha(&alpha[..6], &row0);
        assert_alpha(&alpha[12..18], &row2);
    }

    #[test]
    fn a_circle_of_a_wide_picture_is_as_wide_as_the_shorter_side_is_tall() {
        // 2 x 6: radius 1. The middle pair of columns (dy, dx = +-0.5: hypot 0.7071) has alpha 1.5 - 0.7071; the next
        // pair (dx = 1.5: hypot 1.5811) is clear.
        let middle = 1.5 - std::f32::consts::FRAC_1_SQRT_2;
        let row = [0.0, 0.0, middle, middle, 0.0, 0.0];
        let alpha = circle(2, 6);
        assert_alpha(&alpha[..6], &row);
        assert_alpha(&alpha[6..], &row);
    }

    #[test]
    fn rounded_corners_of_a_4_by_8_picture_with_a_radius_of_one_are_worked_by_hand() {
        // rows: dy = -1.5, -0.5, 0.5, 1.5 against the corners' centres at 1; columns dx = +-3.5 .. +-0.5 against 3. The
        // corner pixel (dy = -1.5, dx = -3.5) lies (0.5, 0.5) beyond the corner's centre: 0.7071 - 1 = -0.2929 -> alpha
        // 1.5 - 0.7071. The pixels next to it along the top are 0.5 beyond in rows only: 0.5 - 1 -> whole, as is row 1.
        let alpha = rounded(4, 8, 1.0);
        let corner = 1.5 - std::f32::consts::FRAC_1_SQRT_2;
        assert_alpha(&alpha[..3], &[corner, 1.0, 1.0]);
        assert_alpha(&alpha[5..8], &[1.0, 1.0, corner]);
        assert_alpha(&alpha[24..27], &[corner, 1.0, 1.0]);
        assert_alpha(&alpha[8..11], &[1.0, 1.0, 1.0]);
    }

    #[test]
    fn a_corner_rounded_by_more_than_a_pixel_and_a_half_is_clear() {
        // 8 x 8 at a radius of 4 is the circle; its corner pixel centre is 4.95 from the middle: 4 - 4.95 + 0.5 < 0
        let alpha = rounded(8, 8, 4.0);
        assert_alpha(&alpha[..1], &[0.0]);
        assert_alpha(&circle(8, 8), &alpha);
    }
}
