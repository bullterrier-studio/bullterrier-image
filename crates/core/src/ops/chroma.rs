//! The colour of a picture smeared along its rows, as composite video smears it.

use super::blur::gaussian_filter_rows;
use super::colorspace::rgb_to_yiq;
use super::edge::Extension;
use super::par;
use super::shift::slide_row;

/// The I and Q colour axes (NTSC YIQ, see [`rgb_to_yiq`]) of the `height` x `width` `pixels` (both at least 1), moved
/// `distance` px right along their rows (as [`slide_row`] moves them), then blurred along the rows by a Gaussian of `sigma`
/// px (cut off at `truncate` sigmas, as [`gaussian_filter_rows`] blurs); `edge` is what lies beyond the left and right
/// edges for both. Nothing is mixed between rows.
///
/// The result holds, for each picture row, `width` values of I and then `width` values of Q: row `r`'s I at
/// `r * 2 * width`, its Q `width` further on. A distance of 0 and a sigma under about `0.5 / truncate` px give the colour
/// as it is. Each row is made from its own row alone, so the result is the same at any thread count.
pub(crate) fn smeared(
    pixels: &[[f32; 3]],
    (height, width): (usize, usize),
    distance: f64,
    (sigma, truncate): (f64, f64),
    edge: Extension,
) -> Vec<f32> {
    let mut planes = vec![0.0; 2 * height * width];
    par::rows(&mut planes, 2 * width, |first_row, band| {
        let mut moved = vec![[0.0; 3]; if distance == 0.0 { 0 } else { width }];
        for (at, line) in band.chunks_exact_mut(2 * width).enumerate() {
            let row = &pixels[(first_row + at) * width..][..width];
            let row = if distance == 0.0 {
                row
            } else {
                slide_row(row, distance, edge, &mut moved);
                &moved
            };
            let (in_phase, quadrature) = line.split_at_mut(width);
            for ((i, q), &pixel) in in_phase.iter_mut().zip(quadrature).zip(row) {
                let [_, chroma_i, chroma_q] = rgb_to_yiq(pixel);
                (*i, *q) = (chroma_i, chroma_q);
            }
        }
    });
    gaussian_filter_rows(&mut planes, width, sigma, truncate, edge);
    planes
}
