//! `grade`: colour balance (temperature, tint), then exposure, contrast around a pivot, saturation and vibrance, in
//! one pass over the picture. What each step does is documented once, on the Python effect; its parameters are
//! declared here.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::color::luminance;
use crate::ops::colorspace::{linear_to_srgb, srgb_to_linear};
use crate::params::Params;
use crate::{Image, Picture};

/// Where `grade` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Colour,
    opacity: 1.0,
    inputs: &[],
};

/// Colour balance (temperature, tint), then exposure, contrast, saturation and vibrance.
#[derive(Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub(crate) struct Params {
    /// Warm-cool shift in stops (-2..2): the difference in stops between the red and the blue gain, shared evenly (red
    /// up and blue down by half of it each). Positive warms the picture (more orange), negative cools it (more blue), 0
    /// leaves the colour balance alone. Use a negative value to correct a picture shot under warm light.
    #[param(default = 0.0, unit = stops, min = -2, max = 2, drivable)]
    pub(crate) temperature: f32,
    /// Green-magenta shift in stops (-1..1): the green gain is multiplied by 2 to the power of minus tint. Positive
    /// adds magenta (less green), negative adds green, 0 leaves the balance alone.
    #[param(default = 0.0, unit = stops, min = -1, max = 1, drivable)]
    pub(crate) tint: f32,
    /// Brightness in photographic stops: +1 doubles every value, -1 halves it.
    #[param(default = 0.0, unit = stops, min = -10, max = 10, drivable)]
    pub(crate) exposure: f32,
    /// How far values are pushed away from the pivot: 1 = unchanged, 2 = twice as far, 0 = flat at the pivot.
    #[param(default = 1.0, unit = ratio, min = 0, max = 5, drivable)]
    pub(crate) contrast: f32,
    /// The brightness (0..1) that contrast leaves where it is; values above it brighten, below it darken.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub(crate) pivot: f32,
    /// Colourfulness: 1 = unchanged, 0 = grey (Rec. 709 brightness kept), above 1 more vivid.
    #[param(default = 1.0, unit = ratio, min = 0, max = 5, drivable)]
    pub(crate) saturation: f32,
    /// How far muted colours are pushed toward full saturation (0..1), after `saturation`: 0 changes nothing, 1 raises
    /// a colour's saturation s to s + s * (1 - s), so a colour at 0.2 goes to 0.36 while one at 0.9 only reaches 0.99.
    /// Greys and fully saturated colours stay as they are; colours are never made duller.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1, drivable)]
    pub(crate) vibrance: f32,
}

/// The graded picture.
pub(crate) fn render(picture: &Picture<'_>, params: &Params) -> Image {
    let balance = colour_balance(params.temperature, params.tint);
    let gain = params.exposure.exp2(); // a stop is a factor of 2 (photographic definition)
    picture.map(|pixel| {
        let balanced = balance.map_or(pixel, |gains| {
            [0, 1, 2].map(|channel| linear_to_srgb(srgb_to_linear(pixel[channel]) * gains[channel]))
        });
        let contrasted = balanced.map(|value| params.pivot + (value * gain - params.pivot) * params.contrast);
        let grey = luminance(contrasted.map(|value| value.clamp(0.0, 1.0)));
        let saturated = contrasted.map(|value| (grey + (value - grey) * params.saturation).clamp(0.0, 1.0));
        vibrance(saturated, params.vibrance)
    })
}

/// The von Kries gains of red, green and blue in linear light, divided by the largest so nothing brightens; none
/// when temperature and tint leave the balance alone.
fn colour_balance(temperature: f32, tint: f32) -> Option<[f32; 3]> {
    if temperature == 0.0 && tint == 0.0 {
        return None;
    }
    let half = temperature / 2.0; // the temperature stops are shared evenly between red and blue
    let gains = [half.exp2(), (-tint).exp2(), (-half).exp2()];
    let largest = gains[0].max(gains[1]).max(gains[2]);
    Some(gains.map(|gain| gain / largest))
}

/// `pixel` with its HSV saturation s raised to s + `amount` * s * (1 - s): the strongest channel is kept and the others
/// move away from it by the same factor, so hue and value stay.
fn vibrance(pixel: [f32; 3], amount: f32) -> [f32; 3] {
    let strongest = pixel[0].max(pixel[1]).max(pixel[2]);
    let weakest = pixel[0].min(pixel[1]).min(pixel[2]);
    let chroma = if strongest > 0.0 { (strongest - weakest) / strongest } else { 0.0 };
    let spread = 1.0 + amount * (1.0 - chroma); // the factor by which saturation grows
    pixel.map(|value| (strongest - spread * (strongest - value)).clamp(0.0, 1.0))
}
