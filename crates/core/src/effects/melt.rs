//! `melt`: the bright parts of the picture run downward in drips, each column by its own length. What it does is documented
//! once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::centre::Centre;
use crate::ops::color::luminance;
use crate::ops::edge::Edge;
use crate::ops::gauss::ndtr;
use crate::ops::noise::{Fractal, field};
use crate::ops::sample::{Interpolation, Sampler};
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `melt` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Distortion,
    opacity: 1.0,
    inputs: &[],
};

/// Pull the bright parts of each column downward, up to `amount` px, by a drip length that differs per column.
///
/// Each column has a drip length between 0 and 1: smooth noise along the row, `scale` px wide and standardised to mean 0 and
/// standard deviation 1, passed through the normal cumulative distribution so that lengths spread evenly. A pixel is pulled
/// down by `amount` x its column's length x its own brightness (Rec. 709 luminance): it shows the picture that many px higher
/// up in the same column, read by `interpolation` with `edge` above the top. Nothing moves sideways, black pixels stay in
/// place, and the same `seed` gives the same drips. The result never leaves 0..1.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The longest drip, in px: a white pixel in the columns that drip most is pulled down by this much, less bright pixels
    /// by their brightness's share of it, black ones not at all. 0 leaves the picture unchanged.
    #[param(default = 40.0, unit = px, min = 0, max = 500)]
    pub amount: f32,
    /// How wide a drip is in px: neighbouring columns drip alike over about this distance. Small = a fringe of thin
    /// needles, large = a few broad runs.
    #[param(default = 12.0, unit = px, min = 1, max = 200)]
    pub scale: f32,
    /// Random seed: the same seed always gives the same drips, another seed a different pattern of long and short columns.
    #[param(default = 0, unit = count, min = 0, max = 4294967295)]
    pub seed: u32,
    /// How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps every value
    /// exactly as it is but edges turn jagged), 'bilinear' blends the pixels above and below it (smooth, slightly soft),
    /// 'bicubic' fits a cubic through the neighbours (sharpest, a little slower, and may ring beside hard edges).
    #[param(default = Bilinear, advanced)]
    pub interpolation: Interpolation,
    /// What the drips read above the picture's top edge (only the top `amount` rows are pulled from there): 'nearest' (the
    /// top row repeated, so the top smears down), 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the
    /// edge pixel), 'wrap' (the bottom of the picture).
    #[param(default = Nearest, advanced)]
    pub edge: Edge,
}

/// The picture with its bright parts pulled down.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    // constant: one octave of noise on a strip one pixel high, so the other two layer settings have no effect
    let strip = field(
        1,
        width,
        &Fractal {
            spacing: f64::from(params.scale),
            octaves: 1,
            persistence: 0.0,
            lacunarity: 1.0,
            seed: params.seed,
            origin: 0.0, // constant: a lattice point on the first column
        },
        0,
    );
    let drip: Vec<f64> = strip.iter().map(|&z| ndtr(f64::from(z))).collect();
    let amount = f64::from(params.amount);
    let sampler = Sampler::new(
        picture.pixels(),
        height,
        width,
        params.interpolation,
        params.edge.extension(),
    );
    Centre::new(height, width, 0.5, 0.5).warp(&sampler, |spot| {
        let bright = f64::from(luminance(picture.pixels()[spot.index]));
        (spot.row - amount * drip[spot.index % width] * bright, spot.column)
    })
}
