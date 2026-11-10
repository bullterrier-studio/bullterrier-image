//! `swirl`: the picture twisted about a centre, most at the centre and not at all at the radius. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::centre::Centre;
use crate::ops::edge::Edge;
use crate::ops::sample::{Interpolation, Sampler};
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `swirl` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Distortion,
    opacity: 1.0,
    inputs: &[],
};

/// What `swirl` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// Twist the picture about the centre `x`, `y` by `angle` degrees there, falling to nothing at `radius`.
///
/// The picture is turned about the centre by an angle that falls smoothly from `angle` at the centre to nothing at
/// `radius`. Each pixel at distance d from the centre, d inside the radius, shows the picture at the point turned about the
/// centre by angle x (1 - d / radius)^2, clockwise for a positive angle, read by `interpolation`. Pixels at the radius or
/// beyond, and the centre itself, keep their place, and the result never leaves 0..1.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How far the picture is turned at the centre, in degrees: positive turns clockwise, negative counter-clockwise, 360
    /// is a whole turn. The turn falls off with the square of the distance left to the radius, so the picture is twisted
    /// into a spiral and unturned at the radius. Larger is a tighter spiral; 0 leaves the picture unchanged.
    #[param(default = 90.0, unit = degrees, min = -720, max = 720)]
    pub angle: f32,
    /// How far from the centre the swirl reaches, as a fraction of the shorter side of the picture, measured from the
    /// first pixel to the last (0.5 = a disc as wide as half of it, 2 = a disc that covers the whole picture and more).
    /// Pixels from this far out are untouched; 0 leaves the picture unchanged.
    #[param(default = 0.5, unit = fraction, min = 0, max = 2)]
    pub radius: f32,
    /// The centre's horizontal position as a fraction of the way across the picture: 0 = the first pixel column, 0.5 = the
    /// middle, 1 = the last.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub x: f32,
    /// The centre's vertical position as a fraction of the way down the picture: 0 = the first pixel row, 0.5 = the
    /// middle, 1 = the last.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub y: f32,
    /// How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps every value
    /// exactly as it is but edges turn jagged), 'bilinear' blends the four pixels around it (smooth, slightly soft),
    /// 'bicubic' fits a cubic through the neighbouring pixels (sharpest, a little slower, and may ring beside hard edges).
    #[param(default = Bilinear, advanced)]
    pub interpolation: Interpolation,
    /// What the swirl sees beyond the picture's edge (a swirl near the border turns pixels in from outside): 'reflect'
    /// (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated),
    /// 'wrap' (the opposite edge).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The picture twisted about the centre.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    #[expect(clippy::cast_precision_loss, reason = "a picture side is far below 2**52")]
    let reach = f64::from(params.radius) * (height.min(width) - 1) as f64;
    if params.angle == 0.0 || reach == 0.0 {
        return picture.map(|pixel| pixel);
    }
    let angle = f64::from(params.angle).to_radians();
    let sampler = Sampler::new(
        picture.pixels(),
        height,
        width,
        params.interpolation,
        params.edge.extension(),
    );
    Centre::new(height, width, f64::from(params.x), f64::from(params.y)).warp(&sampler, |spot| {
        let inside = (1.0 - spot.distance() / reach).clamp(0.0, 1.0);
        spot.moved(1.0, angle * inside * inside)
    })
}
