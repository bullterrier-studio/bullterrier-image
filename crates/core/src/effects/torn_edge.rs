//! `torn_edge`: the picture's border torn away along a ragged line and replaced by a paper colour. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Input, Meta};
use crate::ops::noise::{Fractal, field};
use crate::params::{Color, Params};
use crate::{Canvas, Footprint};
use crate::{Image, Inputs, Picture};

/// Where `torn_edge` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Texture,
    opacity: 1.0,
    inputs: &[Input::Place],
};

/// What `torn_edge` reads around a pixel.
pub(crate) fn footprint(_params: &Params, _canvas: &Canvas) -> Footprint {
    Footprint::Pixel
}

/// Tear the border away: each side's pixels closer than a wandering distance of about `width` px become `fill`.
///
/// The distance along a side is `width` x (1 + `roughness` x noise), the noise fractal (`scale`, `octaves`, `persistence`,
/// `lacunarity`) of zero mean and unit deviation along that side, so it averages `width` and has a standard deviation
/// `roughness` x `width` (a negative distance tears nothing there). A pixel is torn away if it is closer to a side than
/// that side's distance at its position. Every other pixel is kept exactly; the four sides tear independently, and the
/// same `seed` always gives the same tear.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How far the tear is from each side on average, in px: 0 = no tear. Larger = more of the border torn away.
    #[param(default = 24.0, unit = px, min = 0, max = 200)]
    pub width: f32,
    /// How ragged the tear is: the standard deviation of its distance from the side as a fraction of `width`. 0 = a straight
    /// cut exactly `width` in, 0.3 is a gentle tear, 1 wanders between the very edge and twice the width.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub roughness: f32,
    /// Size of the tear's broadest bays and tongues along the side, in px (the spacing of the first noise lattice). Larger
    /// = long slow waves, small = a jagged edge.
    #[param(default = 24.0, unit = px, min = 1, max = 200)]
    pub scale: f32,
    /// How many layers of detail make up the tear, each finer than the last (by `lacunarity`) and weaker (by
    /// `persistence`). 1 = smooth waves, more = a fibrous, ragged edge. Layers finer than a pixel are dropped.
    #[param(default = 3, unit = count, min = 1, max = 8)]
    pub octaves: i32,
    /// How much weaker each finer layer of the tear is than the one before. 0 = only the broadest layer, 1 = every layer
    /// equally strong (very jagged).
    #[param(default = 0.4, unit = ratio, min = 0, max = 1)]
    pub persistence: f32,
    /// How much finer each layer of the tear is than the one before: the lattice spacing is divided by this per layer. 1 =
    /// every layer the same size, larger = detail much finer than the waves.
    #[param(default = 2.0, unit = ratio, min = 1, max = 4)]
    pub lacunarity: f32,
    /// The paper colour that replaces the torn-away border: '#rrggbb', '#rgb' or [r, g, b] 0..1.
    #[param(default = "#f5f0e6")]
    pub fill: Color,
    /// Random seed: the same seed always tears the same line (each side its own), another seed a different one.
    #[param(default = 0, unit = count, min = 0, max = 4294967295)]
    pub seed: u32,
}

/// How far the tear is from a side at each of `length` positions along it, in px; `side` numbers the sides (top 0, bottom 1,
/// left 2, right 3), each its own tear.
fn distances(length: usize, side: u64, params: &Params) -> Vec<f32> {
    // a strip one pixel high: the tear is one line of noise along the side
    let wander = field(
        1,
        length,
        &Fractal {
            spacing: f64::from(params.scale),
            octaves: params.octaves.cast_unsigned(),
            persistence: f64::from(params.persistence),
            lacunarity: f64::from(params.lacunarity),
            seed: params.seed,
            origin: 0.0, // constant: a lattice point on the first pixel
        },
        side,
    );
    wander
        .into_iter()
        .map(|noise| params.width * (1.0 + params.roughness * noise))
        .collect()
}

/// The picture with the border torn away.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    // the tears follow the canvas's four sides, of which the picture holds the rows from `first_row`
    let place = inputs.place(picture);
    let (height, width) = (place.canvas_height, place.canvas_width);
    let (top, bottom) = (distances(width, 0, params), distances(width, 1, params));
    // constant: each of the four edges draws from a stream of its own
    let (left, right) = (distances(height, 2, params), distances(height, 3, params));
    #[expect(clippy::cast_precision_loss, reason = "a picture side is far below 2**24")]
    picture.map_at(|row, column, pixel| {
        let row = place.first_row + row;
        let torn = (row as f32) < top[column]
            || ((height - 1 - row) as f32) < bottom[column]
            || (column as f32) < left[row]
            || ((width - 1 - column) as f32) < right[row];
        if torn { params.fill.0 } else { pixel }
    })
}
