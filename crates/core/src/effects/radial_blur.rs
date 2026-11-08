//! `radial_blur`: the picture smeared around a centre (spin) or away from it (zoom). What it does is documented once, on
//! `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::edge::Edge;
use crate::ops::radial::{self, Motion};
use crate::ops::sample::{Interpolation, Sampler};
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `radial_blur` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Light,
    opacity: 1.0,
    inputs: &[],
};

/// What `radial_blur` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// Smear the picture around a centre (`mode` 'spin') or away from it ('zoom'), by `amount`.
///
/// Every pixel becomes the average of `samples` points spread evenly along the arc or ray through it (centred on the
/// pixel), each read by `interpolation`, each colour channel on its own. The centre stays sharp, flat areas stay flat and
/// the result never leaves the picture's own range.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The direction of the smear: 'spin' averages along the circle around the centre, as a rotating camera or wheel would
    /// blur; 'zoom' averages along the ray through the centre, as zooming the lens during the exposure would. The centre
    /// itself stays sharp in both.
    #[param(default = Zoom)]
    pub mode: Motion,
    /// How far each pixel is smeared. 'spin': the arc, as a fraction of a full turn (0.1 = 36 degrees, 1 = a whole circle),
    /// centred on the pixel. 'zoom': the range of scales, as a fraction of the pixel's distance from the centre (0.1 = from
    /// 5% closer to 5% farther, 1 = from half to one and a half times the distance). Larger is a longer smear and stronger
    /// at the edges; 0 leaves the picture unchanged.
    #[param(default = 0.1, unit = fraction, min = 0, max = 1)]
    pub amount: f32,
    /// The centre's horizontal position as a fraction of the way across the picture: 0 = the first pixel column, 0.5 = the
    /// middle, 1 = the last.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub x: f32,
    /// The centre's vertical position as a fraction of the way down the picture: 0 = the first pixel row, 0.5 = the
    /// middle, 1 = the last.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub y: f32,
    /// How many points along each arc or ray are averaged, spread evenly over its length and read from the picture by the
    /// chosen interpolation: more is smoother and slower (the time grows with it), and a smear of more than a few pixels
    /// per sample shows as separate ghost copies. With 1 the picture is unchanged.
    #[param(default = 8, unit = count, min = 1, max = 256, advanced)]
    pub samples: i32,
    /// How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps every value
    /// exactly as it is but steps and ghosts show in smooth areas), 'bilinear' blends the four pixels around it (smooth,
    /// slightly soft), 'bicubic' fits a cubic spline through the neighbouring pixels (sharpest and most exact on smooth
    /// gradients, a little slower, and may ring beside hard edges).
    #[param(default = Bilinear, advanced)]
    pub interpolation: Interpolation,
    /// What the smear sees beyond the picture's edge (arcs and rays near the corners leave it): 'reflect' (mirrored, edge
    /// pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite
    /// edge).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The picture smeared, held to its own range.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    if params.amount == 0.0 || params.samples == 1 {
        return picture.map(|pixel| pixel);
    }
    #[expect(clippy::cast_precision_loss, reason = "a picture side is far below 2**52")]
    let centre = (
        f64::from(params.y) * (height - 1) as f64,
        f64::from(params.x) * (width - 1) as f64,
    );
    let count = f64::from(params.samples);
    // the offsets spread evenly over -amount / 2 .. amount / 2, symmetric: scales 1 + offset, or angles in radians (a turn is 2 pi)
    let reach = match params.mode {
        Motion::Zoom => f64::from(params.amount),
        Motion::Spin => std::f64::consts::TAU * f64::from(params.amount),
    };
    let readings: Vec<(f64, f64)> = (0..params.samples)
        .map(|k| (reach * ((f64::from(k) + 0.5) / count - 0.5), count.recip()))
        .collect();
    let sampler = Sampler::new(
        picture.pixels(),
        height,
        width,
        params.interpolation,
        params.edge.extension(),
    );
    let (low, high) = picture
        .values()
        .iter()
        .fold((f32::INFINITY, f32::NEG_INFINITY), |(low, high), &value| {
            (low.min(value), high.max(value))
        });
    let smeared = radial::accumulate(&sampler, params.mode, centre, &readings);
    Image::from_pixels(height, width, smeared.into_iter().map(|pixel| pixel.map(|level| level.clamp(low, high))).collect())
}
