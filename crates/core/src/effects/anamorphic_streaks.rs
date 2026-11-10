//! `anamorphic_streaks`: horizontal streaks of light stretched from the highlights, as a lens draws them. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::convolve::{Kernel, convolve};
use crate::ops::edge::Extension;
use crate::ops::light::{highlights, screen};
use crate::params::{Color, Params};
use crate::{Canvas, Footprint};
use crate::{Image, Inputs, Picture};

/// Where `anamorphic_streaks` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Light,
    opacity: 1.0,
    inputs: &[],
};

/// What `anamorphic_streaks` reads around a pixel.
pub(crate) fn footprint(_params: &Params, _canvas: &Canvas) -> Footprint {
    Footprint::Radius(0.0)
}

/// Horizontal streaks of light: what is brighter than `threshold` is smeared along its row, tinted `color` and screened
/// over the picture by `amount`.
///
/// The smear is a two-sided exponential, exp(-|distance| / s) with s = `length` px, scaled to add up to 1: a sharp bright
/// core with long tails, which is what a streak looks like (a Gaussian would not carry far enough). It runs along the
/// rows only, so nothing leaks up or down, and no light comes in from beyond the picture's edge. The picture only ever
/// gets brighter, and black stays black.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How far the streaks reach sideways, in px tuned for a picture about 480 px high: the distance over which a streak
    /// fades to 37% of its brightness (the faint tail runs on, to the picture's edge). Raising it makes longer, fainter
    /// streaks.
    #[param(default = 168.0, unit = px, min = 0.5, max = 480)]
    pub length: f32,
    /// The brightness (0..1, per channel) where a streak starts: only values above it light the streaks, rising to full
    /// at 1. Raising it limits the streaks to the brightest highlights.
    #[param(default = 0.7, unit = fraction, min = 0, max = 0.99)]
    pub threshold: f32,
    /// How strongly the streaks are screened over the picture: 0 = off; the streak is spread thin along its length, so a
    /// long one needs a high amount (above 1 it saturates to white sooner).
    #[param(default = 6.0, unit = ratio, min = 0, max = 10)]
    pub amount: f32,
    /// The tint of the streaks: '#rrggbb', '#rgb' or [r, g, b] 0..1. The default is the cool blue of coated anamorphic
    /// lenses; white keeps the colour of the light that makes them.
    #[param(default = "#8fb8ff")]
    pub color: Color,
}

/// The picture with the streaks screened over it.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let length = f64::from(params.length);
    // One row of weights, as far either way as two columns of a row can be apart.
    let streak = Kernel::from_fn((0, width - 1), |_, right| (-right.abs() / length).exp());
    let bright: Vec<[f32; 3]> = picture
        .pixels()
        .iter()
        .map(|&pixel| highlights(pixel, params.threshold))
        .collect();
    // Nothing comes in from beyond the picture's edge: black lies there.
    let streaks = convolve(&bright, height, width, &streak, Extension::Constant(0.0));
    let tint = params.color.0.map(|channel| params.amount * channel);
    picture.map_at(|row, column, pixel| {
        let light = streaks[row * width + column];
        screen(pixel, [0, 1, 2].map(|channel| light[channel] * tint[channel]))
    })
}
