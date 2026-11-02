//! The alpha of a cut, worked by hand for small pictures: the signed distance from each pixel centre to the shape's edge,
//! turned into a coverage by a ramp one pixel wide. (`cutout.rs` checks larger pictures against a brute-force reference.)

use bullterrier_image::frame::Transparent;
use bullterrier_image::{Error, Frame, Options, Photo, Picture, Presets, Recipe, Step};

/// The alpha of a `height` x `width` picture cut by `transparent` with corners of `corner_radius` (a share of the shorter
/// side), row after row.
fn alpha(height: usize, width: usize, transparent: Transparent, corner_radius: f64) -> Result<Vec<f32>, Error> {
    let values = vec![0.5; height * width * 3];
    let mut recipe = Recipe::new([Step::new("grade").opacity(0.0)]);
    recipe.frame = Frame {
        transparent,
        corner_radius,
        ..Frame::default()
    };
    let image =
        Photo::new(Picture::new(height, width, &values)?).render(&recipe, &Presets::new(), Options::default())?;
    Ok(image
        .into_values()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|pixel| pixel[3])
        .collect())
}

/// `actual` is `expected` to within what an f32 distance loses.
fn assert_alpha(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len());
    for (index, (a, e)) in actual.iter().zip(expected).enumerate() {
        assert!((a - e).abs() < 1e-6, "pixel {index}: {a} is not {e}");
    }
}

#[test]
fn a_circle_of_a_6_by_6_picture_is_worked_by_hand() -> Result<(), Error> {
    // radius 3; a pixel centre lies (dy, dx) = odd halves from the middle, and alpha = 3.5 - hypot(dy, dx) within 0..1.
    // Row 0 (dy = -2.5): dx = 2.5 -> hypot 3.5355 -> 0 (clear); dx = 1.5 -> 2.9155 -> 0.5845; dx = 0.5 -> 2.5495 -> 0.9505
    // clipped to 0.9505. Row 2 (dy = -0.5): dx = 2.5 -> 2.5495 -> 0.9505; dx = 1.5 -> 1.5811 -> 1; dx = 0.5 -> 1.
    let made = alpha(6, 6, Transparent::Circle, 0.2)?;
    let (a, b, c) = (0.0, 3.5 - 2.915_476_f32, 3.5 - 2.549_509_8_f32);
    assert_alpha(&made[..6], &[a, b, c, c, b, a]);
    assert_alpha(&made[12..18], &[c, 1.0, 1.0, 1.0, 1.0, c]);
    Ok(())
}

#[test]
fn a_circle_of_a_wide_picture_is_as_wide_as_the_shorter_side_is_tall() -> Result<(), Error> {
    // 2 x 6: radius 1. The middle pair of columns (dy, dx = +-0.5: hypot 0.7071) has alpha 1.5 - 0.7071; the next pair
    // (dx = 1.5: hypot 1.5811) is clear.
    let middle = 1.5 - std::f32::consts::FRAC_1_SQRT_2;
    let row = [0.0, 0.0, middle, middle, 0.0, 0.0];
    let made = alpha(2, 6, Transparent::Circle, 0.2)?;
    assert_alpha(&made[..6], &row);
    assert_alpha(&made[6..], &row);
    Ok(())
}

#[test]
fn rounded_corners_of_a_4_by_8_picture_with_a_radius_of_one_are_worked_by_hand() -> Result<(), Error> {
    // a radius of one pixel is a quarter of the shorter side, 4. Rows: dy = -1.5, -0.5, 0.5, 1.5 against the corners'
    // centres at 1; columns dx = +-3.5 .. +-0.5 against 3. The corner pixel (dy = -1.5, dx = -3.5) lies (0.5, 0.5) beyond
    // the corner's centre: 0.7071 - 1 = -0.2929 -> alpha 1.5 - 0.7071. The pixels next to it along the top are 0.5 beyond
    // in rows only: 0.5 - 1 -> whole, as is row 1.
    let made = alpha(4, 8, Transparent::Rounded, 0.25)?;
    let corner = 1.5 - std::f32::consts::FRAC_1_SQRT_2;
    assert_alpha(&made[..3], &[corner, 1.0, 1.0]);
    assert_alpha(&made[5..8], &[1.0, 1.0, corner]);
    assert_alpha(&made[24..27], &[corner, 1.0, 1.0]);
    assert_alpha(&made[8..11], &[1.0, 1.0, 1.0]);
    Ok(())
}

#[test]
fn a_corner_rounded_by_more_than_a_pixel_and_a_half_is_clear() -> Result<(), Error> {
    // 8 x 8 at a radius of 4 (half the side) is the circle; its corner pixel centre is 4.95 from the middle: 4 - 4.95 +
    // 0.5 < 0
    let made = alpha(8, 8, Transparent::Rounded, 0.5)?;
    assert_alpha(&made[..1], &[0.0]);
    assert_alpha(&alpha(8, 8, Transparent::Circle, 0.2)?, &made);
    Ok(())
}
