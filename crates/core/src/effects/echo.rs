//! `echo`: translucent copies of the picture, each offset a step further, laid over it. What it does is documented once,
//! on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::edge::Edge;
use crate::ops::shift::shifted;
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `echo` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Glitch,
    opacity: 1.0,
    inputs: &[],
};

/// Lay `copies` translucent copies over the picture, each (`dx`, `dy`) px further from it and weaker by `fade`.
///
/// The result is the weighted average of the picture (weight 1) and copy k, the picture moved by k times (`dx`, `dy`)
/// (weight (1 - `fade`)^k), so the weights add up to one: the picture's brightness is kept, a flat picture stays flat and
/// nothing leaves the range 0..1. What a copy shows where it moved off the picture is set by `edge`.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How many copies are laid over the picture, 0 to 32; 0 leaves the picture unchanged. More copies make a longer smear
    /// along the step, and cost more.
    #[param(default = 4, unit = count, min = 0, max = 32)]
    pub copies: i32,
    /// How far each copy lies to the right of the one before it, in pixels, -500 to 500 (negative is to the left); copy k
    /// is k times this far from the picture. A fraction of a pixel blends the two pixels it lies between.
    #[param(default = 14.0, unit = px, min = -500, max = 500)]
    pub dx: f32,
    /// How far each copy lies below the one before it, in pixels, -500 to 500 (negative is above); copy k is k times this
    /// far from the picture. With dx and dy both 0 the copies lie on the picture and change nothing.
    #[param(default = 0.0, unit = px, min = -500, max = 500)]
    pub dy: f32,
    /// How much weaker each copy is than the one before it, 0 to 1: the original counts 1, copy k counts (1 - fade) to the
    /// power k. 0 gives every copy the strength of the original; 0.5 halves each; 1 makes the copies vanish and leaves the
    /// picture unchanged.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub fade: f32,
    /// What a copy shows where it has moved off the picture's edge: 'nearest' repeats the picture's edge pixels (a smear
    /// along the edge), 'wrap' brings back what left on the other side, 'reflect' mirrors the picture (the edge line
    /// repeated once), 'mirror' mirrors it about the edge line.
    #[param(default = Nearest, advanced)]
    pub edge: Edge,
}

/// The weighted average of the picture and its copies.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let copies = params.copies.unsigned_abs() as usize;
    let keep = f64::from(1.0 - params.fade);
    let mut weight = 1.0_f64; // the picture's own
    let mut total_weight = weight;
    let mut sum: Vec<[f64; 3]> = picture.pixels().iter().map(|pixel| pixel.map(f64::from)).collect();
    for copy in 1..=copies {
        weight *= keep;
        total_weight += weight;
        #[expect(clippy::cast_precision_loss, reason = "at most 32 copies")]
        let steps = copy as f64;
        let moved = shifted(
            picture.pixels(),
            height,
            width,
            steps * f64::from(params.dy),
            steps * f64::from(params.dx),
            params.edge.extension(),
        );
        for (total, pixel) in sum.iter_mut().zip(&moved) {
            for (level, moved) in total.iter_mut().zip(pixel) {
                *level += weight * f64::from(*moved);
            }
        }
    }
    #[expect(clippy::cast_possible_truncation, reason = "the sums are made in f64 and stored in the f32 image")]
    let average = sum.into_iter().map(|pixel| pixel.map(|level| (level / total_weight).clamp(0.0, 1.0) as f32));
    Image::from_pixels(height, width, average.collect())
}
