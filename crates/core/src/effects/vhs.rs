//! `vhs`: a worn videotape or failing analog signal. What it does is documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Input, Meta};
use crate::ops::chroma::smeared;
use crate::ops::colorspace::{rgb_to_yiq, yiq_to_rgb};
use crate::ops::edge::Edge;
use crate::ops::length::whole;
use crate::ops::noise::{clumped, standard_normal};
use crate::ops::par;
use crate::ops::random::{Rng, mix, unit};
use crate::ops::shift::{Band, slide_row};
use crate::params::Params;
use crate::{Image, Inputs, Picture, Place};

/// Where `vhs` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Glitch,
    opacity: 1.0,
    inputs: &[Input::Place],
};

/// What `vhs` reads around a pixel: bands, dropouts and the wobble fall at canvas rows chosen from the seed and the canvas's
/// height, the snow and the noise are numbered by canvas place, and a row reads only itself.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Pixel
}

/// How far `vhs` paints beyond the picture's edge: not worked out.
pub(crate) fn spread(_params: &Params) -> crate::Spread {
    crate::Spread::Unbounded
}

/// Make the picture look like a worn videotape or a failing analog signal: tracking bands, dropouts, wobbling rows, bled
/// colour and tape noise (static, snow).
///
/// In this order: `bands` horizontal bands slide sideways by up to `tracking` px and take `band_static` of grey static
/// (solid bands of snow across a clean picture at 1); every row then drifts sideways by a slow random wander of about
/// `wobble` px; `dropouts` white streaks are stamped on; in YIQ (as `ntsc`) the colour is moved `bleed` px right and blurred
/// sideways by that much, as colour on tape lags and smears; and `noise` of grain is added to the brightness. The bands,
/// dropouts, noise and wobble each draw from their own stream of `seed`, so the same seed gives the same damage; changing
/// the number of bands moves no dropout, noise or wobble. A row in several bands slides by the sum of their distances.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// How far a tracking band slides sideways, at most, in px, 0 to 2000, tuned for a picture about 480 px high: each band
    /// slides by a random whole number of pixels up to this, left or right. 0 = the bands do not move.
    #[param(default = 28.8, unit = px, min = 0, max = 2000)]
    pub tracking: f32,
    /// How many horizontal tracking bands slide, 0 to 50, at random rows; 0 = none.
    #[param(default = 3, unit = count, min = 0, max = 50)]
    pub bands: i32,
    /// How much of each tracking band is replaced by grey static (analog snow), 0 to 1: the share of every pixel in a band
    /// that becomes a random grey. 0 = none; 1 = bands of pure static. Does nothing when `bands` is 0.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1, drivable)]
    pub band_static: f32,
    /// How many dropout streaks, 0 to 2000: short white marks where the signal was lost, each as tall as a speck of tape
    /// noise (`noise_size`) and between half of `tracking` and `tracking` px long. 0 = none; raising it covers more of the
    /// picture.
    #[param(default = 0, unit = count, min = 0, max = 2000)]
    pub dropouts: i32,
    /// How far colour bleeds to the right of what it belongs to, 0 to 100 px, tuned for a picture about 480 px high: colour
    /// lags the brightness by this many pixels and is blurred sideways by a Gaussian of this standard deviation. 0 = no
    /// bleed; raising it smears red and blue far past their edges.
    #[param(default = 5.0, unit = px, min = 0, max = 100)]
    pub bleed: f32,
    /// The standard deviation of the tape noise added to brightness, 0 to 1 in picture units: 0 = none, 0.03 is visible, 0.1
    /// is heavy. Colour is not touched.
    #[param(default = 0.03, unit = fraction, min = 0, max = 1)]
    pub noise: f32,
    /// How far the rows wander sideways, as the standard deviation in px of a slow random drift from row to row, 0 to 50,
    /// tuned for a picture about 480 px high: 0 = straight rows, a few px = the shaky edges of a worn tape.
    #[param(default = 1.5, unit = px, min = 0, max = 50)]
    pub wobble: f32,
    /// Random seed for the bands, the dropouts, the noise and the wobble: the same seed always gives the same damage, any
    /// other a different one.
    #[param(default = 0, unit = count, min = 0, max = 4294967295)]
    pub seed: u32,
    /// The tallest a tracking band can be, in px, 0 to 2000, tuned for a picture about 480 px high; each band is between
    /// half this and this tall (never under 1 px, never taller than the picture).
    #[param(default = 19.2, unit = px, min = 0, max = 2000, advanced)]
    pub band_height: f32,
    /// How wide a speck of tape noise is, 0.5 to 50 px (up to 1 px every pixel is its own speck), tuned for a picture about
    /// 480 px high. Raising it gives coarser grain.
    #[param(default = 1.5, unit = px, min = 0.5, max = 50, advanced)]
    pub noise_size: f32,
    /// How many rows one wander of the wobble spans, 0.5 to 500 px, tuned for a picture about 480 px high: small is a jitter
    /// from row to row, large a slow sway.
    #[param(default = 12.0, unit = px, min = 0.5, max = 500, advanced)]
    pub wobble_size: f32,
    /// Where the blurs of the bleed, the noise and the wobble are cut off, in sigmas, 1 to 10. Lower is faster and gives
    /// slightly sharper tails.
    #[param(default = 4.0, unit = ratio, min = 1, max = 10, advanced)]
    pub truncate: f32,
    /// What lies beyond the picture's left and right edges for everything that moves or blurs sideways: 'nearest' the edge
    /// pixel repeated (a worn tape smears its edge rather than bringing the far side round), 'wrap' the opposite side,
    /// 'reflect' the picture mirrored (edge pixel repeated once), 'mirror' mirrored about the edge pixel. For the noise and
    /// wobble the rules act on the rows and columns of their own fields.
    #[param(default = Nearest, advanced)]
    pub edge: Edge,
}

