//! `gradient_map`: brightness mapped onto a gradient of colours (duotone, tritone...). What it does is documented once,
//! on `Params`.

use serde::Deserialize;

use crate::effect::{Category, Meta};
use crate::ops::cast::single;
use crate::ops::color::luminance;
use crate::ops::ramp;
use crate::params::{Choice, Color, Params};
use crate::{Canvas, Footprint};
use crate::{Image, Inputs, Picture};

/// Where `gradient_map` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Colour,
    opacity: 1.0,
    inputs: &[],
};

/// What `gradient_map` reads around a pixel.
pub(crate) fn footprint(_params: &Params, _canvas: &Canvas) -> Footprint {
    Footprint::Whole
}

/// The smallest brightness range the stretch divides by: only keeps a flat picture (every pixel equally bright) from
/// dividing by zero; it maps to the first stop.
const FLAT: f64 = 1e-6; // constant: only guards the division, a flat picture then maps to the first stop

/// How the stretch's clip points are read from the picture's brightness: the sample-quantile definitions of numpy's
/// `quantile`, `method`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum QuantileMethod {
    /// Interpolates linearly between the two nearest pixels (Hyndman and Fan type 7).
    Linear,
    /// The darker of the two nearest pixels.
    Lower,
    /// The brighter of the two nearest pixels.
    Higher,
    /// The nearer pixel, a tie going to the even position.
    Nearest,
    /// The mean of the two nearest pixels.
    Midpoint,
    /// Hyndman and Fan type 1: the inverse of the empirical distribution.
    InvertedCdf,
    /// Hyndman and Fan type 2: like `inverted_cdf`, averaging at a jump.
    AveragedInvertedCdf,
    /// Hyndman and Fan type 3: the nearest observation.
    ClosestObservation,
    /// Hyndman and Fan type 4: the distribution interpolated linearly.
    InterpolatedInvertedCdf,
    /// Hyndman and Fan type 5.
    Hazen,
    /// Hyndman and Fan type 6.
    Weibull,
    /// Hyndman and Fan type 8.
    MedianUnbiased,
    /// Hyndman and Fan type 9.
    NormalUnbiased,
}

/// Recolour by brightness: darkest pixels get the first stop, brightest the last, evenly spaced between.
///
/// A gradient that brightness is mapped onto, after an automatic contrast stretch. Two stops make a duotone, three a
/// tritone; for example the stops "#0b0020", "#ff2bd6" and "#00f0ff" make a neon scheme. The picture is first stretched
/// to use its full tonal range (cutting `shadow_clip` and `highlight_clip` off its ends), so the whole gradient is
/// always visible.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The gradient's colours from darkest to brightest, evenly spaced; at least two. Two make a duotone, three a
    /// tritone.
    #[param(default = ["#000000", "#ffffff"], min_length = 2)]
    pub stops: Vec<Color>,
    /// Share of the darkest pixels (0..0.5) that the contrast stretch pushes onto the first stop; 0 maps only the
    /// darkest pixel there. Larger gives deeper shadows.
    #[param(default = 0.01, unit = fraction, min = 0, max = 0.5)]
    pub shadow_clip: f32,
    /// Share of the brightest pixels (0..0.5) that the contrast stretch pushes onto the last stop. Larger gives
    /// brighter highlights.
    #[param(default = 0.01, unit = fraction, min = 0, max = 0.5)]
    pub highlight_clip: f32,
    /// How the stretch's clip points are read from the picture's brightness (numpy's quantile methods): `linear`
    /// interpolates between the two nearest pixels; `lower`, `higher`, `nearest` and `midpoint` take the darker, the
    /// brighter, the nearer pixel or their mean; `inverted_cdf`, `averaged_inverted_cdf`, `closest_observation`,
    /// `interpolated_inverted_cdf`, `hazen`, `weibull`, `median_unbiased` and `normal_unbiased` are the textbook
    /// sample-quantile definitions (Hyndman and Fan). Only visible on small or flat pictures.
    #[param(default = Linear, advanced)]
    pub quantile_method: QuantileMethod,
}

/// Where a quantile falls among the sorted values.
enum Place {
    /// On the value at this index.
    At(usize),
    /// At `index` among them, a position that may fall between two values (or outside them: the first or last then
    /// counts); `gamma` is the weight of the second of the two.
    Between { index: f64, gamma: f64 },
}

/// The virtual index of Hyndman and Fan's continuous types: `n q + (alpha + q (1 - alpha - beta)) - 1`.
fn virtual_index(n: f64, q: f64, alpha: f64, beta: f64) -> f64 {
    n * q + (alpha + q * (1.0 - alpha - beta)) - 1.0
}

/// A continuous type: the fractional part of the virtual index weighs the next value.
fn continuous(index: f64) -> Place {
    Place::Between {
        index,
        gamma: index - index.floor(),
    }
}

