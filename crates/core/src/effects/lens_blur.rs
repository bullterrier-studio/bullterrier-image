//! `lens_blur`: the picture as an out-of-focus lens draws it, every point spread into a disc. What it does is documented
//! once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::color::luminance;
use crate::ops::convolve::{Kernel, convolve};
use crate::ops::edge::Edge;
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// The channel of a convolved sample that carries the weight, after red, green and blue.
const WEIGHT: usize = 3; // constant: the fourth channel

/// Where `lens_blur` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Light,
    opacity: 1.0,
    inputs: &[],
};

/// What `lens_blur` reads around a pixel.
pub(crate) fn footprint(params: &Params) -> Footprint {
    Footprint::reach(f64::from(params.radius), params.edge == Edge::Wrap)
}

/// Blur the picture over a disc of `radius` px, as an out-of-focus lens does, favouring bright pixels.
///
/// Every pixel becomes the average of the pixels within `radius` px of it (one on the rim counts for the share of it
/// inside the disc, a linear ramp one pixel wide), each weighted by 1 + `highlights` x its brightness. Flat areas stay
/// flat and the result never leaves the picture's own range. The cost hardly grows with the radius.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The radius of the disc in px at the recipe's size: larger is further out of focus and spreads each point into a
    /// bigger circle; 0 leaves the picture unchanged.
    #[param(default = 8.0, unit = px, min = 0, max = 100)]
    pub radius: f32,
    /// How much brighter pixels count in the average: a pixel of brightness b (0..1) counts 1 + highlights x b times as
    /// much as a black one, so with 0 every pixel counts the same (a plain average), and raising it turns bright lights
    /// into distinct, luminous discs (bokeh) while dark areas hardly bleed into them.
    #[param(default = 0.0, unit = ratio, min = 0, max = 50)]
    pub highlights: f32,
    /// What the blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored
    /// about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The picture blurred over the disc.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    if params.radius == 0.0 {
        return picture.map(|pixel| pixel);
    }
    let radius = f64::from(params.radius);
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a reach in px, small and positive"
    )]
    let reach = radius.ceil() as usize;
    // A pixel's weight is the share of it inside the disc, estimated by a linear ramp one pixel wide across the rim (the
    // usual anti-aliased edge: 1 half a pixel inside, 0 half a pixel outside, 1/2 on the rim itself).
    let disc = Kernel::from_fn((reach, reach), |down, right| {
        (radius + 0.5 - down.hypot(right)).clamp(0.0, 1.0)
    });
    let edge = params.edge.extension();
    if params.highlights == 0.0 {
        return Image::from_pixels(height, width, convolve(picture.pixels(), height, width, &disc, edge));
    }
    // The average of the pixels, each counted by its weight: the weighted pixels summed, over the weights summed. Both
    // sums are one convolution, the weight carried as a fourth channel.
    let weighted: Vec<[f32; WEIGHT + 1]> = picture
        .pixels()
        .iter()
        .map(|&pixel| {
            let weight = 1.0 + params.highlights * luminance(pixel);
            [pixel[0] * weight, pixel[1] * weight, pixel[2] * weight, weight]
        })
        .collect();
    let (low, high) = picture
        .values()
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(low, high), &value| {
            (low.min(value), high.max(value))
        });
    let pixels = convolve(&weighted, height, width, &disc, edge)
        .into_iter()
        .map(|sums| {
            // a mean of the picture's values lies within their range; the division's rounding is held to it
            [0, 1, 2].map(|channel| (sums[channel] / sums[WEIGHT]).clamp(low, high))
        })
        .collect();
    Image::from_pixels(height, width, pixels)
}
