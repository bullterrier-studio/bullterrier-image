//! `displace`: every pixel moved down and across by two independent fields of fractal noise. What it does is documented
//! once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Input, Meta};
use crate::ops::centre::Centre;
use crate::ops::edge::Edge;
use crate::ops::noise::{Fractal, field_rows};
use crate::ops::sample::{Interpolation, Sampler};
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `displace` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Distortion,
    opacity: 1.0,
    inputs: &[Input::Place],
};

/// The noise of `displace`: both moves are fields of it, drawn by canvas place.
fn lattice(params: &Params, spacing: f64) -> Fractal {
    Fractal {
        spacing,
        octaves: params.octaves.cast_unsigned(),
        persistence: f64::from(params.persistence),
        lacunarity: f64::from(params.lacunarity),
        seed: params.seed,
        origin: 0.0, // constant: a lattice point on the first pixel, so a spacing of 1 moves every pixel on its own
    }
}

/// What `displace` reads around a pixel: a move is `amount` times a field no larger than the noise's bound.
pub(crate) fn footprint(params: &Params) -> Footprint {
    // constant: a spacing no octave falls under 1 px from, so the bound holds at any size ratio (a finer lattice has fewer octaves)
    let all_octaves = f64::MAX;
    Footprint::reach(
        f64::from(params.amount) * lattice(params, all_octaves).bound(),
        params.edge == Edge::Wrap,
    )
}

/// How far `displace` paints beyond the picture's edge: as far as it reads.
pub(crate) fn spread(params: &Params) -> crate::Spread {
    crate::Spread::of_footprint(footprint(params))
}

/// Move every pixel by two independent noise fields (down and right), by `amount` px typically: warp, tearing, scatter,
/// frosted glass.
///
/// Each pixel shows the picture at its own place plus a move (down, right) from two fractal noise fields of the same
/// `scale`, `octaves`, `persistence` and `lacunarity`, drawn from `seed` independently of each other. A field has
/// standard deviation 1 at every place, whatever the picture around it, and is multiplied by `amount`: the moves
/// are `amount` px typically, and a few reach about 3 times that, so the `edge` rule matters near the border.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The typical distance a pixel moves, in px (its standard deviation); a few move up to about 3 times as far. Raising it
    /// tears the picture up more; 0 leaves it unchanged.
    #[param(default = 4.0, unit = px, min = 0, max = 500, drivable)]
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
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    if params.amount == 0.0 {
        return picture.map(|pixel| pixel);
    }
    let (height, width) = (picture.height(), picture.width());
    // the noise is that of the canvas place, so a band of the picture is moved as the same rows of the whole are
    let place = inputs.place(picture);
    let noise = lattice(params, f64::from(params.scale)); // the render's px are working px, already scaled
    let [down, across] = [0, 1].map(|stream| field_rows(place.first_row, height, width, &noise, stream));
    let gain = f64::from(params.amount);
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
