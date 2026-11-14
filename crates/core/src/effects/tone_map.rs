//! `tone_map`: HDR-style local tone mapping and shadows and highlights recovery. What it does is documented once, on
//! `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::color::luminance;
use crate::ops::guided::guided_filter;
use crate::ops::length::whole;
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `tone_map` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Light,
    opacity: 1.0,
    inputs: &[],
};

/// What `tone_map` reads around a pixel.
pub(crate) fn footprint(params: &Params) -> Footprint {
    Footprint::Radius(2.0 * f64::from(params.radius))
}

/// How far `tone_map` paints beyond the picture's edge: as far as it reads.
pub(crate) fn spread(params: &Params) -> crate::Spread {
    crate::Spread::of_footprint(footprint(params))
}

/// Luminance below this (about 13 stops under white) counts as this much in the logarithm, so black has a finite
/// logarithm; a guard against log(0), as in Reinhard et al. 2002's small delta.
const LUMINANCE_FLOOR: f32 = 1e-4; // constant: Reinhard et al. 2002's delta

/// HDR tone mapping, shadows and highlights recovery, lifted shadows.
///
/// Compresses the broad brightness differences of the picture and keeps (or boosts) its fine detail.
///
/// Brightness is the base 2 logarithm L of the Rec. 709 luma Y of the sRGB-encoded picture (Y floored at 1e-4), in
/// stops below white. A guided filter (He, Sun and Tang 2010, guided by L itself) over a window of 2 x `radius` + 1 px,
/// edge-preserving by `edge_threshold` stops, splits it into a base B and detail D = L - B (Durand and Dorsey 2002).
/// The new log brightness is `contrast` x (B + `pivot`) - `pivot` + (1 + `amount`) x D, and every pixel's red, green
/// and blue are multiplied by 2^(new - L) = 2^((`contrast` - 1) (B + `pivot`) + `amount` D), so hue and saturation
/// stay and the result is held to 0..1. An even area of luma Y comes out as Y (Y 2^`pivot`)^(`contrast` - 1), which
/// is Y^`contrast` for `pivot` 0. B is an average of values of L, so it never leaves their range: with `amount` 0
/// and `pivot` 0 lowering `contrast` only ever brightens; areas brighter than 2^-`pivot` are darkened.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How much of the broad brightness differences between areas is kept: 1 = all of them (no compression), 0.5 =
    /// halved in stops (a picture with dark areas three stops below its bright ones gets them one and a half apart), 0
    /// = every area brought up to white and only the detail left. Lowering it lightens dark areas the most; white
    /// stays white.
    #[param(default = 0.5, unit = ratio, min = 0, max = 1)]
    pub contrast: f32,
    /// How strongly the fine detail (what is left of the brightness once the broad areas are taken out) is
    /// exaggerated: 0 = kept as it is, 1 = doubled in stops. Raising it brings out texture; it can clip highlights.
    #[param(default = 0.5, unit = ratio, min = 0, max = 5)]
    pub amount: f32,
    /// The brightness, in stops below white, that compression leaves where it is: 0 = white (everything is
    /// lightened, nothing darkened); 2 = a quarter of white's brightness (about 0.25), where areas brighter than it
    /// are darkened and darker ones lightened, recovering highlights as well as shadows. Raising it darkens bright
    /// areas more and lightens dark ones less.
    #[param(default = 0.0, unit = stops, min = 0, max = 8)]
    pub pivot: f32,
    /// Size of what counts as a broad area, in px at the recipe's size (rounded to whole px): the base layer is an
    /// average over a square of 2 x radius + 1 px. Larger treats bigger structures as base and keeps only finer ones
    /// as detail; 0 makes every pixel its own base, a plain global curve (a grey v becomes v^contrast).
    #[param(default = 30.0, unit = px, min = 0, max = 500)]
    pub radius: f32,
    /// The brightness step, in stops, that counts as an edge the base layer must not blur across: steps above it
    /// keep their own side of the base, which avoids halos around them; steps below it are averaged away and count as
    /// detail. Raising it makes the base smoother and more like a plain blur.
    #[param(default = 0.5, unit = stops, min = 0.01, max = 8, advanced)]
    pub edge_threshold: f32,
}

/// The tone-mapped picture.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let log_luma: Vec<f32> = picture
        .pixels()
        .iter()
        .map(|&pixel| luminance(pixel).max(LUMINANCE_FLOOR).log2())
        .collect();
    let epsilon = f64::from(params.edge_threshold).powi(2);
    let base = guided_filter(
        &log_luma,
        &log_luma,
        height,
        width,
        whole(f64::from(params.radius)),
        epsilon,
    );
    picture.map_at(|row, column, pixel| {
        let at = row * width + column;
        let exponent = (params.contrast - 1.0) * (base[at] + params.pivot) + params.amount * (log_luma[at] - base[at]);
        let gain = exponent.exp2();
        pixel.map(|channel| (channel * gain).clamp(0.0, 1.0))
    })
}
