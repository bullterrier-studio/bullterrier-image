//! `pinch_bulge`: the picture pushed out (a bulge) or in (a pinch) about a centre. What it does is documented once, on
//! `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::centre::Centre;
use crate::ops::edge::Edge;
use crate::ops::sample::{Interpolation, Sampler};
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `pinch_bulge` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Distortion,
    opacity: 1.0,
    inputs: &[],
};

/// What `pinch_bulge` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// How far `pinch_bulge` paints beyond the picture's edge: not worked out.
pub(crate) fn spread(_params: &Params) -> crate::Spread {
    crate::Spread::Unbounded
}

/// Push the picture out (`amount` above 0, a bulge) or in (below 0, a pinch) about the centre `x`, `y`.
///
/// The picture about the centre is magnified (bulge) or shrunk (pinch) most at the centre and not at all at `radius`.
/// Each pixel at distance d from the centre, d inside the radius, shows the picture on the same ray from the centre at
/// distance d x (1 - amount x (1 - d / radius)^2), read by `interpolation`: nearer the centre for a bulge, so the middle
/// is magnified, and farther out for a pinch. Pixels at the radius or beyond, and the centre itself, keep their place, no
/// pixel is folded over, and the result never leaves 0..1.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How far the middle is pushed. Positive is a bulge: the picture at the centre is magnified, as seen through a lens
    /// or a bubble, ever more toward the centre and without limit at 1 (the pixels around the centre all show the centre
    /// pixel). Negative is a pinch: the picture at the centre is shrunk, as if drawn into a point, down to half size at -1.
    /// The push fades smoothly to nothing at the radius, so no pixel is folded over; 0 leaves the picture unchanged.
    #[param(default = 0.5, unit = fraction, min = -1, max = 1)]
    pub amount: f32,
    /// How far from the centre the effect reaches, as a fraction of the shorter side of the picture, measured from the
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
    /// exactly as it is but a bulge turns blocky), 'bilinear' blends the four pixels around it (smooth, slightly soft),
    /// 'bicubic' fits a cubic through the neighbouring pixels (sharpest, a little slower, and may ring beside hard edges).
    #[param(default = Bilinear, advanced)]
    pub interpolation: Interpolation,
    /// What a pinch sees beyond the picture's edge (it reads from farther out than it shows): 'reflect' (mirrored, edge
    /// pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the
    /// opposite edge).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The picture pushed about the centre.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    #[expect(clippy::cast_precision_loss, reason = "a picture side is far below 2**52")]
    let reach = f64::from(params.radius) * (height.min(width) - 1) as f64;
    if params.amount == 0.0 || reach == 0.0 {
        return picture.map(|pixel| pixel);
    }
    let amount = f64::from(params.amount);
    let sampler = Sampler::new(
        picture.pixels(),
        height,
        width,
        params.interpolation,
        params.edge.extension(),
    );
    Centre::new(height, width, f64::from(params.x), f64::from(params.y)).warp(&sampler, |spot| {
        let inside = (1.0 - spot.distance() / reach).clamp(0.0, 1.0);
        spot.moved(1.0 - amount * inside * inside, 0.0)
    })
}
