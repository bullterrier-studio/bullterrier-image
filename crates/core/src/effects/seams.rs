//! `seams`: a solid line or band along every border between the pieces of a partition. What it does is documented once,
//! on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Input, Meta};
use crate::ops::pieces::line_coverage;
use crate::params::{Color, Params};
use crate::{Canvas, Footprint};
use crate::{Image, Inputs, Picture};

/// Where `seams` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Stylised,
    opacity: 1.0,
    inputs: &[Input::Partition],
};

/// What `seams` reads around a pixel: the borders as far as half the line reaches, and the row beside each border pixel
/// that makes it one.
pub(crate) fn footprint(params: &Params, _canvas: &Canvas) -> Footprint {
    Footprint::Radius(f64::from(params.width) / 2.0 + 1.0)
}

/// Draw a solid line or band along every border between the pieces of a partition.
///
/// The line is one flat colour, centred on each border and anti-aliased along its edges; the picture's own border is not
/// a border. The pieces come from the step's `partition` key, which names a region of the recipe: a partition (grid,
/// shards, cuts, svg or bands) or a selection such as the subject (counted as two pieces, what it chooses and what it
/// does not), which outlines its hard edge. A partition of a single piece has no border: the picture is unchanged.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Width of the line in px at the recipe's size, centred on each border: 0 draws nothing, 1 is a hairline, 10 or more
    /// a band. Raising it widens the line.
    #[param(default = 2.0, unit = px, min = 0, max = 48)]
    pub width: f32,
    /// Colour of the line, solid: '#rrggbb', '#rgb' or three numbers from 0 to 1.
    #[param(default = "#ffffff")]
    pub color: Color,
}

/// The picture with the line laid along the borders.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    // `Inputs::declared_by` refuses an offer that lacks a declared input, so this branch is out of reach: `render` cannot
    // return an error, and the partition cannot be read without an Option
    let Some(partition) = inputs.partition() else {
        return Image::from_pixels(height, width, picture.pixels().to_vec());
    };
    let cover = line_coverage(partition.values(), (height, width), params.width);
    let tint = params.color.0;
    picture.map_at(|row, column, pixel| {
        let cover = cover[row * width + column];
        [0, 1, 2].map(|channel| pixel[channel] + (tint[channel] - pixel[channel]) * cover)
    })
}
