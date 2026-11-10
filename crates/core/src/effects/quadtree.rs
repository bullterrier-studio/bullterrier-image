//! `quadtree`: flat cells that stay large where the picture is even and shrink where it has detail. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::length::{float, whole};
use crate::ops::par;
use crate::ops::quadtree::{Leaf, leaves};
use crate::params::{Color, Params};
use crate::{Canvas, Footprint};
use crate::{Image, Inputs, Picture};

/// Where `quadtree` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Stylised,
    opacity: 1.0,
    inputs: &[],
};

/// What `quadtree` reads around a pixel: where a cell splits depends on the cells around it and the picture's size.
pub(crate) fn footprint(_params: &Params, _canvas: &Canvas) -> Footprint {
    Footprint::Whole
}

/// Redraw the picture as flat cells that split only where it varies: large in even areas, small at edges.
///
/// Each cell is filled with the average colour of the photo under it. A cell splits into four while its colours spread
/// more than `threshold` and its halves would still be at least `min_cell` wide; the first grid's cells are at most
/// `max_cell` wide. `gap` draws a line between cells and `rounding` rounds their corners.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How uneven a cell has to be before it splits into four: the spread of its colour, the largest standard deviation of
    /// red, green or blue inside it, on a 0 to 0.5 scale (0.5 is the most any cell can have, half black and half white). 0
    /// splits every cell that is not perfectly flat; 0.5 splits none. Raising it gives bigger cells and a coarser picture.
    #[param(default = 0.06, unit = fraction, min = 0, max = 0.5)]
    pub threshold: f32,
    /// The largest a cell can be, in px at the recipe's size: the picture starts as an even grid of cells at most this
    /// wide. Raising it makes the flat parts coarser.
    #[param(default = 60.0, unit = px, min = 2, max = 480)]
    pub max_cell: f32,
    /// The smallest a cell can be, in px at the recipe's size, never above `max_cell`. A cell does not split into halves
    /// smaller than this. Lowering it keeps finer detail at edges.
    #[param(default = 5.0, unit = px, min = 1, max = 480, at_most = max_cell)]
    pub min_cell: f32,
    /// Width of the line between neighbouring cells, in px at the recipe's size: 0 = cells touch, 1 a thin grid, 5 or more
    /// visible tiles. The picture's own edge gets no line. A cell no wider than the line is all line, so keep it below
    /// `min_cell`.
    #[param(default = 1.0, unit = px, min = 0, max = 24)]
    pub gap: f32,
    /// The colour of the lines between cells, and of the corners a `rounding` cuts away: '#rrggbb', '#rgb' or three numbers
    /// 0 to 1.
    #[param(default = "#000000")]
    pub gap_color: Color,
    /// How round each cell's corners are, as a share of half its shorter side: 0 = square cells, 1 = a circle in each square
    /// cell.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1)]
    pub rounding: f32,
}

/// `length` px as the f32 of the picture's coordinates.
#[expect(clippy::cast_possible_truncation, reason = "a coordinate, far below 2^24 px")]
fn coordinate(length: usize) -> f32 {
    float(length) as f32
}

/// Draws `leaf` into `band` (rows `first_row` on, `width` wide): each pixel is the cell's mean, or the line colour near the
/// cell's border, by a rounded box's signed distance in pixel-centre coordinates.
fn draw(
    band: &mut [[f32; 3]],
    (first_row, width): (usize, usize),
    leaf: &Leaf,
    picture_size: (usize, usize),
    params: &Params,
) {
    let rows = band.len() / width;
    let (first, last) = (leaf.top.max(first_row), leaf.bottom.min(first_row + rows));
    if params.gap == 0.0 && params.rounding == 0.0 {
        for row in first..last {
            band[(row - first_row) * width + leaf.left..(row - first_row) * width + leaf.right].fill(leaf.mean);
        }
        return;
    }
    let half_gap = params.gap / 2.0;
    let size = coordinate((leaf.bottom - leaf.top).min(leaf.right - leaf.left));
    // The cell as a box; a side on the picture's edge is pushed out by half the gap, so the edge gets no line.
    let top = coordinate(leaf.top) - if leaf.top == 0 { half_gap } else { 0.0 };
    let left = coordinate(leaf.left) - if leaf.left == 0 { half_gap } else { 0.0 };
    let bottom = coordinate(leaf.bottom) + if leaf.bottom == picture_size.0 { half_gap } else { 0.0 };
    let right = coordinate(leaf.right) + if leaf.right == picture_size.1 { half_gap } else { 0.0 };
    let radius = params.rounding * size / 2.0;
    let (centre_x, centre_y) = (f32::midpoint(left, right), f32::midpoint(top, bottom));
    // how far a pixel centre may lie from the box's centre before it is beyond the straight part of a side
    let (reach_x, reach_y) = ((right - left) / 2.0 - radius, (bottom - top) / 2.0 - radius);
    let line = params.gap_color.0;
    for row in first..last {
        let past_y = (coordinate(row) + 0.5 - centre_y).abs() - reach_y;
        let cells = &mut band[(row - first_row) * width + leaf.left..(row - first_row) * width + leaf.right];
        for (column, pixel) in (leaf.left..).zip(cells) {
            let past_x = (coordinate(column) + 0.5 - centre_x).abs() - reach_x;
            // The signed distance to the rounded box (negative inside), the standard formula for a rounded rectangle.
            let beyond = (past_x.max(0.0).powi(2) + past_y.max(0.0).powi(2)).sqrt();
            let distance = beyond + past_x.max(past_y).min(0.0) - radius;
            // A pixel centre half a pixel inside the cell's border is fully the cell's; the line takes `half_gap` of each
            // side.
            let cover = (0.5 - (distance + half_gap)).clamp(0.0, 1.0);
            *pixel = [0, 1, 2].map(|channel| line[channel] + (leaf.mean[channel] - line[channel]) * cover);
        }
    }
}

/// The picture as cells.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let cells = leaves(
        picture.pixels(),
        (height, width),
        params.threshold,
        (
            whole(f64::from(params.max_cell)).max(1),
            whole(f64::from(params.min_cell)).max(1),
        ),
    );
    let mut out = vec![[0.0; 3]; height * width];
    par::rows(&mut out, width, |first_row, band| {
        for leaf in &cells {
            if leaf.bottom > first_row && leaf.top < first_row + band.len() / width {
                draw(band, (first_row, width), leaf, (height, width), params);
            }
        }
    });
    Image::from_pixels(height, width, out)
}
