//! Text art: a picture redrawn as a grid of characters, each chosen by how much ink it puts down.
//!
//! The one algorithm behind every effect that turns a picture into text. Callers decide what the text looks like: the
//! palette, its size, the colours and how much of the picture shows through. The glyph shapes and the font are
//! [`glyphs`](super::glyphs)'s business.

use super::color::{luminance, vivid};
use super::glyphs::Atlas;
use super::length::{float, whole};
use super::{cast, par};
use crate::{Image, Picture};

/// How a picture is redrawn.
pub(crate) struct Style<'a> {
    /// Character width in px; the height follows from the font (see [`Atlas::height`]).
    pub(crate) cell: usize,
    /// The palette, in any order; each cell gets the character whose ink matches its tone.
    pub(crate) chars: &'a str,
    /// The ink of every character; none colours each from the picture under it, made [`vivid`].
    pub(crate) ink: Option<[f32; 3]>,
    /// With no `ink`: the floor of [`vivid`].
    pub(crate) ink_floor: f32,
    /// The paper behind the characters.
    pub(crate) paper: [f32; 3],
    /// The luminance below which `paper` is dark (dense characters on bright cells), else light.
    pub(crate) paper_threshold: f32,
    /// The exponent on the ink amount.
    pub(crate) gamma: f32,
    /// How much of the picture shows through the paper, 0..=1.
    pub(crate) photo: f32,
}

/// `picture` redrawn in characters, a new image of the same size. The grid starts at the top left corner; a cell cut off by
/// the right or bottom edge is measured as if the border pixels went on. Each cell is the average colour of its pixels; its
/// ink amount is that colour's luminance on a dark paper and its complement on a light one, to the power `gamma`, and the
/// character is the one at that share of the way along the palette sorted by ink.
///
/// # Errors
/// A message if the bundled font cannot be read (see [`Atlas::new`]).
pub(crate) fn text_art(picture: &Picture<'_>, style: &Style<'_>) -> Result<Image, String> {
    let atlas = Atlas::new(style.chars, style.cell)?;
    let (cell_width, cell_height) = (atlas.width(), atlas.height());
    let (height, width) = (picture.height(), picture.width());
    let (rows, columns) = (height.div_ceil(cell_height), width.div_ceil(cell_width));
    let pixels = picture.pixels();
    let dark_paper = luminance(style.paper) < style.paper_threshold;
    let last = float(atlas.len() - 1);
    // the average colour of each cell and the glyph it stands for
    let cells: Vec<([f32; 3], usize)> = (0..rows * columns)
        .map(|cell| {
            let (top, left) = (cell / columns * cell_height, cell % columns * cell_width);
            let mut sum = [0.0_f64; 3];
            for row in top..top + cell_height {
                for column in left..left + cell_width {
                    // a row or column past the picture reads the last one
                    let pixel = pixels[row.min(height - 1) * width + column.min(width - 1)];
                    for channel in 0..3 {
                        sum[channel] += f64::from(pixel[channel]);
                    }
                }
            }
            let count = float(cell_height * cell_width);
            let mean = sum.map(|total| cast::single(total / count));
            let tone = luminance(mean);
            let amount = (if dark_paper { tone } else { 1.0 - tone })
                .clamp(0.0, 1.0)
                .powf(style.gamma);
            (mean, whole(f64::from(amount) * last))
        })
        .collect();
    let mut out = vec![[0.0; 3]; height * width];
    par::rows(&mut out, width, |first, band| {
        for (offset, line) in band.chunks_exact_mut(width).enumerate() {
            let row = first + offset;
            for (column, pixel) in line.iter_mut().enumerate() {
                let (mean, glyph) = cells[row / cell_height * columns + column / cell_width];
                let share = atlas.glyph(glyph)[row % cell_height * cell_width + column % cell_width];
                let text = style.ink.unwrap_or_else(|| vivid(mean, style.ink_floor));
                let photo = pixels[row * width + column];
                *pixel = [0, 1, 2].map(|channel| {
                    let base = style.paper[channel] * (1.0 - style.photo) + photo[channel] * style.photo;
                    base * (1.0 - share) + text[channel] * share
                });
            }
        }
    });
    Ok(Image::from_pixels(height, width, out))
}
