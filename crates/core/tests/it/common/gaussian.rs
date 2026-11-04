//! What the tests of the effects built on a Gaussian blur share: an effect applied to a whole picture, a picture that
//! varies, and scipy's weights worked out from their definition.

#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the reach of a kernel is a small whole number"
)]

use bullterrier_image::params::EffectParams;
use bullterrier_image::{Error, Inputs, Picture, apply};
use serde_json::Value;

/// `params` applied to the `height` x `width` picture `values` (row after row, three channels a pixel).
pub(crate) fn run_picture<P: EffectParams>(
    params: &P,
    height: usize,
    width: usize,
    values: &[f32],
) -> Result<Vec<f32>, Error> {
    let image = apply(
        P::EFFECT,
        &Picture::new(height, width, values)?,
        Value::Object(params.values()),
        &Inputs::default(),
    )?;
    Ok(image.into_values())
}

/// A different value in nearly every channel of every pixel, 0..1.
pub(crate) fn varied(height: usize, width: usize) -> Vec<f32> {
    (0..height * width * 3)
        .map(|index| f32::from(u16::try_from((index * 7919 + index / 13) % 1000).unwrap_or(0)) / 1000.0)
        .collect()
}

/// scipy's weights of a Gaussian of `sigma` at distances -reach..=reach (exp(-d^2 / 2 sigma^2), reach 4 sigma rounded,
/// summing to 1), and the index of the middle one.
pub(crate) fn weights(sigma: f64) -> (Vec<f64>, usize) {
    let reach = (4.0 * sigma + 0.5).floor() as i32;
    let all: Vec<f64> = (-reach..=reach)
        .map(|distance| (-f64::from(distance * distance) / (2.0 * sigma * sigma)).exp())
        .collect();
    let total: f64 = all.iter().sum();
    (all.iter().map(|weight| weight / total).collect(), reach as usize)
}
