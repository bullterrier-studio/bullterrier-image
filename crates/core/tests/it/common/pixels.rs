//! Pixels in, pixels out: an effect applied to a picture of the test's own.

use bullterrier_image::params::EffectParams;
use bullterrier_image::{Error, Inputs, Layer, Picture, apply};
use serde_json::Value;

/// `params` applied to `pixels` (a picture one pixel high).
pub(crate) fn run<P: EffectParams>(params: &P, pixels: &[[f32; 3]]) -> Result<Vec<[f32; 3]>, Error> {
    let values: Vec<f32> = pixels.iter().flatten().copied().collect();
    let image = apply(
        P::EFFECT,
        &Picture::new(1, pixels.len(), &values)?,
        Value::Object(params.values()),
        &Inputs::default(),
    )?;
    Ok(image
        .into_values()
        .as_chunks::<3>()
        .0
        .iter()
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect())
}

/// `params` applied to a `height` x `width` picture whose every channel is `fill`: for an effect that reads only the
/// picture's size. Row after row.
pub(crate) fn run_sized<P: EffectParams>(
    params: &P,
    height: usize,
    width: usize,
    fill: f32,
) -> Result<Vec<[f32; 3]>, Error> {
    let values = vec![fill; height * width * 3];
    let image = apply(
        P::EFFECT,
        &Picture::new(height, width, &values)?,
        Value::Object(params.values()),
        &Inputs::default(),
    )?;
    Ok(image
        .into_values()
        .as_chunks::<3>()
        .0
        .iter()
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect())
}

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

/// `params` applied to the `height` x `width` picture `values`, with `field` (an `height * width` map, 0..1) for the
/// field parameter called `parameter`.
pub(crate) fn run_with_field<P: EffectParams>(
    params: &P,
    parameter: &str,
    field: &[f32],
    height: usize,
    width: usize,
    values: &[f32],
) -> Result<Vec<f32>, Error> {
    let layer = Layer::new(height, width, field.to_vec())?;
    let image = apply(
        P::EFFECT,
        &Picture::new(height, width, values)?,
        Value::Object(params.values()),
        &Inputs::default().with_field(parameter, &layer),
    )?;
    Ok(image.into_values())
}

/// Channel `which` (0 red, 1 green, 2 blue) of the picture `values`, three channels a pixel.
pub(crate) fn channel(values: &[f32], which: usize) -> Vec<f32> {
    values.as_chunks::<3>().0.iter().map(|pixel| pixel[which]).collect()
}

/// Whether `pixel` is `expected` to within a float's last digits (an exact comparison of floats is a lint).
pub(crate) fn is_pixel(pixel: [f32; 3], expected: [f32; 3]) -> bool {
    pixel
        .iter()
        .zip(expected)
        .all(|(level, wanted)| (level - wanted).abs() < 1e-6)
}
