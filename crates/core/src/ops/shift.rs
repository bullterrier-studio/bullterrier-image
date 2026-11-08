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

/// A run of whole rows to slide sideways: `rows` rows from row `top` (a band reaching below the picture slides the rows it
/// has), moved `distance` px right (left if negative).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Band {
    pub(crate) top: usize,
    pub(crate) rows: usize,
    pub(crate) distance: i64,
}

/// The `height` x `width` `pixels` (both at least 1) with each of `bands` slid sideways by whole pixels, what leaves one
/// side coming back on the other, the rest of the picture as it was.
///
/// A row in several bands moves by the sum of their distances; the order of the bands does not matter. Every row keeps
/// exactly the pixels it had, rotated. Each output row reads only its own input row, so the result is the same at any
/// thread count.
pub(crate) fn slide_bands(pixels: &[[f32; 3]], height: usize, width: usize, bands: &[Band]) -> Vec<[f32; 3]> {
    let mut slide = vec![0_i64; height];
    for band in bands {
        for row in slide.iter_mut().skip(band.top).take(band.rows) {
            *row += band.distance;
        }
    }
    #[expect(clippy::cast_possible_wrap, reason = "a picture side is far below 2**63")]
    let columns = width as i64;
    let mut out = vec![[0.0; 3]; height * width];
    par::rows(&mut out, width, |first_row, band| {
        for (at, line) in band.chunks_exact_mut(width).enumerate() {
            let row = first_row + at;
            #[expect(clippy::cast_possible_truncation, reason = "rem_euclid is in 0..width")]
            let moved = slide[row].rem_euclid(columns) as usize;
            let source = &pixels[row * width..][..width];
            // the pixel at column `moved` is the first of the source row
            line[moved..].copy_from_slice(&source[..width - moved]);
            line[..moved].copy_from_slice(&source[width - moved..]);
        }
    });
    out
}

#[cfg(test)]
mod tests {
    //! `slide_bands` on rows of numbered pixels, worked out by hand. The direction of a slide is chosen at random by
    //! the one effect that uses it, so no public route shows which way a positive distance goes.

    #![expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        reason = "tiny whole numbers carried in a float channel"
    )]

    use super::{Band, slide_bands};

    /// `rows` rows of 5 pixels numbered 0..5 in red.
    fn numbered(rows: usize) -> Vec<[f32; 3]> {
        (0..rows * 5).map(|at| [(at % 5) as f32, 0.0, 0.0]).collect()
    }

    /// The red numbers of row `row` of `pixels` (5 wide).
    fn row(pixels: &[[f32; 3]], row: usize) -> Vec<u8> {
        pixels[row * 5..][..5].iter().map(|pixel| pixel[0] as u8).collect()
    }

    #[test]
    fn a_band_slides_right_for_a_positive_distance_left_for_a_negative_one_and_wraps() {
        for (distance, expected) in [
            (2, [3, 4, 0, 1, 2]),
            (-1, [1, 2, 3, 4, 0]),
            (0, [0, 1, 2, 3, 4]),
            (5, [0, 1, 2, 3, 4]),
            (-7, [2, 3, 4, 0, 1]),
            (12, [3, 4, 0, 1, 2]),
        ] {
            let out = slide_bands(
                &numbered(3),
                3,
                5,
                &[Band {
                    top: 1,
                    rows: 1,
                    distance,
                }],
            );
            assert_eq!(row(&out, 1), expected, "distance {distance}");
            // the rows beside the band stay
            assert_eq!((row(&out, 0), row(&out, 2)), (vec![0, 1, 2, 3, 4], vec![0, 1, 2, 3, 4]));
        }
    }

    #[test]
    fn a_row_in_two_bands_moves_by_both_whatever_their_order_and_a_band_past_the_picture_has_the_rows_it_reaches() {
        let bands = [
            Band {
                top: 0,
                rows: 2,
                distance: 1,
            },
            Band {
                top: 1,
                rows: 9,
                distance: 3,
            },
        ];
        let out = slide_bands(&numbered(3), 3, 5, &bands);
        assert_eq!(
            (row(&out, 0), row(&out, 1), row(&out, 2)),
            (vec![4, 0, 1, 2, 3], vec![1, 2, 3, 4, 0], vec![2, 3, 4, 0, 1])
        );
        let reversed = [bands[1], bands[0]];
        assert_eq!(slide_bands(&numbered(3), 3, 5, &reversed), out);
    }
}
