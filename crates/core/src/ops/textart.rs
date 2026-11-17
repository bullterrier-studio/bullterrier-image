//! Text art: a picture redrawn as a grid of characters, each chosen by how much ink it puts down.
//!
//! The one algorithm behind every effect that turns a picture into text. Callers decide what the text looks like: the
//! palette, its size, the colours and how much of the picture shows through. The glyph shapes and the font are
//! [`glyphs`](super::glyphs)'s business.

use super::color::{luminance, vivid};
use super::glyphs::Atlas;
use super::length::{float, whole};
use super::{cast, par};
use crate::{Characters, Image, Picture};

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

/// The size of a grid (rows, columns) and its cells: each one's average colour and glyph.
type Grid = ((usize, usize), Vec<([f32; 3], usize)>);

/// The grid of `picture` in cells of `atlas`: how many rows and columns, and for each cell its average colour and the glyph
/// it stands for. The grid starts at the top left corner; a cell cut off by the right or bottom edge is measured as if
/// the border pixels went on. A cell's ink amount is its colour's luminance on a dark paper and its complement on a light
/// one, to the power `gamma`, and the glyph is the one at that share of the way along the palette sorted by ink.
fn grid(picture: &Picture<'_>, style: &Style<'_>, atlas: &Atlas) -> Grid {
    let (cell_width, cell_height) = (atlas.width(), atlas.height());
    let (height, width) = (picture.height(), picture.width());
    let (rows, columns) = (height.div_ceil(cell_height), width.div_ceil(cell_width));
    let pixels = picture.pixels();
    let dark_paper = luminance(style.paper) < style.paper_threshold;
    let last = float(atlas.len() - 1);
    let cells = (0..rows * columns)
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
    ((rows, columns), cells)
}

/// The colour a character of a cell of average colour `mean` is drawn in.
fn ink_of(style: &Style<'_>, mean: [f32; 3]) -> [f32; 3] {
    style.ink.unwrap_or_else(|| vivid(mean, style.ink_floor))
}

/// `picture` redrawn in characters, as a grid of text (see [`grid`] for how each is chosen) with the colour of each.
pub(crate) fn characters(picture: &Picture<'_>, style: &Style<'_>) -> Characters {
    let atlas = Atlas::new(style.chars, style.cell);
    let ((rows, columns), cells) = grid(picture, style, &atlas);
    Characters::new(
        rows,
        columns,
        cells.iter().map(|&(_, glyph)| atlas.character(glyph)).collect(),
        cells.iter().map(|&(mean, _)| ink_of(style, mean)).collect(),
        style.paper,
    )
}

/// `picture` redrawn in characters, a new image of the same size (see [`grid`] for how each character is chosen).
pub(crate) fn text_art(picture: &Picture<'_>, style: &Style<'_>) -> Image {
    let atlas = Atlas::new(style.chars, style.cell);
    let (cell_width, cell_height) = (atlas.width(), atlas.height());
    let (height, width) = (picture.height(), picture.width());
    let pixels = picture.pixels();
    let ((_, columns), cells) = grid(picture, style, &atlas);
    let mut out = vec![[0.0; 3]; height * width];
    par::rows(&mut out, width, |first, band| {
        for (offset, line) in band.chunks_exact_mut(width).enumerate() {
            let row = first + offset;
            for (column, pixel) in line.iter_mut().enumerate() {
                let (mean, glyph) = cells[row / cell_height * columns + column / cell_width];
                let share = atlas.glyph(glyph)[row % cell_height * cell_width + column % cell_width];
                let text = ink_of(style, mean);
                let photo = pixels[row * width + column];
                *pixel = [0, 1, 2].map(|channel| {
                    let base = style.paper[channel] * (1.0 - style.photo) + photo[channel] * style.photo;
                    base * (1.0 - share) + text[channel] * share
                });
            }
        }
    });
    Image::from_pixels(height, width, out)
}
