//! `modulation`: rows of dashes whose length carries tone. What it does is documented once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Input, Meta};
use crate::ops::cast;
use crate::ops::color::luminance;
use crate::ops::dash_rows::{Rows, Shape};
use crate::ops::edge::Edge;
use crate::ops::length::float;
use crate::ops::par;
use crate::ops::sample::{Interpolation, Sampler};
use crate::params::{Color, Params};
use crate::{Footprint, Image, Inputs, Picture};

/// Where `modulation` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Stylised,
    opacity: 1.0,
    inputs: &[Input::Place],
};

/// The rows are laid on the canvas and a cell reads the picture around it.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// An error carried along a row reaches its far end: not worked out.
pub(crate) fn spread(_params: &Params) -> crate::Spread {
    crate::Spread::Unbounded
}

/// Rows of dashes whose length carries tone.
///
/// Rows `spacing` px apart run `angle` degrees below the x axis; each is cut into cells `dash` px long, a cell as wide as
/// the spacing. A cell takes the mean darkness of the picture under it (how far its luminance lies from the `paper`'s
/// towards the `color`'s), and the row is dithered along its length: error is carried from cell to cell, so a dash is inked
/// where the carried tone reaches a half and the rest goes on. Neighbouring inked cells join into one longer dash. A dash is
/// `thickness` of the spacing wide, so ink covers that share of a cell at most: a tone up to `thickness` is kept exactly
/// (the mean ink is the tone), a darker one prints at `thickness` and no darker. `sway` bends the rows into waves of
/// `wavelength` px, and `weave` carries the error the other way along every second row.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Direction of the rows in degrees, clockwise on screen from the x axis (90 is downward). The rows are anchored at the
    /// picture's top left corner.
    #[param(default = 0.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub angle: f32,
    /// Distance between rows in px. Larger = fewer, coarser rows.
    #[param(default = 8.0, unit = px, min = 2, max = 100)]
    pub spacing: f32,
    /// Length of a cell along its row in px: the finest dash, and the grain of the tone along a row.
    #[param(default = 4.0, unit = px, min = 1, max = 100)]
    pub dash: f32,
    /// Width of a dash as a share of `spacing`, and the darkest tone the rows can print: 0.5 = half the paper shows between
    /// rows even in the darkest areas.
    #[param(default = 0.5, unit = fraction, min = 0.05, max = 1)]
    pub thickness: f32,
    /// The ink.
    #[param(default = "#000000")]
    pub color: Color,
    /// The paper between dashes. Tones darker than it get ink.
    #[param(default = "#ffffff")]
    pub paper: Color,
    /// How far a row swings to either side of its line, in px: 0 = straight rows.
    #[param(default = 0.0, unit = px, min = 0, max = 100, advanced)]
    pub sway: f32,
    /// With a `sway`: the distance along a row between two crests, in px.
    #[param(default = 60.0, unit = px, min = 4, max = 1000, advanced)]
    pub wavelength: f32,
    /// Carries the error from the far end along every second row, so that neighbouring rows do not run in step.
    #[param(default = false, advanced)]
    pub weave: bool,
}

/// The rows of dashes.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let place = inputs.place(picture);
    let rows = Rows::new(Shape {
        angle: f64::from(params.angle),
        spacing: f64::from(params.spacing),
        dash: f64::from(params.dash),
        thickness: f64::from(params.thickness),
        sway: f64::from(params.sway),
        wavelength: f64::from(params.wavelength),
        canvas: (place.canvas_height, place.canvas_width),
    });
    let (paper, color) = (params.paper.0, params.color.0);
    let paper_light = f64::from(luminance(paper));
    let reach = f64::from(luminance(color)) - paper_light;
    let sampler = Sampler::new(
        picture.pixels(),
        height,
        width,
        Interpolation::Bilinear,
        Edge::Nearest.extension(),
    );
    let first = float(place.first_row);
    let darkness = |x: f64, y: f64| {
        if reach.abs() < f64::EPSILON {
            return 0.0;
        }
        let [r, g, b] = sampler.at(y - first - 0.5, x - 0.5).map(cast::single);
        ((f64::from(luminance([r, g, b])) - paper_light) / reach).clamp(0.0, 1.0)
    };
    let inked = rows.inked(params.weave, darkness);
    let mut pixels = vec![[0.0; 3]; height * width];
    par::rows(&mut pixels, width, |top, band| {
        for (offset, line) in band.chunks_exact_mut(width).enumerate() {
            let y = float(place.first_row + top + offset) + 0.5;
            for (column, out) in line.iter_mut().enumerate() {
                let cover = rows.coverage(&inked, float(column) + 0.5, y);
                *out = [0, 1, 2].map(|c| paper[c] + (color[c] - paper[c]) * cover);
            }
        }
    });
    Image::from_pixels(height, width, pixels)
}
