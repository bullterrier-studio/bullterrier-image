//! `pixel_grid`: the picture seen on an LCD screen, each cell split into red, green and blue stripes. What it does is
//! documented once, on the Python effect; its parameters are declared here.

use serde::Deserialize;

use crate::params::Params;
use crate::{Image, Picture};

const CHANNELS: f64 = 3.0; // constant: one stripe for each of red, green and blue

/// An LCD pixel grid: square cells of `cell` px, each split into three vertical sub-pixels (red, green, blue) that
/// light only their own channel, with a dark `gap` between them, mixed into the picture by `amount`, and optionally
/// brightened to keep the picture's average brightness.
#[derive(Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Width and height in px of one grid cell, which holds one red, one green and one blue stripe, each a third of the
    /// cell wide. Larger = a coarser grid; the smallest, 3, gives stripes one pixel wide.
    #[param(default = 4, unit = px, min = 3, max = 32)]
    pub cell: i32,
    /// Width of the dark lines between sub-pixels, as a fraction of a stripe's width (cell / 3): the same lines run
    /// between the stripes and between rows of cells, half of each inside its own sub-pixel. 0 = stripes touch, 1 =
    /// nothing is lit and the grid is black. Raising it darkens the picture and shows the grid.
    #[param(default = 0.15, unit = fraction, min = 0, max = 1)]
    pub gap: f32,
    /// How far the picture is shown through the grid: 0 = the picture unchanged, 1 = the picture as the grid lights it
    /// (each channel only inside its own sub-pixels, a third of the picture's brightness or less), in between a mix of
    /// the two.
    #[param(default = 0.4, unit = fraction, min = 0, max = 1)]
    pub amount: f32,
    /// Whether the grid is brightened by the share it darkens the picture on average, so the average brightness is kept
    /// and raising `amount` or `gap` shows the grid without dimming the picture (a lit stripe is lifted above the
    /// picture's own value and clips at 1 where that is already bright; with `gap` 1 and `amount` 1 nothing is lit and
    /// the picture is black). False = the grid only ever darkens, as an LCD does.
    #[param(default = false)]
    pub keep_brightness: bool,
}

/// The picture on the grid. The share of each pixel a stripe lights is worked out in `f64`, as the shares of whole
/// rows and columns, then mixed in with `amount`.
#[expect(clippy::cast_possible_truncation, reason = "the gain is applied to f32 pictures")]
pub(crate) fn render(picture: &Picture<'_>, params: &Params) -> Image {
    let cell = f64::from(params.cell);
    let amount = f64::from(params.amount);
    let gap = f64::from(params.gap);
    let stripe = cell / CHANNELS;
    let margin = gap * stripe / 2.0; // half the gap is free on each side of a stripe
    // The lengths that stay lit, worked out so that none can be negative: a gap of at most 1 leaves a row more than
    // two thirds of the cell and a stripe `1 - gap` of itself, which is exactly nothing at the documented maximum.
    let lit_rows = cell - 2.0 * margin;
    let lit_stripe = stripe * (1.0 - gap);
    let rows = lit(picture.height(), cell, margin, lit_rows);
    let columns: Vec<[f64; 3]> = {
        let channel = |index: f64| lit(picture.width(), cell, index * stripe + margin, lit_stripe);
        let [red, green, blue] = [0.0, 1.0, 2.0].map(channel);
        red.iter().zip(&green).zip(&blue).map(|((&r, &g), &b)| [r, g, b]).collect()
    };
    let mean_gain = if params.keep_brightness {
        // each channel is lit over its stripe's share of a cell less the gap's border, in both directions
        let lit_share = lit_rows / cell * lit_stripe / cell;
        // a gap of 1 at amount 1 lights nothing: black stays black, so there is no gain to divide by
        Some(1.0 - amount * (1.0 - lit_share)).filter(|&mean| mean > 0.0)
    } else {
        None
    };
    picture.map_at(|row, column, pixel| {
        [0, 1, 2].map(|channel| {
            let share = rows[row] * columns[column][channel];
            let gain = 1.0 - amount * (1.0 - share);
            let gain = mean_gain.map_or(gain, |mean| gain / mean);
            (pixel[channel] * gain as f32).clamp(0.0, 1.0) // the lifted stripes of keep_brightness can pass 1
        })
    })
}

/// The share of each of `count` pixels (pixel i spans i to i + 1) covered by the intervals of `length` from `start`
/// repeated every `period`, as the difference of the length lit below each pixel edge. `length` is not negative and
/// at most `period` (the caller works it out as a length, never as the difference of two positions).
#[expect(clippy::cast_precision_loss, reason = "a pixel index is far below the 2^53 an f64 holds exactly")]
fn lit(count: usize, period: f64, start: f64, length: f64) -> Vec<f64> {
    let lit_below = |edge: usize| {
        let edge = edge as f64;
        (edge / period).floor() * length + (edge.rem_euclid(period) - start).clamp(0.0, length)
    };
    (0..count).map(|index| lit_below(index + 1) - lit_below(index)).collect()
}
