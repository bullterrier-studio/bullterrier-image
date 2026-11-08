//! `highpass`: only the fine detail of the picture, centred on mid-grey. What it does is documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::blur;
use crate::ops::edge::Edge;
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `highpass` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Light,
    opacity: 1.0,
    inputs: &[],
};

/// What `highpass` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// The level the detail is centred on.
const MID_GREY: f32 = 0.5; // constant: halfway between black and white

/// A high-pass filter: the picture minus a blurred copy of it, shown around mid-grey.
///
/// Keep only the detail finer than `radius` px: the picture minus its Gaussian blur, centred on mid-grey. A flat area
/// becomes mid-grey, an edge a dark line on its dark side and a bright one on its bright side. Each colour channel is
/// filtered on its own and the result is kept within 0..1.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Gaussian sigma of the blurred copy in px at the recipe's size: detail finer than this is kept, coarser tones
    /// are removed. Small radii keep only the finest texture, large ones keep edges and local contrast; 0 keeps nothing
    /// but mid-grey.
    #[param(default = 4.0, unit = px, min = 0, max = 100)]
    pub radius: f32,
    /// Where the blur's Gaussian is cut off, in sigmas: the blur reaches truncate x radius px. Larger is more exact and
    /// slower.
    #[param(default = 4.0, unit = ratio, min = 1, max = 10, advanced)]
    pub truncate: f32,
    /// What the blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored
    /// about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The picture minus its blur, around mid-grey.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let blurred = blur::gaussian_filter(
        picture,
        f64::from(params.radius),
        f64::from(params.truncate),
        params.edge.extension(),
    );
    let width = picture.width();
    picture.map_at(|row, column, pixel| {
        let blur = blurred[row * width + column];
        [0, 1, 2].map(|channel| (pixel[channel] - blur[channel] + MID_GREY).clamp(0.0, 1.0))
    })
}
