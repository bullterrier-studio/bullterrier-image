//! `levels`: black point, white point and midtone gamma, as in Photoshop's and Lightroom's Levels. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::tone::Remap;
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `levels` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Colour,
    opacity: 1.0,
    inputs: &[],
};

/// What `levels` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Pixel
}

/// Remap tones with an input black point, white point and midtone `gamma`, then an output range: contrast stretch,
/// faded blacks, or the negative (invert).
///
/// Each channel is normalised between `black` and `white` and clipped to 0..1, raised to `1 / gamma`, and placed
/// between `output_black` and `output_white`. The defaults change nothing.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The input black point (0..1): values at or below it become the output black. Raising it darkens shadows and
    /// adds contrast.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1)]
    pub black: f32,
    /// The input white point (0..1): values at or above it become the output white. Lowering it brightens highlights
    /// and adds contrast. At or below `black` the picture is cut at `black` into the two output ends.
    #[param(default = 1.0, unit = fraction, min = 0, max = 1)]
    pub white: f32,
    /// The midtone gamma: 1 = linear, above 1 brightens the midtones, below 1 darkens them; black and white stay where
    /// they are (output = input ** (1 / gamma), as in Photoshop).
    #[param(default = 1.0, unit = ratio, min = 0.1, max = 10)]
    pub gamma: f32,
    /// The darkest value that can be output (0..1): raising it lifts the blacks to a matte grey, the faded-film look
    /// (0.1 = gentle, 1 = everything white). Above `output_white` the tones are reversed.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1, drivable)]
    pub output_black: f32,
    /// The brightest value that can be output (0..1): lowering it dims the whites.
    #[param(default = 1.0, unit = fraction, min = 0, max = 1)]
    pub output_white: f32,
}

/// The remapped picture.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let remap = Remap::new(
        params.black,
        params.white,
        params.gamma,
        params.output_black,
        params.output_white,
    );
    picture.map(|pixel| pixel.map(|value| remap.apply(value)))
}
