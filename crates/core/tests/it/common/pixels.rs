//! Pixels in, pixels out: an effect applied to a picture one pixel high.

use bullterrier_image::params::EffectParams;
use bullterrier_image::{Error, Inputs, Picture, apply};
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
