//! `gaussian_blur`: the picture softened by a Gaussian of a chosen radius. What it does is documented once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::blur;
use crate::ops::edge::EdgeOrConstant;
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `gaussian_blur` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Light,
    opacity: 1.0,
    inputs: &[],
};

/// Soften the picture with a Gaussian blur of standard deviation `radius` px.
///
/// Each colour channel is blurred on its own. Every pixel becomes a weighted average of its neighbours, the weights a
/// bell curve: the blur keeps flat areas flat and never leaves the picture's range.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The Gaussian's standard deviation (sigma) in px at the recipe's size: larger is softer and spreads detail
    /// further; 0 leaves the picture unchanged.
    #[param(default = 4.0, unit = px, min = 0, max = 500, drivable)]
    pub radius: f32,
    /// Where the Gaussian is cut off, in sigmas: the blur reaches truncate x radius px. Larger is more exact and
    /// slower; below 3 the blur turns visibly boxy.
    #[param(default = 4.0, unit = ratio, min = 1, max = 10, advanced)]
    pub truncate: f32,
    /// What the blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored
    /// about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge), 'constant'
    /// (`constant_value`, so the edge fades toward it).
    #[param(default = Reflect, advanced)]
    pub edge: EdgeOrConstant,
    /// The brightness (0..1, every channel) assumed beyond the edge when `edge` is 'constant': 0 darkens the edge, 1
    /// lightens it; ignored otherwise.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1, advanced)]
    pub constant_value: f32,
}

/// The blurred picture.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let edge = params.edge.extension(params.constant_value);
    let pixels = blur::gaussian_filter(picture, f64::from(params.radius), f64::from(params.truncate), edge);
    Image::from_pixels(picture.height(), picture.width(), pixels)
}