/// The jump types: `index` rounded down where `down` holds for its fractional part and itself, else up; not below the
/// first value.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the index is floored and not below 0, and a picture has far fewer than 2^52 pixels"
)]
fn discrete(index: f64, down: impl Fn(f64, f64) -> bool) -> Place {
    let previous = index.floor();
    let chosen = if down(index - previous, index) {
        previous
    } else {
        previous + 1.0
    };
    Place::At(chosen.max(0.0) as usize)
}

/// Whether `x` has no fractional part: numpy tests a virtual index for it exactly.
fn whole(x: f64) -> bool {
    x.fract() == 0.0
}

/// Where quantile `q` of `n` sorted values falls under `method`: the definitions of numpy's `quantile`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a rounded index is not below 0, and a picture has far fewer than 2^52 pixels"
)]
fn place(n: f64, q: f64, method: QuantileMethod) -> Place {
    use QuantileMethod::{
        AveragedInvertedCdf, ClosestObservation, Hazen, Higher, InterpolatedInvertedCdf, InvertedCdf, Linear, Lower,
        MedianUnbiased, Midpoint, Nearest, NormalUnbiased, Weibull,
    };
    let along = (n - 1.0) * q;
    match method {
        Linear => continuous(along),
        Lower => Place::At(along.floor() as usize),
        Higher => Place::At(along.ceil() as usize),
        Nearest => Place::At(along.round_ties_even() as usize),
        Midpoint => Place::Between {
            index: f64::midpoint(along.floor(), along.ceil()),
            gamma: if whole(along) { 0.0 } else { 0.5 },
        },
        InvertedCdf => discrete(n * q - 1.0, |gamma, _| whole(gamma)),
        // at a jump of the distribution (a whole index) the mean of the value there and the next, else the next value
        AveragedInvertedCdf => Place::Between {
            index: n * q - 1.0,
            gamma: if whole(n * q - 1.0) { 0.5 } else { 1.0 },
        },
        // at a whole index, the even-numbered of the two values (counting from 1)
        // constant: Hyndman and Fan type 3 takes the index n q - 1 less half an observation
        ClosestObservation => discrete(n * q - 1.5, |gamma, index| {
            whole(gamma) && index.floor().rem_euclid(2.0) > 0.5
        }),
        InterpolatedInvertedCdf => continuous(virtual_index(n, q, 0.0, 1.0)),
        Hazen => continuous(virtual_index(n, q, 0.5, 0.5)),
        Weibull => continuous(virtual_index(n, q, 0.0, 0.0)),
        MedianUnbiased => continuous(virtual_index(n, q, 1.0 / 3.0, 1.0 / 3.0)), // constant: Hyndman and Fan type 8, alpha = beta = 1/3
        NormalUnbiased => continuous(virtual_index(n, q, 3.0 / 8.0, 3.0 / 8.0)), // constant: Hyndman and Fan type 9, alpha = beta = 3/8
    }
}

/// The value that would stand at `index` if `values` were sorted. Reorders `values`.
fn order_statistic(values: &mut [f32], index: usize) -> f32 {
    *values.select_nth_unstable_by(index, f32::total_cmp).1
}

/// `below` mixed toward `above` by `gamma`, as numpy mixes them: the difference taken in single precision, then added to
/// `below` in the first half of the way and subtracted from `above` in the second.
fn mix(below: f32, above: f32, gamma: f64) -> f64 {
    let difference = f64::from(above - below);
    if gamma >= 0.5 {
        f64::from(above) - difference * (1.0 - gamma)
    } else {
        f64::from(below) + difference * gamma
    }
}

/// Quantile `q` (0..1) of `values` under `method`. Reorders `values`, which are not empty.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a picture has far fewer than 2^52 pixels; an index in range is floored and not below 0"
)]
fn quantile(values: &mut [f32], q: f64, method: QuantileMethod) -> f64 {
    let last = values.len() - 1;
    match place(values.len() as f64, q, method) {
        Place::At(index) => f64::from(order_statistic(values, index.min(last))),
        Place::Between { index, .. } if index >= last as f64 => f64::from(order_statistic(values, last)),
        Place::Between { index, .. } if index < 0.0 => f64::from(order_statistic(values, 0)),
        Place::Between { index, gamma } => {
            let below = index.floor() as usize;
            let (low, high) = (order_statistic(values, below), order_statistic(values, below + 1));
            mix(low, high, gamma)
        }
    }
}

/// The picture with its brightness stretched and mapped onto the stops.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let stops: Vec<[f32; 3]> = params.stops.iter().map(|stop| stop.0).collect();
    let mut brightness: Vec<f32> = picture.pixels().iter().map(|&pixel| luminance(pixel)).collect();
    let method = params.quantile_method;
    let dark = quantile(&mut brightness, f64::from(params.shadow_clip), method);
    let bright = quantile(&mut brightness, 1.0 - f64::from(params.highlight_clip), method);
    let span = (bright - dark).max(FLAT);
    picture.map(|pixel| {
        let position = ((f64::from(luminance(pixel)) - dark) / span).clamp(0.0, 1.0);
        ramp::at(&stops, single(position))
    })
}
