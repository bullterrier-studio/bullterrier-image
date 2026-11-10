//! `bloom`: light bleeding from the highlights at several scales, a bright core with a wide soft halo. What it does is
//! documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::blur::gaussian_filter_plane;
use crate::ops::edge::EdgeOrConstant;
use crate::ops::light::{highlights, screen};
use crate::params::{Color, Params};
use crate::{Image, Inputs, Picture};

/// Where `bloom` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Light,
    opacity: 1.0,
    inputs: &[],
};

/// What `bloom` reads around a pixel.
pub(crate) fn footprint(params: &Params) -> Footprint {
    Footprint::reach(
        f64::from(params.radius * params.truncate),
        params.edge == EdgeOrConstant::Wrap,
    )
}

/// Bloom, glow, soft focus or halation: light bleeding from what is brighter than `threshold`, blurred at `octaves`
/// scales and screened over the picture.
///
/// The highlights are blurred with sigma `radius`, `radius` / 2, `radius` / 4... (`octaves` of them), weighted by
/// `persistence` per octave and averaged, then screened over the picture by `amount`. It only ever brightens and leaves
/// black black. Each channel blooms on its own, so the light keeps its colour; with `color` the brightest channel sets
/// the strength and the halo takes that colour instead, as film halation does: light that bounces back through the film
/// base exposes one layer whatever the colour of the highlight.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Gaussian sigma of the widest halo in px at the recipe's size: larger spreads the light further; 0 keeps the light
    /// on the highlights themselves. Each further octave is half as wide.
    #[param(default = 32.0, unit = px, min = 0, max = 500, drivable)]
    pub radius: f32,
    /// The brightness (0..1) where the bloom starts, judged on each channel on its own (on the brightest of the three
    /// when `color` is set): only values above it give light, rising to full at 1. Raising it limits the bloom to the
    /// brightest highlights.
    #[param(default = 0.6, unit = fraction, min = 0, max = 0.99)]
    pub threshold: f32,
    /// How strongly the halo is screened over the picture: 0 = off, 1 = the halo at full brightness; above 1 it
    /// saturates to white sooner.
    #[param(default = 0.7, unit = ratio, min = 0, max = 10, drivable)]
    pub amount: f32,
    /// The colour of the light: '#rrggbb', '#rgb' or [r, g, b] 0..1; none (the default) lets each highlight keep its own
    /// colour. A colour makes every highlight bloom in it, whatever its own colour is, with the strength of its brightest
    /// channel: '#ff4a14' is the red-orange of film halation, white a neutral glow.
    #[param()]
    pub color: Option<Color>,
    /// How many scales the halo is built from: octave 1 has the sigma `radius`, each next one half of the one before
    /// (`radius` / 2, `radius` / 4...). 1 = a single Gaussian; more adds a tighter, brighter core around the highlights.
    #[param(default = 4, unit = count, min = 1, max = 8)]
    pub octaves: i32,
    /// How much weight each octave gets relative to the wider one before it (the weights are persistence^0,
    /// persistence^1... scaled to add up to 1): 0 = the widest octave alone, 1 = every octave equally. Lower values give
    /// a softer, more diffuse bloom; higher values a tighter, more concentrated one.
    #[param(default = 0.5, unit = ratio, min = 0, max = 1)]
    pub persistence: f32,
    /// Where each octave's Gaussian is cut off, in sigmas: it reaches truncate x its sigma px. Larger is more exact and
    /// slower.
    #[param(default = 4.0, unit = ratio, min = 1, max = 10, advanced)]
    pub truncate: f32,
    /// What the halo's blur sees beyond the picture's edge: 'reflect' (mirrored, edge pixel repeated), 'mirror' (mirrored
    /// about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge) or 'constant'
    /// (`constant_value`, so the bloom fades toward the edge when it is 0).
    #[param(default = Reflect, advanced)]
    pub edge: EdgeOrConstant,
    /// The highlight (0..1) assumed beyond the edge when `edge` is 'constant'; ignored otherwise.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1, advanced)]
    pub constant_value: f32,
}

/// The picture with the halo screened over it.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let pixels = picture.pixels();
    // The highlights as planes: one for each channel, or one of the brightest channel when a colour sets the hue.
    let planes: Vec<Vec<f32>> = match params.color {
        None => (0..3)
            .map(|channel| {
                pixels
                    .iter()
                    .map(|&pixel| highlights(pixel, params.threshold)[channel])
                    .collect()
            })
            .collect(),
        Some(_) => vec![
            pixels
                .iter()
                .map(|pixel| highlights([pixel[0].max(pixel[1]).max(pixel[2]); 3], params.threshold)[0])
                .collect(),
        ],
    };
    // the halo is an average, so an even picture stays as bright as its highlights
    let shares: Vec<f64> = (0..params.octaves)
        .map(|octave| f64::from(params.persistence).powi(octave))
        .collect();
    let total: f64 = shares.iter().sum();
    let edge = params.edge.extension(params.constant_value);
    let halos: Vec<Vec<f32>> = planes
        .iter()
        .map(|plane| {
            let mut halo = vec![0.0_f32; plane.len()];
            for (octave, share) in (0..params.octaves).zip(&shares) {
                let sigma = f64::from(params.radius) / 2.0_f64.powi(octave); // constant: each octave is half as wide
                let blurred = gaussian_filter_plane(plane, (height, width), sigma, f64::from(params.truncate), edge);
                #[expect(clippy::cast_possible_truncation, reason = "a weight in 0..1 for an f32 picture")]
                let weight = (share / total) as f32;
                for (halo, blurred) in halo.iter_mut().zip(&blurred) {
                    *halo += weight * blurred;
                }
            }
            halo
        })
        .collect();
    picture.map_at(|row, column, pixel| {
        let at = row * width + column;
        let light = match params.color {
            None => [0, 1, 2].map(|channel| params.amount * halos[channel][at]),
            Some(Color(color)) => color.map(|channel| params.amount * halos[0][at] * channel),
        };
        screen(pixel, light)
    })
}