// The independent streams of `seed`; the bands and the dropouts draw sequentially, so each has a stream of its own seed.
const BANDS: u64 = 0; // constant: names of the streams, any distinct numbers
const DROPOUTS: u64 = 1; // constant: names of the streams, any distinct numbers
const NOISE: u64 = 2; // constant: names of the streams, any distinct numbers
const WOBBLE: u64 = 3; // constant: names of the streams, any distinct numbers
const STATIC: u64 = 4; // constant: names of the streams, any distinct numbers

/// The seed of the sequential stream `stream` of `seed`.
fn stream_seed(seed: u32, stream: u64) -> u32 {
    (mix(seed, &[stream]) >> 32) as u32 // constant: the hash's upper half
}

/// The tracking bands: `bands` of them, a height of `band_height / 2` to `band_height` (at least 1, at most the picture's),
/// a row for the top that keeps the band inside the picture, and a whole-pixel slide of up to `tracking` px either way.
fn tracking_bands(height: usize, params: &Params) -> Vec<Band> {
    let mut rng = Rng::seeded(stream_seed(params.seed, BANDS));
    let (tallest, furthest) = (f64::from(params.band_height), f64::from(params.tracking));
    (0..params.bands)
        .map(|_| {
            let rows = whole(tallest / 2.0 + rng.uniform() * tallest / 2.0).clamp(1, height);
            let top = rng.below(height - rows + 1);
            #[expect(
                clippy::cast_possible_truncation,
                reason = "a slide of at most 2000 px, rounded to whole px"
            )]
            let distance = (-furthest + rng.uniform() * 2.0 * furthest).round_ties_even() as i64;
            Band { top, rows, distance }
        })
        .collect()
}

/// How far each row drifts to the right, in px: `wobble` times a slow noise of unit deviation, one value for each row.
/// The drift of each of the canvas's `height` rows (a short list over the whole canvas, made the same whichever rows are drawn).
fn wander(height: usize, params: &Params) -> Vec<f32> {
    if params.wobble == 0.0 {
        return vec![0.0; height];
    }
    // one value for each row: a field one row high and `height` wide
    let mut drift = clumped(
        (1, height),
        (0, 1),
        f64::from(params.wobble_size),
        f64::from(params.truncate),
        params.edge.extension(),
        |at| standard_normal(params.seed, WOBBLE, at),
    );
    for value in &mut drift {
        *value *= params.wobble;
    }
    drift
}

/// White streaks stamped on `pixels` (`width` wide): `dropouts` of them, as tall as a speck of noise and `tracking / 2` to
/// `tracking` px long, at random places inside the picture.
fn stamp_dropouts(pixels: &mut [[f32; 3]], width: usize, place: Place, params: &Params) {
    // the dropouts lie on the canvas (at most 2000 of them, the parameter's maximum), whichever rows are drawn
    let height = place.canvas_height;
    let rows = pixels.len() / width;
    let mut rng = Rng::seeded(stream_seed(params.seed, DROPOUTS));
    let tracking = f64::from(params.tracking);
    let streak = whole(f64::from(params.noise_size)).max(1);
    for _ in 0..params.dropouts {
        let length = whole(tracking / 2.0 + rng.uniform() * tracking / 2.0).clamp(1, width);
        let row = rng.below(height);
        let start = rng.below(width - length + 1);
        // the streak's canvas rows that are in the picture
        let (top, bottom) = (row.max(place.first_row), (row + streak).min(place.first_row + rows));
        for line in pixels
            .chunks_exact_mut(width)
            .skip(top.saturating_sub(place.first_row))
            .take(bottom.saturating_sub(top))
        {
            line[start..start + length].fill([1.0; 3]); // constant: a dropout is white (the top of the 0..1 range)
        }
    }
}

