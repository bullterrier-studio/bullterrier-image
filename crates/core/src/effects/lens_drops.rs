//! `lens_drops`: water drops on the lens, each showing the scene upside down and blurred. What it does is documented once,
//! on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::blur::gaussian_filter;
use crate::ops::edge::Edge;
use crate::ops::random::Rng;
use crate::ops::sample::{Interpolation, Sampler};
use crate::ops::{cast, length, par};
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `lens_drops` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Atmosphere,
    opacity: 1.0,
    inputs: &[],
};

/// Put `count` water drops on the picture, each showing the blurred scene turned upside down by `refraction`.
///
/// A drop at a random place has a random radius from (1 - `size_variation`) x `radius` to all of it. The pixel at offset d
/// from its middle shows the picture, blurred by `blur`, at offset -`refraction` x d from it, read by `interpolation` and
/// `edge`, and the drop's edge is antialiased over a pixel. The `highlight` is white screened over the drop, `highlight` x
/// (distance / radius)^2 x (1 - dy / distance) / 2 with dy the pixel's row below the middle. Later drops cover earlier
/// ones. Pixels outside every drop are exactly as they were; the same `seed` always gives the same drops. The result never
/// leaves 0..1.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How many drops there are, whatever the size of the picture (a number, not a density: the drops' size is in design px,
    /// so the same count looks alike at any size). 0 = none (the picture unchanged).
    #[param(default = 14, unit = count, min = 0, max = 200)]
    pub count: i32,
    /// The radius in px of the biggest drops; the smallest are smaller by `size_variation`. Tuned for a picture about 480 px
    /// high. Larger = bigger drops.
    #[param(default = 24.0, unit = px, min = 1, max = 200)]
    pub radius: f32,
    /// How much the drops differ in size: each has a random share of `radius` from 1 - `size_variation` to 1. 0 = every drop
    /// alike, 1 = from the full radius down to specks.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub size_variation: f32,
    /// How strongly a drop bends the scene: it shows the picture turned upside down about the drop's middle and this many
    /// times as wide. 1 = the same scale, turned round; above 1 a wider view in miniature, as in a real drop; below 1 a
    /// magnified view.
    #[param(default = 1.5, unit = ratio, min = 0.25, max = 4)]
    pub refraction: f32,
    /// How much the scene inside a drop is blurred, the sigma of a Gaussian in px, as a drop is never quite in focus. 0 =
    /// sharp.
    #[param(default = 1.5, unit = px, min = 0, max = 20)]
    pub blur: f32,
    /// How bright the drops' shine is: white light screened along the rim of each drop, strongest at its top (light from
    /// above) and fading to nothing at its bottom and in its middle. 0 = none, so a flat picture stays flat; 0.5 is a soft
    /// glint.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1)]
    pub highlight: f32,
    /// Random seed: the same seed always puts the same drops in the same places, another seed others.
    #[param(default = 0, unit = count, min = 0, max = 4294967295)]
    pub seed: u32,
    /// Where the blur's Gaussian is cut off, in sigmas (only when blur > 0). Larger is more exact and slower.
    #[param(default = 4.0, unit = ratio, min = 1, max = 10, advanced)]
    pub truncate: f32,
    /// How a point between pixels is read from the picture inside a drop: 'nearest' takes the closest pixel (fastest, keeps
    /// every value exactly as it is but edges turn jagged), 'bilinear' blends the four pixels around it (smooth, slightly
    /// soft), 'bicubic' fits a cubic through the neighbouring pixels (sharpest, a little slower, and may ring beside hard
    /// edges).
    #[param(default = Bilinear, advanced)]
    pub interpolation: Interpolation,
    /// What a drop sees beyond the picture's edge (a drop near the border or a strong refraction reads from outside it), and
    /// the blur with it: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the
    /// edge pixel repeated), 'wrap' (the opposite edge).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// One drop: where its middle is, in pixels from the first row and the first column, and its radius.
struct Drop {
    middle_row: f64,
    middle_column: f64,
    reach: f64,
}

impl Drop {
    /// The rows and the columns of a `height` x `width` picture that the drop can touch, as `(first, last + 1)` pairs: the
    /// box round its disc, a pixel wider on each side so that the antialiased edge is inside it.
    fn bounds(&self, height: usize, width: usize) -> ((usize, usize), (usize, usize)) {
        let span = |middle: f64, size: usize| {
            (
                length::whole((middle - self.reach).floor() - 1.0),
                length::whole((middle + self.reach).floor() + 2.0).min(size),
            )
        };
        (span(self.middle_row, height), span(self.middle_column, width))
    }
}

/// The picture with the drops laid on it, one after the other.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let mut rng = Rng::seeded(params.seed);
    let drops: Vec<Drop> = (0..params.count)
        .map(|_| {
            // constant: three draws for each drop, in this order: its x, its y and its size
            let (x, y, size) = (rng.uniform(), rng.uniform(), rng.uniform());
            Drop {
                middle_row: y * length::float(height),
                middle_column: x * length::float(width),
                reach: f64::from(params.radius) * (1.0 - f64::from(params.size_variation) * (1.0 - size)),
            }
        })
        .collect();
    let scene = gaussian_filter(
        picture,
        f64::from(params.blur),
        f64::from(params.truncate),
        params.edge.extension(),
    );
    let sampler = Sampler::new(&scene, height, width, params.interpolation, params.edge.extension());
    let (refraction, highlight) = (f64::from(params.refraction), f64::from(params.highlight));
    let mut out = picture.pixels().to_vec();
    // a pixel is laid on by the drops that reach it, in their order, whichever band it is in
    par::rows(&mut out, width, |first_row, band| {
        let last_row = first_row + band.len() / width;
        for drop in &drops {
            let ((top, bottom), (left, right)) = drop.bounds(height, width);
            for row in top.max(first_row)..bottom.min(last_row) {
                for column in left..right {
                    let down = length::float(row) - drop.middle_row;
                    let across = length::float(column) - drop.middle_column;
                    let distance = down.hypot(across);
                    // a pixel is a unit box: antialiased over one pixel
                    let cover = (drop.reach + 0.5 - distance).clamp(0.0, 1.0);
                    if cover <= 0.0 {
                        continue;
                    }
                    let shown = sampler
                        .at(drop.middle_row - refraction * down, drop.middle_column - refraction * across)
                        .map(|level| level.clamp(0.0, 1.0));
                    let shine = highlight * distance * (distance - down) / (2.0 * drop.reach * drop.reach);
                    let pixel = &mut band[(row - first_row) * width + column];
                    for (level, shown) in pixel.iter_mut().zip(shown) {
                        // the shine is screened over what the drop shows; the sum is held to 0..1 (the shine is above 1 at
                        // the antialiased edge of the biggest highlight)
                        let colour = shown + shine * (1.0 - shown);
                        *level = cast::single((f64::from(*level) * (1.0 - cover) + colour * cover).clamp(0.0, 1.0));
                    }
                }
            }
        }
    });
    Image::from_pixels(height, width, out)
}
