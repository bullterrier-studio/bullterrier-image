//! `sharpen`: an unsharp mask. What it does is documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::blur;
use crate::ops::edge::EdgeOrConstant;
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `sharpen` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Light,
    opacity: 1.0,
    inputs: &[],
};

/// What `sharpen` reads around a pixel.
pub(crate) fn footprint(params: &Params) -> Footprint {
    Footprint::reach(f64::from(params.radius * params.truncate), params.edge == EdgeOrConstant::Wrap)
}

/// An unsharp mask: the difference between the picture and a blurred copy is added back, scaled by `amount`.
///
/// Sharpen, or add clarity: add `amount` times the detail that a `radius` px Gaussian blur removes. Detail smaller
/// than `threshold` is left as it is; each colour channel is sharpened on its own, and the result is kept within 0..1.
/// A small `radius` brings out fine texture; a large one (preset `clarity`: 30 px) is local contrast, which
/// exaggerates how each area differs from its surroundings so features stand out even when the whole face sits in a
/// narrow band of brightness. Put that before effects that turn tone into marks (ascii, halftone), which are blind to
/// small tonal differences.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Gaussian sigma of the blurred copy in px at the recipe's size: small radii sharpen fine texture, large ones add
    /// local contrast around coarse edges; 0 leaves the picture unchanged.
    #[param(default = 2.0, unit = px, min = 0, max = 100)]
    pub radius: f32,
    /// How much of the detail is added back: 0 = off, 1 = the full difference from the blurred copy; above 1 edges
    /// overshoot and halos show.
    #[param(default = 0.8, unit = ratio, min = 0, max = 5, drivable)]
    pub amount: f32,
    /// The smallest difference (0..1, per channel) from the blurred copy that is sharpened: smaller differences are
    /// left alone, which keeps grain and smooth skin from being sharpened; 0 sharpens everything.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1)]
    pub threshold: f32,
    /// Where the blur's Gaussian is cut off, in sigmas: the blur reaches truncate x radius px. Larger is more exact and
    /// slower.
    #[param(default = 4.0, unit = ratio, min = 1, max = 10, advanced)]
    pub truncate: f32,
    /// What the blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored
    /// about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge), 'constant'
    /// (`constant_value`, so the border stands out from it).
    #[param(default = Reflect, advanced)]
    pub edge: EdgeOrConstant,
    /// The brightness (0..1) assumed beyond the edge when `edge` is 'constant'; ignored otherwise.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1, advanced)]
    pub constant_value: f32,
}

/// The picture plus `amount` times the detail the blur takes away, where that detail is at least `threshold`.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let edge = params.edge.extension(params.constant_value);
    let blurred = blur::gaussian_filter(picture, f64::from(params.radius), f64::from(params.truncate), edge);
    let width = picture.width();
    picture.map_at(|row, column, pixel| {
        let blur = blurred[row * width + column];
        [0, 1, 2].map(|channel| {
            let detail = pixel[channel] - blur[channel];
            let kept = if detail.abs() >= params.threshold { detail } else { 0.0 };
            params.amount.mul_add(kept, pixel[channel]).clamp(0.0, 1.0)
        })
    })
}
