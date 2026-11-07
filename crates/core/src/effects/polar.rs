//! `polar`: the picture wrapped round a centre like a tiny planet, or a planet unwrapped into a picture. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::centre::Centre;
use crate::ops::edge::Edge;
use crate::ops::sample::{Interpolation, Sampler};
use crate::params::{Choice, Params};
use crate::{Image, Inputs, Picture};

/// Where `polar` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Distortion,
    opacity: 1.0,
    inputs: &[],
};

/// Which way `polar` converts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Direction {
    /// The picture wrapped round the centre into a tiny planet.
    To,
    /// A planet round the centre unwrapped into a flat picture.
    From,
}

/// Wrap the picture round the centre `x`, `y` into a planet (`mode` 'to'), or unwrap such a planet ('from').
///
/// Rectangular coordinates to polar (the picture wrapped round a centre) or back. The planet has the radius of half the
/// shorter side, from the first pixel to the last. 'to': a pixel d px from the centre shows the row (1 - d / radius) down
/// the picture, from the top, and the column that is the fraction of a whole turn, clockwise from straight up, of the
/// width; the columns wrap round, so the seam shows nothing. 'from': each pixel reads the planet at the distance and angle
/// that its row and column name, the same relation the other way. The result never leaves 0..1.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Which way to convert: 'to' wraps the picture round the centre into a tiny planet (its bottom row at the centre, its
    /// top row on the rim of a circle as wide as the shorter side, the left edge straight up and the columns running
    /// clockwise, so the sky of a panorama ends up outside); 'from' is the opposite, it takes a picture that is such a
    /// planet round the centre and unwraps it into a flat picture, so 'from' undoes 'to'.
    #[param(default = To)]
    pub mode: Direction,
    /// The planet's centre horizontal position as a fraction of the way across the picture: 0 = the first pixel column,
    /// 0.5 = the middle, 1 = the last.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub x: f32,
    /// The planet's centre vertical position as a fraction of the way down the picture: 0 = the first pixel row, 0.5 = the
    /// middle, 1 = the last.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub y: f32,
    /// How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps every value
    /// exactly as it is but the middle of a planet turns blocky), 'bilinear' blends the four pixels around it (smooth,
    /// slightly soft), 'bicubic' fits a cubic through the neighbouring pixels (sharpest, a little slower, and may ring
    /// beside hard edges).
    #[param(default = Bilinear, advanced)]
    pub interpolation: Interpolation,
    /// What lies beyond the planet's rim in 'to', or beyond the picture where an off-centre planet leaves it in 'from' (the
    /// angle itself never has an edge; it wraps round): 'nearest' (the default, unlike other warps' 'reflect': the edge
    /// pixel repeated, so the sky stays one flat tone of the top row and the rim stays clean), 'reflect' (mirrored, edge
    /// pixel repeated), 'mirror' (mirrored about the edge pixel), 'wrap' (the opposite edge).
    #[param(default = Nearest, advanced)]
    pub edge: Edge,
}

/// The picture converted to or from polar coordinates.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    // the planet's radius in px
    #[expect(clippy::cast_precision_loss, reason = "a picture side is far below 2**52")]
    let reach = (height.min(width) - 1) as f64 / 2.0;
    if reach == 0.0 {
        return picture.map(|pixel| pixel);
    }
    #[expect(clippy::cast_precision_loss, reason = "as above")]
    let (last_row, columns) = ((height - 1) as f64, width as f64);
    let sampler = Sampler::new(
        picture.pixels(),
        height,
        width,
        params.interpolation,
        params.edge.extension(),
    );
    let centre = Centre::new(height, width, f64::from(params.x), f64::from(params.y));
    match params.mode {
        // the rows by distance from the centre, the columns by the angle clockwise from straight up: a whole turn wide
        Direction::To => centre.warp(&sampler.wrapping_columns(), |spot| {
            let turn = spot.across.atan2(-spot.down);
            ((1.0 - spot.distance() / reach) * last_row, turn / std::f64::consts::TAU * columns)
        }),
        // the distance from the centre falls from the reach on the first row to 0 on the last
        Direction::From => centre.warp(&sampler, |spot| {
            let out_of = (1.0 - spot.row / last_row) * reach;
            let turn = spot.column / columns * std::f64::consts::TAU;
            spot.place_at(-out_of * turn.cos(), out_of * turn.sin())
        }),
    }
}
