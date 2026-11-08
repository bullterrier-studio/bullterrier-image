//! `equalize`: contrast-limited adaptive histogram equalisation of the picture's brightness. What it does is documented
//! once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `equalize` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Colour,
    opacity: 1.0,
    inputs: &[],
};

/// What `equalize` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

// constant: the clipped excess is redistributed until less than this share of a tile's pixels is left over (a pure
// stopping guard: the loop shrinks the excess geometrically, so this ends it long before f64 rounding matters)
const EXCESS_TOLERANCE: f64 = 1e-9;

/// Equalise the brightness histogram tile by tile (CLAHE, Zuiderveld 1994), keeping each pixel's colour.
///
/// The brightness is the value V = max(R, G, B) of the sRGB-encoded picture (the V of HSV). Every pixel's red, green and
/// blue are multiplied by new V / old V, so hue and HSV saturation are unchanged and no channel can leave 0..1; black
/// stays black. Each of the `tiles` x `tiles` regions gets a histogram of V in `bins` bins, clipped so no bin is taller
/// than `limit` times the flat height (the clipped counts are spread evenly over all bins, repeatedly, until none
/// exceeds it), and its mapping is the cumulative histogram: a pixel in bin k goes to the share of the tile's pixels in
/// bins 0..k. A pixel takes the bilinear blend of the mappings of the four tiles whose centres surround it (the nearest
/// tiles at the borders), so there are no seams. With one tile and a limit of `bins` or more this is classic histogram
/// equalisation.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Tiles per side: the picture is cut into tiles x tiles regions, each equalised by its own histogram and blended
    /// smoothly into its neighbours. 1 = one histogram for the whole picture (classic equalisation); raising it adapts
    /// to smaller regions and brings out local contrast. A picture with fewer pixel rows or columns than tiles gets one
    /// tile per row or column.
    #[param(default = 8, unit = count, min = 1, max = 64)]
    pub tiles: i32,
    /// Clip limit, as a multiple of the height every bin would have if a tile's histogram were flat: no bin may be
    /// taller, so no tone range is stretched more than this many times. 1 = the histogram is made flat, which leaves
    /// tones evenly spread (a gentle lift of the darkest bin by one step); raising it allows more contrast and
    /// amplifies noise in flat areas. A limit of `bins` or more never clips (classic equalisation).
    #[param(default = 3.0, unit = ratio, min = 1, max = 256)]
    pub limit: f32,
    /// Histogram bins over the brightness range 0..1: fewer bins give a coarser mapping and fewer distinct brightnesses
    /// in the result (2 = every tile ends up with two brightnesses), 256 matches 8-bit pictures.
    #[param(default = 256, unit = count, min = 2, max = 256)]
    pub bins: i32,
}

/// For each index along an axis of `length` pixels cut into `tiles` tiles: the tile whose centre lies at or before it
/// (the first tile before the first centre), the next tile, and the share, 0..1, of the way to the next tile's centre.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a picture side is far below 2**52; the position is clamped to the tiles before it becomes an index"
)]
fn neighbours(length: usize, tiles: usize) -> Vec<(usize, usize, f32)> {
    (0..length)
        .map(|index| {
            // in tile widths from the first tile's centre
            let position = (index as f64 + 0.5) * tiles as f64 / length as f64 - 0.5;
            let low = position.floor().clamp(0.0, (tiles - 1) as f64) as usize;
            let weight = (position - low as f64).clamp(0.0, 1.0) as f32;
            (low, (low + 1).min(tiles - 1), weight)
        })
        .collect()
}

/// The bin of brightness `value` among `bins`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "a brightness 0..1 times at most 256 bins is a small non-negative number"
)]
fn bin_of(value: f32, bins: usize) -> usize {
    ((value * bins as f32) as usize).min(bins - 1)
}

/// The brightness of a pixel: its largest channel.
fn brightness(pixel: [f32; 3]) -> f32 {
    pixel[0].max(pixel[1]).max(pixel[2])
}

/// One mapping per tile (row after row of tiles, `bins` entries each): the cumulative histogram of the tile's
/// brightnesses, clipped at `limit` times the flat height.
#[expect(
    clippy::cast_precision_loss,
    reason = "a tile holds far fewer than 2**52 pixels and a histogram has at most 256 bins"
)]
fn mappings(picture: &Picture<'_>, tile_rows: usize, tile_columns: usize, bins: usize, limit: f64) -> Vec<f32> {
    let (height, width) = (picture.height(), picture.width());
    let tile_of_column: Vec<usize> = (0..width).map(|column| column * tile_columns / width).collect();
    let mut histograms = vec![0.0_f64; tile_rows * tile_columns * bins];
    for (row, pixels) in picture.pixels().chunks_exact(width).enumerate() {
        let tile_row = row * tile_rows / height;
        for (&pixel, &tile_column) in pixels.iter().zip(&tile_of_column) {
            histograms[(tile_row * tile_columns + tile_column) * bins + bin_of(brightness(pixel), bins)] += 1.0;
        }
    }
    let mut mappings = Vec::with_capacity(histograms.len());
    for histogram in histograms.chunks_mut(bins) {
        let pixels: f64 = histogram.iter().sum();
        // Clip every bin to `limit` x the flat height and give the excess to all bins alike, until nothing is above it.
        let ceiling = limit * pixels / bins as f64;
        loop {
            let excess: f64 = histogram.iter().map(|&count| (count - ceiling).max(0.0)).sum();
            if excess <= EXCESS_TOLERANCE * pixels {
                break;
            }
            for count in histogram.iter_mut() {
                *count = count.min(ceiling) + excess / bins as f64;
            }
        }
        let mut running = 0.0; // constant: the running total starts empty
        mappings.extend(histogram.iter().map(|&count| {
            running += count;
            #[expect(clippy::cast_possible_truncation, reason = "the mapping is stored in f32, as the picture is")]
            let share = (running / pixels) as f32;
            share
        }));
    }
    mappings
}

/// The picture with its brightness equalised.
#[expect(clippy::cast_sign_loss, reason = "tiles and bins have a minimum of 1 and 2")]
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let bins = params.bins as usize;
    // a tile needs at least one pixel
    let (tile_rows, tile_columns) = ((params.tiles as usize).min(height), (params.tiles as usize).min(width));
    let mappings = mappings(picture, tile_rows, tile_columns, bins, f64::from(params.limit));
    let (down, across) = (neighbours(height, tile_rows), neighbours(width, tile_columns));
    // the mapping of tile row `tile_row`, read at `bin`, blended between the two tile columns around column `column`
    let along = |tile_row: usize, column: usize, bin: usize| {
        let (low, high, weight) = across[column];
        let left = mappings[(tile_row * tile_columns + low) * bins + bin];
        let right = mappings[(tile_row * tile_columns + high) * bins + bin];
        left + (right - left) * weight
    };
    picture.map_at(|row, column, pixel| {
        let value = brightness(pixel);
        let bin = bin_of(value, bins);
        let (low, high, weight) = down[row];
        let top = along(low, column, bin);
        let bottom = along(high, column, bin);
        let new_value = top + (bottom - top) * weight;
        let gain = if value > 0.0 { new_value / value } else { 0.0 };
        pixel.map(|channel| channel * gain)
    })
}
