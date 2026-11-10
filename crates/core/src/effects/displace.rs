//! `displace`: every pixel moved down and across by two independent fields of fractal noise. What it does is documented
//! once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::centre::Centre;
use crate::ops::edge::Edge;
use crate::ops::noise::{Fractal, fractal};
use crate::ops::sample::{Interpolation, Sampler};
use crate::params::Params;
use crate::{Canvas, Footprint};
use crate::{Image, Inputs, Picture};

/// Where `displace` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Distortion,
    opacity: 1.0,
    inputs: &[],
};

/// What `displace` reads around a pixel.
pub(crate) fn footprint(_params: &Params, _canvas: &Canvas) -> Footprint {
    Footprint::Whole
}

/// Move every pixel by two independent noise fields (down and right), the longest move being `amount` px: warp, tearing,
/// scatter, frosted glass.
///
/// Each pixel shows the picture at its own place plus a move (down, right) from two fractal noise fields of the same
/// `scale`, `octaves`, `persistence` and `lacunarity`, drawn from `seed` independently of each other. The moves are scaled
/// together, by one number, so that the longest of them is `amount` px: no pixel reads from farther than that, so the
/// `edge` rule only matters within `amount` px of the border.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The farthest any pixel's move is, in pixels: the moves are scaled so that the largest one, wherever the noise is
    /// strongest, is exactly this far. Raising it tears the picture up more; 0 leaves it unchanged.
    #[param(default = 12.0, unit = px, min = 0, max = 500, drivable)]
    pub amount: f32,
    /// Size of the coarsest waves of the displacement in pixels, the spacing of the first noise lattice. Larger = broad,
    /// slow wobbles that bend the picture; small = fine, jittery tearing; 1 (with 1 octave and 'nearest') = every pixel
    /// moves on its own, frosted glass.
    #[param(default = 80.0, unit = px, min = 1, max = 1024)]
    pub scale: f32,
    /// How many layers of noise are summed, each finer than the last (by `lacunarity`) and weaker (by `persistence`). 1 =
    /// smooth waves, more = more fine, ragged detail. Layers finer than one pixel are dropped.
    #[param(default = 3, unit = count, min = 1, max = 8)]
    pub octaves: i32,
    /// How much weaker each finer layer is than the one before: each layer's weight is this to the power of its number. 0 =
    /// only the coarsest layer, 1 = every layer equally strong (very rough).
    #[param(default = 0.5, unit = ratio, min = 0, max = 1)]
    pub persistence: f32,
    /// How much finer each layer is than the one before: the lattice spacing is divided by this per layer. 1 = every layer
    /// the same size, 2 = each half as coarse, larger = detail drops off faster in size.
    #[param(default = 2.0, unit = ratio, min = 1, max = 4)]
    pub lacunarity: f32,
    /// Random seed: the same seed always gives the same displacement, another seed a different one.
    #[param(default = 0, unit = count, min = 0, max = 4294967295)]
    pub seed: u32,
    /// How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps every value
    /// exactly as it is but edges turn jagged), 'bilinear' blends the four pixels around it (smooth, slightly soft),
    /// 'bicubic' fits a cubic through the neighbouring pixels (sharpest, a little slower, and may ring beside hard edges).
    #[param(default = Bilinear, advanced)]
    pub interpolation: Interpolation,
    /// What a pixel near the border sees when its move reads from beyond the picture: 'reflect' (mirrored, edge pixel
    /// repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite
    /// edge).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The picture with every pixel moved by the noise.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    if params.amount == 0.0 {
        return picture.map(|pixel| pixel);
    }
    let (height, width) = (picture.height(), picture.width());
    let [down, across] = fractal(
        height,
        width,
        &Fractal {
            spacing: f64::from(params.scale),
            octaves: params.octaves.cast_unsigned(),
            persistence: f64::from(params.persistence),
            lacunarity: f64::from(params.lacunarity),
            seed: params.seed,
            origin: 0.0, // constant: a lattice point on the first pixel, so a spacing of 1 moves every pixel on its own
        },
    );
    // Each field has unit spread, so (unless the picture is a single pixel, where it is all 0) a move of at least 1 exists
    // somewhere: dividing by the longer of that and 1 makes the longest move exactly `amount`.
    let longest = down
        .iter()
        .zip(&across)
        .map(|(down, across)| f64::from(*down).hypot(f64::from(*across)))
        .fold(1.0, f64::max);
    let gain = f64::from(params.amount) / longest;
    let sampler = Sampler::new(
        picture.pixels(),
        height,
        width,
        params.interpolation,
        params.edge.extension(),
    );
    Centre::new(height, width, 0.5, 0.5).warp(&sampler, |spot| {
        (
            spot.row + gain * f64::from(down[spot.index]),
            spot.column + gain * f64::from(across[spot.index]),
        )
    })
}
