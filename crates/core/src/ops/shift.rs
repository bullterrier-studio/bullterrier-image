//! Moving a picture by a distance in pixels, whole or not, with a rule for what the move uncovers at the edge.

use super::edge::Extension;
use super::par;
use super::sample::{Interpolation, Sampler};

/// The `height` x `width` `pixels` (row after row, both at least 1) moved down by `dy` and right by `dx` px (a negative
/// distance moves them up or left), as a new picture of the same size.
///
/// Pixel (i, j) of the result is the picture at (i - `dy`, j - `dx`). A distance that is not a whole number reads between
/// the two pixels it lies between, linearly along each axis (so both together are bilinear): moved by 1.25, a pixel puts
/// three quarters of its value on the pixel 1 away and a quarter on the pixel 2 away. A whole distance moves pixels
/// exactly; the values never leave the range of the picture. `edge` says what lies beyond the picture's edge, which a move
/// uncovers on one side; any distance is allowed, also beyond the picture's size. With `Extension::Wrap` the picture's total
/// is kept.
pub(crate) fn shifted(
    pixels: &[[f32; 3]],
    height: usize,
    width: usize,
    dy: f64,
    dx: f64,
    edge: Extension,
) -> Vec<[f32; 3]> {
    let sampler = Sampler::new(pixels, height, width, Interpolation::Bilinear, edge);
    let mut out = vec![[0.0; 3]; height * width];
    par::rows(&mut out, width, |first_row, band| {
        for (at, pixel) in band.iter_mut().enumerate() {
            #[expect(
                clippy::cast_precision_loss,
                clippy::cast_possible_truncation,
                reason = "a picture side is far below 2**52; the reading is made in f64 and stored in the f32 picture"
            )]
            {
                let (row, column) = (first_row + at / width, at % width);
                *pixel = sampler
                    .at(row as f64 - dy, column as f64 - dx)
                    .map(|level| level as f32);
            }
        }
    });
    out
}