/// What makes one row of the damaged picture, which depends on its row of the picture alone: the slide of the bands over
/// it, their static and the drift of the wobble.
struct Rows<'a> {
    pixels: &'a [[f32; 3]],
    width: usize,
    bands: &'a [Band],
    /// The canvas row of the picture's first row.
    first_row: usize,
    slides: Vec<i64>,
    drift: Vec<f32>,
    params: &'a Params,
}

impl Rows<'_> {
    /// Row `row` of the damaged picture into `line`.
    #[expect(clippy::cast_precision_loss, reason = "a slide of at most 2000 px")]
    fn make(&self, row: usize, slid: &mut [[f32; 3]], line: &mut [[f32; 3]]) {
        let canvas_row = self.first_row + row;
        let edge = self.params.edge.extension();
        let source = &self.pixels[row * self.width..][..self.width];
        let share = self.params.band_static;
        let mut laid = false;
        if self.slides[row] != 0 {
            slide_row(source, self.slides[row] as f64, edge, slid);
            laid = true;
        }
        if share > 0.0 {
            for (index, _) in self
                .bands
                .iter()
                .enumerate()
                .filter(|(_, band)| (band.top..band.top + band.rows).contains(&canvas_row))
            {
                if !laid {
                    slid.copy_from_slice(source);
                    laid = true;
                }
                for (column, pixel) in slid.iter_mut().enumerate() {
                    let at = (canvas_row * self.width + column) as u64;
                    #[expect(clippy::cast_possible_truncation, reason = "a grey in 0..1, kept in the f32 picture")]
                    let snow = unit(mix(self.params.seed, &[STATIC, index as u64, at])) as f32;
                    *pixel = pixel.map(|level| level * (1.0 - share) + snow * share);
                }
            }
        }
        let current: &[[f32; 3]] = if laid { slid } else { source };
        slide_row(current, f64::from(self.drift[canvas_row]), edge, line);
    }
}

/// The picture worn.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    let place = inputs.place(picture);
    // the bands (at most 50) and the drift are made over the canvas, so every band of rows meets the same ones
    let bands = tracking_bands(place.canvas_height, params);
    let slides: Vec<i64> = (place.first_row..place.first_row + height)
        .map(|canvas_row| {
            bands
                .iter()
                .filter(|band| (band.top..band.top + band.rows).contains(&canvas_row))
                .map(|band| band.distance)
                .sum()
        })
        .collect();
    let rows = Rows {
        pixels: picture.pixels(),
        width,
        bands: &bands,
        first_row: place.first_row,
        slides,
        drift: wander(place.canvas_height, params),
        params,
    };
    let mut out = vec![[0.0; 3]; height * width];
    par::rows(&mut out, width, |first_row, band| {
        let mut slid = vec![[0.0; 3]; width];
        for (at, line) in band.chunks_exact_mut(width).enumerate() {
            rows.make(first_row + at, &mut slid, line);
        }
    });
    stamp_dropouts(&mut out, width, place, params);
    if params.bleed > 0.0 || params.noise > 0.0 {
        wear_signal(&mut out, (height, width), place, params);
    }
    Image::from_pixels(height, width, out)
}

/// `pixels` with their colour moved `bleed` px right and smeared by as much, and the tape noise added to their brightness.
fn wear_signal(pixels: &mut [[f32; 3]], (height, width): (usize, usize), place: Place, params: &Params) {
    let edge = params.edge.extension();
    let truncate = f64::from(params.truncate);
    let bleed = f64::from(params.bleed);
    let planes = (bleed > 0.0).then(|| smeared(pixels, (height, width), bleed, (bleed, truncate), edge));
    let grain = (params.noise > 0.0).then(|| {
        clumped(
            (place.canvas_height, width),
            (place.first_row, height),
            f64::from(params.noise_size),
            truncate,
            edge,
            |at| standard_normal(params.seed, NOISE, at),
        )
    });
    par::rows(pixels, width, |first_row, band| {
        for (at, line) in band.chunks_exact_mut(width).enumerate() {
            let row = first_row + at;
            for (column, pixel) in line.iter_mut().enumerate() {
                let [mut luma, mut i, mut q] = rgb_to_yiq(*pixel);
                if let Some(planes) = &planes {
                    let place = row * 2 * width + column;
                    (i, q) = (planes[place], planes[place + width]);
                }
                if let Some(grain) = &grain {
                    luma += params.noise * grain[row * width + column];
                }
                *pixel = yiq_to_rgb([luma, i, q]).map(|level| level.clamp(0.0, 1.0));
            }
        }
    });
}
