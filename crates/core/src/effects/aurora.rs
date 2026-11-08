//! `aurora`: soft vertical light curtains, brightest at a wavy base, over a dark sky. What it does is documented once, on
//! `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::gauss::ndtr;
use crate::ops::noise::{Fractal, field};
use crate::ops::{cast, length, ramp};
use crate::params::{Color, Params};
use crate::{Image, Inputs, Picture};

/// Where `aurora` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Backgrounds,
    opacity: 1.0,
    inputs: &[],
};

/// What `aurora` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// Replace the picture with an aurora: a curtain of light with a wavy base and vertical rays over a dark sky.
///
/// A generated background: the input is used only for its height and width, its content is ignored. In column x the curtain's
/// base is the row `base` x height + `waviness` x ripple(x), the ripple being fractal noise of unit deviation along the picture
/// (`scale`). A pixel d = (rows above the base) / `height` up the curtain has brightness (1 - d)^`falloff`, 0 below the base
/// (a 1 px anti-aliased edge) and above d = 1, times the column's ray, 1 - `rays` x (1 - r) with r in 0..1 the rank of a
/// second fractal noise along the picture (`ray_width`). Its colour is `colors` at d, mixed into `sky` by that brightness.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The curtain's colours from its base to its top, evenly spaced along the way up; at least two. The classic aurora is
    /// green at the base turning violet or red higher up.
    #[param(default = ["#2bff9a", "#7a2cff"], min_length = 2)]
    pub colors: Vec<Color>,
    /// The colour of the night sky, seen below the curtain's base, above its top and between its rays: '#rrggbb', '#rgb' or
    /// [r, g, b] 0..1. A very dark blue looks better than pure black.
    #[param(default = "#02050f")]
    pub sky: Color,
    /// Where the curtain's bright lower edge lies, as a fraction of the picture's height from the top (0 = the top edge, 1 =
    /// the bottom edge). Below it the sky is dark. Waviness moves it up and down along the picture.
    #[param(default = 0.75, unit = fraction, min = 0, max = 1)]
    pub base: f32,
    /// How far up from its base the curtain reaches before it has faded out completely, in pixels; tuned for a picture about
    /// 480 px high (the default is 0.7 of that). Larger = a taller curtain.
    #[param(default = 336.0, unit = px, min = 1, max = 4000)]
    pub height: f32,
    /// How fast the light fades going up: the brightness at a height h above the base, as a fraction of the curtain's height,
    /// is (1 - h) to this power. 1 = a straight fade, above 1 = a bright edge with the glow quickly falling away, below 1 =
    /// light that reaches high before dimming.
    #[param(default = 0.8, unit = ratio, min = 0.2, max = 8, advanced)]
    pub falloff: f32,
    /// How far the curtain's base wanders up and down across the picture, in pixels: the typical distance from the `base`
    /// line (the standard deviation of the ripple); tuned for a picture about 480 px high. 0 = a straight, level base.
    #[param(default = 40.0, unit = px, min = 0, max = 500)]
    pub waviness: f32,
    /// Size in pixels, along the picture, of the curtain's coarsest ripples: the spacing of the first noise lattice of the
    /// base's wobble. Larger = long, slow folds; small = a quickly rippling hem.
    #[param(default = 200.0, unit = px, min = 2, max = 2048, advanced)]
    pub scale: f32,
    /// How strongly the curtain is broken into vertical rays: each ray has a brightness of its own and this is how far the
    /// dimmest drop below the brightest. 0 = a smooth, even curtain, 1 = rays separated by dark sky.
    #[param(default = 0.7, unit = fraction, min = 0, max = 1)]
    pub rays: f32,
    /// Width in pixels of the broadest rays, the spacing of the first noise lattice across the picture; tuned for a picture
    /// about 480 px high. Larger = a few wide shafts; small = a fine comb of streaks.
    #[param(default = 10.0, unit = px, min = 1, max = 256, advanced)]
    pub ray_width: f32,
    /// How many layers of noise are summed for the base's wobble and for the rays, each finer than the last (by `lacunarity`)
    /// and weaker (by `persistence`). 1 = smooth, more = more ragged detail. Layers finer than one pixel are dropped.
    #[param(default = 3, unit = count, min = 1, max = 8)]
    pub octaves: i32,
    /// How much weaker each finer layer of the wobble and the rays is than the one before: each layer's weight is this to the
    /// power of its number. 0 = only the coarsest layer, 1 = every layer equally strong.
    #[param(default = 0.5, unit = ratio, min = 0, max = 1, advanced)]
    pub persistence: f32,
    /// How much finer each layer is than the one before: the lattice spacing is divided by this per layer. 1 = every layer
    /// the same size, 2 = each half as coarse, larger = detail drops off faster in size.
    #[param(default = 2.0, unit = ratio, min = 1, max = 4, advanced)]
    pub lacunarity: f32,
    /// Random seed: the same seed always gives the same curtain (its folds and its rays), another seed a different one.
    #[param(default = 0, unit = count, min = 0, max = 4294967295)]
    pub seed: u32,
}

/// The aurora, in the picture's size: the picture is used only for its height and width.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (rows, width) = (picture.height(), picture.width());
    // constant: a one-row field, so the noise runs along the picture only
    let along = |spacing: f32, stream| {
        field(
            1,
            width,
            &Fractal {
                spacing: f64::from(spacing),
                octaves: params.octaves.cast_unsigned(),
                persistence: f64::from(params.persistence),
                lacunarity: f64::from(params.lacunarity),
                seed: params.seed,
                origin: 0.0, // constant: a lattice point on the first pixel
            },
            stream,
        )
    };
    let ripple = along(params.scale, 0);
    // constant: stream 1 of the seed, independent of the ripple's stream 0
    let ray: Vec<f64> = along(params.ray_width, 1)
        .into_iter()
        .map(|z| 1.0 - f64::from(params.rays) * (1.0 - ndtr(f64::from(z))))
        .collect();
    let stops: Vec<[f32; 3]> = params.colors.iter().map(|colour| colour.0).collect();
    let sky = params.sky.0;
    let (base, tall, falloff) = (
        f64::from(params.base) * length::float(rows),
        f64::from(params.height),
        f64::from(params.falloff),
    );
    picture.map_at(|row, column, _| {
        let above = base + f64::from(params.waviness) * f64::from(ripple[column]) - length::float(row);
        let up = above / tall; // 0 at the base, 1 at the curtain's top
        let brightness =
            cast::single((above + 0.5).clamp(0.0, 1.0) * (1.0 - up).clamp(0.0, 1.0).powf(falloff) * ray[column]);
        let colour = ramp::at(&stops, cast::single(up.clamp(0.0, 1.0)));
        [0, 1, 2].map(|channel| sky[channel] + brightness * (colour[channel] - sky[channel]))
    })
}
