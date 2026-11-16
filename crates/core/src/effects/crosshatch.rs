//! `crosshatch`: layers of straight lines whose weight carries tone. What it does is documented once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Input, Meta};
use crate::ops::color::luminance;
use crate::ops::hatch::Lines;
use crate::ops::length::float;
use crate::ops::par;
use crate::params::{Color, Params};
use crate::{Footprint, Image, Inputs, Picture};

/// Where `crosshatch` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Stylised,
    opacity: 1.0,
    inputs: &[Input::Place],
};

/// A pixel reads only its own tone; where the lines lie is told by the canvas, not by the picture.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Pixel
}

/// Layers of straight lines whose weight carries tone: pen hatching in ink on paper.
///
/// `layers` families of parallel lines `spacing` px apart, the first running `angle` degrees below the x axis and the
/// others turned by equal steps of a half turn over the layers (4 layers: `angle` and 45, 90 and 135 degrees further), all
/// anchored at the picture's top left corner. The darkness at a pixel is how far its luminance lies from the `paper`'s
/// towards the `color`'s. The first layer is drawn as heavy as the darkness asks, up to `thickness` of the spacing; what it
/// leaves is asked of the next layer, and so on, each as if printed over the last at random: the ink covers the tone
/// exactly (the mean ink is the tone) up to `1 - (1 - thickness)^layers`, and a darker tone prints at that and no
/// darker. So light areas show one layer, and every layer more joins in as the tone deepens. A line's edge is a ramp one
/// px wide.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How many families of lines: 1 = one direction, more = a crossed net in the darks. More layers print darker tones.
    #[param(default = 4, unit = count, min = 1, max = 6)]
    pub layers: i32,
    /// Direction of the first layer in degrees, clockwise on screen from the x axis (90 is downward).
    #[param(default = 45.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub angle: f32,
    /// Distance between neighbouring lines of a layer in px. Larger = fewer, coarser lines.
    #[param(default = 6.0, unit = px, min = 2, max = 100)]
    pub spacing: f32,
    /// The widest a line gets, as a share of `spacing`: the darkest tone one layer can print (a tone beyond it is left to
    /// the next layers). Smaller = finer lines and more layers needed for the darks.
    #[param(default = 0.5, unit = fraction, min = 0.05, max = 1)]
    pub thickness: f32,
    /// The ink.
    #[param(default = "#000000")]
    pub color: Color,
    /// The paper between lines. Tones darker than it get ink.
    #[param(default = "#ffffff")]
    pub paper: Color,
}

/// The hatching.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let place = inputs.place(picture);
    let spacing = f64::from(params.spacing);
    let thickness = f64::from(params.thickness);
    let layers = usize::try_from(params.layers).unwrap_or(1);
    let turn = 180.0 / float(layers); // constant: the layers share a half turn, a line looking the same turned round
    let families: Vec<Lines> = (0..layers)
        .map(|k| Lines::new(f64::from(params.angle) + turn * float(k), spacing))
        .collect();
    let (paper, color) = (params.paper.0, params.color.0);
    let paper_light = f64::from(luminance(paper));
    let reach = f64::from(luminance(color)) - paper_light;
    let pixels = picture.pixels();
    let mut out = vec![[0.0; 3]; height * width];
    par::rows(&mut out, width, |top, band| {
        for (offset, line) in band.chunks_exact_mut(width).enumerate() {
            let row = top + offset;
            let y = float(place.first_row + row) + 0.5;
            for (column, pixel) in line.iter_mut().enumerate() {
                let darkness = if reach.abs() < f64::EPSILON {
                    0.0
                } else {
                    ((f64::from(luminance(pixels[row * width + column])) - paper_light) / reach).clamp(0.0, 1.0)
                };
                let x = float(column) + 0.5;
                // `bare` is the share of the pixel no layer has inked, `left` the share of paper the tone still has to cover
                let (mut bare, mut left) = (1.0, 1.0);
                for lines in &families {
                    if left <= f64::EPSILON {
                        break;
                    }
                    let weight = (1.0 - (1.0 - darkness) / left).clamp(0.0, thickness);
                    left *= 1.0 - weight;
                    bare *= 1.0 - lines.cover(lines.across(x, y), weight * spacing);
                }
                #[expect(clippy::cast_possible_truncation, reason = "a share")]
                let cover = (1.0 - bare) as f32;
                *pixel = [0, 1, 2].map(|c| paper[c] + (color[c] - paper[c]) * cover);
            }
        }
    });
    Image::from_pixels(height, width, out)
}
