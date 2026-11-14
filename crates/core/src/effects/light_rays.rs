//! `light_rays`: god rays, the highlights streaked away from a point of light. What it does is documented once, on
//! `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::edge::Edge;
use crate::ops::light::{highlights, screen};
use crate::ops::radial::{self, Motion};
use crate::ops::sample::{Interpolation, Sampler};
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `light_rays` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Light,
    opacity: 1.0,
    inputs: &[],
};

/// What `light_rays` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// How far `light_rays` paints beyond the picture's edge: not worked out.
pub(crate) fn spread(_params: &Params) -> crate::Spread {
    crate::Spread::Unbounded
}

/// God rays: what is brighter than `threshold` is streaked away from (`x`, `y`) and screened over the picture by `amount`.
///
/// Every pixel gathers the highlights from `samples` points on the line between itself and the light, from itself to
/// `length` of the way there, each reading `decay` times as strong as the one before and the weights adding up to 1. A
/// highlight streaks outward from the light, up to 1 / (1 - length) times its distance from it. The picture only ever gets
/// brighter, and black stays black.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The light's horizontal position as a fraction of the way across the picture: 0 = the first pixel column, 0.5 = the
    /// middle, 1 = the last. The rays point away from it.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub x: f32,
    /// The light's vertical position as a fraction of the way down the picture: 0 = the first pixel row, 0.5 = the middle,
    /// 1 = the last. The rays point away from it.
    #[param(default = 0.3, unit = fraction, min = 0, max = 1)]
    pub y: f32,
    /// How far the rays reach back toward the light, as a fraction of each pixel's distance from it: a pixel gathers light
    /// from the line between itself and the light, as far as this share of the way (0.5 = half way, 1 = all the way to the
    /// light). A highlight streaks outward from itself up to 1 / (1 - length) times its distance from the light; 0 gives
    /// no rays.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub length: f32,
    /// The brightness (0..1, per channel) where light starts: only values above it make rays, rising to full at 1. Raising
    /// it limits the rays to the brightest highlights.
    #[param(default = 0.7, unit = fraction, min = 0, max = 0.99)]
    pub threshold: f32,
    /// How strongly the rays are screened over the picture: 0 = off, 1 = the rays at full brightness; above 1 they
    /// saturate to white sooner. The rays are an average along the pixel's line to the light, so a highlight far from the
    /// pixel gives them much less than the highlight itself: a few times 1 is usual.
    #[param(default = 2.5, unit = ratio, min = 0, max = 10)]
    pub amount: f32,
    /// How many readings of the picture along each ray are added up, spread evenly over its length: more is smoother and
    /// slower (the time grows with it); few show as separate ghost copies of a highlight. With 1 the highlights only
    /// brighten themselves.
    #[param(default = 8, unit = count, min = 1, max = 256, advanced)]
    pub samples: i32,
    /// How much less each reading counts than the one nearer the pixel (the weights are decay^0, decay^1... scaled to add
    /// up to 1): 1 = all alike, rays of even brightness to their tips; lower values make them fade away from the
    /// highlight; 0 = no rays at all.
    #[param(default = 0.9, unit = ratio, min = 0, max = 1)]
    pub decay: f32,
    /// How a point between pixels is read from the picture: 'nearest' takes the closest pixel (fastest, but steps and ghosts
    /// show), 'bilinear' blends the four pixels around it (smooth, slightly soft), 'bicubic' fits a cubic through the
    /// neighbouring pixels (sharpest, a little slower).
    #[param(default = Bilinear, advanced)]
    pub interpolation: Interpolation,
    /// What a reading sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored about
    /// the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge). The readings lie between a pixel
    /// and the light, inside the picture; this rule matters only to 'bicubic', whose cubic fit looks one pixel past the
    /// point it reads, and then only beside a light that sits on the border.
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// The picture with the streaked highlights screened over it.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    if params.amount == 0.0 {
        return picture.map(|pixel| pixel);
    }
    let bright: Vec<[f32; 3]> = picture
        .pixels()
        .iter()
        .map(|&pixel| highlights(pixel, params.threshold))
        .collect();
    #[expect(clippy::cast_precision_loss, reason = "a picture side is far below 2**52")]
    let light = (
        f64::from(params.y) * (height - 1) as f64,
        f64::from(params.x) * (width - 1) as f64,
    );
    // the rays are an average, so a picture bright all over stays as bright as its highlights: the weights add up to 1
    let weights: Vec<f64> = (0..params.samples).map(|k| f64::from(params.decay).powi(k)).collect();
    let total: f64 = weights.iter().sum();
    let count = f64::from(params.samples);
    // scale 1 down to 1 - length: toward the light
    let readings: Vec<(f64, f64)> = (0..params.samples)
        .zip(&weights)
        .map(|(k, weight)| (-f64::from(params.length) * f64::from(k) / count, weight / total))
        .collect();
    let sampler = Sampler::new(&bright, height, width, params.interpolation, params.edge.extension());
    let rays = radial::accumulate(&sampler, Motion::Zoom, light, &readings);
    let pixels = picture
        .pixels()
        .iter()
        .zip(rays)
        .map(|(&pixel, ray)| screen(pixel, ray.map(|level| params.amount * level)))
        .collect();
    Image::from_pixels(height, width, pixels)
}
