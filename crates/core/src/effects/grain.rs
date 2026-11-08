//! `grain`: seeded noise whose strength follows the brightness. What it does is documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::blur::gaussian_filter_plane;
use crate::ops::cast;
use crate::ops::color::luminance;
use crate::ops::edge::Edge;
use crate::ops::noise::standardised;
use crate::ops::par;
use crate::ops::random::{mix, unit};
use crate::params::{Choice, Params};
use crate::{Image, Inputs, Picture};

/// Where `grain` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Texture,
    opacity: 1.0,
    inputs: &[],
};

/// What `grain` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Whole
}

/// Where the noise values come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Choice)]
pub enum Distribution {
    /// A bell curve: most values small, a few large.
    Gaussian,
    /// Every value up to a limit of about 1.7 times the deviation equally likely.
    Uniform,
}

/// Film grain or sensor noise: noise of standard deviation `amount` at mid-grey, clumped to `size` px, heavier where
/// `balance` says.
///
/// The noise, Gaussian or uniform by `distribution` (zero mean, unit standard deviation either way), is multiplied by the
/// deviation at the pixel's brightness, its Rec. 709 luminance L: `amount` x (1 + `balance` x (2 L - 1)). It is the same in
/// all channels (grey grain) unless `color`, which gives each channel its own. The result is clipped to 0..1; the same
/// `seed` always gives the same grain.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Standard deviation of the grain at mid-grey, in 0..1 picture units: 0 = none, 0.08 is visible, 0.2 is heavy. With
    /// `balance` the grain at black and at white is up to 2 x this.
    #[param(default = 0.08, unit = fraction, min = 0, max = 1, drivable)]
    pub amount: f32,
    /// Where the grain is heavier, from -1 to 1: -1 = only in the shadows, 0 = even, 1 = only in the highlights. The grain
    /// at brightness L (0 black, 1 white) is `amount` x (1 + `balance` x (2 L - 1)), so the shadows get `amount` x (1 -
    /// `balance`) and the highlights `amount` x (1 + `balance`). Real film is grainiest in the shadows (a negative balance).
    #[param(default = 0.0, unit = ratio, min = -1, max = 1, drivable)]
    pub balance: f32,
    /// Grain size in px: up to 1 every pixel varies on its own; above 1 the noise is blurred (Gaussian sigma = size / 2)
    /// into specks about this wide, keeping its standard deviation. Larger = coarser, softer grain.
    #[param(default = 1.0, unit = px, min = 0, max = 50)]
    pub size: f32,
    /// True gives every channel its own grain (coloured speckle, like a high-ISO sensor); False adds the same grain to all
    /// three (brightness only, grey speckle).
    #[param(default = false)]
    pub color: bool,
    /// Where the noise values come from: 'gaussian' (bell curve: most values small, a few large) or 'uniform' (every value
    /// up to a limit of about 1.7 x the deviation equally likely: an even, bounded speckle). Both have the standard
    /// deviation set by `amount` and `balance`, and both are clumped by `size`.
    #[param(default = Gaussian)]
    pub distribution: Distribution,
    /// Random seed: the same seed always gives the same grain, another seed a different pattern.
    #[param(default = 0, unit = count, min = 0, max = 4294967295)]
    pub seed: u32,
    /// Where the clumping blur's Gaussian is cut off, in sigmas (only when size > 1). Larger is more exact and slower.
    #[param(default = 4.0, unit = ratio, min = 1, max = 10, advanced)]
    pub truncate: f32,
    /// What the clumping blur sees beyond the picture's edge (only when size > 1): 'reflect' (mirrored, edge pixel
    /// repeated), 'mirror' (mirrored about the edge pixel), 'nearest' (the edge pixel repeated), 'wrap' (the opposite edge,
    /// so the grain tiles seamlessly).
    #[param(default = Reflect, advanced)]
    pub edge: Edge,
}

/// One value of the unit-deviation noise of channel `stream` at pixel `at`: a pure function of its arguments, so the noise does
/// not depend on how the rows are cut into bands.
fn draw(params: &Params, stream: u64, at: usize) -> f32 {
    let word = |part: u64| unit(mix(params.seed, &[stream, at as u64, part]));
    match params.distribution {
        // Box-Muller (1958): two independent uniforms make a standard normal; 1 - u is in (0, 1], so its log is finite
        Distribution::Gaussian => {
            let radius = (-2.0 * (1.0 - word(0)).ln()).sqrt();
            cast::single(radius * (std::f64::consts::TAU * word(1)).cos())
        }
        // uniform on -b..b has deviation b / sqrt(3), so b = sqrt(3) gives 1
        Distribution::Uniform => cast::single((2.0 * word(0) - 1.0) * 3.0_f64.sqrt()), // constant: the uniform's deviation
    }
}

/// The noise of channel `stream`, `height` x `width`: mean 0 and deviation 1, clumped into specks about `size` px wide.
fn noise(height: usize, width: usize, params: &Params, stream: u64) -> Vec<f32> {
    let mut plane = vec![0.0; height * width];
    par::rows(&mut plane, width, |first_row, band| {
        for (offset, value) in band.iter_mut().enumerate() {
            *value = draw(params, stream, first_row * width + offset);
        }
    });
    // a speck of 1 px or less is a single pixel: nothing to clump
    if params.size <= 1.0 {
        return plane;
    }
    let blurred = gaussian_filter_plane(
        &plane,
        (height, width),
        f64::from(params.size) / 2.0,
        f64::from(params.truncate),
        params.edge.extension(),
    );
    standardised(blurred)
}

/// The picture with the grain laid over it.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let (height, width) = (picture.height(), picture.width());
    // one noise for all three channels, or one for each
    let channels = if params.color { 3 } else { 1 }; // constant: colour grain is one noise per red, green, blue
    let planes: Vec<Vec<f32>> = (0..channels)
        .map(|stream| noise(height, width, params, stream))
        .collect();
    picture.map_at(|row, column, pixel| {
        let at = row * width + column;
        let deviation = params.amount * (1.0 + params.balance * (2.0 * luminance(pixel) - 1.0));
        let mut out = pixel;
        for (channel, level) in out.iter_mut().enumerate() {
            let plane = &planes[channel % planes.len()];
            *level = (*level + deviation * plane[at]).clamp(0.0, 1.0);
        }
        out
    })
}
