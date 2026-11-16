//! What the dither tests share: running a picture through the effect and comparing the pixels it gives.

use std::sync::Arc;

use bullterrier_image::effects::dither::Params;
use bullterrier_image::{Error, Image, Options, Photo, Picture, Presets, Recipe, Step};
use serde_json::{Value, json};

use crate::common::pixels::run_picture;

pub(crate) const BLACK: [f32; 3] = [0.0; 3];
pub(crate) const WHITE: [f32; 3] = [1.0; 3];

/// The pixels of `picture`.
pub(crate) fn pixels(picture: &[f32]) -> Vec<[f32; 3]> {
    picture
        .as_chunks::<3>()
        .0
        .iter()
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect()
}

pub(crate) fn run(params: &Params, (height, width, picture): &(usize, usize, Vec<f32>)) -> Result<Vec<f32>, Error> {
    run_picture(params, *height, *width, picture)
}

pub(crate) fn near(pixel: [f32; 3], colour: [f32; 3]) -> bool {
    pixel.iter().zip(colour).all(|(a, b)| (a - b).abs() < 1e-5)
}

/// `method` (with `extra` parameters) on `picture`, black and white in sRGB, given the picture `matrix` as `"m"`.
pub(crate) fn with_matrix(
    method: &str,
    matrix: Option<&Image>,
    extra: &[(&str, Value)],
    picture: &(usize, usize, Vec<f32>),
) -> Result<Vec<f32>, Error> {
    let mut step = Step::new("dither")
        .param("method", method)
        .param("palette", "custom")
        .param("custom", json!(["#000000", "#ffffff"]))
        .param("space", "rgb");
    for (key, value) in extra {
        step = step.param(*key, value.clone());
    }
    let mut photo = Photo::new(Picture::new(picture.0, picture.1, &picture.2)?);
    if let Some(matrix) = matrix {
        photo = photo.with_image("m", Arc::new(matrix.clone()));
    }
    Ok(photo
        .render(&Recipe::new([step]), &Presets::new(), Options::default())?
        .into_values())
}

pub(crate) fn rgb(hex: u32) -> [f32; 3] {
    [hex >> 16, (hex >> 8) & 0xff, hex & 0xff].map(|byte| f32::from(u8::try_from(byte).unwrap_or(0)) / 255.0)
}

/// A grey picture of `rows` x `columns` whose cells are `greys`, row after row.
pub(crate) fn grey_matrix(rows: usize, columns: usize, greys: &[f32]) -> Result<Image, Error> {
    Image::new(
        rows,
        columns,
        &greys.iter().flat_map(|&grey| [grey; 3]).collect::<Vec<_>>(),
    )
}
