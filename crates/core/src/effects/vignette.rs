//! `vignette`: the picture fades toward a colour at its edges and corners, the centre left alone. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::params::{Color, Params};
use crate::{Image, Inputs, Picture};

/// Where `vignette` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Light,
    opacity: 1.0,
    inputs: &[],
};

/// What `vignette` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// Fade the picture toward `color` from `radius` out to its corners, the centre untouched.
///
/// A pixel's distance from the centre is 0 in the middle and 1 at a corner; the contours of equal distance are
/// superellipses through the corners (`roundness` 1: an ellipse, 0: a rectangle). From `radius` the pixel is mixed
/// toward `color`, rising over `feather` along a smoothstep to `amount`.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How far the edges are mixed toward the colour where the vignette is fullest: 0 = off, 0.5 = halfway, 1 = the
    /// colour itself at the corners.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1, drivable)]
    pub amount: f32,
    /// Where the vignette starts, as a fraction of the distance from the centre to a corner: everything inside is
    /// untouched. Raising it clears more of the picture; 1 leaves even the corners alone.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub radius: f32,
    /// How far beyond the radius the vignette takes to reach full strength, as a fraction of the distance from the
    /// centre to a corner (a smooth S-curve). 0 = a hard edge; raising it softens the transition.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub feather: f32,
    /// The shape of the vignette's contours: 1 = an ellipse that follows the picture's proportions, 0 = a rectangle,
    /// which darkens the middle of each edge as much as a corner; in between, a rounded rectangle.
    #[param(default = 1.0, unit = fraction, min = 0, max = 1)]
    pub roundness: f32,
    /// The colour the edges fade to: '#rrggbb', '#rgb' or [r, g, b] 0..1. Black darkens, white lightens.
    #[param(default = "#000000")]
    pub color: Color,
}

/// How far a pixel centre is from the middle along one axis of `length` pixels: 0 in the middle, near 1 at the edge.
#[expect(clippy::cast_precision_loss, reason = "a picture side is far below 2**52")]
fn offsets(length: usize) -> Vec<f64> {
    (0..length)
        .map(|index| (2.0 * index as f64 + 1.0 - length as f64).abs() / length as f64)
        .collect()
}

/// The picture faded toward the colour.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (amount, radius, feather) = (f64::from(params.amount), f64::from(params.radius), f64::from(params.feather));
    // The norm with exponent p = 2 / roundness, scaled so that a corner is at 1 (p = infinity, a rectangle, is the
    // larger offset). Dividing by the larger offset first keeps the power from underflowing when p is large.
    let exponent = (params.roundness > 0.0).then(|| 2.0 / f64::from(params.roundness));
    let (down, across) = (offsets(picture.height()), offsets(picture.width()));
    let tint = params.color.0.map(f64::from);
    #[expect(clippy::cast_possible_truncation, reason = "the mix is made in f64 and stored in the f32 picture")]
    picture.map_at(|row, column, pixel| {
        let (far, near) = (down[row].max(across[column]), down[row].min(across[column]));
        let distance = match exponent {
            Some(exponent) if far > 0.0 => far * f64::midpoint((near / far).powf(exponent), 1.0).powf(exponent.recip()),
            Some(_) => 0.0, // the middle pixel of a picture odd on both sides
            None => far,
        };
        let t = if feather > 0.0 {
            ((distance - radius) / feather).clamp(0.0, 1.0)
        } else {
            f64::from(u8::from(distance > radius))
        };
        // constant: 3t^2 - 2t^3 is the standard smoothstep Hermite curve
        let shade = amount * t * t * (3.0 - 2.0 * t);
        [0, 1, 2].map(|channel| (f64::from(pixel[channel]) * (1.0 - shade) + tint[channel] * shade) as f32)
    })
}
