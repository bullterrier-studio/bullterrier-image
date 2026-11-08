//! `color_balance`: a colour pushed into the shadows, the midtones and the highlights. What it does is documented once,
//! on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::color::luminance;
use crate::params::{Color, Params};
use crate::{Image, Inputs, Picture};

/// Where `color_balance` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Colour,
    opacity: 1.0,
    inputs: &[],
};

/// What `color_balance` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Pixel
}

const NEUTRAL: f32 = 0.5; // constant: mid grey, the colour that leaves a range alone

/// Push the shadows, midtones and highlights each toward a colour: split toning, duotone, teal and orange grading.
///
/// Three colour pushes, one for the dark, middle and bright tones, that by default leave brightness alone.
///
/// A pixel's tone is its Rec. 709 luma L of the sRGB-encoded picture. The shadow/midtone and midtone/highlight
/// hand-overs are centred on `balance` / 2 and (1 + `balance`) / 2, each `softness` / 2 wide, and run along the
/// smoothstep s(x) = 3x^2 - 2x^3 of their argument held to 0..1. Its weights are shadows 1 - lower, midtones lower -
/// upper and highlights upper, so black is all shadows, mid grey (at the defaults) all midtones, white all highlights,
/// and the three always add up to 1. Each channel moves by `amount` times the sum, over the three ranges, of the weight
/// times (the range's colour - 0.5); the result is held to 0..1. With `preserve_luminosity` the luma the pushes
/// changed is then restored by adding the same amount to every channel (before the hold).
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The colour the dark parts are pushed toward: '#rrggbb', '#rgb' or [r, g, b] 0..1. Mid grey '#808080' leaves
    /// them alone; a lighter or darker colour than that also lightens or darkens them unless `preserve_luminosity` is
    /// on.
    #[param(default = "#3a1bff")]
    pub shadows: Color,
    /// The colour the middle tones are pushed toward: '#rrggbb', '#rgb' or [r, g, b] 0..1. Mid grey '#808080' leaves
    /// them alone.
    #[param(default = "#808080")]
    pub midtones: Color,
    /// The colour the bright parts are pushed toward: '#rrggbb', '#rgb' or [r, g, b] 0..1. Mid grey '#808080' leaves
    /// them alone.
    #[param(default = "#ff2bd6")]
    pub highlights: Color,
    /// How far each range is pushed toward its colour: 0 = off, 0.2 a hint, 0.6 strong, 1 moves a range by the full
    /// distance from mid grey to its colour.
    #[param(default = 0.3, unit = fraction, min = 0, max = 1, drivable)]
    pub amount: f32,
    /// Where the three ranges sit on the brightness scale (0..1): 0.5 splits it evenly, shadows below a quarter and
    /// highlights above three quarters. Raising it lets the shadows reach higher up and shrinks the highlights;
    /// lowering it does the reverse.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1, drivable)]
    pub balance: f32,
    /// How far neighbouring ranges overlap (0.01..1): 1 blends each into the next over half the brightness scale, with
    /// no pure midtones except at the centre; smaller makes sharper hand-overs and a wider band of pure midtones.
    #[param(default = 1.0, unit = fraction, min = 0.01, max = 1)]
    pub softness: f32,
    /// Keep each pixel's Rec. 709 luma while pushing colours: on, the brightness a push adds or removes is taken off
    /// or put back on all three channels, so only the colour changes (Photoshop's 'Preserve Luminosity'); off, a push
    /// toward a lighter or darker colour also lightens or darkens. Where a channel reaches 0 or 1 the luma is only kept
    /// as far as the clipping allows.
    #[param(default = true, advanced)]
    pub preserve_luminosity: bool,
}

/// The picture with each range pushed toward its colour.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let centre = params.balance / 2.0;
    let width = params.softness / 2.0;
    let pushes =
        [params.shadows, params.midtones, params.highlights].map(|colour| colour.0.map(|value| value - NEUTRAL));
    picture.map(|pixel| {
        let luma = luminance(pixel);
        let lower = handover(luma, centre, width);
        let upper = handover(luma, centre + 0.5, width); // the next hand-over is half the scale on
        let weights = [1.0 - lower, lower - upper, upper];
        let mut out = pixel;
        for (weight, push) in weights.into_iter().zip(&pushes) {
            for (value, shift) in out.iter_mut().zip(push) {
                *value += params.amount * weight * shift;
            }
        }
        if params.preserve_luminosity {
            let restore = luma - luminance(out);
            out = out.map(|value| value + restore);
        }
        out.map(|value| value.clamp(0.0, 1.0))
    })
}

/// 0 below `centre` - `width` / 2, 1 above `centre` + `width` / 2, and a smoothstep in between.
fn handover(luma: f32, centre: f32, width: f32) -> f32 {
    let x = ((luma - centre) / width + 0.5).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x) // constant: the cubic Hermite smoothstep, 3x^2 - 2x^3
}
