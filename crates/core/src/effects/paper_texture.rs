//! `paper_texture`: paper fibres, canvas or linen weave laid over the picture as a brightness texture. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::cast;
use crate::ops::noise::{Fractal, field, standardised};
use crate::ops::random::{mix, unit};
use crate::params::{Choice, Params};
use crate::{Image, Inputs, Picture};

/// Where `paper_texture` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Texture,
    opacity: 1.0,
    inputs: &[],
};

/// The surfaces `paper_texture` can lay.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Kind {
    /// Random fibres only.
    Paper,
    /// An even plain weave of threads `spacing` px apart, with the fibres mixed in.
    Canvas,
    /// The same weave, but every thread has its own random strength, so it looks uneven.
    Linen,
}

/// Lay a paper, canvas or linen texture over the picture: each pixel is multiplied by 1 + `amount` x texture.
///
/// The texture has zero mean and unit standard deviation over the picture, so `amount` is its relative strength and the
/// average brightness is kept (before clipping to 0..1). The fibres are fractal noise (`scale`, `octaves`, `persistence`,
/// `lacunarity`); a weave is a cosine wave of period `spacing` across the columns plus one down the rows, mixed with the
/// fibres by `fibre`. The same texture multiplies every channel; the same `seed` gives the same texture.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The surface: 'paper' (random fibres only), 'canvas' (an even plain weave of threads `spacing` px apart, with the
    /// fibres mixed in) or 'linen' (the same weave, but every thread has its own random strength, so it looks uneven).
    #[param(default = Paper)]
    pub kind: Kind,
    /// How strong the texture is: the standard deviation of the brightness change as a fraction of each pixel's own value
    /// (the texture multiplies the picture, so black stays black and the average brightness is kept). 0 = unchanged, 0.1 is
    /// visible, 0.3 is heavy; values lighten and darken alike, and the result is clipped to 0..1.
    #[param(default = 0.1, unit = fraction, min = 0, max = 1)]
    pub amount: f32,
    /// Size of the coarsest fibres in px, the spacing of the first noise lattice. Larger = broader, softer mottling; small
    /// values give a fine tooth.
    #[param(default = 6.0, unit = px, min = 1, max = 256)]
    pub scale: f32,
    /// How many layers of noise are summed for the fibres, each finer than the last (by `lacunarity`) and weaker (by
    /// `persistence`). 1 = smooth blotches, more = more fine detail. Layers finer than one pixel are dropped.
    #[param(default = 3, unit = count, min = 1, max = 8)]
    pub octaves: i32,
    /// How much weaker each finer layer of fibres is than the one before: each layer's weight is this to the power of its
    /// number. 0 = only the coarsest layer, 1 = every layer equally strong (very rough).
    #[param(default = 0.5, unit = ratio, min = 0, max = 1, advanced)]
    pub persistence: f32,
    /// How much finer each layer of fibres is than the one before: the lattice spacing is divided by this per layer. 1 =
    /// every layer the same size, 2 = each half as coarse, larger = detail drops off faster in size.
    #[param(default = 2.0, unit = ratio, min = 1, max = 4, advanced)]
    pub lacunarity: f32,
    /// The thread pitch of 'canvas' and 'linen' in px: the distance from one thread to the next. Ignored for 'paper'.
    /// Larger = coarser weave.
    #[param(default = 6.0, unit = px, min = 3, max = 64)]
    pub spacing: f32,
    /// For 'canvas' and 'linen', the share of the texture's variance that comes from the random fibres rather than the
    /// weave: 0 = a pure, regular weave, 1 = no weave (the same as 'paper'). Ignored for 'paper'.
    #[param(default = 0.3, unit = fraction, min = 0, max = 1, advanced)]
    pub fibre: f32,
    /// Random seed: the same seed always gives the same texture (fibres and, for 'linen', thread strengths), another seed a
    /// different one.
    #[param(default = 0, unit = count, min = 0, max = 4294967295)]
    pub seed: u32,
}

/// The wave of one axis of the weave: `cos(2 pi p / spacing)` at each of `length` positions `p`, scaled for 'linen' by a
/// strength drawn for the thread (one pitch wide) the position lies in. `axis` keeps the rows' threads apart from the columns'.
fn wave(length: usize, axis: u64, params: &Params) -> Vec<f64> {
    let spacing = f64::from(params.spacing);
    (0..length)
        .map(|at| {
            let position = at as f64;
            let wave = (std::f64::consts::TAU * position / spacing).cos();
            if params.kind == Kind::Linen {
                // constant: stream 2 of the seed's hashes, apart from the fibres' lattices (streams 0 and 1)
                wave * unit(mix(params.seed, &[2, axis, (position / spacing) as u64]))
            } else {
                wave
            }
        })
        .collect()
}

/// The picture with the texture laid over it.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let mut texture = field(
        height,
        width,
        &Fractal {
            spacing: f64::from(params.scale),
            octaves: params.octaves.cast_unsigned(),
            persistence: f64::from(params.persistence),
            lacunarity: f64::from(params.lacunarity),
            seed: params.seed,
            origin: 0.0, // constant: a lattice point on the first pixel
        },
        0,
    );
    if params.kind != Kind::Paper {
        let (down, across) = (wave(height, 0, params), wave(width, 1, params));
        let weave = standardised(
            (0..height * width)
                .map(|at| cast::single(down[at / width] + across[at % width]))
                .collect(),
        );
        let (weave_share, fibre_share) = (
            f64::from(1.0 - params.fibre).sqrt(),
            f64::from(params.fibre).sqrt(),
        );
        texture = standardised(
            weave
                .iter()
                .zip(&texture)
                .map(|(weave, fibres)| cast::single(weave_share * f64::from(*weave) + fibre_share * f64::from(*fibres)))
                .collect(),
        );
    }
    picture.map_at(|row, column, pixel| {
        let gain = 1.0 + params.amount * texture[row * width + column];
        pixel.map(|level| (level * gain).clamp(0.0, 1.0))
    })
}
