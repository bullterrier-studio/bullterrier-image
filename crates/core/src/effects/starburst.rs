//! `starburst`: diffraction spikes drawn through the bright points of a picture. What it does is documented once, on
//! `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Input, Meta};
use crate::ops::convolve::{Kernel, convolve};
use crate::ops::edge::Extension;
use crate::ops::length::float;
use crate::ops::light::{highlights, screen};
use crate::ops::random::{mix, unit};
use crate::params::{Color, Params};
use crate::{Image, Inputs, Picture};

/// Where `starburst` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Light,
    opacity: 1.0,
    inputs: &[Input::SizeRatio],
};

/// What `starburst` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// The highest `threshold` there is (the `max` of the field); `limit` raises a threshold no further, so near-white pixels
/// still light.
const MAX_THRESHOLD: f32 = 0.99; // constant: the field's documented maximum

/// Diffraction spikes, sparkle or star glints: what is brighter than `threshold` gets `rays` thin lines of light, tinted
/// `color` and screened over the picture by `amount`. `limit` caps how many pixels count as bright, `spacing` thins them
/// to a random few, `variation` dims each by a random amount (both from `seed`).
///
/// Each spike is `width` px wide, starts at the point at full brightness and fades linearly to nothing at `length` px;
/// they leave the point at equal angles, the first at `angle`. Each point's light is scaled by 0 or 1 (`spacing`) and by
/// a random number from 1 - `variation` to 1; the numbers depend on `seed` and the picture's size only, so a point keeps
/// its draw while the picture around it changes. Light is only drawn from points inside the picture, so nothing enters
/// from beyond its edge. The picture only ever gets brighter, and black stays black.
///
/// A recipe's size keeps the light of a bright area, and the number of glints, the same at any resolution.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How many spikes leave each bright point, evenly spread round it: 4 = a cross, 6 = a six-pointed star, 1 = a single
    /// comet tail. Raising it makes a denser star.
    #[param(default = 4, unit = count, min = 1, max = 32)]
    pub rays: i32,
    /// How far each spike reaches, in px tuned for a picture about 480 px high: it fades linearly from the point to
    /// nothing at this distance. Raising it makes longer, fainter-tipped spikes.
    #[param(default = 60.0, unit = px, min = 0.5, max = 600)]
    pub length: f32,
    /// The thickness in px of each spike, tuned for a picture about 480 px high, with a soft edge one px wide: 1 = a
    /// hairline. Raising it makes bolder spikes.
    #[param(default = 1.0, unit = px, min = 1, max = 20, advanced)]
    pub width: f32,
    /// The brightness (0..1, per channel) where a spike starts: only values above it make light, rising to full at 1.
    /// Raising it limits the spikes to the brightest points. Every pixel above it sends out spikes, so at a low
    /// threshold a large bright area (a window, a sky, white fur) floods the picture with light: raise the threshold, or
    /// cap the number of points with `limit`.
    #[param(default = 0.95, unit = fraction, min = 0, max = 0.99)]
    pub threshold: f32,
    /// The most pixels that send out spikes, as a share of all the picture's pixels, the brightest first: the threshold
    /// is raised just enough to keep to it (never above 0.99, so a picture with more than this share of near-white
    /// pixels still lights those). 1 = no cap, 0.001 = at most one pixel in a thousand, 0 = none. Lower it to keep a low
    /// `threshold` from flooding large bright areas.
    #[param(default = 1.0, unit = fraction, min = 0, max = 1, advanced)]
    pub limit: f32,
    /// How strongly the spikes are screened over the picture: 0 = off, 1 = a spike as bright as the point that makes it,
    /// at the point; above 1 it saturates to white sooner.
    #[param(default = 0.8, unit = ratio, min = 0, max = 10)]
    pub amount: f32,
    /// The direction of the first spike, in degrees clockwise from pointing right, as seen on screen: 0 = to the right,
    /// 90 = down. The other spikes follow at equal angles, so with 4 spikes 45 turns the cross into an X.
    #[param(default = 0.0, unit = degrees, min = 0, max = 360, cyclic)]
    pub angle: f32,
    /// The average distance in px, tuned for a picture about 480 px high, between the points that send out spikes inside
    /// a bright area: each bright pixel sends them with the chance 1 / spacing^2 (the same ones for the same `seed`;
    /// raising it only drops points), so the glints keep their number per area of the picture at any size. 0, or
    /// anything below 1, = every bright point, each with its share of a design pixel's light, so a bright area gives the
    /// same total light at any size. Raise it to turn a bright area into separate glints; 13 gives a scatter of glitter.
    #[param(default = 0.0, unit = px, min = 0, max = 200)]
    pub spacing: f32,
    /// How much the points differ in brightness, and so in how far their spikes show: each one's light is scaled by a
    /// random number from 1 - `variation` to 1. 0 = all alike, 1 = from full down to nothing.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1, advanced)]
    pub variation: f32,
    /// Random seed for `spacing` and `variation`: the same seed picks the same points of the same picture, another seed
    /// others.
    #[param(default = 0, unit = count, min = 0, max = 4294967295, advanced)]
    pub seed: u32,
    /// The tint of the spikes: '#rrggbb', '#rgb' or [r, g, b] 0..1. White keeps the colour of the light that makes them.
    #[param(default = "#ffffff")]
    pub color: Color,
}

