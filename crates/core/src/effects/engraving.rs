//! `engraving`: one family of lines whose width carries tone, bent round the picture's forms. What it does is documented
//! once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Input, Meta};
use crate::ops::blur::{gaussian_filter_plane, is_fourier};
use crate::ops::color::luminance;
use crate::ops::edge::Extension;
use crate::ops::hatch::{Lines, Tone};
use crate::ops::length::float;
use crate::ops::par;
use crate::params::{Color, Params};
use crate::{Footprint, Image, Inputs, Picture};

/// Where `engraving` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Stylised,
    opacity: 1.0,
    inputs: &[Input::Place],
};

const TRUNCATE: f64 = 4.0; // constant: scipy's default reach for a Gaussian

/// Straight lines read only the pixel; a `bend` reads the picture smoothed, as far as the Gaussian reaches.
pub(crate) fn footprint(params: &Params) -> Footprint {
    if params.bend <= 0.0 {
        return Footprint::Pixel;
    }
    let sigma = f64::from(params.smooth);
    // constant: the kernel reaches int(truncate x sigma + 0.5) px
    Footprint::Radius(TRUNCATE * sigma + 1.0).transformed(is_fourier(sigma, TRUNCATE))
}

/// Lines whose width carries tone and whose course follows the picture's forms, as on a banknote.
///
/// Parallel lines `spacing` px apart run `angle` degrees below the x axis, anchored at the picture's top left corner. A line
/// is as wide as the darkness asks, a share of the spacing: darkness is how far the pixel's luminance lies from the `paper`'s
/// towards the `color`'s, so the ink covers the tone exactly (the mean ink is the tone) and the darkest tone fills the
/// spacing. `bend` moves each line across its course by that many px times the picture's luminance, smoothed by a Gaussian of
/// `smooth` px, so that lines swell and bow with the light and dark of the forms. The ink stays the tone while the bend is
/// steady: a steady slope of the smoothed luminance across the lines only stretches them apart, and the width stretches
/// with the spacing. Where the bend jumps (an edge in the picture), the pixels sample the lines unevenly there and some
/// ink is gained or lost, at most one line's worth (`spacing` px of ink per pixel column) at each jump; where the slope
/// times `bend` passes 1 the lines fold over each other and break up into specks. A line's edge is a ramp one px wide.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Direction of the lines in degrees, clockwise on screen from the x axis (90 is downward).
    #[param(default = 20.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub angle: f32,
    /// Distance between neighbouring lines in px. Larger = fewer, coarser lines.
    #[param(default = 5.0, unit = px, min = 2, max = 100)]
    pub spacing: f32,
    /// How far in px the lines are moved across their course for a luminance of 1 (none for 0): 0 = straight lines; larger
    /// = lines flowing more strongly round the forms.
    #[param(default = 8.0, unit = px, min = 0, max = 100)]
    pub bend: f32,
    /// With a `bend`: the sigma in px of the Gaussian that smooths the luminance the lines flow by. Larger = lines follow
    /// the large shapes only; 0 = every pixel's own luminance.
    #[param(default = 6.0, unit = px, min = 0, max = 100)]
    pub smooth: f32,
    /// The ink.
    #[param(default = "#000000")]
    pub color: Color,
    /// The paper between lines. Tones darker than it get ink.
    #[param(default = "#ffffff")]
    pub paper: Color,
}

/// The engraved lines.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let place = inputs.place(picture);
    let spacing = f64::from(params.spacing);
    let lines = Lines::new(f64::from(params.angle), spacing);
    let (paper, color) = (params.paper.0, params.color.0);
    let tone = Tone::new(paper, color);
    let lights: Vec<f32> = picture.pixels().iter().map(|&pixel| luminance(pixel)).collect();
    let flow = (params.bend > 0.0).then(|| {
        gaussian_filter_plane(
            &lights,
            (height, width),
            f64::from(params.smooth),
            TRUNCATE,
            Extension::Nearest,
        )
    });
    let bend = f64::from(params.bend);
    let mut out = vec![[0.0; 3]; height * width];
    par::rows(&mut out, width, |top, band| {
        for (offset, line) in band.chunks_exact_mut(width).enumerate() {
            let row = top + offset;
            let y = float(place.first_row + row) + 0.5;
            for (column, pixel) in line.iter_mut().enumerate() {
                let at = row * width + column;
                let moved = flow.as_ref().map_or(0.0, |flow| bend * f64::from(flow[at]));
                let across = lines.across(float(column) + 0.5, y) - moved;
                let width_px = tone.of_light(lights[at]) * spacing;
                #[expect(clippy::cast_possible_truncation, reason = "a share")]
                let cover = lines.cover(across, width_px) as f32;
                *pixel = [0, 1, 2].map(|c| paper[c] + (color[c] - paper[c]) * cover);
            }
        }
    });
    Image::from_pixels(height, width, out)
}
