//! `channel_mixer`: each output channel as a weighted sum of the input red, green and blue. What it does is documented
//! once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `channel_mixer` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Colour,
    opacity: 1.0,
    inputs: &[],
};

/// What `channel_mixer` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// Build each output channel as a weighted sum of the input red, green and blue, set by a 3x3 matrix of weights: black
/// and white, infrared, colour swaps.
///
/// The weights are named `<output>_from_<input>`, so a row of the matrix is one output channel: each output is the sum
/// of the three input channels, each times its weight in that row. The defaults are the identity matrix. A row summing
/// to 1 keeps greys at their brightness in that channel. The sum is made on the gamma-encoded values, as in Photoshop's Channel Mixer, not in linear light, and
/// the result is clipped to 0..1.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Weight of input red in output red (-3..3): 1 keeps it as it is (the default), 0 removes it, above 1
    /// amplifies it, a negative weight inverts it.
    #[param(default = 1.0, unit = ratio, min = -3, max = 3)]
    pub red_from_red: f32,
    /// Weight of input green in output red (-3..3): 1 passes it through, 0 leaves it out, a negative weight
    /// subtracts it.
    #[param(default = 0.0, unit = ratio, min = -3, max = 3)]
    pub red_from_green: f32,
    /// Weight of input blue in output red (-3..3): 1 passes it through, 0 leaves it out, a negative weight
    /// subtracts it.
    #[param(default = 0.0, unit = ratio, min = -3, max = 3)]
    pub red_from_blue: f32,
    /// Weight of input red in output green (-3..3): 1 passes it through, 0 leaves it out, a negative weight
    /// subtracts it.
    #[param(default = 0.0, unit = ratio, min = -3, max = 3)]
    pub green_from_red: f32,
    /// Weight of input green in output green (-3..3): 1 keeps it as it is (the default), 0 removes it, above 1
    /// amplifies it, a negative weight inverts it.
    #[param(default = 1.0, unit = ratio, min = -3, max = 3)]
    pub green_from_green: f32,
    /// Weight of input blue in output green (-3..3): 1 passes it through, 0 leaves it out, a negative weight
    /// subtracts it.
    #[param(default = 0.0, unit = ratio, min = -3, max = 3)]
    pub green_from_blue: f32,
    /// Weight of input red in output blue (-3..3): 1 passes it through, 0 leaves it out, a negative weight
    /// subtracts it.
    #[param(default = 0.0, unit = ratio, min = -3, max = 3)]
    pub blue_from_red: f32,
    /// Weight of input green in output blue (-3..3): 1 passes it through, 0 leaves it out, a negative weight
    /// subtracts it.
    #[param(default = 0.0, unit = ratio, min = -3, max = 3)]
    pub blue_from_green: f32,
    /// Weight of input blue in output blue (-3..3): 1 keeps it as it is (the default), 0 removes it, above 1
    /// amplifies it, a negative weight inverts it.
    #[param(default = 1.0, unit = ratio, min = -3, max = 3)]
    pub blue_from_blue: f32,
}

/// The mixed picture.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let matrix = [
        [params.red_from_red, params.red_from_green, params.red_from_blue],
        [params.green_from_red, params.green_from_green, params.green_from_blue],
        [params.blue_from_red, params.blue_from_green, params.blue_from_blue],
    ];
    picture.map(|pixel| {
        matrix.map(|weights| {
            let sum = weights[0] * pixel[0] + weights[1] * pixel[1] + weights[2] * pixel[2];
            sum.clamp(0.0, 1.0)
        })
    })
}
