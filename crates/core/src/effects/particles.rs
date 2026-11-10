//! `particles`: dots and streaks of light at different depths. What it does is documented once, on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Input, Meta};
use crate::ops::length;
use crate::ops::light::screen;
use crate::ops::random::{mix, unit};
use crate::ops::segments::{Segment, draw};
use crate::params::{Color, Params};
use crate::{Canvas, Footprint};
use crate::{Image, Inputs, Picture};

/// Where `particles` sits in the catalogue: the light is laid over the picture partway, so a step lays it at 0.8.
pub(crate) const META: Meta = Meta {
    category: Category::Atmosphere,
    opacity: 0.8, // constant: the default opacity of a step: light screened partway keeps the picture showing
    inputs: &[Input::Place],
};

/// What `particles` reads around a pixel: none but its own; the particles lie at random places over the canvas.
pub(crate) fn footprint(_params: &Params, _canvas: &Canvas) -> Footprint {
    Footprint::Pixel
}

/// Screen `density` particles of `color` per 1000 px² at random places, each a dot or a streak along `angle`: snow, rain,
/// embers, floating dust, stars.
///
/// Every particle lies at a random nearness d from 0 (farthest) to 1 (nearest), or in one of `layers` planes k / n. Its width
/// and streak length are `size` and `length` times 1 - `size_falloff` x (1 - d), and it screens 1 - `brightness_falloff` x
/// (1 - d) of `color` along its middle, with an edge `softness` of its radius soft, so the picture has depth. Where
/// particles overlap the brighter one shows; they are cut off by the picture's edge and only lighten it. A higher `density`
/// keeps every particle of a lower one and adds more; the same `seed` always gives the same particles.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How many particles there are: particles per 1000 design px² (a 32 x 32 px area at the recipe's size), so any picture
    /// size looks alike. 0 = none (the picture unchanged), 1 is a steady shower of rain, 7 a clear night sky of stars, 244 a
    /// dense field of tiny ones.
    #[param(default = 1.0, unit = density, min = 0, max = 300)]
    pub density: f32,
    /// The width in px of the nearest, largest particles; farther ones are narrower by `size_falloff`. Tuned for a picture
    /// about 480 px high. Larger = bigger dots and thicker streaks.
    #[param(default = 6.0, unit = px, min = 1, max = 200)]
    pub size: f32,
    /// The length in px of the streak each nearest particle draws from its head along `angle`; farther ones are shorter by
    /// `size_falloff`. Tuned for a picture about 480 px high. 0 = round dots, larger = longer streaks, as with a slower
    /// shutter.
    #[param(default = 0.0, unit = px, min = 0, max = 200)]
    pub length: f32,
    /// The direction a streak runs from its head, in degrees clockwise from pointing right, as seen on screen: 0 = right,
    /// 90 = down, 180 = left, 270 = up. Only streaks show it: with a `length` of 0 it changes nothing.
    #[param(default = 90.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub angle: f32,
    /// How soft a particle's edge is: it falls from full brightness to none across this fraction of the particle's radius (at
    /// least one pixel), so large ones are blurrier than small ones, like particles out of focus. 0 = crisp one-pixel
    /// edges, 1 = fading out from the middle.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub softness: f32,
    /// The colour of the light: '#rrggbb', '#rgb' or [r, g, b] 0..1. Particles are screened over the picture, so they only
    /// lighten it.
    #[param(default = "#ffffff")]
    pub color: Color,
    /// How many depth planes the particles are in. In plane k of n the particle is k / n of the way from the farthest to the
    /// nearest, so more planes = a smoother range of depths, and 1 = every particle at the nearest depth. 0 = no planes,
    /// every depth between the nearest and the farthest occurs.
    #[param(default = 0, unit = count, min = 0, max = 10, advanced)]
    pub layers: i32,
    /// How much smaller the farthest particles are: their width and length are 1 - this of the nearest ones'. 0 = every
    /// particle the same size, 1 = from the nearest down to nothing at the farthest.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1, advanced)]
    pub size_falloff: f32,
    /// How much fainter the farthest particles are: their brightness is 1 - this of the nearest ones'. 0 = every particle
    /// equally bright, 1 = from the nearest down to nothing at the farthest.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1, advanced)]
    pub brightness_falloff: f32,
    /// Random seed: the same seed always puts the same particles in the same places, another seed others.
    #[param(default = 0, unit = count, min = 0, max = 4294967295)]
    pub seed: u32,
}

/// The picture with the particles screened over it.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let place = inputs.place(picture);
    let mut layer = vec![0.0_f32; height * width];
    // the particles are those of the canvas, whichever rows are drawn; their number, at most 300 per 1000 px^2 times the
    // canvas's area, is bounded by the density's maximum and the files' pixel limit (a row in `limits.rs`)
    let (rows, columns) = (length::float(place.canvas_height), length::float(width));
    // constant: density counts particles per 1000 square px
    let count = length::whole(f64::from(params.density) * rows * columns / 1000.0);
    let heading = f64::from(params.angle).to_radians();
    let (size, streak) = (f64::from(params.size), f64::from(params.length));
    let layers = f64::from(params.layers);
    for particle in 0..count {
        // particle number `particle`, its `part`-th number: a hash, so a higher count keeps every particle of a lower one
        // constant: the numbers of the three draws that make a particle, its x, y and nearness
        let [x, y, drawn] = [0, 1, 2].map(|part| unit(mix(params.seed, &[particle as u64, part])));
        let nearness = if params.layers > 0 {
            ((drawn * layers).floor() + 1.0) / layers
        } else {
            drawn
        };
        let scale = 1.0 - f64::from(params.size_falloff) * (1.0 - nearness);
        let start = (x * columns, y * rows);
        let reach = streak * scale;
        let radius = size / 2.0 * scale;
        draw(
            &mut layer,
            (height, width),
            place.first_row,
            &Segment {
                start,
                end: (start.0 + reach * heading.cos(), start.1 + reach * heading.sin()),
                radius,
                weight: 1.0 - f64::from(params.brightness_falloff) * (1.0 - nearness),
                feather: f64::from(params.softness) * radius,
            },
        );
    }
    let tint = params.color.0;
    picture.map_at(|row, column, pixel| {
        let light = layer[row * width + column];
        screen(pixel, tint.map(|level| level * light))
    })
}
