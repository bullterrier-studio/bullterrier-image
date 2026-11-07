//! `droste`: the picture shown again, smaller and turned, in its own middle, over and over. What it does is documented
//! once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::centre::Centre;
use crate::ops::edge::Edge;
use crate::ops::sample::{Interpolation, Sampler};
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `droste` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Distortion,
    opacity: 1.0,
    inputs: &[],
};

/// Show the picture again in its own middle, `scale` times as large and turned by `turn`, up to `repeats` times.
///
/// A Droste effect: a picture that contains a smaller copy of itself, which contains a smaller copy. Measure each pixel's
/// distance from the middle as a share of the picture's half-width or half-height, whichever is larger (1 at the edge, so
/// the picture and its copies are rectangles of the picture's shape). A pixel nearer than `scale` is inside a copy: it
/// shows the picture at 1 / `scale` times its own distance, turned by `turn`, and if that is nearer than `scale` again, it
/// is inside a copy of the copy, and so on, `repeats` times at most. The outside of the picture is the picture as it is.
/// The result never leaves 0..1.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How much smaller each copy is than the one around it, strictly between 0 and 1: 0.5 = each copy is half the size of
    /// the last, 0.9 = a slow shrink with many copies close together, 0.1 = one small copy. The copy has the picture's own
    /// shape and sits in its middle.
    #[param(default = 0.5, unit = ratio, min = 0.1, max = 0.9)]
    pub scale: f32,
    /// How far each copy is turned from the one around it, in degrees: positive turns the copies clockwise as the picture
    /// is seen, negative anticlockwise, 0 leaves them upright (nested frames); other values make the copies spiral into
    /// the middle.
    #[param(default = 20.0, unit = degrees, min = -180, max = 180, cyclic)]
    pub turn: f32,
    /// How many copies are drawn inside one another at most, 1 to 12: the copy this deep shows the picture's middle instead
    /// of a further copy. Raising it adds ever smaller copies towards the middle.
    #[param(default = 4, unit = count, min = 1, max = 12)]
    pub repeats: i32,
    /// How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, keeps every value
    /// exactly as it is but the small copies turn blocky), 'bilinear' blends the four pixels around it (smooth, slightly
    /// soft), 'bicubic' fits a cubic through the neighbouring pixels (sharpest, a little slower, and may ring beside hard
    /// edges).
    #[param(default = Bilinear, advanced)]
    pub interpolation: Interpolation,
    /// What a turned copy shows where it reads from beyond the picture (a turned copy's corners reach past the edge):
    /// 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel
    /// repeated), 'wrap' (the opposite edge).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The picture with copies of itself inside.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    #[expect(clippy::cast_precision_loss, reason = "a picture side is far below 2**52")]
    let (half_height, half_width) = (
        (height - 1).max(1) as f64 / 2.0,
        (width - 1).max(1) as f64 / 2.0,
    );
    let scale = f64::from(params.scale);
    let (copies, turn) = (f64::from(params.repeats), f64::from(params.turn).to_radians());
    // the smallest size that is told apart from 0, as `repeats` allows: a pixel in the very middle would need endlessly
    // many copies, so the count stops at `repeats`
    let nearest = scale.powi(params.repeats + 1);
    let sampler = Sampler::new(
        picture.pixels(),
        height,
        width,
        params.interpolation,
        params.edge.extension(),
    );
    Centre::new(height, width, 0.5, 0.5).warp(&sampler, |spot| {
        let size = (spot.down.abs() / half_height).max(spot.across.abs() / half_width);
        // the copy the pixel is in: how many steps of 1 / scale carry its size out to `scale` or more
        let depth = ((scale / size.max(nearest)).ln() / -scale.ln()).ceil().clamp(0.0, copies);
        spot.moved(scale.powf(-depth), turn * depth)
    })
}
