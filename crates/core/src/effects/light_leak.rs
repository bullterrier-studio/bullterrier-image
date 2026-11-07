//! `light_leak`: warm, seeded washes of light bleeding in from one side, as from a film leak. What it does is documented
//! once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::light::screen;
use crate::ops::random::Rng;
use crate::params::{Color, Params};
use crate::{Image, Inputs, Picture};

/// Where `light_leak` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Light,
    opacity: 1.0,
    inputs: &[],
};

/// Add `blobs` seeded patches of warm light coming in from the side `angle` points to, tinted `color` and screened
/// over the picture by `amount`.
///
/// Each patch is a Gaussian bump, centred a random depth (up to `size` px) in from that side and a random place along
/// it, with a random width of half to all of `size` px, stretched along the edge by up to twice and weighted by half to
/// all. The patches add up and are squashed smoothly into 0..1 (1 - exp(-sum)), so overlaps never clip into flat
/// plateaus. The picture only ever gets brighter.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The tint of the light: '#rrggbb', '#rgb' or [r, g, b] 0..1. The default is the orange of a leaking film back;
    /// red or yellow give other leaks.
    #[param(default = "#ff8a3d")]
    pub color: Color,
    /// How strongly the light is screened over the picture: 0 = off, 1 = the brightest patch at full brightness in
    /// `color`; above 1 it saturates to white sooner.
    #[param(default = 0.8, unit = ratio, min = 0, max = 2)]
    pub amount: f32,
    /// How big the patches are and how deep they reach into the picture, in px tuned for a picture about 480 px high:
    /// each patch is a soft bump whose width is between half of this and all of it, centred between the edge and this
    /// far in. Raising it floods more of the picture.
    #[param(default = 150.0, unit = px, min = 6, max = 600)]
    pub size: f32,
    /// The side the light comes in from, as the direction from the picture's centre, in degrees clockwise from
    /// pointing right, as seen on screen: 0 = the right edge, 90 = the bottom, 180 = the left, 270 = the top; in
    /// between, the corner.
    #[param(default = 0.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub angle: f32,
    /// How many patches of light there are, at random places along that side (each brighter or fainter and stretched
    /// along the edge by chance). Overlapping patches merge into one wash; raising it gives a more even, wider leak.
    #[param(default = 3, unit = count, min = 1, max = 16)]
    pub blobs: i32,
    /// Random seed: the same seed always gives the same leak (the same patches in the same places), another seed a
    /// different one.
    #[param(default = 0, unit = count, min = 0, max = 4294967295)]
    pub seed: u32,
}

/// One patch of light, in px: where its peak lies (in from the side, and along it from the middle), its widths in
/// those two directions and its weight.
struct Patch {
    depth: f64,
    along: f64,
    sigma: f64,
    sigma_along: f64,
    weight: f64,
}

/// The patches `params` asks for, drawn from its seed. `half_edge` is how far the picture reaches along the side from
/// its middle.
fn patches(params: &Params, half_edge: f64) -> Vec<Patch> {
    let mut rng = Rng::seeded(params.seed);
    let size = f64::from(params.size);
    (0..params.blobs)
        .map(|_| {
            let (place, offset, spread, stretch, weight) =
                (rng.uniform(), rng.uniform(), rng.uniform(), rng.uniform(), rng.uniform());
            let sigma = size * (0.5 + 0.5 * spread);
            Patch {
                depth: place * size,
                along: (2.0 * offset - 1.0) * half_edge,
                sigma,
                sigma_along: sigma * (1.0 + stretch),
                weight: 0.5 + 0.5 * weight,
            }
        })
        .collect()
}

/// The picture with the leak screened over it.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    #[expect(clippy::cast_precision_loss, reason = "a picture side is far below 2**52")]
    let (height, width) = (picture.height() as f64, picture.width() as f64);
    // the maths below turns counterclockwise on the picture, `angle` clockwise
    let (sin, cos) = (-f64::from(params.angle).to_radians()).sin_cos();
    // how far the side lies from the centre and how far the picture reaches along it, at this angle
    let half_depth = f64::midpoint(width * cos.abs(), height * sin.abs());
    let half_edge = f64::midpoint(width * sin.abs(), height * cos.abs());
    let patches = patches(params, half_edge);
    let tint = params.color.0.map(|channel| params.amount * channel);
    #[expect(clippy::cast_precision_loss, reason = "a picture side is far below 2**52")]
    #[expect(clippy::cast_possible_truncation, reason = "the wash is summed in f64 and stored in the f32 picture")]
    picture.map_at(|row, column, pixel| {
        let right = column as f64 - (width - 1.0) / 2.0;
        let up = (height - 1.0) / 2.0 - row as f64;
        let depth = half_depth - (right * cos + up * sin); // px in from the side
        let along = up * cos - right * sin; // px along that side, counterclockwise
        let wash: f64 = patches
            .iter()
            .map(|patch| {
                let (in_from, across) = ((depth - patch.depth) / patch.sigma, (along - patch.along) / patch.sigma_along);
                patch.weight * (-0.5 * (in_from * in_from + across * across)).exp()
            })
            .sum();
        let light = (1.0 - (-wash).exp()) as f32;
        screen(pixel, tint.map(|channel| channel * light))
    })
}
