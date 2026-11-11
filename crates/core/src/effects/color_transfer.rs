//! `color_transfer`: give the photo the colour mood of a reference picture. What it does is documented once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::colorspace::{oklab_to_rgb, rgb_to_oklab};
use crate::params::{Params, PictureName};
use crate::{Footprint, Image, Inputs, Picture};

/// Where `color_transfer` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Colour,
    opacity: 1.0,
    inputs: &[],
};

/// What `color_transfer` reads around a pixel: the photo's own mean and spread, so any pixel depends on the whole.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// The smallest spread of a photo's channel that is scaled up, in Oklab units.
const MIN_SPREAD: f64 = 0.002; // constant: half an 8-bit step of Oklab lightness; less is rounding noise, not picture

/// Give the photo the colour mood of a reference picture, so the two look alike.
///
/// Reinhard's statistics transfer, in Oklab: each of Oklab's three channels (lightness L, green-red a, blue-yellow b)
/// is shifted and scaled so that its mean and standard deviation over the photo become the reference's, then the
/// result is mixed with the original by `amount` (as sRGB levels).
/// A channel of the photo flatter than 0.002 is scaled as if it had that spread, so a flat photo or reference never
/// divides by zero: a flat photo takes the reference's mean, a flat reference flattens the channel to its mean. Where
/// the reference has transparency its pixels count in proportion to their alpha, so a fully transparent pixel is
/// ignored; a reference with no visible pixel leaves the photo as it is, as no reference does. Colours the transfer
/// pushes outside the sRGB gamut are clipped to it.
///
/// The mean and spread are taken over the whole photo and the whole reference, so the transfer is one global grade; to
/// grade only part of the photo, give the step a `mask`.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The picture whose colours the photo takes on: the path of an image file in a recipe file (relative to the file)
    /// or in `fx`, or, in Python, a Picture or an array; in Rust, the name of a picture the photo was given or the path
    /// of an image file. Pixels it makes transparent are ignored. None leaves the photo as it is.
    #[param()]
    pub reference: PictureName,
    /// How far the photo moves toward the reference's colour mood, 0..1: 0 keeps the photo, 1 matches the reference's
    /// mean and spread in every channel, in between mixes the two. Raising it makes the photo look more like the reference.
    #[param(default = 1.0, unit = fraction, min = 0, max = 1)]
    pub amount: f32,
    /// True: only the colours change; every pixel keeps its own brightness, so light and shadow stay as shot. False:
    /// brightness and contrast are matched to the reference too, so a dark photo with a bright reference gets brighter.
    #[param(default = false)]
    pub preserve_luminance: bool,
}

/// The mean and standard deviation of each Oklab channel.
struct Spread {
    mean: [f64; 3],
    deviation: [f64; 3],
}

/// The weighted spread of `pixels` (each an Oklab colour and a weight), or none if the weights add up to nothing.
fn spread(pixels: impl Iterator<Item = ([f32; 3], f64)> + Clone) -> Option<Spread> {
    let total: f64 = pixels.clone().map(|(_, weight)| weight).sum();
    if total <= 0.0 {
        return None;
    }
    let mut mean = [0.0; 3];
    for (lab, weight) in pixels.clone() {
        for (sum, level) in mean.iter_mut().zip(lab) {
            *sum += weight * f64::from(level) / total;
        }
    }
    let mut deviation = [0.0; 3];
    for (lab, weight) in pixels {
        for ((sum, level), centre) in deviation.iter_mut().zip(lab).zip(mean) {
            *sum += weight * (f64::from(level) - centre).powi(2) / total;
        }
    }
    Some(Spread {
        mean,
        deviation: deviation.map(f64::sqrt),
    })
}

/// The photo with the reference's colour mood; the photo itself without a reference.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let reference = inputs.image("reference").and_then(|asset| {
        let alpha = asset.alpha();
        let pixels = asset.as_picture().pixels().iter().enumerate();
        spread(pixels.map(|(at, &pixel)| (rgb_to_oklab(pixel), alpha.map_or(1.0, |alpha| f64::from(alpha[at])))))
    });
    let own = spread(picture.pixels().iter().map(|&pixel| (rgb_to_oklab(pixel), 1.0)));
    let (Some(target), Some(own)) = (reference, own) else {
        return picture.map(|pixel| pixel);
    };
    #[expect(clippy::cast_possible_truncation, reason = "Oklab levels of an f32 picture")]
    let (target_mean, own_mean) = (
        target.mean.map(|level| level as f32),
        own.mean.map(|level| level as f32),
    );
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a ratio of Oklab spreads, for an f32 picture"
    )]
    let gain: [f32; 3] =
        std::array::from_fn(|channel| (target.deviation[channel] / own.deviation[channel].max(MIN_SPREAD)) as f32);
    picture.map(|pixel| {
        let lab = rgb_to_oklab(pixel);
        let moved: [f32; 3] = std::array::from_fn(|channel| {
            if channel == 0 && params.preserve_luminance {
                lab[0]
            } else {
                target_mean[channel] + gain[channel] * (lab[channel] - own_mean[channel])
            }
        });
        // mixed as sRGB levels, so that amount 0 returns the pixel bit for bit
        let moved = oklab_to_rgb(moved).map(|level| level.clamp(0.0, 1.0));
        std::array::from_fn(|channel| pixel[channel] + params.amount * (moved[channel] - pixel[channel]))
    })
}
