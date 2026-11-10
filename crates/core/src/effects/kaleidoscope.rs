//! `kaleidoscope`: the picture folded about a centre into repeated sectors, each a wedge and its mirror. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::centre::Centre;
use crate::ops::edge::Edge;
use crate::ops::sample::{Interpolation, Sampler};
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `kaleidoscope` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Distortion,
    opacity: 1.0,
    inputs: &[],
};

/// What `kaleidoscope` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// Fold the picture about the centre `x`, `y` into `segments` sectors, each a wedge of the picture and its mirror: a
/// kaleidoscope, or with one segment a plain mirror.
///
/// A wedge of the picture and its mirror image repeated round a centre. A pixel at angle a about the centre (clockwise from
/// pointing right) and distance d shows the picture at the same distance d and the angle that a folds to: a less `angle`,
/// taken modulo the sector 360 / `segments`, and if that passes half a sector, mirrored to what it falls short of the
/// sector by, then `angle` added back. The wedge from `angle` for half a sector is the picture as it is, the next half a
/// sector its mirror image, and so on round the centre, which keeps its own pixel. One segment is a single mirror axis:
/// the half of the picture clockwise from `angle` is kept, its reflection replaces the other half. The result never
/// leaves 0..1.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How many times the pattern repeats round the centre: the full circle is cut into this many equal sectors, each
    /// holding a wedge of the picture (half a sector wide) and its mirror image, so every wedge borders its own reflection
    /// and the seams between them do not show. 1 = one line through the centre: the half of the picture clockwise from
    /// `angle` is kept and its reflection replaces the other half (a face made symmetric, a reflection in water); 2 =
    /// mirrored across that line and again across the line at right angles (the kept quarter copied into the other three);
    /// raising it makes more, narrower wedges.
    #[param(default = 6, unit = count, min = 1, max = 64)]
    pub segments: i32,
    /// Where the pattern starts, as the direction in degrees, clockwise from pointing right (as CSS rotate), of the first
    /// mirror axis and of the wedge that follows it: 0 puts an axis along the line right of the centre; turning it turns
    /// the mirror axes, and shows a different wedge of the picture. With 1 segment, 0 keeps the bottom half, 90 the left,
    /// 180 the top and 270 the right. The pattern repeats every 360 / segments degrees.
    #[param(default = 0.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub angle: f32,
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
    /// With 'nearest' every output pixel is an input pixel, so a mirror (1 or 2 segments through the middle) is exact and
    /// exactly symmetric; the other two blend, and a mirror then differs from an exact reflection by rounding error only.
    #[param(default = Bilinear, advanced)]
    pub interpolation: Interpolation,
    /// What the kaleidoscope sees beyond the picture's edge (a wedge reaches past it in the corners, and wherever the centre
    /// is off to one side): 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest'
    /// (the edge pixel repeated), 'wrap' (the opposite edge).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The picture folded into sectors about the centre.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let sector = std::f64::consts::TAU / f64::from(params.segments);
    let start = f64::from(params.angle).to_radians();
    let sampler = Sampler::new(
        picture.pixels(),
        height,
        width,
        params.interpolation,
        params.edge.extension(),
    );
    Centre::new(height, width, f64::from(params.x), f64::from(params.y)).warp(&sampler, |spot| {
        // clockwise from pointing right, as rows run down
        let pixel_angle = spot.down.atan2(spot.across);
        let into_sector = (pixel_angle - start).rem_euclid(sector);
        // the sector's second half runs back
        let shown_angle = start + into_sector.min(sector - into_sector);
        spot.moved(1.0, pixel_angle - shown_angle)
    })
}
