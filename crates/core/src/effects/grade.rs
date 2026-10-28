//! `grade`: colour balance (temperature, tint), then exposure, contrast around a pivot, saturation and vibrance, in
//! one pass over the picture. What each step does is documented once, on the Python effect.

use serde::Deserialize;

use crate::ops::color::luminance;
use crate::ops::colorspace::{linear_to_srgb, srgb_to_linear};
use crate::{Image, Picture};

/// The parameters of `grade` (`GradeParams` in Python), validated there.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Params {
    temperature: f32,
    tint: f32,
    exposure: f32,
    contrast: f32,
    pivot: f32,
    saturation: f32,
    vibrance: f32,
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
