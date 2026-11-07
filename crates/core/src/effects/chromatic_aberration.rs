//! `chromatic_aberration`: colour fringes, from copies of the picture magnified a little differently. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::centre::Centre;
use crate::ops::edge::Edge;
use crate::ops::sample::{Interpolation, Sampler};
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `chromatic_aberration` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Light,
    opacity: 1.0,
    inputs: &[],
};

/// The spectrum as a simple rule, not a colour-matching table: the pure colours of the HSV colour wheel from red to blue
/// (red, yellow, green, cyan, blue), equally spaced, with the weight of each channel changing linearly between them.
// constant: the five pure colours of the standard HSV colour wheel from red to blue, one RGB row each
const SPECTRUM: [[f64; 3]; 5] = [
    [1.0, 0.0, 0.0],
    [1.0, 1.0, 0.0],
    [0.0, 1.0, 0.0],
    [0.0, 1.0, 1.0],
    [0.0, 0.0, 1.0],
];

/// Lens colour fringes or a prism rainbow: add up `samples` copies of the picture, scaled about the centre `x`, `y` and
/// coloured as a spectrum.
///
/// Copy k stands for the colour k / (samples - 1) of the way from red to blue and shows each pixel d px from the centre
/// read at d x (1 - e x (1 - 2k / (samples - 1))), with e = `amount` / R, R the distance of the farthest corner (red is
/// shifted outward, blue inward, the middle copy stays). A channel of the result is the weighted average of that channel in
/// the copies, the weights following the simple spectrum (red from the red and yellow copies, green from yellow to cyan,
/// blue from cyan and blue) and adding up to 1, so a flat picture and the centre are unchanged and the result never leaves
/// 0..1. With 3 samples red is the red copy, green the picture itself and blue the blue copy. Each copy is read by
/// `interpolation`.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How far red and blue are pulled apart, in pixels at the farthest corner of the picture: there the red copy is shifted
    /// this far outward from the centre and the blue copy this far inward, while the middle one stays where it is. The shift
    /// grows in proportion to the distance from the centre, so it is half as much halfway out and nothing at the centre. A
    /// larger amount is wider fringes; 0 leaves the picture unchanged. A shift larger than the distance to the farthest
    /// corner is limited to it.
    #[param(default = 4.0, unit = px, min = 0, max = 100, drivable)]
    pub amount: f32,
    /// How many copies make the spectrum, from red to blue in equal steps of scale: 3 gives a red, a green and a blue copy,
    /// as a lens's colour fringe does; more copies fill in yellow and cyan between them and turn the fringe into a smooth
    /// rainbow, but cost more time. Fewer copies than the amount needs show as separate ghosts.
    #[param(default = 3, unit = count, min = 3, max = 32)]
    pub samples: i32,
    /// The centre's horizontal position as a fraction of the way across the picture: 0 = the first pixel column, 0.5 = the
    /// middle, 1 = the last.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub x: f32,
    /// The centre's vertical position as a fraction of the way down the picture: 0 = the first pixel row, 0.5 = the middle,
    /// 1 = the last.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub y: f32,
    /// How a point between pixels is read from a copy: 'nearest' takes the closest pixel (fastest, but the fringes step),
    /// 'bilinear' blends the four pixels around it (smooth, slightly soft), 'bicubic' fits a cubic through the neighbouring
    /// pixels (sharpest, a little slower, and may ring beside hard edges).
    #[param(default = Bilinear, advanced)]
    pub interpolation: Interpolation,
    /// What the copies see beyond the picture's edge (the bluer ones read from farther out than they show): 'reflect'
    /// (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated),
    /// 'wrap' (the opposite edge).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// How much of each channel of each of `samples` copies (red to blue) the result takes: along the spectrum, each channel's
/// weights scaled to add up to 1, so each channel of the result is an average of that channel in the copies.
fn weights(copies: usize) -> Vec<[f64; 3]> {
    let mut weights: Vec<[f64; 3]> = (0..copies)
        .map(|copy| {
            #[expect(clippy::cast_precision_loss, reason = "at most 32 copies")]
            let along = copy as f64 / (copies - 1) as f64;
            let position = along * 4.0; // constant: the five colours are one step apart, so blue is at 4
            let below = position.floor().min(3.0); // constant: the last of the four steps between the five colours
            let t = position - below;
            #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss, reason = "from 0 to 3")]
            let below = below as usize;
            std::array::from_fn(|channel| SPECTRUM[below][channel] * (1.0 - t) + SPECTRUM[below + 1][channel] * t)
        })
        .collect();
    let totals: [f64; 3] = std::array::from_fn(|channel| weights.iter().map(|weight| weight[channel]).sum());
    for weight in &mut weights {
        for (level, total) in weight.iter_mut().zip(totals) {
            *level /= total;
        }
    }
    weights
}

/// The picture seen through `samples` copies scaled about the centre, each channel an average of the copies.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a picture side is far below 2**52; the sums are made in f64 and stored in the f32 image; samples is from 3 to 32"
)]
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let (x, y) = (f64::from(params.x), f64::from(params.y));
    // the farthest corner, in pixels: where the shift is the whole amount
    let farthest = (y.max(1.0 - y) * (height - 1) as f64).hypot(x.max(1.0 - x) * (width - 1) as f64);
    if params.amount == 0.0 || farthest == 0.0 {
        return picture.map(|pixel| pixel);
    }
    // as a fraction of the distance from the centre, up to all of it
    let shift = (f64::from(params.amount) / farthest).min(1.0);
    let copies = params.samples as usize;
    let sampler = Sampler::new(
        picture.pixels(),
        height,
        width,
        params.interpolation,
        params.edge.extension(),
    );
    let centre = Centre::new(height, width, x, y);
    let mut sum = vec![[0.0_f32; 3]; height * width];
    for (copy, weight) in weights(copies).into_iter().enumerate() {
        let scale = 1.0 - shift * (1.0 - 2.0 * copy as f64 / (copies - 1) as f64);
        // the middle copy (of an odd number) is the picture itself; reading it would only add rounding noise
        let seen = (2 * copy != copies - 1).then(|| centre.warp(&sampler, |spot| spot.moved(scale, 0.0)));
        let shown = seen.as_ref().map_or_else(|| picture.pixels(), |image| image.as_picture().pixels());
        for (total, pixel) in sum.iter_mut().zip(shown) {
            for channel in 0..3 {
                total[channel] += weight[channel] as f32 * pixel[channel];
            }
        }
    }
    let clamped = sum.into_iter().map(|pixel| pixel.map(|level| level.clamp(0.0, 1.0)));
    Image::from_pixels(height, width, clamped.collect())
}