/// The weights of `rays` spikes round a point: each `width` px wide with a soft edge, at full weight at the point and
/// fading linearly to nothing at `length` px, the first at `angle` degrees clockwise from the right.
fn spikes(params: &Params) -> Kernel {
    let (length, width) = (f64::from(params.length), f64::from(params.width));
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a reach in px, small and positive"
    )]
    let half = length.max(width).ceil() as usize; // the kernel covers the spike's length and its thickness
    let first = f64::from(params.angle).to_radians();
    let turn = std::f64::consts::TAU / f64::from(params.rays);
    Kernel::from_fn((half, half), |down, right| {
        (0..params.rays)
            .map(|spike| {
                let (sin, cos) = (first + turn * f64::from(spike)).sin_cos();
                let along = right * cos + down * sin; // px out along the spike's direction
                let across = (down * cos - right * sin).abs(); // px sideways from its line
                let thin = (width / 2.0 + 0.5 - across).clamp(0.0, 1.0); // a line `width` px wide, soft at its sides
                let fade = if along >= 0.0 { (1.0 - along / length).clamp(0.0, 1.0) } else { 0.0 };
                thin * fade
            })
            .fold(0.0, f64::max)
    })
}

/// The brightness above which pixels send out spikes: `params.threshold`, raised to the brightness that only `limit` of
/// the pixels exceed, but not above [`MAX_THRESHOLD`].
fn threshold(picture: &Picture<'_>, params: &Params) -> f32 {
    if params.limit >= 1.0 {
        return params.threshold;
    }
    let mut brightest: Vec<f32> = picture
        .pixels()
        .iter()
        .map(|pixel| pixel[0].max(pixel[1]).max(pixel[2]))
        .collect();
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a share of the pixels, rounded down"
    )]
    let allowed = (f64::from(params.limit) * float(brightest.len())) as usize;
    // the brightest value of the pixels that are not allowed: the (allowed + 1)th largest
    let at = brightest.len() - (allowed + 1);
    let (_, cut, _) = brightest.select_nth_unstable_by(at, f32::total_cmp);
    params.threshold.max(*cut).min(MAX_THRESHOLD)
}

/// The picture with the spikes screened over it.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    if params.amount == 0.0 || params.limit == 0.0 {
        return picture.map(|pixel| pixel);
    }
    let (height, width) = (picture.height(), picture.width());
    let kernel = spikes(params);
    let threshold = threshold(picture, params);
    let ratio = inputs.size_ratio();
    // More than one design px apart: a chance of 1 / spacing^2 per pixel. Else every bright pixel, each with its share of
    // the design pixel's light, so that a bright area keeps its total light at any size.
    let spacing = f64::from(params.spacing);
    let sparse = spacing > ratio;
    let chance = if sparse { 1.0 / (spacing * spacing) } else { 1.0 }; // constant: a chance of 1 is every pixel
    let weight = if sparse { 1.0 } else { 1.0 / (ratio * ratio) }; // constant: a weight of 1 leaves a point's light as it is
    let variation = f64::from(params.variation);
    let seed = params.seed;
    // The draws are of the pixel's place and the seed: a point keeps its draw while the picture around it changes.
    let draw = |index: usize, which: u64| unit(mix(seed, &[index as u64, which]));
    let bright: Vec<[f32; 3]> = picture
        .pixels()
        .iter()
        .enumerate()
        .map(|(index, &pixel)| {
            let light = highlights(pixel, threshold);
            if light.iter().all(|&value| value <= 0.0) {
                return light; // no draw for a pixel that makes no light
            }
            let chosen = chance >= 1.0 || draw(index, 0) < chance;
            let scale = if variation == 0.0 { 1.0 } else { 1.0 - variation * draw(index, 1) };
            let scale = if chosen { weight * scale } else { 0.0 };
            #[expect(clippy::cast_possible_truncation, reason = "narrowing is the point")]
            light.map(|value| value * scale as f32)
        })
        .collect();
    // Nothing comes in from beyond the picture's edge: black lies there.
    let spread = convolve(&bright, height, width, &kernel, Extension::Constant(0.0));
    #[expect(clippy::cast_possible_truncation, reason = "narrowing is the point")]
    let strength = (f64::from(params.amount) * kernel.total()) as f32;
    let tint = params.color.0.map(|channel| strength * channel);
    picture
        .map_at(|row, column, pixel| screen(pixel, [0, 1, 2].map(|c| spread[row * width + column][c] * tint[c])))
}
