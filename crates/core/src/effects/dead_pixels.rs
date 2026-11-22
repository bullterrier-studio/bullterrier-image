//! `dead_pixels`: a screen's faulty pixels, some stuck on a colour and some dead black. What it does is documented once, on
//! `Params`.

use std::collections::HashMap;

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Input, Meta};
use crate::ops::length;
use crate::ops::random::Rng;
use crate::params::{Color, Params};
use crate::{Image, Inputs, Picture, Place};

/// Where `dead_pixels` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Glitch,
    opacity: 1.0,
    inputs: &[Input::Place],
};

/// What `dead_pixels` reads around a pixel: none but its own; the faulty cells are those of the canvas.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Pixel
}

/// Make `count` cells of `size` px faulty: a share `stuck` of them shows `color`, the rest black.
///
/// The picture is divided into cells of `size` px (the last row and column of cells may be cut off by the picture's edge).
/// `seed` shuffles the cells and the first `count` are the faulty ones, so a larger count keeps every pixel a smaller one
/// had; the first `round(count x stuck)` of those are stuck (`count` is capped at the number of cells). Everything else is the picture, bit for bit.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How many faulty pixels there are, each a square of `size` px: 0 leaves the picture unchanged. They lie on a grid of
    /// `size` px cells, so none overlaps another, and each is a different one. A count larger than the number of cells
    /// fills every cell. Raising it adds faulty pixels and never moves one already there. A count, not a density: the
    /// pixels' size is in design px, so the same count covers the same share of any picture size.
    #[param(default = 120, unit = count, min = 0, max = 100000)]
    pub count: i32,
    /// The width and height in px of one faulty pixel, tuned for a picture about 480 px high: 1 is a single pixel of the
    /// picture, larger is a screen with bigger pixels, a block at the edge being cut off by it.
    #[param(default = 4, unit = px, min = 1, max = 64)]
    pub size: i32,
    /// The share of the faulty pixels that are stuck on `color`; the rest are dead and black. 0 = all dead, 1 = all stuck.
    /// The number stuck is `count` x `stuck`, rounded to a whole pixel (`count` is capped at the number of cells).
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub stuck: f32,
    /// The colour a stuck pixel shows whatever the picture is under it: '#rrggbb', '#rgb' or [r, g, b] 0..1. White is the
    /// usual fault; a red, green or blue is one stuck sub-pixel.
    #[param(default = "#ffffff")]
    pub color: Color,
    /// Chooses where the faulty pixels are and which of them are stuck; the same seed and count give the same picture,
    /// another seed another one.
    #[param(default = 0, unit = count, min = 0, max = 4294967295)]
    pub seed: u32,
}

/// What a faulty cell of the grid shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Cell {
    Stuck,
    Dead,
}

/// The faulty cells of the canvas `place` lies on, by their number (row after row of cells), and the cells of a row.
fn faulty_cells(place: Place, params: &Params) -> (HashMap<usize, Cell>, usize) {
    let size = params.size.cast_unsigned() as usize;
    // the cells are those of the canvas: exactly `count` of them (at most 100000, the parameter's maximum) are drawn once
    // from the seed, whichever rows are rendered, and the picture takes those in its rows
    let columns = place.canvas_width.div_ceil(size);
    let cells = place.canvas_height.div_ceil(size) * columns; // the last cells are cut off by the edge
    let faulty = Rng::seeded(params.seed).shuffled(cells, (params.count.cast_unsigned() as usize).min(cells));
    // the first `count` x `stuck` of them (halves to the even number) are stuck
    let stuck = length::whole(length::float(faulty.len()) * f64::from(params.stuck));
    let grid = faulty
        .iter()
        .enumerate()
        .map(|(rank, &cell)| (cell, if rank < stuck { Cell::Stuck } else { Cell::Dead }))
        .collect();
    (grid, columns)
}

/// The picture with its faulty pixels.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let place = inputs.place(picture);
    let size = params.size.cast_unsigned() as usize;
    let (grid, columns) = faulty_cells(place, params);
    picture.map_at(
        |row, column, pixel| match grid.get(&((place.first_row + row) / size * columns + column / size)) {
            None => pixel,
            Some(Cell::Stuck) => params.color.0,
            Some(Cell::Dead) => [0.0; 3], // constant: a dead pixel emits no light
        },
    )
}

/// The faulty pixels on the GPU (`dead_pixels.wgsl`): the CPU's own shuffle says which cells are faulty (a table with a number
/// for every cell: 0 sound, 1 stuck, 2 dead), the kernel looks the pixel's cell up. The picture is otherwise untouched.
#[cfg(feature = "gpu")]
pub(crate) fn kernel() -> crate::gpu::Kernel<Params> {
    crate::gpu::Kernel::new(
        include_str!("dead_pixels.wgsl"),
        |params: &Params, header: &crate::gpu::Header| {
            let place = header.place();
            let [red, green, blue] = params.color.0;
            let columns = place.canvas_width.div_ceil(params.size.cast_unsigned() as usize);
            vec![
                [red, green, blue, 0.0],
                [
                    f32::from_bits(params.size.cast_unsigned()),
                    f32::from_bits(u32::try_from(columns).unwrap_or(u32::MAX)),
                    0.0,
                    0.0,
                ],
            ]
        },
    )
    .with_table(|params: &Params, header: &crate::gpu::Header| {
        let place = header.place();
        let size = params.size.cast_unsigned() as usize;
        let (grid, columns) = faulty_cells(place, params);
        let mut cells = vec![0.0_f32; place.canvas_height.div_ceil(size) * columns];
        for (cell, state) in grid {
            cells[cell] = if state == Cell::Stuck { 1.0 } else { 2.0 };
        }
        cells
    })
}
