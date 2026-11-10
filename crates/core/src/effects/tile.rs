//! `tile`: the whole picture, scaled down, repeated in a grid of columns by rows, the grid slid around. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::edge::Extension;
use crate::ops::shift::shifted;
use crate::params::Params;
use crate::{Canvas, Footprint};
use crate::{Image, Inputs, Picture};

/// Where `tile` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Distortion,
    opacity: 1.0,
    inputs: &[],
};

/// What `tile` reads around a pixel.
pub(crate) fn footprint(_params: &Params, _canvas: &Canvas) -> Footprint {
    Footprint::Whole
}

/// Repeat the picture scaled down in a grid, or slide it around: a rolling scan, a wrap-around shift.
///
/// The picture is averaged down by area (each new pixel is the mean of the source pixels it covers, fractions of a pixel
/// counted in part) to a cell of whole pixels, width // `columns` by height // `rows` and at least 1 px each. The cell is
/// slid by `offset_x` and `offset_y` of its size, wrapping around within the cell (a fraction of a pixel blends the two
/// pixels it lies between), and repeated across the picture. When the grid does not divide the picture, the leftover
/// pixels (width mod `columns` on the right, height mod `rows` at the bottom) show the start of the next copy, cut off. One
/// column and one row give the picture itself, or with an offset the picture rolled around, as on a television whose sync
/// slipped (the `scan_roll` preset).
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How many copies of the picture sit side by side, 1 to 100; 1 keeps the picture whole across. Each copy is the width
    /// of the picture divided by this, rounded down (at least 1 px).
    #[param(default = 3, unit = count, min = 1, max = 100)]
    pub columns: i32,
    /// How many copies sit one above another, 1 to 100; 1 keeps the picture whole down. Each copy is the height of the
    /// picture divided by this, rounded down (at least 1 px).
    #[param(default = 3, unit = count, min = 1, max = 100)]
    pub rows: i32,
    /// How far the copies slide sideways, as a fraction of one copy's width, -1 to 1: positive moves them to the right,
    /// negative to the left, and what leaves one side of a copy comes back on the other; 0 leaves them where they are, and
    /// so does a whole copy (1 or -1). A distance that ends between two pixels blends the two.
    #[param(default = 0.0, unit = fraction, min = -1, max = 1)]
    pub offset_x: f32,
    /// How far the copies slide vertically, as a fraction of one copy's height, -1 to 1: positive moves them down,
    /// negative up, and what leaves the bottom of a copy comes back at its top; 0 leaves them where they are, and so does a
    /// whole copy (1 or -1). A distance that ends between two pixels blends the two.
    #[param(default = 0.0, unit = fraction, min = -1, max = 1)]
    pub offset_y: f32,
}

/// For each of `target` equal stretches of a line of `source` pixels: the first pixel it covers and the share of the
/// stretch that each pixel from there covers (they add up to 1), the weights of an average by area.
fn area_weights(source: usize, target: usize) -> Vec<(usize, Vec<f64>)> {
    #[expect(clippy::cast_precision_loss, reason = "a picture side is far below 2**52")]
    let (source_len, step) = (source as f64, source as f64 / target as f64);
    (0..target)
        .map(|stretch| {
            #[expect(clippy::cast_precision_loss, reason = "a picture side is far below 2**52")]
            let (start, end) = (stretch as f64 * step, (stretch + 1) as f64 * step);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "inside 0..=source"
            )]
            let (first, past) = (start.floor() as usize, (end.ceil().min(source_len)) as usize);
            let weights = (first..past)
                .map(|pixel| {
                    #[expect(clippy::cast_precision_loss, reason = "a picture side is far below 2**52")]
                    let pixel = pixel as f64;
                    ((end.min(pixel + 1.0) - start.max(pixel)).max(0.0)) / step
                })
                .collect();
            (first, weights)
        })
        .collect()
}

/// `pixels` (`height` x `width`) averaged by area down to `cell_height` x `cell_width`, rows first, then columns.
fn averaged(pixels: &[[f32; 3]], height: usize, width: usize, cell_height: usize, cell_width: usize) -> Vec<[f32; 3]> {
    let mut down = vec![[0.0_f64; 3]; cell_height * width];
    for ((first, weights), line) in area_weights(height, cell_height)
        .iter()
        .zip(down.chunks_exact_mut(width))
    {
        for (offset, weight) in weights.iter().enumerate() {
            let row = &pixels[(first + offset) * width..][..width];
            for (total, pixel) in line.iter_mut().zip(row) {
                for (level, value) in total.iter_mut().zip(pixel) {
                    *level += weight * f64::from(*value);
                }
            }
        }
    }
    let columns = area_weights(width, cell_width);
    let mut cell = Vec::with_capacity(cell_height * cell_width);
    for line in down.chunks_exact(width) {
        for (first, weights) in &columns {
            let mut sum = [0.0_f64; 3];
            for (weight, pixel) in weights.iter().zip(&line[*first..]) {
                for (level, value) in sum.iter_mut().zip(pixel) {
                    *level += weight * value;
                }
            }
            #[expect(
                clippy::cast_possible_truncation,
                reason = "the sums are made in f64 and stored in the f32 cell"
            )]
            cell.push(sum.map(|level| level as f32));
        }
    }
    cell
}

/// The picture averaged down to a cell, the cell slid, and the cell repeated over the picture.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    #[expect(clippy::cast_sign_loss, reason = "columns and rows are at least 1")]
    let (cell_height, cell_width) = (
        (height / params.rows as usize).max(1),
        (width / params.columns as usize).max(1),
    );
    let cell = averaged(picture.pixels(), height, width, cell_height, cell_width);
    #[expect(clippy::cast_precision_loss, reason = "a picture side is far below 2**52")]
    let (down, across) = (
        f64::from(params.offset_y) * cell_height as f64,
        f64::from(params.offset_x) * cell_width as f64,
    );
    let cell = shifted(&cell, cell_height, cell_width, down, across, Extension::Wrap);
    picture.map_at(|row, column, _| {
        cell[(row % cell_height) * cell_width + column % cell_width].map(|level| level.clamp(0.0, 1.0))
    })
}
