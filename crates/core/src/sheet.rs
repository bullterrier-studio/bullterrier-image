//! Contact sheets: many results side by side with labels, for choosing between experiments.

use crate::ops::glyphs::text_row;
use crate::ops::length::{float, whole};
use crate::ops::resample::Resize;
use crate::{Error, Image};

/// The gap between tiles and round the sheet, px.
const GAP: usize = 14; // constant: the layout of the sheet
/// The height under each tile that holds its label, px.
const LABEL_ROW: usize = 30; // constant: the layout of the sheet
/// Where a label starts below its tile, px.
const LABEL_OFFSET: usize = 6; // constant: the layout of the sheet
/// The sheet's background, as a share of full level: #242424.
const BACKGROUND: f32 = 36.0 / 255.0; // constant: #24 of 255
/// The label's colour, as a share of full level: #d8d8d8.
const INK: f32 = 216.0 / 255.0; // constant: #d8 of 255
/// The side of a square of the checkerboard that shows through transparency, px.
const CHECKER: usize = 8; // constant: the squares of the boards image editors show
/// The greys of the checkerboard, as in the boards image editors show.
const LIGHT: f32 = 0.8; // constant: the light grey of the board
const DARK: f32 = 0.6; // constant: the dark grey of the board
/// The type sizes of a label, px: the largest, and the smallest it shrinks to before it is cut.
const LABEL_SIZE: usize = 16; // constant: the label's type size
const LABEL_SMALLEST: usize = 10; // constant: the smallest type size before the label is cut
/// The width of a character of the monospaced font as a share of its size (`JetBrains Mono`'s advance is 0.6 em).
const ADVANCE: f64 = 0.6; // constant: JetBrains Mono's advance width, in em

/// One result of a sheet: its picture and the label under it.
#[derive(Clone, Copy, Debug)]
pub struct Tile<'a> {
    label: &'a str,
    image: &'a Image,
}

impl<'a> Tile<'a> {
    /// A tile showing `image` with `label` under it.
    #[must_use]
    pub const fn new(label: &'a str, image: &'a Image) -> Self {
        Self { label, image }
    }
}

/// `tiles` laid out in a grid of `columns`, each picture scaled to fit a `tile` px square (its aspect kept, centred) with
/// its label underneath. A label wider than the tile is set smaller, and cut with `…` if it still does not fit. A picture
/// with transparency is laid over a checkerboard, so what is clear shows as the board and not as the colour hidden under
/// it. The sheet is an RGB image: a 14 px gap round and between the tiles on a dark ground.
///
/// # Errors
/// [`Error::Recipe`] for no tile, no column or a `tile` of 0, naming which; the size error of a `tile` that makes a
/// picture over the pixel limit.
pub fn contact_sheet(tiles: &[Tile<'_>], columns: usize, tile: usize) -> Result<Image, Error> {
    for (what, value) in [("tile", tiles.len()), ("column", columns), ("tile size", tile)] {
        if value == 0 {
            return Err(Error::Recipe(format!(
                "a contact sheet needs at least one {what}, got 0"
            )));
        }
    }
    let (rows, across) = (tiles.len().div_ceil(columns), columns.min(tiles.len()));
    let (width, height) = (GAP + across * (tile + GAP), GAP + rows * (tile + LABEL_ROW + GAP));
    let mut sheet = vec![[BACKGROUND; 3]; width * height];
    for (index, one) in tiles.iter().enumerate() {
        let (x, y) = (
            GAP + (index % columns) * (tile + GAP),
            GAP + (index / columns) * (tile + LABEL_ROW + GAP),
        );
        let (small, small_height, small_width) = scaled(one.image, tile)?;
        let (left, top) = (
            x + (tile - small_width.min(tile)) / 2,
            y + (tile - small_height.min(tile)) / 2,
        );
        for row in 0..small_height.min(tile) {
            let line = &small[row * small_width..][..small_width.min(tile)];
            sheet[(top + row) * width + left..][..line.len()].copy_from_slice(line);
        }
        label(&mut sheet, width, (x, y + tile + LABEL_OFFSET), tile, one.label);
    }
    Ok(Image::from_pixels(height, width, sheet))
}

/// `image` scaled so its longest side is `side` px, and over the checkerboard if it has transparency: the pixels, then the
/// rows and columns.
fn scaled(image: &Image, side: usize) -> Result<(Vec<[f32; 3]>, usize, usize), Error> {
    let picture = image.as_picture();
    let (height, width) = (picture.height(), picture.width());
    let pixels = picture.values().as_chunks::<3>().0;
    let longest = u32::try_from(side).unwrap_or(u32::MAX);
    let (colour, alpha, height, width) = match Resize::to_longest_side(height, width, longest)? {
        None => (pixels.to_vec(), image.alpha().map(<[f32]>::to_vec), height, width),
        Some(resize) => {
            let alpha = image.alpha().map(|alpha| {
                let planes: Vec<[f32; 1]> = alpha.iter().map(|&value| [value]).collect();
                resize.apply(&planes).into_iter().map(|[value]| value).collect()
            });
            (resize.apply(pixels), alpha, resize.height(), resize.width())
        }
    };
    let Some(alpha) = alpha else {
        return Ok((colour, height, width));
    };
    let over = colour
        .iter()
        .zip(&alpha)
        .enumerate()
        .map(|(index, (pixel, &opacity))| {
            let (row, column) = (index / width, index % width);
            let board = if (row / CHECKER + column / CHECKER).is_multiple_of(2) {
                LIGHT
            } else {
                DARK
            };
            pixel.map(|channel| channel * opacity + board * (1.0 - opacity))
        })
        .collect();
    Ok((over, height, width))
}

/// The width in px of a character at type size `size`.
fn advance(size: usize) -> usize {
    whole(ADVANCE * float(size))
}

/// `text` drawn at `origin` (left, top) on `sheet`, inside `room` px: at the type size when it fits, else at the largest
/// smaller whole size that fits (down to the smallest), else cut at the smallest size and ended with `…`.
fn label(sheet: &mut [[f32; 3]], sheet_width: usize, origin: (usize, usize), room: usize, text: &str) {
    let mut characters: Vec<char> = text.chars().collect();
    let size = (LABEL_SMALLEST..=LABEL_SIZE)
        .rev()
        .find(|&size| characters.len() * advance(size) <= room)
        .unwrap_or(LABEL_SMALLEST);
    let cell = advance(size);
    if characters.len() * cell > room {
        while !characters.is_empty() && (characters.len() + 1) * cell > room {
            characters.pop();
        }
        characters.push('\u{2026}');
    }
    let (coverage, width, height) = text_row(&characters.iter().collect::<String>(), cell);
    for row in 0..height {
        for column in 0..width {
            let covered = coverage[row * width + column];
            if covered > 0.0
                && let Some(pixel) = sheet.get_mut((origin.1 + row) * sheet_width + origin.0 + column)
            {
                for channel in pixel.iter_mut() {
                    *channel += (INK - *channel) * covered;
                }
            }
        }
    }
}
