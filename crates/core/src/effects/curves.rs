//! `curves`: a tone curve through control points (master and per colour channel), as in Photoshop's and Lightroom's
//! Curves. What it does is documented once, on `Params`.

use serde::Deserialize;

use crate::Footprint;
use crate::effect::{Category, Meta};
use crate::ops::tone::Curve;
use crate::params::Params;
use crate::{Image, Inputs, Picture};

/// Where `curves` sits in the catalogue.
pub(crate) const META: Meta = Meta {
    category: Category::Colour,
    opacity: 1.0,
    inputs: &[],
};

/// What `curves` reads around a pixel.
pub(crate) fn footprint(_params: &Params) -> Footprint {
    Footprint::Pixel
}

/// Where the control points sit on the input axis: black, a quarter, the middle, three quarters and white.
const INPUTS: [f64; 5] = [0.0, 0.25, 0.5, 0.75, 1.0]; // constant: the five points the parameters name, evenly spaced

/// Bend tones along smooth curves through control points: a master curve, then one per colour channel (cross process,
/// solarize).
///
/// The master curve has five points at input 0, 0.25, 0.5, 0.75 and 1 (`black`, `shadows`, `midtones`, `highlights`,
/// `white`) and is applied to all channels; each channel curve then has its own three middle points between a fixed
/// black and white. A curve passes exactly through its points and is a monotone cubic (PCHIP): it never overshoots its
/// neighbouring points, so rising points never darken a brighter tone. The defaults are the straight line.
#[derive(Debug, Clone, PartialEq, Deserialize, Params)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// The master curve's output for pure black, on the input axis at 0 (0..1): raising it lifts the blacks to a
    /// matte grey.
    #[param(default = 0.0, unit = fraction, min = 0, max = 1)]
    pub black: f32,
    /// The master curve's output for the input a quarter of the way up (0..1; 0.25 = unchanged): lower darkens the
    /// shadows, higher opens them up.
    #[param(default = 0.25, unit = fraction, min = 0, max = 1)]
    pub shadows: f32,
    /// The master curve's output for the middle input, 0.5 (0..1; 0.5 = unchanged): lower darkens the midtones,
    /// higher brightens them.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1)]
    pub midtones: f32,
    /// The master curve's output for the input three quarters of the way up (0..1; 0.75 = unchanged): lower dims
    /// the highlights, higher makes them brighter.
    #[param(default = 0.75, unit = fraction, min = 0, max = 1)]
    pub highlights: f32,
    /// The master curve's output for pure white, at 1 on the input axis (0..1): lowering it dims the whites.
    #[param(default = 1.0, unit = fraction, min = 0, max = 1)]
    pub white: f32,
    /// The red curve's output for the input a quarter of the way up (0..1; 0.25 = unchanged), applied after the
    /// master curve: raising it adds red to the shadows, lowering it takes red away. Its ends stay at black and
    /// white.
    #[param(default = 0.25, unit = fraction, min = 0, max = 1, advanced)]
    pub red_shadows: f32,
    /// The red curve's output for the input the middle, 0.5 (0..1; 0.5 = unchanged), applied after the master
    /// curve: raising it adds red to the midtones, lowering it takes red away. Its ends stay at black and white.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1, advanced)]
    pub red_midtones: f32,
    /// The red curve's output for the input three quarters of the way up (0..1; 0.75 = unchanged), applied after
    /// the master curve: raising it adds red to the highlights, lowering it takes red away. Its ends stay at black
    /// and white.
    #[param(default = 0.75, unit = fraction, min = 0, max = 1, advanced)]
    pub red_highlights: f32,
    /// The green curve's output for the input a quarter of the way up (0..1; 0.25 = unchanged), applied after the
    /// master curve: raising it adds green to the shadows, lowering it takes green away. Its ends stay at black and
    /// white.
    #[param(default = 0.25, unit = fraction, min = 0, max = 1, advanced)]
    pub green_shadows: f32,
    /// The green curve's output for the input the middle, 0.5 (0..1; 0.5 = unchanged), applied after the master
    /// curve: raising it adds green to the midtones, lowering it takes green away. Its ends stay at black and
    /// white.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1, advanced)]
    pub green_midtones: f32,
    /// The green curve's output for the input three quarters of the way up (0..1; 0.75 = unchanged), applied after
    /// the master curve: raising it adds green to the highlights, lowering it takes green away. Its ends stay at
    /// black and white.
    #[param(default = 0.75, unit = fraction, min = 0, max = 1, advanced)]
    pub green_highlights: f32,
    /// The blue curve's output for the input a quarter of the way up (0..1; 0.25 = unchanged), applied after the
    /// master curve: raising it adds blue to the shadows, lowering it takes blue away. Its ends stay at black and
    /// white.
    #[param(default = 0.25, unit = fraction, min = 0, max = 1, advanced)]
    pub blue_shadows: f32,
    /// The blue curve's output for the input the middle, 0.5 (0..1; 0.5 = unchanged), applied after the master
    /// curve: raising it adds blue to the midtones, lowering it takes blue away. Its ends stay at black and white.
    #[param(default = 0.5, unit = fraction, min = 0, max = 1, advanced)]
    pub blue_midtones: f32,
    /// The blue curve's output for the input three quarters of the way up (0..1; 0.75 = unchanged), applied after
    /// the master curve: raising it adds blue to the highlights, lowering it takes blue away. Its ends stay at
    /// black and white.
    #[param(default = 0.75, unit = fraction, min = 0, max = 1, advanced)]
    pub blue_highlights: f32,
}

/// The curved picture.
pub(crate) fn render(picture: &Picture<'_>, params: &Params, _inputs: &Inputs<'_>) -> Image {
    let master = Curve::through(
        &INPUTS,
        &[
            params.black,
            params.shadows,
            params.midtones,
            params.highlights,
            params.white,
        ]
        .map(f64::from),
    );
    // a channel curve starts at black and ends at white; only its middle three points move
    let curve_of = |heights: [f32; 3]| {
        let [shadows, midtones, highlights] = heights.map(f64::from);
        Curve::through(&INPUTS, &[0.0, shadows, midtones, highlights, 1.0])
    };
    let channels = [
        curve_of([params.red_shadows, params.red_midtones, params.red_highlights]),
        curve_of([params.green_shadows, params.green_midtones, params.green_highlights]),
        curve_of([params.blue_shadows, params.blue_midtones, params.blue_highlights]),
    ];
    picture.map(|pixel| {
        let toned = pixel.map(|value| master.apply(value));
        [0, 1, 2].map(|channel| channels[channel].apply(toned[channel]))
    })
}
